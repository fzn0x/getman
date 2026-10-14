//! Putting requests on the wire and capturing what comes back.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, Once};
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use reqwest_cookie_store::CookieStoreMutex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::{Error, Result};
use crate::model::Settings;

/// Responses larger than this are cut short and marked truncated.
pub const MAX_BODY_BYTES: usize = 100 * 1024 * 1024;

/// Value of the `User-Agent` header Getman sends unless the request sets
/// one.
pub const USER_AGENT: &str = concat!("getman/", env!("CARGO_PKG_VERSION"));

/// Bytes that serialize as `{"text": ...}` when they are UTF-8 and as
/// `{"base64": ...}` otherwise.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Payload(pub Vec<u8>);

impl Payload {
    /// The bytes as text, replacing invalid UTF-8.
    pub fn text(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.0)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PayloadRepr {
    Text(String),
    Base64(String),
}

impl Serialize for Payload {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let repr = match std::str::from_utf8(&self.0) {
            Ok(text) => PayloadRepr::Text(text.to_owned()),
            Err(_) => PayloadRepr::Base64(BASE64.encode(&self.0)),
        };
        repr.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Payload {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match PayloadRepr::deserialize(deserializer)? {
            PayloadRepr::Text(text) => Ok(Self(text.into_bytes())),
            PayloadRepr::Base64(data) => BASE64
                .decode(data)
                .map(Self)
                .map_err(serde::de::Error::custom),
        }
    }
}

/// A header as sent or received.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    /// Header name.
    pub name: String,
    /// Header value.
    pub value: String,
}

impl Header {
    /// A header.
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}

/// Finds a header by name, ignoring case.
pub fn find_header<'a>(headers: &'a [Header], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str())
}

/// Sets a header, replacing any with the same name.
pub fn set_header(headers: &mut Vec<Header>, name: &str, value: impl Into<String>) {
    headers.retain(|h| !h.name.eq_ignore_ascii_case(name));
    headers.push(Header::new(name, value));
}

/// A request with every variable replaced, ready to send.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outgoing {
    /// HTTP method.
    pub method: String,
    /// Absolute URL.
    pub url: String,
    /// Headers, in order.
    pub headers: Vec<Header>,
    /// Body.
    pub body: OutgoingBody,
}

/// The body of an [`Outgoing`] request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutgoingBody {
    /// No body.
    #[default]
    None,
    /// These bytes.
    Bytes {
        /// The bytes.
        data: Payload,
    },
    /// The contents of a file, read when sending.
    File {
        /// Absolute path of the file.
        path: PathBuf,
    },
    /// A `multipart/form-data` body.
    Multipart {
        /// The parts, in order.
        parts: Vec<Part>,
    },
}

/// A part of a multipart body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Part {
    /// Field name.
    pub name: String,
    /// Text value, when `file` is not set.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub value: String,
    /// Absolute path of a file to upload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<PathBuf>,
    /// Content type of the part.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
}

/// Transport settings with every default filled in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Transport {
    /// Give up after this many milliseconds; 0 waits forever.
    pub timeout_ms: u64,
    /// Follow redirects.
    pub follow_redirects: bool,
    /// Most redirects to follow.
    pub max_redirects: u32,
    /// Check TLS certificates.
    pub verify_tls: bool,
}

impl Default for Transport {
    fn default() -> Self {
        Self::from(Settings::default())
    }
}

impl From<Settings> for Transport {
    fn from(settings: Settings) -> Self {
        Self {
            timeout_ms: settings.timeout_ms.unwrap_or(30_000),
            follow_redirects: settings.follow_redirects.unwrap_or(true),
            max_redirects: settings.max_redirects.unwrap_or(10),
            verify_tls: settings.verify_tls.unwrap_or(true),
        }
    }
}

/// A response, read in full.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    /// Status code.
    pub status: u16,
    /// Standard reason phrase for the status code.
    pub status_text: String,
    /// Protocol version, such as `HTTP/1.1`.
    pub http_version: String,
    /// Final URL, after any redirects.
    pub url: String,
    /// Response headers, in order.
    pub headers: Vec<Header>,
    /// Body, decompressed.
    pub body: Payload,
    /// Whether the body was cut short at [`MAX_BODY_BYTES`].
    pub truncated: bool,
    /// Milliseconds from sending until the body was read.
    pub time_ms: f64,
    /// Milliseconds from sending until the headers arrived.
    pub headers_ms: f64,
    /// Size of the headers and the body.
    pub size: Size,
    /// Address of the server that answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_addr: Option<String>,
}

