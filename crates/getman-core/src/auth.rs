//! Authentication that needs a round trip: HTTP Digest, which answers the
//! server's challenge, and OAuth 2.0, which fetches a token first or signs
//! in with a browser. Also the signatures computed from the request itself,
//! such as AWS Signature Version 4, which are added as the last step of
//! preparing it.

mod callback;
mod edgegrid;
mod hawk;
mod jwt;
mod keys;
mod oauth1;
mod oauth2;
mod sigv4;

pub use oauth2::{Authorization, Token};

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use hmac::digest::KeyInit;
use hmac::{Hmac, Mac};
use md5::Md5;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC};
use sha2::{Digest as _, Sha256};
use tokio::sync::Notify;

use crate::error::{Error, Result};
use crate::http::{HttpClient, Outgoing, OutgoingBody, Response, Transport, set_header};
use crate::model::{Auth, OAuth2};
use crate::vars::VarStack;

/// Characters escaped by RFC 3986 percent-encoding: all but the unreserved
/// `A-Z a-z 0-9 - . _ ~`.
const RFC3986: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// Signs a prepared request with the kinds of authentication computed from
/// the request itself. `render` replaces variables in the settings, and
/// key files are looked up under `root`. Other kinds are left alone.
pub(crate) fn sign(
    auth: &Auth,
    render: &mut dyn FnMut(&str) -> String,
    outgoing: &mut Outgoing,
    root: &Path,
) -> Result<()> {
    let now = SystemTime::now();
    match auth {
        Auth::AwsSigV4(settings) => sigv4::sign(&sigv4::render(settings, render), outgoing, now),
        Auth::Jwt(settings) => jwt::apply(&jwt::render(settings, render), outgoing, root, now),
        Auth::Asap(settings) => {
            jwt::apply_asap(&jwt::render_asap(settings, render), outgoing, root, now)
        }
        Auth::Oauth1(settings) => {
            oauth1::apply(&oauth1::render(settings, render), outgoing, root, now)
        }
        Auth::Hawk(settings) => hawk::apply(&hawk::render(settings, render), outgoing, now),
        Auth::Edgegrid(settings) => {
            edgegrid::apply(&edgegrid::render(settings, render), outgoing, now)
        }
        _ => Ok(()),
    }
}

/// The body's bytes, for hashing: `None` for a multipart body, whose bytes
/// are only known once it is sent.
fn body_bytes(body: &OutgoingBody) -> Result<Option<Cow<'_, [u8]>>> {
    Ok(match body {
        OutgoingBody::None => Some(Cow::Borrowed(&[])),
        OutgoingBody::Bytes { data } => Some(Cow::Borrowed(&data.0)),
        OutgoingBody::File { path } => Some(Cow::Owned(
            std::fs::read(path).map_err(|e| Error::io(path, e))?,
        )),
        OutgoingBody::Multipart { .. } => None,
    })
}

/// An HMAC of `data` under `key`.
fn hmac<M: Mac + KeyInit>(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = <M as KeyInit>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    hmac::<Hmac<Sha256>>(key, data)
}

/// `N` bytes from the operating system's random generator.
fn random_bytes<const N: usize>() -> Result<[u8; N]> {
    let mut bytes = [0_u8; N];
    getrandom::fill(&mut bytes)
        .map_err(|e| Error::Http(format!("Could not get random bytes from the system: {e}")))?;
    Ok(bytes)
}

/// Seconds since the Unix epoch.
fn unix_seconds(time: SystemTime) -> u64 {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// A time in UTC as `YYYYMMDD` and `HHMMSS`.
fn utc_compact(time: SystemTime) -> (String, String) {
    // 2026-10-14T05:07:09.120Z
    let iso = crate::time::iso8601(time);
    let date = iso[..10].replace('-', "");
    let clock = iso[11..19].replace(':', "");
    (date, clock)
}

/// Escapes a string with RFC 3986 percent-encoding.
fn rfc3986(text: &str) -> String {
    percent_encoding::utf8_percent_encode(text, RFC3986).to_string()
}

/// A URL from an outgoing request, for signing.
fn parse_url(outgoing: &Outgoing) -> Result<url::Url> {
    url::Url::parse(&outgoing.url)
        .map_err(|e| Error::Invalid(format!("{:?} is not a valid URL: {e}", outgoing.url)))
}

/// The host and port the request goes to, as a `Host` header names it:
/// the port only when it is not the scheme's default.
fn host_header(outgoing: &Outgoing, url: &url::Url) -> String {
    if let Some(host) = crate::http::find_header(&outgoing.headers, "host") {
        return host.trim().to_owned();
    }
    let host = url.host_str().unwrap_or_default();
    match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_owned(),
    }
}

