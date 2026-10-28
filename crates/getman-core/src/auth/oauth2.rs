//! OAuth 2.0: token requests for every grant, refresh tokens, the browser
//! sign-in of the authorization code and implicit grants, and the file
//! that keeps their tokens between runs.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::Path;

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD as BASE64, URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::callback::Callback;
use super::random_bytes;
use crate::error::{Error, Result};
use crate::http::{Header, HttpClient, Outgoing, OutgoingBody, Payload, Response, Transport};
use crate::model::{ApiKeyLocation, ClientAuth, KeyValue, OAuth2, OAuth2Grant, Pkce};
use crate::vars::VarStack;

/// Tokens are renewed this long before they expire, in milliseconds.
const EXPIRY_MARGIN_MS: i64 = 30_000;
/// How long to keep a token whose lifetime the server did not state, when
/// a new one can be fetched without the user, in milliseconds.
const UNSTATED_LIFETIME_MS: i64 = 10 * 60 * 1000;

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
    /// When the token expires, in milliseconds since the Unix epoch, if the
    /// server said.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    /// Whether a refresh token came with it, so Getman can renew it.
    pub refreshable: bool,
}

/// A browser sign-in that has started: the address to open, and the
/// callback Getman listens on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Authorization {
    /// Identifies the sign-in, for waiting on it or cancelling it.
    pub id: String,
    /// The authorization URL to open in a browser.
    pub url: String,
    /// The redirect URI the provider sends the browser back to.
    pub redirect_uri: String,
    /// Things the user should know, such as a callback URL that could not
    /// be used.
    pub notes: Vec<String>,
}

/// A token as cached in memory and in the token file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Record {
    pub(super) access_token: String,
    #[serde(default)]
    pub(super) token_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) expires_in: Option<u64>,
    /// When it expires, in milliseconds since the Unix epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) expires_at: Option<i64>,
    /// When to stop using it, in milliseconds since the Unix epoch; never
    /// when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) renew_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) refresh_token: Option<String>,
}

impl Record {
    /// A record for a token received at `now`. Tokens from a browser
    /// sign-in without a stated lifetime are kept until cleared, since
    /// getting another needs the user.
    fn new(
        access_token: String,
        token_type: String,
        expires_in: Option<u64>,
        scope: Option<String>,
        refresh_token: Option<String>,
        now: i64,
        browser: bool,
    ) -> Self {
        let expires_at = expires_in.map(|seconds| {
            now.saturating_add(
                i64::try_from(seconds)
                    .unwrap_or(i64::MAX)
                    .saturating_mul(1000),
            )
        });
        let renew_at = match expires_at {
            Some(at) => Some(at.saturating_sub(EXPIRY_MARGIN_MS)),
            None if browser => None,
            None => Some(now.saturating_add(UNSTATED_LIFETIME_MS)),
        };
        Self {
            access_token,
            token_type,
            scope,
            expires_in,
            expires_at,
            renew_at,
            refresh_token: refresh_token.filter(|t| !t.is_empty()),
        }
    }

    /// Whether the token can still be used at `now`.
    pub(super) fn fresh(&self, now: i64) -> bool {
        self.renew_at.is_none_or(|at| now < at)
    }

    /// The token as the API shows it.
    pub(super) fn token(&self) -> Token {
        Token {
            access_token: self.access_token.clone(),
            token_type: self.token_type.clone(),
            expires_in: self.expires_in,
            scope: self.scope.clone(),
            expires_at: self.expires_at,
            refreshable: self.refresh_token.is_some(),
        }
    }
}

/// The settings with variables replaced.
pub(super) fn render(settings: &OAuth2, vars: &VarStack) -> OAuth2 {
    let params = |list: &[KeyValue]| -> Vec<KeyValue> {
        list.iter()
            .filter(|p| p.enabled && !p.name.trim().is_empty())
            .map(|p| KeyValue::new(vars.render(p.name.trim()), vars.render(&p.value)))
            .collect()
    };
    OAuth2 {
        grant: settings.grant,
        auth_url: vars.render(settings.auth_url.trim()),
        token_url: vars.render(settings.token_url.trim()),
        client_id: vars.render(&settings.client_id),
        client_secret: vars.render(&settings.client_secret),
        callback_url: vars.render(settings.callback_url.trim()),
        pkce: settings.pkce,
        scope: vars.render(settings.scope.trim()),
        audience: vars.render(settings.audience.trim()),
        resource: vars.render(settings.resource.trim()),
        username: vars.render(&settings.username),
        password: vars.render(&settings.password),
        client_auth: settings.client_auth,
        auth_params: params(&settings.auth_params),
        token_params: params(&settings.token_params),
        prefix: vars.render(settings.prefix.trim()),
        token_in: settings.token_in,
    }
}