/// Sizes of a response, in bytes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Size {
    /// The status line and headers, as they appear on HTTP/1.1.
    pub headers: u64,
    /// The decompressed body.
    pub body: u64,
}

impl Response {
    /// The first header with this name, ignoring case.
    pub fn header(&self, name: &str) -> Option<&str> {
        find_header(&self.headers, name)
    }

    /// The body as text, replacing invalid UTF-8.
    pub fn text(&self) -> Cow<'_, str> {
        self.body.text()
    }

    /// The body parsed as JSON, if it is JSON.
    pub fn json(&self) -> Option<serde_json::Value> {
        serde_json::from_slice(&self.body.0).ok()
    }
}

/// A cookie the client holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CookieInfo {
    /// Domain the cookie is sent to.
    pub domain: String,
    /// Path the cookie is sent to.
    pub path: String,
    /// Cookie name.
    pub name: String,
    /// Cookie value.
    pub value: String,
    /// Expiry as milliseconds since the Unix epoch; `None` for session
    /// cookies.
    pub expires: Option<i64>,
    /// Sent over HTTPS only.
    pub secure: bool,
    /// Hidden from scripts in browsers.
    pub http_only: bool,
}

/// Sends requests, sharing connections and one cookie jar.
#[derive(Debug)]
pub struct HttpClient {
    cookies: Arc<CookieStoreMutex>,
    clients: Mutex<HashMap<ClientKey, reqwest::Client>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ClientKey {
    follow_redirects: bool,
    max_redirects: u32,
    verify_tls: bool,
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpClient {
    /// A client with an empty cookie jar.
    pub fn new() -> Self {
        static PROVIDER: Once = Once::new();
        PROVIDER.call_once(|| {
            // Fails only if another provider is installed, which is fine.
            let _ = rustls::crypto::ring::default_provider().install_default();
        });
        Self {
            cookies: Arc::new(CookieStoreMutex::default()),
            clients: Mutex::new(HashMap::new()),
        }
    }

    fn client(&self, transport: Transport) -> Result<reqwest::Client> {
        let key = ClientKey {
            follow_redirects: transport.follow_redirects,
            max_redirects: transport.max_redirects,
            verify_tls: transport.verify_tls,
        };
        let mut clients = self.clients.lock().expect("client cache lock");
        if let Some(client) = clients.get(&key) {
            return Ok(client.clone());
        }
        let redirect = if key.follow_redirects {
            reqwest::redirect::Policy::limited(key.max_redirects as usize)
        } else {
            reqwest::redirect::Policy::none()
        };
        let client = reqwest::Client::builder()
            .cookie_provider(Arc::clone(&self.cookies))
            .redirect(redirect)
            .tls_danger_accept_invalid_certs(!key.verify_tls)
            .connect_timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| Error::Http(describe(&e)))?;
        clients.insert(key, client.clone());
        Ok(client)
    }

    /// Sends a request and reads the whole response.
    pub async fn send(&self, outgoing: &Outgoing, transport: Transport) -> Result<Response> {
        let client = self.client(transport)?;
        let request = build(&client, outgoing, transport).await?;
        let started = Instant::now();
        let mut response = client
            .execute(request)
            .await
            .map_err(|e| Error::Http(describe(&e)))?;
        let headers_ms = millis(started.elapsed());

        let status = response.status();
        let http_version = format!("{:?}", response.version());
        let url = response.url().to_string();
        let remote_addr = response.remote_addr().map(|a| a.to_string());
        let headers: Vec<Header> = response
            .headers()
            .iter()
            .map(|(name, value)| {
                Header::new(
                    name.as_str(),
                    String::from_utf8_lossy(value.as_bytes()).into_owned(),
                )
            })
            .collect();

        let mut body = Vec::new();
        let mut truncated = false;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| Error::Http(describe(&e)))?
        {
            let room = MAX_BODY_BYTES - body.len();
            if chunk.len() > room {
                body.extend_from_slice(&chunk[..room]);
                truncated = true;
                break;
            }
            body.extend_from_slice(&chunk);
        }
        let time_ms = millis(started.elapsed());

        let status_text = status.canonical_reason().unwrap_or_default().to_owned();
        let header_bytes = http_version.len()
            + 5
            + status_text.len()
            + 2
            + headers
                .iter()
                .map(|h| h.name.len() + h.value.len() + 4)
                .sum::<usize>()
            + 2;
        Ok(Response {
            status: status.as_u16(),
            status_text,
            http_version,
            url,
            size: Size {
                headers: header_bytes as u64,
                body: body.len() as u64,
            },
            headers,
            body: Payload(body),
            truncated,
            time_ms,
            headers_ms,
            remote_addr,
        })
    }