/// File in the workspace's state directory that keeps OAuth 2.0 tokens
/// from browser sign-ins.
pub const TOKEN_FILE: &str = "oauth2-tokens.json";

/// Sends requests with any kind of authentication, remembering OAuth 2.0
/// tokens until they expire. Tokens from a browser sign-in are also kept
/// in a file, so later runs and `getman run` can use them.
#[derive(Debug, Default)]
pub struct Authenticator {
    tokens: Mutex<HashMap<String, oauth2::Record>>,
    store: Option<PathBuf>,
    flows: Mutex<HashMap<String, Flow>>,
}

/// A browser sign-in: a way to cancel it, and its listener until someone
/// waits on it.
#[derive(Debug)]
struct Flow {
    cancel: Arc<Notify>,
    pending: Option<oauth2::Pending>,
}

impl Authenticator {
    /// An authenticator without cached tokens, which keeps tokens in
    /// memory only.
    pub fn new() -> Self {
        Self::default()
    }

    /// An authenticator that keeps tokens from browser sign-ins in `file`,
    /// usually [`TOKEN_FILE`] in the workspace's state directory.
    pub fn with_store(file: impl Into<PathBuf>) -> Self {
        Self {
            store: Some(file.into()),
            ..Self::default()
        }
    }

    /// Forgets every token cached in memory.
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
                // A server may offer several algorithms, preferred first.
                let challenges: Vec<_> = first
                    .headers
                    .iter()
                    .filter(|h| h.name.eq_ignore_ascii_case("www-authenticate"))
                    .filter_map(|h| parse_challenge(&h.value))
                    .collect();
                let Some(challenge) = challenges
                    .iter()
                    .find(|c| digest_hash(c).is_some())
                    .or(challenges.first())
                    .cloned()
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
                oauth2::apply_token(&oauth2::render(settings, vars), &token, outgoing)?;
                http.send(outgoing, transport).await
            }
            _ => http.send(outgoing, transport).await,
        }
    }

    /// The token for these settings: a cached one while it is fresh, a
    /// refreshed one when it came with a refresh token, else a new one
    /// from the token endpoint. `renew` skips the cache. The grants that
    /// sign in with a browser never start one here; they need a token from
    /// [`Authenticator::authorize`] first.
    pub async fn token(
        &self,
        http: &HttpClient,
        settings: &OAuth2,
        vars: &VarStack,
        transport: Transport,
        renew: bool,
    ) -> Result<Token> {
        let s = oauth2::render(settings, vars);
        let browser = s.grant.uses_browser();
        let key = oauth2::cache_key(&s);
        if !renew && let Some(record) = self.lookup(&key, browser) {
            if record.fresh(crate::time::now_millis()) {
                return Ok(record.token());
            }
            match &record.refresh_token {
                Some(refresh_token) => {
                    match oauth2::refresh(http, &s, refresh_token, transport).await {
                        Ok(renewed) => {
                            self.remember(&key, &renewed, browser)?;
                            return Ok(renewed.token());
                        }
                        Err(e) if browser => {
                            return Err(Error::Http(format!(
                                "The OAuth 2.0 token expired and could not be refreshed. {e}. Get a new token under Auth in Getman's browser UI."
                            )));
                        }
                        // Fetch a new one below.
                        Err(_) => {}
                    }
                }
                None if browser => {
                    return Err(Error::Invalid(
                        "The OAuth 2.0 token expired and came without a refresh token. Get a new token under Auth in Getman's browser UI.".into(),
                    ));
                }
                None => {}
            }
        }
        if browser {
            return Err(oauth2::needs_browser(s.grant));
        }
        let record = oauth2::fetch(http, &s, transport).await?;
        self.remember(&key, &record, false)?;
        Ok(record.token())
    }

    /// The token cached for these settings, fresh or not.
    pub fn cached_token(&self, settings: &OAuth2, vars: &VarStack) -> Option<Token> {
        let s = oauth2::render(settings, vars);
        self.lookup(&oauth2::cache_key(&s), s.grant.uses_browser())
            .map(|record| record.token())
    }

    /// Forgets the token for these settings, in memory and in the token
    /// file. Returns whether there was one.
    pub fn clear_token(&self, settings: &OAuth2, vars: &VarStack) -> Result<bool> {
        let key = oauth2::cache_key(&oauth2::render(settings, vars));
        let mut found = self
            .tokens
            .lock()
            .expect("token cache lock")
            .remove(&key)
            .is_some();
        if let Some(path) = &self.store {
            let mut stored = oauth2::load(path);
            if stored.remove(&key).is_some() {
                found = true;
                oauth2::save(path, &stored)?;
            }
        }
        Ok(found)
    }

    /// Starts a browser sign-in for the authorization code or implicit
    /// grant: listens for the callback and returns the URL to open. Any
    /// sign-in already under way is cancelled. Finish it with
    /// [`Authenticator::wait_for_token`].
    pub async fn authorize(&self, settings: &OAuth2, vars: &VarStack) -> Result<Authorization> {
        let s = oauth2::render(settings, vars);
        {
            // One at a time: this frees the callback port of the last one.
            let mut flows = self.flows.lock().expect("sign-in lock");
            for flow in flows.values() {
                flow.cancel.notify_one();
            }
            flows.clear();
        }
        let (pending, authorization) = oauth2::begin(s).await?;
        self.flows.lock().expect("sign-in lock").insert(
            authorization.id.clone(),
            Flow {
                cancel: Arc::new(Notify::new()),
                pending: Some(pending),
            },
        );
        Ok(authorization)
    }

    /// Waits for the browser sign-in `id` to come back, at most `timeout`,
    /// then gets and keeps its token.
    pub async fn wait_for_token(
        &self,
        http: &HttpClient,
        id: &str,
        transport: Transport,
        timeout: Duration,
    ) -> Result<Token> {
        let gone = || Error::Invalid("This sign-in was cancelled or has already finished.".into());
        let (pending, cancel) = {
            let mut flows = self.flows.lock().expect("sign-in lock");
            let flow = flows.get_mut(id).ok_or_else(gone)?;
            (
                flow.pending.take().ok_or_else(gone)?,
                Arc::clone(&flow.cancel),
            )
        };
        let key = pending.key();
        let result = tokio::select! {
            result = oauth2::finish(http, pending, transport) => result,
            () = cancel.notified() => Err(Error::Invalid("The sign-in was cancelled.".into())),
            () = tokio::time::sleep(timeout) => Err(Error::Http(
                "Getman stopped waiting for the browser sign-in. Start it again.".into(),
            )),
        };
        self.flows.lock().expect("sign-in lock").remove(id);
        let record = result?;
        self.remember(&key, &record, true)?;
        Ok(record.token())
    }

    /// Cancels the browser sign-in `id`, if it is still under way.
    pub fn cancel_authorization(&self, id: &str) {
        let mut flows = self.flows.lock().expect("sign-in lock");
        if let Some(flow) = flows.get_mut(id) {
            flow.cancel.notify_one();
            if flow.pending.take().is_some() {
                flows.remove(id);
            }
        }
    }

    /// A cached token: from memory, or for a browser grant, from the file.
    fn lookup(&self, key: &str, browser: bool) -> Option<oauth2::Record> {
        if let Some(record) = self.tokens.lock().expect("token cache lock").get(key) {
            return Some(record.clone());
        }
        let path = self.store.as_ref().filter(|_| browser)?;
        let record = oauth2::load(path).remove(key)?;
        self.tokens
            .lock()
            .expect("token cache lock")
            .insert(key.to_owned(), record.clone());
        Some(record)
    }

    /// Caches a token, and writes tokens of browser grants to the file.
    fn remember(&self, key: &str, record: &oauth2::Record, browser: bool) -> Result<()> {
        self.tokens
            .lock()
            .expect("token cache lock")
            .insert(key.to_owned(), record.clone());
        if browser && let Some(path) = &self.store {
            let mut stored = oauth2::load(path);
            stored.insert(key.to_owned(), record.clone());
            oauth2::save(path, &stored)?;
        }
        Ok(())
    }
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