/// Identifies the token for a set of rendered settings: a hash, so the
/// secrets that go into it are not written to the token file.
pub(super) fn cache_key(s: &OAuth2) -> String {
    let params = |list: &[KeyValue]| {
        list.iter()
            .map(|p| format!("{}={}", p.name, p.value))
            .collect::<Vec<_>>()
            .join("&")
    };
    let parts: Vec<String> = if s.grant.uses_browser() {
        // The same sign-in, whatever the secret or the extra parameters.
        vec![
            grant_name(s.grant).into(),
            s.auth_url.clone(),
            s.token_url.clone(),
            s.client_id.clone(),
            s.scope.clone(),
            s.audience.clone(),
            s.resource.clone(),
        ]
    } else {
        vec![
            grant_name(s.grant).into(),
            s.token_url.clone(),
            s.client_id.clone(),
            s.client_secret.clone(),
            s.scope.clone(),
            s.audience.clone(),
            s.resource.clone(),
            s.username.clone(),
            s.password.clone(),
            params(&s.token_params),
        ]
    };
    hex::encode(Sha256::digest(parts.join("\u{0}").as_bytes()))
}

/// The grant's name, as `grant_type` and the settings write it.
pub(super) fn grant_name(grant: OAuth2Grant) -> &'static str {
    match grant {
        OAuth2Grant::ClientCredentials => "client_credentials",
        OAuth2Grant::Password => "password",
        OAuth2Grant::AuthorizationCode => "authorization_code",
        OAuth2Grant::Implicit => "implicit",
    }
}

/// Why a request cannot get a token without the user.
pub(super) fn needs_browser(grant: OAuth2Grant) -> Error {
    let name = match grant {
        OAuth2Grant::Implicit => "implicit",
        _ => "authorization code",
    };
    Error::Invalid(format!(
        "No OAuth 2.0 token yet: the {name} grant signs in with a browser. Open this request in Getman's browser UI and choose Get new token under Auth; getman run and getman send then use the saved token."
    ))
}

/// Adds the token to the request, as a header or a query parameter.
pub(super) fn apply_token(s: &OAuth2, token: &Token, outgoing: &mut Outgoing) -> Result<()> {
    match s.token_in {
        ApiKeyLocation::Header => {
            let scheme = if !s.prefix.is_empty() {
                s.prefix.as_str()
            } else if token.token_type.is_empty() || token.token_type.eq_ignore_ascii_case("bearer")
            {
                "Bearer"
            } else {
                token.token_type.as_str()
            };
            crate::http::set_header(
                &mut outgoing.headers,
                "Authorization",
                format!("{scheme} {}", token.access_token),
            );
        }
        ApiKeyLocation::Query => {
            let mut url = super::parse_url(outgoing)?;
            url.query_pairs_mut()
                .append_pair("access_token", &token.access_token);
            outgoing.url = url.to_string();
        }
    }
    Ok(())
}

/// Gets a token with the client credentials or password grant.
pub(super) async fn fetch(http: &HttpClient, s: &OAuth2, transport: Transport) -> Result<Record> {
    let mut fields = vec![("grant_type".to_owned(), grant_name(s.grant).to_owned())];
    if s.grant == OAuth2Grant::Password {
        fields.push(("username".into(), s.username.clone()));
        fields.push(("password".into(), s.password.clone()));
    }
    request(http, s, fields, true, transport, false).await
}

/// Renews a token with its refresh token. A new refresh token replaces
/// the old one; without one the old one is kept.
pub(super) async fn refresh(
    http: &HttpClient,
    s: &OAuth2,
    refresh_token: &str,
    transport: Transport,
) -> Result<Record> {
    let fields = vec![
        ("grant_type".to_owned(), "refresh_token".to_owned()),
        ("refresh_token".to_owned(), refresh_token.to_owned()),
    ];
    let mut record = request(http, s, fields, false, transport, s.grant.uses_browser()).await?;
    if record.refresh_token.is_none() {
        record.refresh_token = Some(refresh_token.to_owned());
    }
    Ok(record)
}