    /// The cookies in the jar, expired ones excluded.
    pub fn cookies(&self) -> Vec<CookieInfo> {
        let store = self.cookies.lock().expect("cookie jar lock");
        let mut list: Vec<CookieInfo> = store
            .iter_unexpired()
            .map(|cookie| CookieInfo {
                domain: cookie
                    .domain
                    .as_cow()
                    .map(Cow::into_owned)
                    .unwrap_or_default(),
                path: cookie.path.to_string(),
                name: cookie.name().to_owned(),
                value: cookie.value().to_owned(),
                expires: match &cookie.expires {
                    cookie_store::CookieExpiration::AtUtc(at) => {
                        i64::try_from(at.unix_timestamp_nanos() / 1_000_000).ok()
                    }
                    cookie_store::CookieExpiration::SessionEnd => None,
                },
                secure: cookie.secure().unwrap_or(false),
                http_only: cookie.http_only().unwrap_or(false),
            })
            .collect();
        list.sort_by(|a, b| (&a.domain, &a.path, &a.name).cmp(&(&b.domain, &b.path, &b.name)));
        list
    }

    /// Removes one cookie. Returns whether it existed.
    pub fn remove_cookie(&self, domain: &str, path: &str, name: &str) -> bool {
        let mut store = self.cookies.lock().expect("cookie jar lock");
        store.remove(domain, path, name).is_some()
    }

    /// Empties the cookie jar.
    pub fn clear_cookies(&self) {
        self.cookies.lock().expect("cookie jar lock").clear();
    }
}

async fn build(
    client: &reqwest::Client,
    outgoing: &Outgoing,
    transport: Transport,
) -> Result<reqwest::Request> {
    let method = reqwest::Method::from_bytes(outgoing.method.trim().as_bytes())
        .map_err(|_| Error::Invalid(format!("{:?} is not a valid method", outgoing.method)))?;
    let url = reqwest::Url::parse(&outgoing.url)
        .map_err(|e| Error::Invalid(format!("{:?} is not a valid URL: {e}", outgoing.url)))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(Error::Invalid(format!(
            "only http and https URLs can be sent, not {}",
            url.scheme()
        )));
    }
    let mut builder = client.request(method, url);
    for header in &outgoing.headers {
        let name = reqwest::header::HeaderName::from_bytes(header.name.trim().as_bytes())
            .map_err(|_| Error::Invalid(format!("{:?} is not a valid header name", header.name)))?;
        let value = reqwest::header::HeaderValue::from_str(&header.value).map_err(|_| {
            Error::Invalid(format!(
                "the value of header {} contains characters headers cannot carry",
                header.name
            ))
        })?;
        builder = builder.header(name, value);
    }
    if transport.timeout_ms > 0 {
        builder = builder.timeout(Duration::from_millis(transport.timeout_ms));
    }
    builder = match &outgoing.body {
        OutgoingBody::None => builder,
        OutgoingBody::Bytes { data } => builder.body(data.0.clone()),
        OutgoingBody::File { path } => {
            let data = tokio::fs::read(path)
                .await
                .map_err(|e| Error::io(path, e))?;
            builder.body(data)
        }
        OutgoingBody::Multipart { parts } => {
            let mut form = reqwest::multipart::Form::new();
            for part in parts {
                let mut piece = match &part.file {
                    Some(path) => {
                        let data = tokio::fs::read(path)
                            .await
                            .map_err(|e| Error::io(path, e))?;
                        let file_name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        let guessed = mime_guess::from_path(path).first_or_octet_stream();
                        reqwest::multipart::Part::bytes(data)
                            .file_name(file_name)
                            .mime_str(guessed.essence_str())
                            .map_err(|e| Error::Http(describe(&e)))?
                    }
                    None => reqwest::multipart::Part::text(part.value.clone()),
                };
                if let Some(content_type) = &part.content_type {
                    piece = piece.mime_str(content_type).map_err(|_| {
                        Error::Invalid(format!("{content_type:?} is not a valid content type"))
                    })?;
                }
                form = form.part(part.name.clone(), piece);
            }
            builder.multipart(form)
        }
    };
    builder.build().map_err(|e| Error::Http(describe(&e)))
}

