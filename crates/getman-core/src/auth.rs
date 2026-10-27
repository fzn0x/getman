//! Authentication that needs a round trip: HTTP Digest, which answers the
//! server's challenge, and OAuth 2.0, which fetches a token first.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use md5::Md5;
use serde::Serialize;
use sha2::{Digest as _, Sha256};

use crate::error::{Error, Result};
use crate::http::{
    Header, HttpClient, Outgoing, OutgoingBody, Payload, Response, Transport, set_header,
};
use crate::model::{Auth, ClientAuth, OAuth2, OAuth2Grant};
use crate::vars::VarStack;

/// Tokens are renewed this long before they expire.
const EXPIRY_MARGIN: Duration = Duration::from_secs(30);
/// How long to keep a token whose lifetime the server did not state.
const UNSTATED_LIFETIME: Duration = Duration::from_secs(10 * 60);

/// Sends requests with any kind of authentication, remembering OAuth 2.0
/// tokens until they expire.
#[derive(Debug, Default)]
pub struct Authenticator {
    tokens: Mutex<HashMap<String, CachedToken>>,
}

#[derive(Debug, Clone)]
struct CachedToken {
    token: Token,
    renew_at: Instant,
}

/// An OAuth 2.0 access token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Token {
    /// The access token.
    pub access_token: String,
    /// Token type, usually `Bearer`.
    pub token_type: String,
    /// Lifetime in seconds, as the server stated it.
    pub expires_in: Option<u64>,
    /// Scopes the server granted, if it said.
    pub scope: Option<String>,
}

impl Authenticator {
    /// An authenticator without cached tokens.
    pub fn new() -> Self {
        Self::default()
    }

    /// Forgets every cached token.
    pub fn clear_tokens(&self) {
        self.tokens.lock().expect("token cache lock").clear();
    }

    /// Sends `outgoing`, completing Digest or OAuth 2.0 authentication.
    /// Other kinds were already applied when the request was prepared.
    /// Afterwards `outgoing` holds the credentials that were sent.
    pub async fn send(
        &self,
        http: &HttpClient,
        auth: &Auth,
        vars: &VarStack,
        outgoing: &mut Outgoing,
        transport: Transport,
    ) -> Result<Response> {
        match auth {
            Auth::Digest { username, password } => {
                let first = http.send(outgoing, transport).await?;
                if first.status != 401 {
                    return Ok(first);
                }
                let Some(challenge) = first
                    .headers
                    .iter()
                    .filter(|h| h.name.eq_ignore_ascii_case("www-authenticate"))
                    .find_map(|h| parse_challenge(&h.value))
                else {
                    return Ok(first);
                };
                let header = digest_header(
                    &challenge,
                    &vars.render(username),
                    &vars.render(password),
                    outgoing,
                )?;
                set_header(&mut outgoing.headers, "Authorization", header);
                http.send(outgoing, transport).await
            }
            Auth::Oauth2(settings) => {
                let token = self.token(http, settings, vars, transport, false).await?;
                let scheme = if token.token_type.eq_ignore_ascii_case("bearer")
                    || token.token_type.is_empty()
                {
                    "Bearer"
                } else {
                    token.token_type.as_str()
                };
                set_header(
                    &mut outgoing.headers,
                    "Authorization",
                    format!("{scheme} {}", token.access_token),
                );
                http.send(outgoing, transport).await
            }
            _ => http.send(outgoing, transport).await,
        }
    }

    /// The token for these settings: a cached one while it is fresh, else
    /// a new one from the token endpoint. `renew` skips the cache.
    pub async fn token(
        &self,
        http: &HttpClient,
        settings: &OAuth2,
        vars: &VarStack,
        transport: Transport,
        renew: bool,
    ) -> Result<Token> {
        let s = render_oauth2(settings, vars);
        let key = [
            s.token_url.as_str(),
            s.client_id.as_str(),
            s.client_secret.as_str(),
            s.scope.as_str(),
            s.audience.as_str(),
            s.username.as_str(),
            s.password.as_str(),
            match s.grant {
                OAuth2Grant::ClientCredentials => "client_credentials",
                OAuth2Grant::Password => "password",
            },
        ]
        .join("\u{0}");
        if !renew {
            let tokens = self.tokens.lock().expect("token cache lock");
            if let Some(cached) = tokens.get(&key)
                && Instant::now() < cached.renew_at
            {
                return Ok(cached.token.clone());
            }
        }
        let token = fetch_token(http, &s, transport).await?;
        let lifetime = token
            .expires_in
            .map_or(UNSTATED_LIFETIME, Duration::from_secs);
        self.tokens.lock().expect("token cache lock").insert(
            key,
            CachedToken {
                token: token.clone(),
                renew_at: Instant::now() + lifetime.saturating_sub(EXPIRY_MARGIN),
            },
        );
        Ok(token)
    }
}