/// Sends a token request with these grant fields, and reads the answer.
/// `with_scope` adds the scope, audience and resource.
async fn request(
    http: &HttpClient,
    s: &OAuth2,
    fields: Vec<(String, String)>,
    with_scope: bool,
    transport: Transport,
    browser: bool,
) -> Result<Record> {
    if s.token_url.is_empty() {
        return Err(Error::Invalid("OAuth 2.0 needs a token URL".into()));
    }
    let outgoing = token_request(s, &fields, with_scope);
    let response = http
        .send(&outgoing, transport)
        .await
        .map_err(|e| Error::Http(format!("Could not get an OAuth 2.0 token: {e}")))?;
    read_token(&response, browser)
}

/// The token request. Built outside the async functions: the form encoder
/// is not `Send`, so it must not live across an await.
fn token_request(s: &OAuth2, fields: &[(String, String)], with_scope: bool) -> Outgoing {
    let mut form = url::form_urlencoded::Serializer::new(String::new());
    for (name, value) in fields {
        form.append_pair(name, value);
    }
    if with_scope {
        for (name, value) in [
            ("scope", &s.scope),
            ("audience", &s.audience),
            ("resource", &s.resource),
        ] {
            if !value.is_empty() {
                form.append_pair(name, value);
            }
        }
    }
    for param in &s.token_params {
        form.append_pair(&param.name, &param.value);
    }
    let mut headers = vec![
        Header::new("Content-Type", "application/x-www-form-urlencoded"),
        Header::new("Accept", "application/json"),
    ];
    // A public client, with no secret, names itself in the body.
    if s.client_auth == ClientAuth::Basic && !s.client_secret.is_empty() {
        // RFC 6749 section 2.3.1: both parts are form-encoded first.
        let encode =
            |text: &str| url::form_urlencoded::byte_serialize(text.as_bytes()).collect::<String>();
        let credentials = format!("{}:{}", encode(&s.client_id), encode(&s.client_secret));
        headers.push(Header::new(
            "Authorization",
            format!("Basic {}", BASE64.encode(credentials)),
        ));
    } else {
        form.append_pair("client_id", &s.client_id);
        if !s.client_secret.is_empty() {
            form.append_pair("client_secret", &s.client_secret);
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

fn read_token(response: &Response, browser: bool) -> Result<Record> {
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
    Ok(Record::new(
        access_token.to_owned(),
        json["token_type"].as_str().unwrap_or("Bearer").to_owned(),
        json["expires_in"]
            .as_u64()
            .or_else(|| json["expires_in"].as_str().and_then(|s| s.parse().ok())),
        json["scope"].as_str().map(str::to_owned),
        json["refresh_token"].as_str().map(str::to_owned),
        crate::time::now_millis(),
        browser,
    ))
}

/// A browser sign-in waiting for its callback.
#[derive(Debug)]
pub(super) struct Pending {
    callback: Callback,
    settings: OAuth2,
    state: String,
    verifier: Option<String>,
}

impl Pending {
    /// The key its token is cached under.
    pub(super) fn key(&self) -> String {
        cache_key(&self.settings)
    }
}

/// Starts a browser sign-in: listens for the callback and builds the
/// authorization URL.
pub(super) async fn begin(s: OAuth2) -> Result<(Pending, Authorization)> {
    if !s.grant.uses_browser() {
        return Err(Error::Invalid(
            "Only the authorization code and implicit grants sign in with a browser; this grant gets its token when a request is sent".into(),
        ));
    }
    let grant = if s.grant == OAuth2Grant::Implicit {
        "implicit"
    } else {
        "authorization code"
    };
    if s.auth_url.is_empty() {
        return Err(Error::Invalid(format!(
            "OAuth 2.0 with the {grant} grant needs an authorization URL"
        )));
    }
    if s.grant == OAuth2Grant::AuthorizationCode && s.token_url.is_empty() {
        return Err(Error::Invalid(
            "OAuth 2.0 with the authorization code grant needs a token URL".into(),
        ));
    }
    if s.client_id.trim().is_empty() {
        return Err(Error::Invalid("OAuth 2.0 needs a client ID".into()));
    }
    let mut url = url::Url::parse(&s.auth_url).map_err(|e| {
        Error::Invalid(format!(
            "{:?} is not a valid authorization URL: {e}",
            s.auth_url
        ))
    })?;
    // The URL is opened with the system's handler, which must be a browser.
    if !matches!(url.scheme(), "http" | "https") {
        return Err(Error::Invalid(format!(
            "The authorization URL must start with https:// or http://, not {}:",
            url.scheme()
        )));
    }
    let callback = Callback::bind(&s.callback_url).await?;
    let state = URL_SAFE_NO_PAD.encode(random_bytes::<16>()?);
    let verifier = if s.grant == OAuth2Grant::AuthorizationCode && !s.pkce.is_off() {
        Some(URL_SAFE_NO_PAD.encode(random_bytes::<32>()?))
    } else {
        None
    };
    {
        let mut query = url.query_pairs_mut();
        query.append_pair(
            "response_type",
            if s.grant == OAuth2Grant::Implicit {
                "token"
            } else {
                "code"
            },
        );
        query.append_pair("client_id", s.client_id.trim());
        query.append_pair("redirect_uri", &callback.redirect_uri);
        if !s.scope.is_empty() {
            query.append_pair("scope", &s.scope);
        }
        query.append_pair("state", &state);
        if let Some(verifier) = &verifier {
            let (challenge, method) = match s.pkce {
                Pkce::Plain => (verifier.clone(), "plain"),
                Pkce::S256 | Pkce::Off => (
                    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())),
                    "S256",
                ),
            };
            query.append_pair("code_challenge", &challenge);
            query.append_pair("code_challenge_method", method);
        }
        for (name, value) in [("audience", &s.audience), ("resource", &s.resource)] {
            if !value.is_empty() {
                query.append_pair(name, value);
            }
        }
        for param in &s.auth_params {
            query.append_pair(&param.name, &param.value);
        }
    }
    let authorization = Authorization {
        id: crate::dynamic::uuid_v4(),
        url: url.to_string(),
        redirect_uri: callback.redirect_uri.clone(),
        notes: callback.note.iter().cloned().collect(),
    };
    Ok((
        Pending {
            callback,
            settings: s,
            state,
            verifier,
        },
        authorization,
    ))
}

/// Waits for the provider to send the browser back, then exchanges the
/// code for a token, or takes the token the implicit grant sent.
pub(super) async fn finish(
    http: &HttpClient,
    pending: Pending,
    transport: Transport,
) -> Result<Record> {
    let implicit = pending.settings.grant == OAuth2Grant::Implicit;
    let params = pending.callback.receive(&pending.state, implicit).await?;
    let get = |name: &str| {
        params
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
            .filter(|v| !v.is_empty())
    };
    if let Some(error) = get("error") {
        return Err(Error::Http(format!(
            "The provider did not sign you in: {error}{}",
            get("error_description")
                .map(|d| format!(" ({d})"))
                .unwrap_or_default()
        )));
    }
    if implicit {
        let Some(access_token) = get("access_token") else {
            return Err(Error::Http(
                "The provider sent the browser back without an access token".into(),
            ));
        };
        return Ok(Record::new(
            access_token.to_owned(),
            get("token_type").unwrap_or("Bearer").to_owned(),
            get("expires_in").and_then(|s| s.parse().ok()),
            get("scope").map(str::to_owned),
            None,
            crate::time::now_millis(),
            true,
        ));
    }
    let Some(code) = get("code") else {
        return Err(Error::Http(
            "The provider sent the browser back without an authorization code".into(),
        ));
    };
    let mut fields = vec![
        ("grant_type".to_owned(), "authorization_code".to_owned()),
        ("code".to_owned(), code.to_owned()),
        (
            "redirect_uri".to_owned(),
            pending.callback.redirect_uri.clone(),
        ),
    ];
    if let Some(verifier) = &pending.verifier {
        fields.push(("code_verifier".into(), verifier.clone()));
    }
    let s = &pending.settings;
    request(http, s, fields, false, transport, true).await
}

/// Reads the token file; a missing or unreadable file holds no tokens.
pub(super) fn load(path: &Path) -> BTreeMap<String, Record> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Writes the token file, readable by its owner only where the system
/// allows. The state directory gets its own `.gitignore`, in case the
/// repository's does not cover it.
pub(super) fn save(path: &Path, tokens: &BTreeMap<String, Record>) -> Result<()> {
    if tokens.is_empty() {
        if path.exists() {
            std::fs::remove_file(path).map_err(|e| Error::io(path, e))?;
        }
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        crate::workspace::create_state_dir(dir)?;
    }
    let text = serde_json::to_string_pretty(tokens)
        .map_err(|e| Error::Invalid(format!("Could not save the OAuth 2.0 tokens: {e}")))?;
    let tmp = path.with_extension("json.tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options.open(&tmp).map_err(|e| Error::io(&tmp, e))?;
    file.write_all(text.as_bytes())
        .map_err(|e| Error::io(&tmp, e))?;
    drop(file);
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        Error::io(path, e)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::find_header;

    fn body_of(outgoing: &Outgoing) -> String {
        let OutgoingBody::Bytes { data } = &outgoing.body else {
            panic!("expected a form body")
        };
        data.text().into_owned()
    }

    #[test]
    fn public_clients_name_themselves_in_the_body() {
        let settings = OAuth2 {
            token_url: "https://id.example.com/token".into(),
            client_id: "public".into(),
            scope: "read".into(),
            audience: "api".into(),
            resource: "https://api.example.com/".into(),
            token_params: vec![KeyValue::new("tenant", "t1")],
            ..OAuth2::default()
        };
        let outgoing = token_request(
            &settings,
            &[("grant_type".into(), "client_credentials".into())],
            true,
        );
        assert!(find_header(&outgoing.headers, "authorization").is_none());
        assert_eq!(
            body_of(&outgoing),
            "grant_type=client_credentials&scope=read&audience=api&resource=https%3A%2F%2Fapi.example.com%2F&tenant=t1&client_id=public"
        );

        let confidential = OAuth2 {
            client_secret: "s3cret".into(),
            ..settings
        };
        let outgoing = token_request(&confidential, &[], false);
        assert!(find_header(&outgoing.headers, "authorization").is_some());
        assert_eq!(body_of(&outgoing), "tenant=t1");
    }

    #[test]
    fn tokens_use_the_prefix_or_their_type() {
        let token = Record::new("abc".into(), "MAC".into(), None, None, None, 0, false).token();
        let mut outgoing = Outgoing {
            method: "GET".into(),
            url: "https://api.example.com/".into(),
            headers: vec![],
            body: OutgoingBody::None,
        };
        apply_token(&OAuth2::default(), &token, &mut outgoing).unwrap();
        assert_eq!(
            find_header(&outgoing.headers, "authorization"),
            Some("MAC abc")
        );
        let settings = OAuth2 {
            prefix: "Token".into(),
            ..OAuth2::default()
        };
        apply_token(&settings, &token, &mut outgoing).unwrap();
        assert_eq!(
            find_header(&outgoing.headers, "authorization"),
            Some("Token abc")
        );
    }

    #[test]
    fn records_know_when_to_renew() {
        let unstated = Record::new("a".into(), String::new(), None, None, None, 0, false);
        assert!(unstated.fresh(UNSTATED_LIFETIME_MS - 1));
        assert!(!unstated.fresh(UNSTATED_LIFETIME_MS));
        let browser = Record::new("a".into(), String::new(), None, None, None, 0, true);
        assert!(browser.fresh(i64::MAX));
        let stated = Record::new("a".into(), String::new(), Some(60), None, None, 0, true);
        assert_eq!(stated.expires_at, Some(60_000));
        assert!(!stated.fresh(60_000 - EXPIRY_MARGIN_MS));
        let huge = Record::new(
            "a".into(),
            String::new(),
            Some(u64::MAX),
            None,
            None,
            1,
            true,
        );
        assert_eq!(huge.expires_at, Some(i64::MAX));
    }

    #[tokio::test]
    async fn only_web_addresses_are_opened() {
        let settings = OAuth2 {
            grant: OAuth2Grant::Implicit,
            auth_url: "file:///etc/passwd".into(),
            client_id: "app".into(),
            ..OAuth2::default()
        };
        let error = begin(settings).await.unwrap_err();
        assert!(error.to_string().contains("https://"), "{error}");
    }

    #[test]
    fn keys_do_not_depend_on_secrets_for_browser_grants() {
        let settings = OAuth2 {
            grant: OAuth2Grant::AuthorizationCode,
            client_id: "app".into(),
            client_secret: "one".into(),
            ..OAuth2::default()
        };
        let other_secret = OAuth2 {
            client_secret: "two".into(),
            ..settings.clone()
        };
        assert_eq!(cache_key(&settings), cache_key(&other_secret));
        let other_grant = OAuth2 {
            grant: OAuth2Grant::ClientCredentials,
            ..settings.clone()
        };
        assert_ne!(cache_key(&settings), cache_key(&other_grant));
        assert_eq!(cache_key(&settings).len(), 64);
    }
}