/// Explains a transport error in one line, most general cause first.
fn describe(error: &reqwest::Error) -> String {
    let mut message = String::from(if error.is_timeout() {
        "The request timed out"
    } else if error.is_connect() {
        "Could not connect"
    } else if error.is_redirect() {
        "Stopped following redirects"
    } else if error.is_builder() {
        "The request is invalid"
    } else if error.is_body() || error.is_decode() {
        "The response body could not be read"
    } else {
        "The request failed"
    });
    let mut seen: Vec<String> = Vec::new();
    let mut source: Option<&dyn std::error::Error> = std::error::Error::source(error);
    while let Some(cause) = source {
        let text = cause.to_string();
        if !text.is_empty() && !seen.iter().any(|s| s.contains(&text)) {
            let _ = write!(message, ": {text}");
            seen.push(text);
        }
        source = cause.source();
    }
    message
}

fn millis(duration: Duration) -> f64 {
    (duration.as_secs_f64() * 1000.0 * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_server;

    fn get(url: String) -> Outgoing {
        Outgoing {
            method: "GET".into(),
            url,
            headers: vec![],
            body: OutgoingBody::None,
        }
    }

    #[tokio::test]
    async fn sends_method_headers_and_body() {
        let server = test_server::spawn().await;
        let client = HttpClient::new();
        let outgoing = Outgoing {
            method: "PATCH".into(),
            url: server.url("/echo?x=1"),
            headers: vec![Header::new("X-Test", "yes")],
            body: OutgoingBody::Bytes {
                data: Payload(b"hello".to_vec()),
            },
        };
        let response = client.send(&outgoing, Transport::default()).await.unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.status_text, "OK");
        let echo = response.json().unwrap();
        assert_eq!(echo["method"], "PATCH");
        assert_eq!(echo["query"], "x=1");
        assert_eq!(echo["headers"]["x-test"], "yes");
        assert_eq!(echo["body"], "hello");
        assert!(response.size.body > 0);
        assert!(response.time_ms >= response.headers_ms);
    }

    #[tokio::test]
    async fn redirects_follow_the_settings() {
        let server = test_server::spawn().await;
        let client = HttpClient::new();
        let followed = client
            .send(&get(server.url("/redirect")), Transport::default())
            .await
            .unwrap();
        assert_eq!(followed.status, 200);
        assert!(followed.url.ends_with("/echo"));
        let stopped = client
            .send(
                &get(server.url("/redirect")),
                Transport {
                    follow_redirects: false,
                    ..Transport::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(stopped.status, 302);
        assert_eq!(stopped.header("location"), Some("/echo"));
    }

    #[tokio::test]
    async fn cookies_persist_between_requests() {
        let server = test_server::spawn().await;
        let client = HttpClient::new();
        client
            .send(
                &get(server.url("/cookies/set?flavor=oat")),
                Transport::default(),
            )
            .await
            .unwrap();
        let echo = client
            .send(&get(server.url("/echo")), Transport::default())
            .await
            .unwrap()
            .json()
            .unwrap();
        assert_eq!(echo["headers"]["cookie"], "flavor=oat");
        let cookies = client.cookies();
        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0].name, "flavor");
        assert!(client.remove_cookie(&cookies[0].domain, &cookies[0].path, "flavor"));
        assert_eq!(client.cookies(), []);
    }

    #[tokio::test]
    async fn binary_bodies_serialize_as_base64() {
        let server = test_server::spawn().await;
        let response = HttpClient::new()
            .send(&get(server.url("/bytes")), Transport::default())
            .await
            .unwrap();
        assert_eq!(response.body.0, [0, 159, 146, 150]);
        let json = serde_json::to_value(&response.body).unwrap();
        assert_eq!(json["base64"], "AJ+Slg==");
        let back: Payload = serde_json::from_value(json).unwrap();
        assert_eq!(back, response.body);
    }

    #[tokio::test]
    async fn timeouts_and_refused_connections_explain_themselves() {
        let server = test_server::spawn().await;
        let client = HttpClient::new();
        let error = client
            .send(
                &get(server.url("/slow")),
                Transport {
                    timeout_ms: 50,
                    ..Transport::default()
                },
            )
            .await
            .unwrap_err();
        assert!(
            error.to_string().starts_with("The request timed out"),
            "{error}"
        );

        let error = client
            .send(&get("http://127.0.0.1:9/".into()), Transport::default())
            .await
            .unwrap_err();
        assert!(
            error.to_string().starts_with("Could not connect"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn invalid_input_is_reported() {
        let client = HttpClient::new();
        let mut outgoing = get("ftp://example.com/".into());
        assert!(matches!(
            client.send(&outgoing, Transport::default()).await,
            Err(Error::Invalid(_))
        ));
        outgoing.url = "http://example.com/".into();
        outgoing.method = "GE T".into();
        assert!(matches!(
            client.send(&outgoing, Transport::default()).await,
            Err(Error::Invalid(_))
        ));
    }
}