/// A hash function that returns lowercase hex.
type HashFn = fn(&[u8]) -> String;

/// The challenge's algorithm, as written.
fn digest_algorithm(challenge: &HashMap<String, String>) -> &str {
    match challenge.get("algorithm").map(String::as_str) {
        None | Some("") => "MD5",
        Some(algorithm) => algorithm,
    }
}

/// The hash function a challenge asks for, and whether it is a `-sess`
/// variant; `None` when Getman does not support it.
fn digest_hash(challenge: &HashMap<String, String>) -> Option<(HashFn, bool)> {
    let hash: (HashFn, bool) = match digest_algorithm(challenge).to_ascii_uppercase().as_str() {
        "MD5" => (md5_hex, false),
        "MD5-SESS" => (md5_hex, true),
        "SHA-256" => (sha256_hex, false),
        "SHA-256-SESS" => (sha256_hex, true),
        "SHA-512-256" => (sha512_256_hex, false),
        "SHA-512-256-SESS" => (sha512_256_hex, true),
        _ => return None,
    };
    Some(hash)
}

fn digest_header(
    challenge: &HashMap<String, String>,
    username: &str,
    password: &str,
    outgoing: &Outgoing,
) -> Result<String> {
    // The client nonce guards against chosen-plaintext attacks by the
    // server, which only works if the server cannot predict it.
    let mut random = [0_u8; 8];
    getrandom::fill(&mut random)
        .map_err(|e| Error::Http(format!("Could not make a Digest client nonce: {e}")))?;
    digest_header_with(
        challenge,
        username,
        password,
        outgoing,
        &hex::encode(random),
    )
}