fn render_oauth2(settings: &OAuth2, vars: &VarStack) -> OAuth2 {
    OAuth2 {
        grant: settings.grant,
        token_url: vars.render(settings.token_url.trim()),
        client_id: vars.render(&settings.client_id),
        client_secret: vars.render(&settings.client_secret),
        scope: vars.render(&settings.scope),
        audience: vars.render(&settings.audience),
        username: vars.render(&settings.username),
        password: vars.render(&settings.password),
        client_auth: settings.client_auth,
    }
}

async fn fetch_token(http: &HttpClient, s: &OAuth2, transport: Transport) -> Result<Token> {
    if s.token_url.is_empty() {
        return Err(Error::Invalid("OAuth 2.0 needs a token URL".into()));
    }
    let request = token_request(s);
    let response = http
        .send(&request, transport)
        .await
        .map_err(|e| Error::Http(format!("Could not get an OAuth 2.0 token: {e}")))?;
    read_token(&response)
}

/// The token request. Built outside the async function: the form encoder
/// is not `Send`, so it must not live across an await.
fn token_request(s: &OAuth2) -> Outgoing {
    let mut form = url::form_urlencoded::Serializer::new(String::new());
    match s.grant {
        OAuth2Grant::ClientCredentials => {
            form.append_pair("grant_type", "client_credentials");
        }
        OAuth2Grant::Password => {
            form.append_pair("grant_type", "password");
            form.append_pair("username", &s.username);
            form.append_pair("password", &s.password);
        }
    }
    if !s.scope.trim().is_empty() {
        form.append_pair("scope", s.scope.trim());
    }
    if !s.audience.trim().is_empty() {
        form.append_pair("audience", s.audience.trim());
    }
    let mut headers = vec![
        Header::new("Content-Type", "application/x-www-form-urlencoded"),
        Header::new("Accept", "application/json"),
    ];
    match s.client_auth {
        ClientAuth::Basic => {
            // RFC 6749 section 2.3.1: both parts are form-encoded first.
            let encode = |text: &str| {
                url::form_urlencoded::byte_serialize(text.as_bytes()).collect::<String>()
            };
            let credentials = format!("{}:{}", encode(&s.client_id), encode(&s.client_secret));
            headers.push(Header::new(
                "Authorization",
                format!("Basic {}", BASE64.encode(credentials)),
            ));
        }
        ClientAuth::Body => {
            form.append_pair("client_id", &s.client_id);
            if !s.client_secret.is_empty() {
                form.append_pair("client_secret", &s.client_secret);
            }
        }
    }
    Outgoing {
        method: "POST".into(),
        url: s.token_url.clone(),
        headers,
        body: OutgoingBody::Bytes {
            data: Payload(form.finish().into_bytes()),
        },
    }
}

