//! A request's headers and authentication as gRPC metadata.

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD as BASE64, STANDARD_NO_PAD};
use std::path::Path;

use super::transport::{Target, decode_binary, frame};
use crate::auth::Authenticator;
use crate::error::{Error, Result};
use crate::http::{
    Header, HttpClient, Outgoing, OutgoingBody, Payload, Transport, find_header, set_header,
};
use crate::model::{ApiKeyLocation, Auth, KeyValue, OAuth1Location};
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

/// Renders the effective headers and adds the credentials that do not
/// depend on the call. Names are lowercased; values of names ending in
/// `-bin` must be Base64 and are sent unpadded; other values must be
/// printable ASCII. OAuth 2.0 tokens are fetched with `credentials`, and
/// left out without them. Tokens and signatures made from the call itself
/// are added by [`Signing::apply`].
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
        // Signed credentials come from `Signing`, once the call is known.
        Auth::AwsSigV4(_)
        | Auth::Jwt(_)
        | Auth::Asap(_)
        | Auth::Oauth1(_)
        | Auth::Hawk(_)
        | Auth::Edgegrid(_)
        | Auth::Inherit
        | Auth::None => {}
    }
    Ok(metadata)
}

/// What a request signature covers of a call's body.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Body<'a> {
    /// The call's one message, protobuf-encoded: unary and server-streaming
    /// calls, whose whole body is known up front.
    Message(&'a [u8]),
    /// Messages not known up front: client and bidirectional streams, and
    /// server reflection.
    Stream,
}

/// What it takes to sign a call with the authentication that is computed
/// from the request: AWS Signature, JWT, ASAP, OAuth 1.0, Hawk and
/// EdgeGrid.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Signing<'a> {
    /// The effective authentication.
    pub(crate) auth: &'a Auth,
    /// Variables for the authentication's settings.
    pub(crate) vars: &'a VarStack,
    /// The workspace root, where key files are looked up.
    pub(crate) root: &'a Path,
    /// The server.
    pub(crate) target: &'a Target,
}

impl Signing<'_> {
    /// `base` with the token or signature for a call to `path`, such as
    /// `/shop.Orders/GetOrder`, added.
    ///
    /// The call is signed as the HTTP/2 request it is: a `POST` to
    /// `https://host/shop.Orders/GetOrder` (`http` without TLS) with the
    /// metadata as headers and the length-prefixed message as the body.
    /// JWT and ASAP tokens do not depend on either. When the body is a
    /// [`Body::Stream`], the signature leaves it out: AWS Signature signs
    /// `UNSIGNED-PAYLOAD`, OAuth 1.0 sends no body hash, and Hawk no
    /// payload hash. EdgeGrid, which signs the body of a `POST`, cannot.
    /// Variables the settings use but lack are added to `missing`.
    pub(crate) fn apply(
        &self,
        base: &[Header],
        path: &str,
        body: Body<'_>,
        missing: &mut Vec<String>,
    ) -> Result<Vec<Header>> {
        let Some(auth) = self.prepared(body)? else {
            return Ok(base.to_vec());
        };
        let url = self.target.http_url(path);
        let mut headers = vec![Header::new("content-type", "application/grpc")];
        for header in base {
            set_header(&mut headers, &header.name, &header.value);
        }
        let before = headers.clone();
        let mut outgoing = Outgoing {
            method: "POST".into(),
            url: url.clone(),
            headers,
            body: match body {
                Body::Message(message) => OutgoingBody::Bytes {
                    data: Payload(frame(message).to_vec()),
                },
                Body::Stream => OutgoingBody::None,
            },
        };
        let mut render = |text: &str| {
            let out = self.vars.interpolate(text);
            for name in out.missing {
                if !missing.contains(&name) {
                    missing.push(name);
                }
            }
            out.text
        };
        crate::auth::sign(&auth, &mut render, &mut outgoing, self.root)?;

        // What signing added or changed is what the call sends.
        let mut metadata = base.to_vec();
        for header in &outgoing.headers {
            if find_header(&before, &header.name) != Some(header.value.as_str()) {
                add(
                    &mut metadata,
                    &header.name.to_ascii_lowercase(),
                    &header.value,
                )?;
            }
        }
        // A JWT may be set to go in the query string; as metadata, that is
        // an entry of the parameter's name.
        if outgoing.url != url
            && let Ok(signed) = url::Url::parse(&outgoing.url)
        {
            for (name, value) in signed.query_pairs() {
                add(&mut metadata, &name.to_ascii_lowercase(), &value)?;
            }
        }
        Ok(metadata)
    }

    /// The authentication as it signs a call with this `body`, or `None`
    /// if it is not one that signs.
    fn prepared(&self, body: Body<'_>) -> Result<Option<Auth>> {
        let stream = matches!(body, Body::Stream);
        Ok(Some(match self.auth {
            Auth::AwsSigV4(settings) => {
                if settings.location != ApiKeyLocation::Header {
                    return Err(Error::Invalid(
                        "AWS Signature can sign a gRPC call only in the header; a presigned query string has no place in one".into(),
                    ));
                }
                let mut settings = settings.clone();
                settings.unsigned_payload |= stream;
                Auth::AwsSigV4(settings)
            }
            Auth::Jwt(_) | Auth::Asap(_) => self.auth.clone(),
            Auth::Oauth1(settings) => {
                if settings.location != OAuth1Location::Header {
                    return Err(Error::Invalid(
                        "OAuth 1.0 can sign a gRPC call only in the header; a query string or a form body has no place in one".into(),
                    ));
                }
                let mut settings = settings.clone();
                settings.body_hash &= !stream;
                Auth::Oauth1(settings)
            }
            Auth::Hawk(settings) => {
                let mut settings = settings.clone();
                settings.payload_hash &= !stream;
                Auth::Hawk(settings)
            }
            Auth::Edgegrid(settings) => {
                if stream {
                    return Err(Error::Invalid(
                        "Akamai EdgeGrid signs the body of a POST, so it cannot sign a call that streams messages from the client or a schema read with reflection; use unary or server-streaming methods, and a .proto file".into(),
                    ));
                }
                let mut settings = settings.clone();
                // The call goes to its own address.
                settings.base_url.clear();
                Auth::Edgegrid(settings)
            }
            _ => return Ok(None),
        }))
    }
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