fn digest_header_with(
    challenge: &HashMap<String, String>,
    username: &str,
    password: &str,
    outgoing: &Outgoing,
    cnonce: &str,
) -> Result<String> {
    let get = |name: &str| challenge.get(name).map_or("", String::as_str);
    let algorithm = digest_algorithm(challenge);
    let Some((hash, session)) = digest_hash(challenge) else {
        return Err(Error::Http(format!(
            "The server asks for Digest with {algorithm}, which Getman does not support"
        )));
    };
    let url = url::Url::parse(&outgoing.url).map_err(|e| Error::Invalid(e.to_string()))?;
    let uri = match url.query() {
        Some(query) => format!("{}?{query}", url.path()),
        None => url.path().to_owned(),
    };
    let (realm, nonce) = (get("realm"), get("nonce"));
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

fn sha512_256_hex(data: &[u8]) -> String {
    hex::encode(sha2::Sha512_256::digest(data))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ClientAuth, KeyValue, OAuth2Grant, Pkce};
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

    #[test]
    fn digest_matches_the_rfc_7616_examples() {
        let get = |url: &str| Outgoing {
            method: "GET".into(),
            url: url.into(),
            headers: vec![],
            body: OutgoingBody::None,
        };
        let response = |header: String| {
            header
                .split("response=\"")
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap()
                .to_owned()
        };
        // Section 3.9.1, with SHA-256 and with MD5.
        for (algorithm, expected) in [
            (
                "SHA-256",
                "753927fa0e85d155564e2e272a28d1802ca10daf4496794697cf8db5856cb6c1",
            ),
            ("MD5", "8ca523f5e9506fed4657c9700eebdbec"),
        ] {
            let challenge = parse_challenge(&format!(
                r#"Digest realm="http-auth@example.org", qop="auth, auth-int", algorithm={algorithm}, nonce="7ypf/xlj9XXwfDPEoM4URrv/xwf94BcCAzFZH4GiTo0v", opaque="FQhe/qaU925kfnzjCev0ciny7QMkPqMAFRtzCUYo5tdS""#
            ))
            .unwrap();
            let header = digest_header_with(
                &challenge,
                "Mufasa",
                "Circle of Life",
                &get("http://www.example.org/dir/index.html"),
                "f2/wE4q74E6zIJEtWaHKaf5wv/H5QzzpXusqGemxURZJ",
            )
            .unwrap();
            assert_eq!(response(header), expected, "{algorithm}");
        }
        // The inputs of section 3.9.2, with SHA-512-256. The response printed
        // there does not follow from them; this one was computed from the
        // same inputs with Python's hashlib.
        let challenge = parse_challenge(
            r#"Digest realm="api@example.org", qop="auth", algorithm=SHA-512-256, nonce="5TsQWLVdgBdmrQ0XsxbDODV+57QdFR34I9HAbC/RVvkK", opaque="HRPCssKJSGjCrkzDg8OhwpzCiGPChXYjwrI2QmXDnsOS", charset=UTF-8, userhash=true"#,
        )
        .unwrap();
        let header = digest_header_with(
            &challenge,
            "J\u{e4}s\u{f8}n Doe",
            "Secret, or not?",
            &get("http://api.example.org/doe.json"),
            "NTg6RKcb9boFIAS3KrFK9BGeh+iDa/sm6jUMp2wds69v",
        )
        .unwrap();
        assert_eq!(
            response(header.clone()),
            "3798d4131c277846293534c3edc11bd8a5e4cdcbff78b05db9d95eeb1cec68a5"
        );
        assert!(header.contains("algorithm=SHA-512-256"), "{header}");

        let mut sess = challenge.clone();
        sess.insert("algorithm".into(), "SHA-512-256-sess".into());
        assert!(digest_hash(&sess).is_some_and(|(_, session)| session));
        sess.insert("algorithm".into(), "SHA-1".into());
        assert!(digest_hash(&sess).is_none());
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

    /// Visits a URL the way a browser would, returning status and body.
    async fn visit(url: String) -> (u16, String) {
        let response = HttpClient::new()
            .send(&get(url), Transport::default())
            .await
            .unwrap();
        (response.status, response.text().into_owned())
    }

    fn query_param(url: &str, name: &str) -> Option<String> {
        url::Url::parse(url)
            .unwrap()
            .query_pairs()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.into_owned())
    }

    fn code_grant(server: &test_server::TestServer) -> OAuth2 {
        OAuth2 {
            grant: OAuth2Grant::AuthorizationCode,
            auth_url: "https://id.example.com/authorize?tenant=t1".into(),
            token_url: server.url("/token"),
            client_id: "app".into(),
            client_secret: "{{secret}}".into(),
            pkce: Pkce::S256,
            scope: "read write".into(),
            auth_params: vec![KeyValue::new("prompt", "consent")],
            token_params: vec![KeyValue::new("lifetime", "1")],
            ..OAuth2::default()
        }
    }

    #[tokio::test]
    async fn the_authorization_code_grant_signs_in_refreshes_and_persists() {
        let server = test_server::spawn().await;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(".getman").join(TOKEN_FILE);
        let http = HttpClient::new();
        let mut vars = VarStack::new();
        vars.push(
            Scope::Environment,
            [("secret".to_owned(), "s3cret".to_owned())]
                .into_iter()
                .collect(),
        );
        let settings = code_grant(&server);
        let authenticator = Authenticator::with_store(&file);

        let error = authenticator
            .token(&http, &settings, &vars, Transport::default(), false)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("Get new token"), "{error}");

        let authorization = authenticator.authorize(&settings, &vars).await.unwrap();
        let url = &authorization.url;
        assert!(url.starts_with("https://id.example.com/authorize?tenant=t1&"));
        assert_eq!(query_param(url, "response_type").as_deref(), Some("code"));
        assert_eq!(query_param(url, "client_id").as_deref(), Some("app"));
        assert_eq!(query_param(url, "scope").as_deref(), Some("read write"));
        assert_eq!(query_param(url, "prompt").as_deref(), Some("consent"));
        assert_eq!(
            query_param(url, "code_challenge_method").as_deref(),
            Some("S256")
        );
        assert_eq!(
            query_param(url, "redirect_uri").unwrap(),
            authorization.redirect_uri
        );
        let state = query_param(url, "state").unwrap();
        assert!(state.len() >= 20);
        let challenge = query_param(url, "code_challenge").unwrap();

        let redirect = authorization.redirect_uri.clone();
        let browser = tokio::spawn(async move {
            let (status, _) = visit(format!("{redirect}?code=good-code&state=forged")).await;
            assert_eq!(status, 400);
            let (status, page) = visit(format!("{redirect}?code=good-code&state={state}")).await;
            assert_eq!(status, 200);
            assert!(page.contains("Signed in"), "{page}");
        });
        let token = authenticator
            .wait_for_token(
                &http,
                &authorization.id,
                Transport::default(),
                Duration::from_secs(10),
            )
            .await
            .unwrap();
        browser.await.unwrap();
        assert_eq!(token.access_token, format!("code-token-{challenge}"));
        assert!(token.refreshable);
        assert!(token.expires_at.is_some());
        assert!(file.is_file());
        assert!(dir.path().join(".getman/.gitignore").is_file());
        let saved = std::fs::read_to_string(&file).unwrap();
        assert!(!saved.contains("s3cret"), "{saved}");

        // It lives for a second, less than the margin, so the next request
        // refreshes it.
        let mut outgoing = get(server.url("/echo"));
        let response = authenticator
            .send(
                &http,
                &Auth::Oauth2(settings.clone()),
                &vars,
                &mut outgoing,
                Transport::default(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.json().unwrap()["headers"]["authorization"],
            "Bearer refreshed-with-refresh-1"
        );

        // Another run reads the file.
        let later = Authenticator::with_store(&file);
        let cached = later.cached_token(&settings, &vars).unwrap();
        assert_eq!(cached.access_token, "refreshed-with-refresh-1");
        assert!(cached.refreshable);
        assert!(later.clear_token(&settings, &vars).unwrap());
        assert!(later.cached_token(&settings, &vars).is_none());
        assert!(!file.exists());
    }

    #[tokio::test]
    async fn the_implicit_grant_takes_the_token_from_the_fragment() {
        let authenticator = Authenticator::new();
        let settings = OAuth2 {
            grant: OAuth2Grant::Implicit,
            auth_url: "https://id.example.com/authorize".into(),
            client_id: "public".into(),
            ..OAuth2::default()
        };
        let vars = VarStack::new();
        let authorization = authenticator.authorize(&settings, &vars).await.unwrap();
        assert_eq!(
            query_param(&authorization.url, "response_type").as_deref(),
            Some("token")
        );
        assert!(query_param(&authorization.url, "code_challenge").is_none());
        let state = query_param(&authorization.url, "state").unwrap();
        let redirect = authorization.redirect_uri.clone();
        let browser = tokio::spawn(async move {
            let (status, page) = visit(redirect.clone()).await;
            assert_eq!(status, 200);
            assert!(page.contains("location.hash"), "{page}");
            let (status, _) = visit(format!(
                "{redirect}?access_token=imp&token_type=bearer&expires_in=120&state={state}"
            ))
            .await;
            assert_eq!(status, 200);
        });
        let token = authenticator
            .wait_for_token(
                &HttpClient::new(),
                &authorization.id,
                Transport::default(),
                Duration::from_secs(10),
            )
            .await
            .unwrap();
        browser.await.unwrap();
        assert_eq!(token.access_token, "imp");
        assert_eq!(token.expires_in, Some(120));
        assert!(!token.refreshable);
        assert_eq!(
            authenticator
                .cached_token(&settings, &vars)
                .unwrap()
                .access_token,
            "imp"
        );
    }

    #[tokio::test]
    async fn sign_ins_can_be_cancelled_and_replaced() {
        let server = test_server::spawn().await;
        let authenticator = Arc::new(Authenticator::new());
        let mut settings = code_grant(&server);
        settings.client_secret = "s3cret".into();
        let vars = VarStack::new();
        let first = authenticator.authorize(&settings, &vars).await.unwrap();
        let waiting = {
            let authenticator = Arc::clone(&authenticator);
            let id = first.id.clone();
            tokio::spawn(async move {
                authenticator
                    .wait_for_token(
                        &HttpClient::new(),
                        &id,
                        Transport::default(),
                        Duration::from_secs(10),
                    )
                    .await
            })
        };
        tokio::time::sleep(Duration::from_millis(50)).await;
        authenticator.cancel_authorization(&first.id);
        let error = waiting.await.unwrap().unwrap_err();
        assert!(error.to_string().contains("cancelled"), "{error}");

        // A new sign-in replaces one nobody waits on yet.
        let second = authenticator.authorize(&settings, &vars).await.unwrap();
        let third = authenticator.authorize(&settings, &vars).await.unwrap();
        let error = authenticator
            .wait_for_token(
                &HttpClient::new(),
                &second.id,
                Transport::default(),
                Duration::from_secs(1),
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("already finished"), "{error}");
        let error = authenticator
            .wait_for_token(
                &HttpClient::new(),
                &third.id,
                Transport::default(),
                Duration::from_millis(100),
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("stopped waiting"), "{error}");
    }

    #[tokio::test]
    async fn password_tokens_refresh_and_can_go_in_the_query() {
        let server = test_server::spawn().await;
        let http = HttpClient::new();
        let settings = OAuth2 {
            grant: OAuth2Grant::Password,
            token_url: server.url("/token"),
            client_id: "app".into(),
            client_secret: "s3cret".into(),
            username: "ada".into(),
            token_params: vec![KeyValue::new("lifetime", "1")],
            token_in: crate::model::ApiKeyLocation::Query,
            ..OAuth2::default()
        };
        let authenticator = Authenticator::new();
        let first = authenticator
            .token(
                &http,
                &settings,
                &VarStack::new(),
                Transport::default(),
                false,
            )
            .await
            .unwrap();
        assert_eq!(first.access_token, "token-for-password-");
        assert!(first.refreshable);
        let mut outgoing = get(server.url("/echo?a=1"));
        let response = authenticator
            .send(
                &http,
                &Auth::Oauth2(settings),
                &VarStack::new(),
                &mut outgoing,
                Transport::default(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.json().unwrap()["query"],
            "a=1&access_token=refreshed-with-refresh-pw"
        );
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