fn read_token(response: &Response) -> Result<Token> {
    let json = response.json().unwrap_or_default();
    if !(200..300).contains(&response.status) {
        let reason = [json["error"].as_str(), json["error_description"].as_str()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(": ");
        return Err(Error::Http(format!(
            "The token endpoint answered {} {}{}",
            response.status,
            response.status_text,
            if reason.is_empty() {
                String::new()
            } else {
                format!(" ({reason})")
            }
        )));
    }
    let Some(access_token) = json["access_token"].as_str() else {
        return Err(Error::Http(
            "The token endpoint's answer has no access_token".into(),
        ));
    };
    Ok(Token {
        access_token: access_token.to_owned(),
        token_type: json["token_type"].as_str().unwrap_or("Bearer").to_owned(),
        expires_in: json["expires_in"]
            .as_u64()
            .or_else(|| json["expires_in"].as_str().and_then(|s| s.parse().ok())),
        scope: json["scope"].as_str().map(str::to_owned),
    })
}

/// The `Authorization` header that answers the Digest challenge in these
/// response headers, for requests that read their response as a stream
/// and so cannot go through [`Authenticator::send`]. `None` when the
/// headers hold no Digest challenge.
pub(crate) fn answer_digest(
    headers: &[Header],
    username: &str,
    password: &str,
    outgoing: &Outgoing,
) -> Option<Result<String>> {
    let challenge = headers
        .iter()
        .filter(|h| h.name.eq_ignore_ascii_case("www-authenticate"))
        .find_map(|h| parse_challenge(&h.value))?;
    Some(digest_header(&challenge, username, password, outgoing))
}

/// The parameters of a `Digest` challenge, with lowercase names.
fn parse_challenge(header: &str) -> Option<HashMap<String, String>> {
    let header = header.trim_start();
    let (scheme, rest) = header.split_once(char::is_whitespace)?;
    if !scheme.eq_ignore_ascii_case("digest") {
        return None;
    }
    let mut params = HashMap::new();
    let mut chars = rest.chars().peekable();
    loop {
        while chars.next_if(|c| c.is_whitespace() || *c == ',').is_some() {}
        let name: String = std::iter::from_fn(|| chars.next_if(|c| *c != '=' && *c != ','))
            .collect::<String>()
            .trim()
            .to_ascii_lowercase();
        if name.is_empty() {
            break;
        }
        if chars.next_if_eq(&'=').is_none() {
            continue;
        }
        let value = if chars.next_if_eq(&'"').is_some() {
            let mut value = String::new();
            while let Some(c) = chars.next() {
                match c {
                    '\\' => value.extend(chars.next()),
                    '"' => break,
                    c => value.push(c),
                }
            }
            value
        } else {
            std::iter::from_fn(|| chars.next_if(|c| *c != ','))
                .collect::<String>()
                .trim()
                .to_owned()
        };
        params.insert(name, value);
    }
    params.contains_key("nonce").then_some(params)
}

fn digest_header(
    challenge: &HashMap<String, String>,
    username: &str,
    password: &str,
    outgoing: &Outgoing,
) -> Result<String> {
    let get = |name: &str| challenge.get(name).map_or("", String::as_str);
    let algorithm = if get("algorithm").is_empty() {
        "MD5"
    } else {
        get("algorithm")
    };
    let (hash, session): (fn(&[u8]) -> String, bool) = match algorithm.to_ascii_uppercase().as_str()
    {
        "MD5" => (md5_hex, false),
        "MD5-SESS" => (md5_hex, true),
        "SHA-256" => (sha256_hex, false),
        "SHA-256-SESS" => (sha256_hex, true),
        other => {
            return Err(Error::Http(format!(
                "The server asks for Digest with {other}, which Getman does not support"
            )));
        }
    };
    let url = url::Url::parse(&outgoing.url).map_err(|e| Error::Invalid(e.to_string()))?;
    let uri = match url.query() {
        Some(query) => format!("{}?{query}", url.path()),
        None => url.path().to_owned(),
    };
    let (realm, nonce) = (get("realm"), get("nonce"));
    // The client nonce guards against chosen-plaintext attacks by the
    // server, which only works if the server cannot predict it.
    let mut random = [0_u8; 8];
    getrandom::fill(&mut random)
        .map_err(|e| Error::Http(format!("Could not make a Digest client nonce: {e}")))?;
    let cnonce = hex::encode(random);
    let nc = "00000001";
    let qops: Vec<&str> = get("qop").split(',').map(str::trim).collect();
    let body_bytes = match &outgoing.body {
        OutgoingBody::Bytes { data } => Some(data.0.as_slice()),
        OutgoingBody::None => Some(&[][..]),
        _ => None,
    };
    let qop = if qops.contains(&"auth") {
        Some("auth")
    } else if qops.contains(&"auth-int") && body_bytes.is_some() {
        Some("auth-int")
    } else {
        None
    };

    let mut ha1 = hash(format!("{username}:{realm}:{password}").as_bytes());
    if session {
        ha1 = hash(format!("{ha1}:{nonce}:{cnonce}").as_bytes());
    }
    let ha2 = if qop == Some("auth-int") {
        let body_hash = hash(body_bytes.unwrap_or_default());
        hash(format!("{}:{uri}:{body_hash}", outgoing.method).as_bytes())
    } else {
        hash(format!("{}:{uri}", outgoing.method).as_bytes())
    };
    let response = match qop {
        Some(qop) => hash(format!("{ha1}:{nonce}:{nc}:{cnonce}:{qop}:{ha2}").as_bytes()),
        None => hash(format!("{ha1}:{nonce}:{ha2}").as_bytes()),
    };

    let quote = |text: &str| text.replace('\\', "\\\\").replace('"', "\\\"");
    let mut header = format!(
        r#"Digest username="{}", realm="{}", nonce="{}", uri="{}", algorithm={algorithm}, response="{response}""#,
        quote(username),
        quote(realm),
        quote(nonce),
        quote(&uri),
    );
    if let Some(qop) = qop {
        header.push_str(&format!(r#", qop={qop}, nc={nc}, cnonce="{cnonce}""#));
    }
    if let Some(opaque) = challenge.get("opaque") {
        header.push_str(&format!(r#", opaque="{}""#, quote(opaque)));
    }
    Ok(header)
}

fn md5_hex(data: &[u8]) -> String {
    hex::encode(Md5::digest(data))
}

fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_server;
    use crate::vars::Scope;

    fn get(url: String) -> Outgoing {
        Outgoing {
            method: "GET".into(),
            url,
            headers: vec![],
            body: OutgoingBody::None,
        }
    }

    #[test]
    fn challenges_parse_with_quoted_commas() {
        let params = parse_challenge(
            r#"Digest realm="a, b", qop="auth,auth-int", nonce="n1", algorithm=MD5-sess, stale=FALSE"#,
        )
        .unwrap();
        assert_eq!(params["realm"], "a, b");
        assert_eq!(params["qop"], "auth,auth-int");
        assert_eq!(params["algorithm"], "MD5-sess");
        assert_eq!(params["stale"], "FALSE");
        assert!(parse_challenge(r#"Basic realm="x""#).is_none());
    }

    #[test]
    fn digest_matches_the_rfc_2617_example() {
        // RFC 2617 section 3.5, with a fixed client nonce.
        let challenge = parse_challenge(
            r#"Digest realm="testrealm@host.com", qop="auth", nonce="dcd98b7102dd2f0e8b11d0f600bfb0c093", opaque="5ccc069c403ebaf9f0171e9517f40e41""#,
        )
        .unwrap();
        let ha1 = md5_hex(b"Mufasa:testrealm@host.com:Circle Of Life");
        let ha2 = md5_hex(b"GET:/dir/index.html");
        let response = md5_hex(
            format!("{ha1}:dcd98b7102dd2f0e8b11d0f600bfb0c093:00000001:0a4f113b:auth:{ha2}")
                .as_bytes(),
        );
        assert_eq!(response, "6629fae49393a05397450978507c4ef1");
        let header = digest_header(
            &challenge,
            "Mufasa",
            "Circle Of Life",
            &get("http://www.nowhere.org/dir/index.html".into()),
        )
        .unwrap();
        assert!(header.starts_with(r#"Digest username="Mufasa", realm="testrealm@host.com""#));
        assert!(header.contains(r#"uri="/dir/index.html""#));
        assert!(header.contains(r#"opaque="5ccc069c403ebaf9f0171e9517f40e41""#));
    }

    #[tokio::test]
    async fn digest_answers_the_challenge() {
        let server = test_server::spawn().await;
        let http = HttpClient::new();
        let auth = Auth::Digest {
            username: "ada".into(),
            password: "{{pw}}".into(),
        };
        let mut vars = VarStack::new();
        vars.push(
            Scope::Environment,
            [("pw".to_owned(), "lovelace".to_owned())]
                .into_iter()
                .collect(),
        );
        let mut outgoing = get(server.url("/digest"));
        let response = Authenticator::new()
            .send(&http, &auth, &vars, &mut outgoing, Transport::default())
            .await
            .unwrap();
        assert_eq!(response.status, 200);
        assert!(outgoing.headers[0].value.starts_with("Digest "));

        let wrong = Auth::Digest {
            username: "ada".into(),
            password: "babbage".into(),
        };
        let response = Authenticator::new()
            .send(
                &http,
                &wrong,
                &vars,
                &mut get(server.url("/digest")),
                Transport::default(),
            )
            .await
            .unwrap();
        assert_eq!(response.status, 401);
    }

    #[tokio::test]
    async fn oauth2_tokens_are_fetched_and_cached() {
        let server = test_server::spawn().await;
        let http = HttpClient::new();
        let auth = Auth::Oauth2(OAuth2 {
            token_url: server.url("/token"),
            client_id: "app".into(),
            client_secret: "s3cret".into(),
            scope: "read".into(),
            ..OAuth2::default()
        });
        let authenticator = Authenticator::new();
        let mut outgoing = get(server.url("/echo"));
        let response = authenticator
            .send(
                &http,
                &auth,
                &VarStack::new(),
                &mut outgoing,
                Transport::default(),
            )
            .await
            .unwrap();
        let echo = response.json().unwrap();
        assert_eq!(
            echo["headers"]["authorization"],
            "Bearer token-for-client_credentials-read"
        );
        assert_eq!(authenticator.tokens.lock().unwrap().len(), 1);

        let Auth::Oauth2(mut in_body) = auth.clone() else {
            unreachable!()
        };
        in_body.client_auth = ClientAuth::Body;
        in_body.grant = OAuth2Grant::Password;
        in_body.username = "ada".into();
        let token = authenticator
            .token(
                &http,
                &in_body,
                &VarStack::new(),
                Transport::default(),
                false,
            )
            .await
            .unwrap();
        assert_eq!(token.access_token, "token-for-password-read");
        assert_eq!(token.expires_in, Some(3600));

        in_body.client_secret = "wrong".into();
        let error = authenticator
            .token(
                &http,
                &in_body,
                &VarStack::new(),
                Transport::default(),
                false,
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("invalid_client"), "{error}");
    }
}
