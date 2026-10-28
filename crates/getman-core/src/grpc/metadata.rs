//! A request's headers and authentication as gRPC metadata.

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD as BASE64, STANDARD_NO_PAD};

use super::transport::decode_binary;
use crate::auth::Authenticator;
use crate::error::{Error, Result};
use crate::http::{Header, HttpClient, Transport, set_header};
use crate::model::{Auth, KeyValue};
use crate::vars::VarStack;

/// Names gRPC sets itself. Headers with these names, such as a folder's
/// `Content-Type` meant for HTTP requests, are not sent.
const RESERVED: &[&str] = &[
    "content-type",
    "content-length",
    "te",
    "host",
    "connection",
    "transfer-encoding",
    "keep-alive",
    "upgrade",
    "grpc-timeout",
    "grpc-encoding",
    "grpc-accept-encoding",
];

/// Renders the effective headers and adds the credentials. Names are
/// lowercased; values of names ending in `-bin` must be Base64 and are
/// sent unpadded; other values must be printable ASCII. OAuth 2.0 tokens
/// are fetched with `credentials`, and left out without them.
pub(crate) async fn build(
    headers: &[KeyValue],
    auth: &Auth,
    vars: &VarStack,
    missing: &mut Vec<String>,
    credentials: Option<(&Authenticator, &HttpClient, Transport)>,
) -> Result<Vec<Header>> {
    let mut render = |text: &str| {
        let out = vars.interpolate(text);
        for name in out.missing {
            if !missing.contains(&name) {
                missing.push(name);
            }
        }
        out.text
    };
    let mut metadata: Vec<Header> = Vec::new();
    for header in headers
        .iter()
        .filter(|h| h.enabled && !h.name.trim().is_empty())
    {
        let name = render(header.name.trim()).to_ascii_lowercase();
        let value = render(&header.value);
        add(&mut metadata, &name, &value)?;
    }
    match auth {
        Auth::Basic { username, password } => {
            let credentials = format!("{}:{}", render(username), render(password));
            let value = format!("Basic {}", BASE64.encode(credentials));
            add(&mut metadata, "authorization", &value)?;
        }
        Auth::Bearer { token, prefix } => {
            let prefix = if prefix.trim().is_empty() {
                "Bearer".to_owned()
            } else {
                render(prefix.trim())
            };
            let value = format!("{prefix} {}", render(token));
            add(&mut metadata, "authorization", &value)?;
        }
        Auth::ApiKey { key, value, .. } => {
            let key = render(key).trim().to_ascii_lowercase();
            if key.is_empty() {
                return Err(Error::Invalid("the API key has no name".into()));
            }
            let value = render(value);
            add(&mut metadata, &key, &value)?;
        }
        Auth::Oauth2(settings) => {
            let Some((authenticator, http, transport)) = credentials else {
                return Ok(metadata);
            };
            let token = authenticator
                .token(http, settings, vars, transport, false)
                .await?;
            let scheme =
                if token.token_type.eq_ignore_ascii_case("bearer") || token.token_type.is_empty() {
                    "Bearer"
                } else {
                    token.token_type.as_str()
                };
            let value = format!("{scheme} {}", token.access_token);
            add(&mut metadata, "authorization", &value)?;
        }
        Auth::Digest { .. } => {
            return Err(Error::Invalid(
                "Digest authentication does not work with gRPC; use Basic, Bearer, an API key or OAuth 2.0".into(),
            ));
        }
        Auth::AwsSigV4(_)
        | Auth::Jwt(_)
        | Auth::Asap(_)
        | Auth::Oauth1(_)
        | Auth::Hawk(_)
        | Auth::Edgegrid(_) => {
            return Err(Error::Invalid(
                "This authentication is not sent with gRPC calls yet; use Basic, Bearer, an API key or OAuth 2.0".into(),
            ));
        }
        Auth::Inherit | Auth::None => {}
    }
    Ok(metadata)
}

/// Adds one entry, replacing any with the same name.
fn add(metadata: &mut Vec<Header>, name: &str, value: &str) -> Result<()> {
    let valid_name = !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-_.".contains(&b));
    if !valid_name {
        return Err(Error::Invalid(format!(
            "{name:?} is not a valid metadata name; use lowercase letters, digits, - _ and ."
        )));
    }
    if RESERVED.contains(&name) {
        return Ok(());
    }
    let value = if name.ends_with("-bin") {
        let bytes = decode_binary(value).map_err(|_| {
            Error::Invalid(format!(
                "metadata {name} ends in -bin, so its value must be Base64"
            ))
        })?;
        STANDARD_NO_PAD.encode(bytes)
    } else {
        if !value.bytes().all(|b| (0x20..0x7f).contains(&b)) {
            return Err(Error::Invalid(format!(
                "the value of metadata {name} has characters outside printable ASCII; name it with a -bin ending and give the value as Base64"
            )));
        }
        value.trim().to_owned()
    };
    set_header(metadata, name, value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ApiKeyLocation;
    use crate::vars::Scope;

    async fn metadata(headers: &[KeyValue], auth: &Auth) -> Result<Vec<Header>> {
        let mut vars = VarStack::new();
        vars.push(
            Scope::Environment,
            [("token".to_owned(), "t0k3n".to_owned())]
                .into_iter()
                .collect(),
        );
        let mut missing = Vec::new();
        let authenticator = Authenticator::new();
        let http = HttpClient::new();
        build(
            headers,
            auth,
            &vars,
            &mut missing,
            Some((&authenticator, &http, Transport::default())),
        )
        .await
    }

    #[tokio::test]
    async fn headers_become_metadata_with_credentials() {
        let headers = vec![
            KeyValue::new("X-Trace", "abc"),
            KeyValue::new("Content-Type", "application/json"),
            KeyValue::new("trace-bin", "AQID"),
        ];
        let list = metadata(
            &headers,
            &Auth::Bearer {
                token: "{{token}}".into(),
                prefix: String::new(),
            },
        )
        .await
        .unwrap();
        let pairs: Vec<_> = list
            .iter()
            .map(|h| (h.name.as_str(), h.value.as_str()))
            .collect();
        assert_eq!(
            pairs,
            [
                ("x-trace", "abc"),
                ("trace-bin", "AQID"),
                ("authorization", "Bearer t0k3n")
            ]
        );

        let list = metadata(
            &[],
            &Auth::ApiKey {
                key: "X-Api-Key".into(),
                value: "k".into(),
                location: ApiKeyLocation::Query,
            },
        )
        .await
        .unwrap();
        assert_eq!(list[0].name, "x-api-key");
    }

    #[tokio::test]
    async fn invalid_metadata_explains_itself() {
        let error = metadata(&[KeyValue::new("x-bin", "%%%")], &Auth::None)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("must be Base64"), "{error}");
        let error = metadata(&[KeyValue::new("x-name", "café")], &Auth::None)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("-bin ending"), "{error}");
        let error = metadata(
            &[],
            &Auth::Digest {
                username: "a".into(),
                password: "b".into(),
            },
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("Digest"), "{error}");
    }
}
