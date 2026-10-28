//! Putting requests on the wire and capturing what comes back.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Once};
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use reqwest_cookie_store::CookieStoreMutex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::{Error, Result};
use crate::http3;
use crate::model::HttpVersion;
use crate::timing::{self, Probe, TimedConnectLayer, TimedResolver};
use crate::tls::{CertificateInfo, ClientIdentity, TlsDetails};

/// Responses larger than this are cut short and marked truncated, unless
/// the `max_response_mb` setting says otherwise.
pub const MAX_BODY_BYTES: usize = 100 * 1024 * 1024;

/// Name of the file in the workspace's state directory that keeps the
/// cookie jar between runs of the browser UI.
pub const COOKIE_FILE: &str = "cookies.json";

/// Clients kept for different settings before the cache starts over.
const MAX_CLIENTS: usize = 32;

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

/// Transport settings with every default filled in, variables replaced
/// and certificate files read; see [`crate::prepare::transport`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Transport {
    /// Give up after this many milliseconds; 0 waits forever.
    pub timeout_ms: u64,
    /// Follow redirects.
    pub follow_redirects: bool,
    /// Most redirects to follow.
    pub max_redirects: u32,
    /// Keep the method and body across a 301 or 302, rather than turning
    /// a POST into a GET.
    pub follow_original_method: bool,
    /// Keep `Authorization` across a redirect to another host, port or
    /// scheme.
    pub follow_authorization: bool,
    /// Check TLS certificates.
    pub verify_tls: bool,
    /// HTTP version to speak.
    pub http_version: HttpVersion,
    /// Send HTTP/1.1 header names in Title-Case.
    pub title_case_headers: bool,
    /// Proxy to send through.
    pub proxy: ProxyRoute,
    /// Certificate to present when a server asks for one.
    pub client_identity: Option<ClientIdentity>,
    /// Extra certificate authorities to trust, each PEM text.
    pub ca_certs: Vec<Vec<u8>>,
    /// Send and store cookies with the cookie jar.
    pub cookies: bool,
    /// Most body bytes to read; 0 reads everything.
    pub max_body_bytes: u64,
}

impl Default for Transport {
    fn default() -> Self {
        Self {
            timeout_ms: 30_000,
            follow_redirects: true,
            max_redirects: 10,
            follow_original_method: false,
            follow_authorization: false,
            verify_tls: true,
            http_version: HttpVersion::Auto,
            title_case_headers: true,
            proxy: ProxyRoute::System,
            client_identity: None,
            ca_certs: Vec::new(),
            cookies: true,
            max_body_bytes: MAX_BODY_BYTES as u64,
        }
    }
}

/// How requests reach servers, as far as proxies go.
#[derive(Clone, Default, PartialEq, Eq, Hash)]
pub enum ProxyRoute {
    /// Through the system's proxy settings and the `HTTPS_PROXY`,
    /// `HTTP_PROXY` and `NO_PROXY` environment variables.
    #[default]
    System,
    /// Straight to the server, ignoring every proxy setting.
    Off,
    /// Through this proxy.
    Url {
        /// The proxy's URL, with any credentials in it.
        url: String,
        /// Hosts reached directly, separated by commas.
        bypass: String,
    },
}

impl fmt::Debug for ProxyRoute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::System => f.write_str("System"),
            Self::Off => f.write_str("Off"),
            Self::Url { url, bypass } => {
                // Credentials stay out of logs.
                let shown = url::Url::parse(url).map_or_else(
                    |_| "(invalid)".to_owned(),
                    |mut parsed| {
                        if parsed.password().is_some() {
                            let _ = parsed.set_password(Some("***"));
                        }
                        parsed.to_string()
                    },
                );
                f.debug_struct("Url")
                    .field("url", &shown)
                    .field("bypass", bypass)
                    .finish()
            }
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
    /// Whether the body was cut short at the size limit.
    pub truncated: bool,
    /// Milliseconds from sending until the body was read.
    pub time_ms: f64,
    /// Milliseconds from sending until the headers arrived.
    pub headers_ms: f64,
    /// The redirects followed on the way, in order. Empty when the first
    /// request got this response.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hops: Vec<Hop>,
    /// Size of the headers and the body.
    pub size: Size,
    /// Address of the server that answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_addr: Option<String>,
    /// Where the time went.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timings: Option<Timings>,
    /// The TLS connection, for `https://` URLs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls: Option<TlsDetails>,
    /// Whether the server offered HTTP/3 in an `Alt-Svc` header while the
    /// request went out with the `auto` HTTP version.
    #[serde(default, skip_serializing_if = "is_false")]
    pub http3_available: bool,
    /// Whether the body is an event stream that was still open when the
    /// request timed out. The body holds what arrived until then.
    #[serde(default)]
    pub stream_open: bool,
}

/// A redirect Getman followed on the way to a response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hop {
    /// Status the redirect came with, such as 302.
    pub status: u16,
    /// URL that was requested and answered with the redirect.
    pub url: String,
    /// Milliseconds from sending that request until its headers arrived.
    pub time_ms: f64,
}

/// Where a response's time went, in milliseconds. The parts add up to
/// the response's `time_ms`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Timings {
    /// Looking up the host's addresses. 0 when nothing was looked up: the
    /// connection was reused, the host is an IP address, or a proxy looks
    /// names up.
    pub dns_ms: f64,
    /// Opening the connection: TCP, any proxy tunnel and the TLS
    /// handshake. 0 when the connection was reused; `None` over HTTP/3,
    /// where the QUIC handshake is part of `first_byte_ms`.
    pub connect_ms: Option<f64>,
    /// From sending the request until the response headers arrived.
    pub first_byte_ms: f64,
    /// Reading the body.
    pub download_ms: f64,
    /// Whether the request went over a connection that was already open.
    pub reused: bool,
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CookieInfo {
    /// Domain the cookie is sent to.
    pub domain: String,
    /// Path the cookie is sent to.
    #[serde(default = "root_path")]
    pub path: String,
    /// Cookie name.
    pub name: String,
    /// Cookie value.
    #[serde(default)]
    pub value: String,
    /// Expiry as milliseconds since the Unix epoch; `None` for session
    /// cookies.
    #[serde(default)]
    pub expires: Option<i64>,
    /// Sent over HTTPS only.
    #[serde(default)]
    pub secure: bool,
    /// Hidden from scripts in browsers.
    #[serde(default)]
    pub http_only: bool,
    /// Sent to `domain` only, not to its subdomains.
    #[serde(default)]
    pub host_only: bool,
}

fn root_path() -> String {
    "/".to_owned()
}

/// Sends requests, sharing connections and one cookie jar.
#[derive(Debug)]
pub struct HttpClient {
    cookies: Arc<CookieStoreMutex>,
    clients: Mutex<HashMap<ClientKey, Session>>,
    /// Where the jar is kept between runs, if anywhere, and what was last
    /// written there.
    cookie_file: Option<(PathBuf, Mutex<String>)>,
}

/// What requests with some settings are sent through.
#[derive(Debug, Clone)]
enum Session {
    /// reqwest, for HTTP/1.1 and HTTP/2.
    Tcp(reqwest::Client),
    /// Getman's own HTTP/3, see [`crate::http3`].
    Quic(Arc<http3::Client>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(clippy::struct_excessive_bools)]
struct ClientKey {
    verify_tls: bool,
    http_version: HttpVersion,
    title_case_headers: bool,
    proxy: ProxyRoute,
    client_identity: Option<ClientIdentity>,
    ca_certs: Vec<Vec<u8>>,
    cookies: bool,
}

impl ClientKey {
    fn new(transport: &Transport) -> Self {
        Self {
            verify_tls: transport.verify_tls,
            http_version: transport.http_version,
            title_case_headers: transport.title_case_headers,
            proxy: transport.proxy.clone(),
            client_identity: transport.client_identity.clone(),
            ca_certs: transport.ca_certs.clone(),
            cookies: transport.cookies,
        }
    }
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
            cookie_file: None,
        }
    }

    /// A client whose cookie jar is read from `file` and written back to
    /// it whenever it changes. Cookies that expired in between are
    /// dropped; a missing or unreadable file starts an empty jar.
    pub fn with_cookie_file(file: impl Into<PathBuf>) -> Self {
        let file = file.into();
        let mut client = Self::new();
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        if let Some(store) = parse_jar(&text) {
            client.cookies = Arc::new(CookieStoreMutex::new(store));
        }
        client.cookie_file = Some((file, Mutex::new(text)));
        client
    }

    fn client(&self, transport: &Transport) -> Result<Session> {
        let key = ClientKey::new(transport);
        let mut clients = self.clients.lock().expect("client cache lock");
        if let Some(client) = clients.get(&key) {
            return Ok(client.clone());
        }
        let client = self.build_client(&key)?;
        if clients.len() >= MAX_CLIENTS {
            clients.clear();
        }
        clients.insert(key, client.clone());
        Ok(client)
    }

    fn build_client(&self, key: &ClientKey) -> Result<Session> {
        if key.http_version == HttpVersion::Http3 {
            let jar = key.cookies.then(|| Arc::clone(&self.cookies));
            let client = http3::Client::new(
                key.verify_tls,
                &key.ca_certs,
                key.client_identity.as_ref(),
                jar,
            )?;
            return Ok(Session::Quic(Arc::new(client)));
        }
        // Redirects are followed in `follow`, which can keep the method and
        // the credentials the way the settings ask.
        let mut builder = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .tls_danger_accept_invalid_certs(!key.verify_tls)
            .tls_info(true)
            .connect_timeout(Duration::from_secs(30))
            .dns_resolver(TimedResolver)
            .connector_layer(TimedConnectLayer);
        if key.cookies {
            builder = builder.cookie_provider(Arc::clone(&self.cookies));
        }
        if key.title_case_headers {
            builder = builder.http1_title_case_headers();
        }
        builder = match key.http_version {
            HttpVersion::Auto | HttpVersion::Http3 => builder,
            HttpVersion::Http1 => builder.http1_only(),
            HttpVersion::Http2 => builder.http2_prior_knowledge(),
        };
        builder = match &key.proxy {
            ProxyRoute::System => builder,
            ProxyRoute::Off => builder.no_proxy(),
            ProxyRoute::Url { url, bypass } => {
                let proxy = reqwest::Proxy::all(url.as_str())
                    .map_err(|e| Error::Invalid(format!("the proxy URL is not valid: {e}")))?
                    .no_proxy(reqwest::NoProxy::from_string(bypass));
                builder.proxy(proxy)
            }
        };
        if let Some(identity) = &key.client_identity {
            let identity = reqwest::Identity::from_pem(identity.pem())
                .map_err(|e| Error::Invalid(describe(&e)))?;
            builder = builder.identity(identity);
        }
        for pem in &key.ca_certs {
            for cert in reqwest::Certificate::from_pem_bundle(pem)
                .map_err(|e| Error::Invalid(describe(&e)))?
            {
                builder = builder.add_root_certificate(cert);
            }
        }
        builder
            .build()
            .map(Session::Tcp)
            .map_err(|e| Error::Http(describe(&e)))
    }

    /// Sends a request and reads the whole response, up to the size
    /// limit.
    pub async fn send(&self, outgoing: &Outgoing, transport: Transport) -> Result<Response> {
        check_route(outgoing, &transport)?;
        let client = self.client(&transport)?;
        let wire = wire(outgoing).await?;
        let probe = Arc::new(Probe::default());
        let exchange = timing::measure(Arc::clone(&probe), exchange(&client, wire, &transport));
        let result = exchange.await;
        if transport.cookies {
            self.save_cookies_quietly();
        }
        let mut response = result.map_err(|e| match e {
            Error::Http(mut message)
                if transport.http_version == HttpVersion::Http3
                    && !message.starts_with("Stopped following redirects") =>
            {
                message.push_str(
                    ". The server did not answer over HTTP/3 (QUIC, on UDP); if it does not support HTTP/3, set the HTTP version to auto",
                );
                Error::Http(message)
            }
            other => other,
        })?;

        let connects = probe.connects();
        let dns = probe.dns();
        let connect = probe.connect().saturating_sub(dns);
        let over_quic = response.http_version == "HTTP/3";
        let setup = millis(dns) + if over_quic { 0.0 } else { millis(connect) };
        response.timings = Some(Timings {
            dns_ms: millis(dns),
            connect_ms: (!over_quic).then(|| millis(connect)),
            first_byte_ms: round((response.headers_ms - setup).max(0.0)),
            download_ms: round((response.time_ms - response.headers_ms).max(0.0)),
            reused: connects == 0,
        });
        response.http3_available = transport.http_version == HttpVersion::Auto
            && !over_quic
            && advertises_http3(&response.headers);
        Ok(response)
    }

    /// Sends a request and returns once the response headers arrive,
    /// leaving the body to be read as it streams in. The timeout covers
    /// only the wait for the headers. Also returns the milliseconds that
    /// took.
    pub(crate) async fn open(
        &self,
        outgoing: &Outgoing,
        transport: Transport,
    ) -> Result<(Incoming, f64)> {
        let client = self.client(&transport)?;
        let wire = wire(outgoing).await?;
        let started = Instant::now();
        let deadline = deadline(&transport);
        let (response, _) = follow(&client, wire, &transport, deadline).await?;
        Ok((response, millis(started.elapsed())))
    }

    /// The value of the `Cookie` header the jar sends to `url`, if it holds
    /// any cookie for it. For connections Getman opens itself, such as
    /// WebSocket handshakes; reqwest does the same for HTTP requests.
    pub(crate) fn cookie_header(&self, url: &url::Url) -> Option<String> {
        let store = self.cookies.lock().expect("cookie jar lock");
        let pairs: Vec<String> = store
            .get_request_values(url)
            .map(|(name, value)| format!("{name}={value}"))
            .collect();
        (!pairs.is_empty()).then(|| pairs.join("; "))
    }

    /// Stores the cookies of `Set-Cookie` header values that answered a
    /// request to `url`. Values that are not valid cookies are ignored.
    pub(crate) fn store_cookies<'a>(
        &self,
        url: &url::Url,
        set_cookie: impl IntoIterator<Item = &'a str>,
    ) {
        let mut stored = false;
        {
            let mut store = self.cookies.lock().expect("cookie jar lock");
            for value in set_cookie {
                stored |= store.parse(value, url).is_ok();
            }
        }
        if stored {
            self.save_cookies_quietly();
        }
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
                host_only: matches!(cookie.domain, cookie_store::CookieDomain::HostOnly(_)),
            })
            .collect();
        list.sort_by(|a, b| (&a.domain, &a.path, &a.name).cmp(&(&b.domain, &b.path, &b.name)));
        list
    }

    /// Adds a cookie to the jar, or replaces the one with the same domain,
    /// path and name. An expiry in the past is refused.
    pub fn set_cookie(&self, cookie: &CookieInfo) -> Result<()> {
        let built = build_cookie(cookie)?;
        let mut store = self.cookies.lock().expect("cookie jar lock");
        let (cookie, url) = built;
        store
            .insert(cookie, &url)
            .map_err(|e| Error::Invalid(format!("the cookie cannot be stored: {e}")))?;
        drop(store);
        self.save_cookies()
    }

    /// Removes one cookie. Returns whether it existed.
    pub fn remove_cookie(&self, domain: &str, path: &str, name: &str) -> bool {
        let mut store = self.cookies.lock().expect("cookie jar lock");
        let removed = store.remove(domain, path, name).is_some();
        drop(store);
        self.save_cookies_quietly();
        removed
    }

    /// Empties the cookie jar.
    pub fn clear_cookies(&self) {
        self.cookies.lock().expect("cookie jar lock").clear();
        self.save_cookies_quietly();
    }

    /// Writes the jar to its file, if it has one and it changed. Session
    /// cookies are kept too: a restart of Getman is not the end of a
    /// session.
    pub fn save_cookies(&self) -> Result<()> {
        let Some((file, saved)) = &self.cookie_file else {
            return Ok(());
        };
        let text = {
            let store = self.cookies.lock().expect("cookie jar lock");
            let cookies: Vec<&cookie_store::Cookie<'static>> = store.iter_unexpired().collect();
            serde_json::to_string(&cookies).map_err(|e| Error::Invalid(e.to_string()))?
        };
        let mut saved = saved.lock().expect("cookie file lock");
        if *saved == text {
            return Ok(());
        }
        write_private(file, &text)?;
        *saved = text;
        Ok(())
    }

    fn save_cookies_quietly(&self) {
        // A jar that cannot be saved still works for this run.
        let _ = self.save_cookies();
    }
}

/// Reads a jar written by [`HttpClient::save_cookies`], dropping cookies
/// that have expired.
fn parse_jar(text: &str) -> Option<cookie_store::CookieStore> {
    if text.trim().is_empty() {
        return None;
    }
    let cookies: Vec<cookie_store::Cookie<'static>> = serde_json::from_str(text).ok()?;
    cookie_store::CookieStore::from_cookies(
        cookies.into_iter().map(Ok::<_, std::convert::Infallible>),
        false,
    )
    .ok()
}

/// Writes a file only its owner can read where the platform allows,
/// replacing it in one step so a crash cannot leave half a file.
fn write_private(file: &Path, text: &str) -> Result<()> {
    if let Some(dir) = file.parent() {
        crate::workspace::create_state_dir(dir)?;
    }
    let temporary = file.with_extension("json.tmp");
    std::fs::write(&temporary, text).map_err(|e| Error::io(&temporary, e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&temporary, file).map_err(|e| Error::io(file, e))
}

/// Turns what a person typed into a cookie and the URL it counts as set
/// from.
fn build_cookie(info: &CookieInfo) -> Result<(cookie_store::Cookie<'static>, url::Url)> {
    let domain = info
        .domain
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    let name = info.name.trim();
    if domain.is_empty() {
        return Err(Error::Invalid("a cookie needs a domain".into()));
    }
    if name.is_empty() {
        return Err(Error::Invalid("a cookie needs a name".into()));
    }
    let path = if info.path.trim().starts_with('/') {
        info.path.trim().to_owned()
    } else {
        format!("/{}", info.path.trim())
    };
    let scheme = if info.secure { "https" } else { "http" };
    let url = url::Url::parse(&format!("{scheme}://{domain}{path}"))
        .map_err(|e| Error::Invalid(format!("{domain:?} is not a valid cookie domain: {e}")))?;
    let mut raw = cookie_store::RawCookie::new(name.to_owned(), info.value.clone());
    raw.set_path(path);
    raw.set_secure(info.secure);
    raw.set_http_only(info.http_only);
    if !info.host_only {
        raw.set_domain(domain);
    }
    let mut cookie = cookie_store::Cookie::try_from_raw_cookie(&raw, &url)
        .map_err(|e| Error::Invalid(format!("the cookie is not valid: {e}")))?
        .into_owned();
    if let Some(expires) = info.expires {
        let seconds = (expires - crate::time::now_millis()) / 1000;
        let seconds = u64::try_from(seconds)
            .ok()
            .filter(|s| *s > 0)
            .ok_or_else(|| Error::Invalid("the cookie's expiry is in the past".into()))?;
        cookie.expires = cookie_store::CookieExpiration::from(seconds);
    }
    Ok((cookie, url))
}

/// Refuses combinations that cannot work before trying them.
fn check_route(outgoing: &Outgoing, transport: &Transport) -> Result<()> {
    if transport.http_version != HttpVersion::Http3 {
        return Ok(());
    }
    if !outgoing
        .url
        .trim_start()
        .to_ascii_lowercase()
        .starts_with("https://")
    {
        return Err(Error::Invalid(
            "HTTP/3 needs an https:// URL; it always runs over TLS".into(),
        ));
    }
    if matches!(transport.proxy, ProxyRoute::Url { .. }) {
        return Err(Error::Invalid(
            "HTTP/3 cannot go through a proxy; turn the proxy off or choose another HTTP version"
                .into(),
        ));
    }
    Ok(())
}

/// The point in time a request must be answered by, if it has a timeout.
fn deadline(transport: &Transport) -> Option<tokio::time::Instant> {
    (transport.timeout_ms > 0)
        .then(|| tokio::time::Instant::now() + Duration::from_millis(transport.timeout_ms))
}

fn timed_out() -> Error {
    Error::Http("The request timed out".into())
}

/// Waits for `future`, at most until `deadline`.
async fn until<F: std::future::Future>(
    deadline: Option<tokio::time::Instant>,
    future: F,
) -> Result<F::Output> {
    match deadline {
        Some(deadline) => tokio::time::timeout_at(deadline, future)
            .await
            .map_err(|_| timed_out()),
        None => Ok(future.await),
    }
}

/// A request, parsed and ready to be sent as often as redirects need.
struct Wire {
    method: http::Method,
    url: url::Url,
    headers: http::HeaderMap,
    body: Option<bytes::Bytes>,
}

impl Wire {
    fn request(&self, client: &reqwest::Client) -> Result<reqwest::Request> {
        let mut builder = client
            .request(self.method.clone(), self.url.clone())
            .headers(self.headers.clone());
        if let Some(body) = &self.body {
            builder = builder.body(body.clone());
        }
        builder.build().map_err(|e| Error::Http(describe(&e)))
    }
}

/// A response whose headers have arrived, and the rest of it still to
/// read.
pub(crate) struct Incoming {
    status: http::StatusCode,
    version: String,
    url: url::Url,
    headers: http::HeaderMap,
    remote_addr: Option<String>,
    tls: Option<TlsDetails>,
    body: IncomingBody,
}

enum IncomingBody {
    Tcp(reqwest::Response),
    Quic(Box<http3::Body>),
}

impl Incoming {
    fn from_reqwest(response: reqwest::Response) -> Self {
        let tls = response
            .extensions()
            .get::<reqwest::tls::TlsInfo>()
            .map(|info| TlsDetails {
                version: info
                    .version()
                    .and_then(crate::tls::version_name)
                    .map(str::to_owned),
                certificate: info.peer_certificate().and_then(CertificateInfo::from_der),
            });
        Self {
            status: response.status(),
            version: version_text(response.version()),
            url: response.url().clone(),
            headers: response.headers().clone(),
            remote_addr: response.remote_addr().map(|a| a.to_string()),
            tls,
            body: IncomingBody::Tcp(response),
        }
    }

    fn from_http3(reply: http3::Reply, url: url::Url) -> Self {
        Self {
            status: reply.status,
            version: "HTTP/3".into(),
            url,
            headers: reply.headers,
            remote_addr: Some(reply.remote_addr),
            tls: Some(reply.tls),
            body: IncomingBody::Quic(Box::new(reply.body)),
        }
    }

    pub(crate) fn status(&self) -> http::StatusCode {
        self.status
    }

    pub(crate) fn headers(&self) -> &http::HeaderMap {
        &self.headers
    }

    /// The next piece of the body, `None` at its end.
    pub(crate) async fn chunk(&mut self) -> Result<Option<bytes::Bytes>> {
        match &mut self.body {
            IncomingBody::Tcp(response) => response
                .chunk()
                .await
                .map_err(|e| Error::Http(describe(&e))),
            IncomingBody::Quic(body) => body.chunk().await,
        }
    }
}

/// Whether a status asks the client to go elsewhere, as these do.
fn is_redirect(status: http::StatusCode) -> bool {
    matches!(status.as_u16(), 301 | 302 | 303 | 307 | 308)
}

/// Whether two URLs name the same scheme, host and port.
fn same_origin(a: &url::Url, b: &url::Url) -> bool {
    a.scheme() == b.scheme()
        && a.host_str() == b.host_str()
        && a.port_or_known_default() == b.port_or_known_default()
}

/// Turns a request into the one a redirect asks for: `next` replaces the
/// URL, the method and body change where the status says so, and
/// credentials stay behind when the request leaves its origin.
fn redirected(wire: &mut Wire, status: http::StatusCode, next: url::Url, transport: &Transport) {
    // A 303 asks for the result with GET (or HEAD, if that is what was
    // asked); 301 and 302 do the same for a POST, as browsers do.
    let drop_body = match status.as_u16() {
        303 => true,
        301 | 302 => wire.method == http::Method::POST && !transport.follow_original_method,
        _ => false,
    };
    if drop_body {
        if wire.method != http::Method::HEAD {
            wire.method = http::Method::GET;
        }
        wire.body = None;
        for name in [
            "content-type",
            "content-length",
            "content-encoding",
            "content-language",
            "content-location",
            "transfer-encoding",
        ] {
            wire.headers.remove(name);
        }
    }
    if !same_origin(&wire.url, &next) {
        wire.headers.remove("cookie");
        wire.headers.remove("cookie2");
        wire.headers.remove("host");
        if !transport.follow_authorization {
            wire.headers.remove("authorization");
            wire.headers.remove("proxy-authorization");
        }
    }
    wire.url = next;
}

/// Sends the request once, and returns once the response headers arrive.
async fn round_trip(session: &Session, wire: &Wire) -> Result<Incoming> {
    match session {
        Session::Tcp(client) => {
            let response = client
                .execute(wire.request(client)?)
                .await
                .map_err(|e| Error::Http(describe(&e)))?;
            Ok(Incoming::from_reqwest(response))
        }
        Session::Quic(client) => {
            // Boxed: the future of a QUIC request is large.
            let reply =
                Box::pin(client.send(&wire.method, &wire.url, &wire.headers, wire.body.as_ref()))
                    .await?;
            Ok(Incoming::from_http3(reply, wire.url.clone()))
        }
    }
}

/// Sends the request and follows its redirects, as the transport says.
/// The timeout, if any, covers the whole chain. Returns the response that
/// ended it, and the redirects on the way.
async fn follow(
    client: &Session,
    mut wire: Wire,
    transport: &Transport,
    deadline: Option<tokio::time::Instant>,
) -> Result<(Incoming, Vec<Hop>)> {
    let mut hops: Vec<Hop> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    seen.insert((wire.method.clone(), wire.url.clone()));
    loop {
        let started = Instant::now();
        let incoming = until(deadline, round_trip(client, &wire)).await??;
        if !transport.follow_redirects || !is_redirect(incoming.status) {
            return Ok((incoming, hops));
        }
        let Some(location) = incoming
            .headers
            .get(http::header::LOCATION)
            .map(|value| String::from_utf8_lossy(value.as_bytes()).trim().to_owned())
        else {
            return Ok((incoming, hops));
        };
        let next = wire.url.join(&location).map_err(|e| {
            Error::Http(format!(
                "Stopped following redirects: {} at {} led to {location:?}, which is not a valid URL ({e})",
                incoming.status.as_u16(),
                wire.url
            ))
        })?;
        if !matches!(next.scheme(), "http" | "https") {
            return Err(Error::Http(format!(
                "Stopped following redirects: {} at {} led to {next}, and only http and https URLs can be sent",
                incoming.status.as_u16(),
                wire.url
            )));
        }
        if hops.len() >= transport.max_redirects as usize {
            return Err(Error::Http(format!(
                "Stopped following redirects: there are more than {}, the most the max_redirects setting allows. The last one led to {next}",
                transport.max_redirects
            )));
        }
        hops.push(Hop {
            status: incoming.status.as_u16(),
            url: wire.url.to_string(),
            time_ms: millis(started.elapsed()),
        });
        redirected(&mut wire, incoming.status, next, transport);
        if !seen.insert((wire.method.clone(), wire.url.clone())) {
            return Err(Error::Http(format!(
                "Stopped following redirects: the server sent {} {} back to a request already made, so the redirects go in a circle",
                wire.method, wire.url
            )));
        }
    }
}

/// Sends the request and reads the response, inside the timing scope.
async fn exchange(client: &Session, wire: Wire, transport: &Transport) -> Result<Response> {
    let started = Instant::now();
    let deadline = deadline(transport);
    let (mut response, hops) = follow(client, wire, transport, deadline).await?;
    let headers_ms = millis(started.elapsed());

    let status = response.status;
    let http_version = response.version.clone();
    let url = response.url.to_string();
    let remote_addr = response.remote_addr.take();
    let tls = response.tls.take();
    let headers: Vec<Header> = response
        .headers
        .iter()
        .map(|(name, value)| {
            Header::new(
                name.as_str(),
                String::from_utf8_lossy(value.as_bytes()).into_owned(),
            )
        })
        .collect();

    let limit = match usize::try_from(transport.max_body_bytes) {
        Ok(0) | Err(_) => usize::MAX,
        Ok(limit) => limit,
    };
    let event_stream = headers.iter().any(|h| {
        h.name.eq_ignore_ascii_case("content-type")
            && h.value
                .trim_start()
                .to_ascii_lowercase()
                .starts_with("text/event-stream")
    });
    let mut body = Vec::new();
    let mut truncated = false;
    let mut stream_open = false;
    loop {
        let chunk = match until(deadline, response.chunk()).await {
            Ok(chunk) => match chunk? {
                Some(chunk) => chunk,
                None => break,
            },
            // An event stream need not end; keep what arrived in time.
            Err(_) if event_stream => {
                stream_open = true;
                break;
            }
            Err(e) => return Err(e),
        };
        let room = limit - body.len();
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
        hops,
        remote_addr,
        timings: None,
        tls,
        http3_available: false,
        stream_open,
    })
}

/// Names a protocol version as people write it: `HTTP/2`, not `HTTP/2.0`.
fn version_text(version: reqwest::Version) -> String {
    match version {
        reqwest::Version::HTTP_09 => "HTTP/0.9".into(),
        reqwest::Version::HTTP_10 => "HTTP/1.0".into(),
        reqwest::Version::HTTP_11 => "HTTP/1.1".into(),
        reqwest::Version::HTTP_2 => "HTTP/2".into(),
        reqwest::Version::HTTP_3 => "HTTP/3".into(),
        other => format!("{other:?}"),
    }
}

/// Whether `Alt-Svc` headers offer HTTP/3 (`h3`, or a draft such as
/// `h3-29`).
fn advertises_http3(headers: &[Header]) -> bool {
    headers
        .iter()
        .filter(|h| h.name.eq_ignore_ascii_case("alt-svc"))
        .flat_map(|h| h.value.split(','))
        .filter_map(|entry| entry.split('=').next())
        .map(str::trim)
        .any(|protocol| protocol == "h3" || protocol.starts_with("h3-"))
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(value: &bool) -> bool {
    !*value
}

/// Parses a request into what is sent. Multipart bodies and files are
/// read here.
async fn wire(outgoing: &Outgoing) -> Result<Wire> {
    let method = http::Method::from_bytes(outgoing.method.trim().as_bytes())
        .map_err(|_| Error::Invalid(format!("{:?} is not a valid method", outgoing.method)))?;
    let url = url::Url::parse(&outgoing.url)
        .map_err(|e| Error::Invalid(format!("{:?} is not a valid URL: {e}", outgoing.url)))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(Error::Invalid(format!(
            "only http and https URLs can be sent, not {}",
            url.scheme()
        )));
    }
    let mut source = outgoing.headers.clone();
    // Multipart bodies are built here, with the boundary the request names,
    // so that whatever signed them hashed the same bytes.
    let body = match &outgoing.body {
        OutgoingBody::None => None,
        OutgoingBody::Bytes { data } => Some(bytes::Bytes::from(data.0.clone())),
        OutgoingBody::File { path } => {
            let data = tokio::fs::read(path)
                .await
                .map_err(|e| Error::io(path, e))?;
            Some(bytes::Bytes::from(data))
        }
        OutgoingBody::Multipart { parts } => {
            let boundary = crate::multipart::ensure_boundary(&mut source)?;
            let parts = parts.clone();
            let body =
                tokio::task::spawn_blocking(move || crate::multipart::encode(&parts, &boundary))
                    .await
                    .map_err(|e| {
                        Error::Http(format!("Could not build the multipart body: {e}"))
                    })??;
            Some(bytes::Bytes::from(body))
        }
    };
    let mut headers = http::HeaderMap::new();
    for header in &source {
        let name = http::HeaderName::from_bytes(header.name.trim().as_bytes())
            .map_err(|_| Error::Invalid(format!("{:?} is not a valid header name", header.name)))?;
        let value = http::HeaderValue::from_str(&header.value).map_err(|_| {
            Error::Invalid(format!(
                "the value of header {} contains characters headers cannot carry",
                header.name
            ))
        })?;
        headers.append(name, value);
    }
    Ok(Wire {
        method,
        url,
        headers,
        body,
    })
}

/// Response headers as a list, in order.
pub(crate) fn header_list(headers: &reqwest::header::HeaderMap) -> Vec<Header> {
    headers
        .iter()
        .map(|(name, value)| {
            Header::new(
                name.as_str(),
                String::from_utf8_lossy(value.as_bytes()).into_owned(),
            )
        })
        .collect()
}

/// Explains a transport error in one line, most general cause first.
pub(crate) fn describe(error: &reqwest::Error) -> String {
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
    round(duration.as_secs_f64() * 1000.0)
}

/// Rounds milliseconds to hundredths.
fn round(ms: f64) -> f64 {
    (ms * 100.0).round() / 100.0
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

    fn post(url: String, body: &str) -> Outgoing {
        Outgoing {
            method: "POST".into(),
            url,
            headers: vec![Header::new("Content-Type", "text/plain")],
            body: OutgoingBody::Bytes {
                data: Payload(body.as_bytes().to_vec()),
            },
        }
    }

    #[tokio::test]
    async fn a_post_becomes_a_get_after_301_302_and_303_unless_told_otherwise() {
        let server = test_server::spawn().await;
        let client = HttpClient::new();
        for code in [301, 302, 303] {
            let url = server.url(&format!("/go/{code}?to=/echo"));
            let echo = client
                .send(&post(url, "payload"), Transport::default())
                .await
                .unwrap()
                .json()
                .unwrap();
            assert_eq!(echo["method"], "GET", "{code}");
            assert_eq!(echo["body"], "", "{code}");
            assert!(echo["headers"].get("content-type").is_none(), "{code}");
        }
        let keep = Transport {
            follow_original_method: true,
            ..Transport::default()
        };
        for (code, method, body) in [
            (301, "POST", "payload"),
            (302, "POST", "payload"),
            // 303 always asks for the result with a GET.
            (303, "GET", ""),
            (307, "POST", "payload"),
            (308, "POST", "payload"),
        ] {
            let url = server.url(&format!("/go/{code}?to=/echo"));
            let echo = client
                .send(&post(url, "payload"), keep.clone())
                .await
                .unwrap()
                .json()
                .unwrap();
            assert_eq!(echo["method"], method, "{code}");
            assert_eq!(echo["body"], body, "{code}");
        }
        // 307 and 308 keep the method and body without being asked.
        let echo = client
            .send(
                &post(server.url("/go/307?to=/echo"), "payload"),
                Transport::default(),
            )
            .await
            .unwrap()
            .json()
            .unwrap();
        assert_eq!(
            (echo["method"].as_str(), echo["body"].as_str()),
            (Some("POST"), Some("payload"))
        );
    }

    #[test]
    fn a_head_request_stays_a_head_request_and_other_methods_follow_the_status() {
        let wire = |method: http::Method| Wire {
            method,
            url: url::Url::parse("http://a.test/start").unwrap(),
            headers: http::HeaderMap::new(),
            body: Some(bytes::Bytes::from_static(b"x")),
        };
        let next = || url::Url::parse("http://a.test/next").unwrap();
        let transport = Transport::default();
        let status = |code: u16| http::StatusCode::from_u16(code).unwrap();

        let mut head = wire(http::Method::HEAD);
        redirected(&mut head, status(303), next(), &transport);
        assert_eq!(head.method, http::Method::HEAD);
        assert!(head.body.is_none());

        let mut put = wire(http::Method::PUT);
        redirected(&mut put, status(302), next(), &transport);
        assert_eq!(
            put.method,
            http::Method::PUT,
            "only a POST turns into a GET"
        );
        assert!(put.body.is_some());
        redirected(&mut put, status(303), next(), &transport);
        assert_eq!(put.method, http::Method::GET);
        assert!(put.body.is_none());
    }

    #[tokio::test]
    async fn credentials_stay_behind_at_another_host_unless_told_otherwise() {
        let server = test_server::spawn().await;
        let client = HttpClient::new();
        let mut outgoing = get(server.url(&format!(
            "/go/302?to=http://localhost:{}/echo",
            server.port()
        )));
        outgoing.headers = vec![
            Header::new("Authorization", "Bearer secret"),
            Header::new("Proxy-Authorization", "Basic c2VjcmV0"),
            Header::new("Cookie", "mine=1"),
            Header::new("X-Keep", "yes"),
        ];
        let echo = client
            .send(&outgoing, Transport::default())
            .await
            .unwrap()
            .json()
            .unwrap();
        for dropped in ["authorization", "proxy-authorization", "cookie"] {
            assert!(echo["headers"].get(dropped).is_none(), "{dropped}: {echo}");
        }
        assert_eq!(echo["headers"]["x-keep"], "yes");

        let echo = client
            .send(
                &outgoing,
                Transport {
                    follow_authorization: true,
                    ..Transport::default()
                },
            )
            .await
            .unwrap()
            .json()
            .unwrap();
        assert_eq!(echo["headers"]["authorization"], "Bearer secret");
        assert_eq!(echo["headers"]["proxy-authorization"], "Basic c2VjcmV0");
        assert!(echo["headers"].get("cookie").is_none(), "{echo}");

        // The same host keeps everything.
        outgoing.url = server.url("/go/302?to=/echo");
        let echo = client
            .send(&outgoing, Transport::default())
            .await
            .unwrap()
            .json()
            .unwrap();
        assert_eq!(echo["headers"]["authorization"], "Bearer secret");
        assert_eq!(echo["headers"]["cookie"], "mine=1");
    }

    #[tokio::test]
    async fn the_hops_are_listed_and_the_timings_still_add_up() {
        let server = test_server::spawn().await;
        let client = HttpClient::new();
        let response = client
            .send(&get(server.url("/count/3")), Transport::default())
            .await
            .unwrap();
        assert_eq!(response.text(), "done");
        assert!(response.url.ends_with("/count/0"));
        let hops: Vec<(u16, &str)> = response
            .hops
            .iter()
            .map(|hop| (hop.status, hop.url.rsplit('/').next().unwrap()))
            .collect();
        assert_eq!(hops, [(302, "3"), (302, "2"), (302, "1")]);
        assert!(response.hops.iter().all(|hop| hop.time_ms > 0.0));
        assert!(response.hops.iter().map(|hop| hop.time_ms).sum::<f64>() <= response.headers_ms);

        let timings = response.timings.unwrap();
        let parts = timings.dns_ms
            + timings.connect_ms.unwrap()
            + timings.first_byte_ms
            + timings.download_ms;
        assert!((parts - response.time_ms).abs() < 0.1, "{timings:?}");

        let plain = client
            .send(&get(server.url("/echo")), Transport::default())
            .await
            .unwrap();
        assert!(plain.hops.is_empty());
        let json = serde_json::to_value(&plain).unwrap();
        assert!(json.get("hops").is_none(), "an empty list is not written");
    }

    #[tokio::test]
    async fn too_many_redirects_and_circles_are_named() {
        let server = test_server::spawn().await;
        let client = HttpClient::new();
        let few = Transport {
            max_redirects: 2,
            ..Transport::default()
        };
        let error = client
            .send(&get(server.url("/count/5")), few)
            .await
            .unwrap_err();
        let message = error.to_string();
        assert!(
            message.starts_with("Stopped following redirects") && message.contains("max_redirects"),
            "{message}"
        );
        let enough = Transport {
            max_redirects: 5,
            ..Transport::default()
        };
        let response = client
            .send(&get(server.url("/count/5")), enough)
            .await
            .unwrap();
        assert_eq!(response.hops.len(), 5);

        let error = client
            .send(&get(server.url("/loop/a")), Transport::default())
            .await
            .unwrap_err();
        let message = error.to_string();
        assert!(
            message.starts_with("Stopped following redirects") && message.contains("circle"),
            "{message}"
        );

        let error = client
            .send(
                &get(server.url("/go/302?to=ftp://example.com/file")),
                Transport::default(),
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("only http and https"), "{error}");
    }

    #[tokio::test]
    async fn cookies_set_on_every_hop_are_stored() {
        let server = test_server::spawn().await;
        let client = HttpClient::new();
        let url =
            server.url("/go/302?cookie=first%3D1&to=/go/302%3Fcookie%3Dsecond%3D2%26to%3D/echo");
        let echo = client
            .send(&get(url), Transport::default())
            .await
            .unwrap()
            .json()
            .unwrap();
        // The second hop already sent the first one's cookie back.
        let mut sent: Vec<&str> = echo["headers"]["cookie"]
            .as_str()
            .unwrap()
            .split("; ")
            .collect();
        sent.sort_unstable();
        assert_eq!(sent, ["first=1", "second=2"]);
        let names: Vec<String> = client.cookies().into_iter().map(|c| c.name).collect();
        assert_eq!(names, ["first", "second"]);
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
    async fn an_event_stream_keeps_what_arrived_before_the_timeout() {
        let server = test_server::spawn().await;
        let response = HttpClient::new()
            .send(
                &get(server.url("/sse/forever")),
                Transport {
                    timeout_ms: 300,
                    ..Transport::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(response.status, 200);
        assert!(response.stream_open);
        assert!(
            response.text().contains("data: tick 0"),
            "{}",
            response.text()
        );

        let finished = HttpClient::new()
            .send(&get(server.url("/sse")), Transport::default())
            .await
            .unwrap();
        assert!(!finished.stream_open);
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

    fn with_version(http_version: HttpVersion) -> Transport {
        Transport {
            http_version,
            ..Transport::default()
        }
    }

    /// Trusts the test CA, so TLS servers can be verified.
    fn trusting() -> Transport {
        Transport {
            ca_certs: vec![test_server::pki().ca_pem.clone().into_bytes()],
            ..Transport::default()
        }
    }

    #[tokio::test]
    async fn the_http_version_follows_the_setting() {
        let server = test_server::spawn().await;
        let client = HttpClient::new();
        for (version, expected) in [
            (HttpVersion::Auto, "HTTP/1.1"),
            (HttpVersion::Http1, "HTTP/1.1"),
            (HttpVersion::Http2, "HTTP/2"),
        ] {
            let response = client
                .send(&get(server.url("/echo")), with_version(version))
                .await
                .unwrap();
            assert_eq!(response.http_version, expected, "{version:?}");
            let echo = response.json().unwrap();
            assert_eq!(
                echo["version"],
                expected.replace("HTTP/2", "HTTP/2.0"),
                "{version:?}"
            );
        }

        // Over TLS, auto takes HTTP/2 when the server offers it.
        let tls = test_server::spawn_tls(false).await;
        let response = client
            .send(&get(tls.url("/echo")), trusting())
            .await
            .unwrap();
        assert_eq!(response.http_version, "HTTP/2");
        let response = client
            .send(
                &get(tls.url("/echo")),
                Transport {
                    http_version: HttpVersion::Http1,
                    ..trusting()
                },
            )
            .await
            .unwrap();
        assert_eq!(response.http_version, "HTTP/1.1");
    }

    #[tokio::test]
    async fn http3_goes_over_quic() {
        let server = test_server::spawn_h3();
        let client = HttpClient::new();
        for transport in [
            Transport {
                http_version: HttpVersion::Http3,
                ..trusting()
            },
            Transport {
                http_version: HttpVersion::Http3,
                verify_tls: false,
                ..Transport::default()
            },
        ] {
            let response = client
                .send(&get(server.url("/echo/quic")), transport)
                .await
                .unwrap();
            assert_eq!(response.status, 200);
            assert_eq!(response.http_version, "HTTP/3");
            let echo = response.json().unwrap();
            assert_eq!(echo["version"], "HTTP/3.0");
            assert_eq!(echo["path"], "/echo/quic");
            assert_eq!(response.timings.unwrap().connect_ms, None);
        }
    }

    #[tokio::test]
    async fn http3_explains_a_server_without_it() {
        // Plain TCP: nothing answers QUIC on this port.
        let server = test_server::spawn().await;
        let error = HttpClient::new()
            .send(
                &get(format!("https://127.0.0.1:{}/echo", server.port())),
                Transport {
                    http_version: HttpVersion::Http3,
                    timeout_ms: 2000,
                    verify_tls: false,
                    ..Transport::default()
                },
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("HTTP/3"), "{error}");

        let error = HttpClient::new()
            .send(&get(server.url("/echo")), with_version(HttpVersion::Http3))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("https://"), "{error}");
    }

    fn h3(extra: Transport) -> Transport {
        Transport {
            http_version: HttpVersion::Http3,
            ..extra
        }
    }

    #[tokio::test]
    async fn http3_sends_headers_and_bodies_and_reports_the_connection() {
        let server = test_server::spawn_h3();
        let client = HttpClient::new();
        let mut request = post(server.url("/echo"), "over quic");
        request.headers.push(Header::new("X-Custom", "yes"));
        let first = client.send(&request, h3(trusting())).await.unwrap();
        let echo = first.json().unwrap();
        assert_eq!(echo["method"], "POST");
        assert_eq!(echo["body"], "over quic");
        assert_eq!(echo["headers"]["x-custom"], "yes");
        assert_eq!(echo["headers"]["content-type"], "text/plain");
        assert_eq!(echo["headers"]["content-length"], "9");

        let timings = first.timings.unwrap();
        assert!(!timings.reused);
        assert_eq!(timings.connect_ms, None);
        let parts = timings.dns_ms + timings.first_byte_ms + timings.download_ms;
        assert!((parts - first.time_ms).abs() < 0.1, "{timings:?}");
        assert_eq!(
            first.remote_addr,
            Some(format!("127.0.0.1:{}", server.port()))
        );
        let tls = first.tls.unwrap();
        assert_eq!(tls.version.as_deref(), Some("TLSv1.3"));
        assert!(
            tls.certificate
                .unwrap()
                .subject
                .contains("getman test server"),
            "the certificate is reported"
        );

        let second = client
            .send(&get(server.url("/echo")), h3(trusting()))
            .await
            .unwrap();
        assert!(
            second.timings.unwrap().reused,
            "the QUIC connection is kept"
        );
    }

    #[tokio::test]
    async fn http3_checks_certificates_and_presents_client_ones() {
        let server = test_server::spawn_h3_with(true);
        let client = HttpClient::new();
        let url = server.url("/echo");
        // The system does not trust the test CA.
        assert!(
            client
                .send(&get(url.clone()), h3(Transport::default()))
                .await
                .is_err()
        );
        // The server wants a certificate.
        assert!(
            client
                .send(&get(url.clone()), h3(trusting()))
                .await
                .is_err()
        );

        let pki = test_server::pki();
        let pem = format!("{}{}", pki.client_cert_pem, pki.client_key_pem);
        let identity = ClientIdentity::from_pem(pem.into_bytes(), "client.pem").unwrap();
        let response = client
            .send(
                &get(url),
                h3(Transport {
                    client_identity: Some(identity),
                    ..trusting()
                }),
            )
            .await
            .unwrap();
        assert_eq!(response.status, 200);
    }

    #[tokio::test]
    async fn http3_follows_redirects_and_keeps_cookies_from_every_hop() {
        let server = test_server::spawn_h3();
        let client = HttpClient::new();
        let response = client
            .send(&get(server.url("/redirect")), h3(trusting()))
            .await
            .unwrap();
        assert_eq!(response.http_version, "HTTP/3");
        assert_eq!(response.hops.len(), 1);
        assert_eq!(response.hops[0].status, 302);
        assert!(response.url.ends_with("/echo/after"));
        let echo = response.json().unwrap();
        assert_eq!(echo["headers"]["cookie"], "hop=1", "the redirect's cookie");

        client
            .send(&get(server.url("/cookie")), h3(trusting()))
            .await
            .unwrap();
        let names: Vec<String> = client.cookies().into_iter().map(|c| c.name).collect();
        assert_eq!(names, ["hop", "jar"]);
        let echo = client
            .send(&get(server.url("/echo")), h3(trusting()))
            .await
            .unwrap()
            .json()
            .unwrap();
        let mut sent: Vec<&str> = echo["headers"]["cookie"]
            .as_str()
            .unwrap()
            .split("; ")
            .collect();
        sent.sort_unstable();
        assert_eq!(sent, ["hop=1", "jar=1"]);

        let jarless = Transport {
            cookies: false,
            ..trusting()
        };
        let echo = client
            .send(&get(server.url("/echo")), h3(jarless))
            .await
            .unwrap()
            .json()
            .unwrap();
        assert!(echo["headers"].get("cookie").is_none(), "{echo}");

        let stopped = client
            .send(
                &get(server.url("/redirect")),
                h3(Transport {
                    follow_redirects: false,
                    ..trusting()
                }),
            )
            .await
            .unwrap();
        assert_eq!(stopped.status, 302);
        assert_eq!(stopped.header("location"), Some("/echo/after"));
    }

    #[tokio::test]
    async fn http3_decodes_compressed_bodies() {
        let server = test_server::spawn_h3();
        let client = HttpClient::new();
        for encoding in ["gzip", "deflate", "br", "zstd"] {
            let response = client
                .send(
                    &get(server.url(&format!("/encoded/{encoding}"))),
                    h3(trusting()),
                )
                .await
                .unwrap();
            assert_eq!(response.text(), "hello from h3", "{encoding}");
            assert!(response.header("content-encoding").is_none(), "{encoding}");
        }
    }

    #[tokio::test]
    async fn http3_stops_at_the_size_limit_and_at_the_timeout() {
        let server = test_server::spawn_h3();
        let client = HttpClient::new();
        let limited = Transport {
            max_body_bytes: 1024 * 1024,
            ..trusting()
        };
        let response = client
            .send(&get(server.url("/big/2097152")), h3(limited))
            .await
            .unwrap();
        assert!(response.truncated);
        assert_eq!(response.body.0.len(), 1024 * 1024);
        let whole = client
            .send(&get(server.url("/big/2097152")), h3(trusting()))
            .await
            .unwrap();
        assert!(!whole.truncated);
        assert_eq!(whole.body.0.len(), 2 * 1024 * 1024);

        let error = client
            .send(
                &get(server.url("/slow")),
                h3(Transport {
                    timeout_ms: 200,
                    ..trusting()
                }),
            )
            .await
            .unwrap_err();
        assert!(
            error.to_string().starts_with("The request timed out"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn http3_is_refused_through_a_proxy() {
        let error = HttpClient::new()
            .send(
                &get("https://127.0.0.1:9/".into()),
                h3(Transport {
                    proxy: ProxyRoute::Url {
                        url: "http://127.0.0.1:3128".into(),
                        bypass: String::new(),
                    },
                    ..Transport::default()
                }),
            )
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("cannot go through a proxy"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn alt_svc_offers_http3_without_switching() {
        let server = test_server::spawn().await;
        let client = HttpClient::new();
        let response = client
            .send(&get(server.url("/alt-svc")), Transport::default())
            .await
            .unwrap();
        assert!(response.http3_available);
        assert_eq!(response.http_version, "HTTP/1.1");
        let pinned = client
            .send(
                &get(server.url("/alt-svc")),
                with_version(HttpVersion::Http1),
            )
            .await
            .unwrap();
        assert!(!pinned.http3_available);
        let plain = client
            .send(&get(server.url("/echo")), Transport::default())
            .await
            .unwrap();
        assert!(!plain.http3_available);
    }

    #[tokio::test]
    async fn header_names_go_out_in_title_case_unless_turned_off() {
        let (addr, mut heads) =
            test_server::spawn_raw("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n").await;
        let client = HttpClient::new();
        let mut outgoing = get(format!("http://{addr}/"));
        outgoing.headers.push(Header::new("x-custom-thing", "1"));
        client.send(&outgoing, Transport::default()).await.unwrap();
        let head = heads.recv().await.unwrap();
        assert!(head.contains("\r\nX-Custom-Thing: 1\r\n"), "{head}");
        client
            .send(
                &outgoing,
                Transport {
                    title_case_headers: false,
                    ..Transport::default()
                },
            )
            .await
            .unwrap();
        let head = heads.recv().await.unwrap();
        assert!(head.contains("\r\nx-custom-thing: 1\r\n"), "{head}");
    }

    #[tokio::test]
    async fn zstd_bodies_are_decoded() {
        let server = test_server::spawn().await;
        let client = HttpClient::new();
        let response = client
            .send(&get(server.url("/zstd")), Transport::default())
            .await
            .unwrap();
        assert_eq!(response.text(), "zstd works");
        let echo = client
            .send(&get(server.url("/echo")), Transport::default())
            .await
            .unwrap()
            .json()
            .unwrap();
        assert!(
            echo["headers"]["accept-encoding"]
                .as_str()
                .unwrap()
                .contains("zstd"),
            "{echo}"
        );
    }

    #[tokio::test]
    async fn bodies_stop_at_the_size_limit() {
        let server = test_server::spawn().await;
        let client = HttpClient::new();
        let limited = Transport {
            max_body_bytes: 1024 * 1024,
            ..Transport::default()
        };
        let response = client
            .send(&get(server.url("/bytes/2097152")), limited)
            .await
            .unwrap();
        assert!(response.truncated);
        assert_eq!(response.body.0.len(), 1024 * 1024);
        let unlimited = Transport {
            max_body_bytes: 0,
            ..Transport::default()
        };
        let response = client
            .send(&get(server.url("/bytes/2097152")), unlimited)
            .await
            .unwrap();
        assert!(!response.truncated);
        assert_eq!(response.body.0.len(), 2 * 1024 * 1024);
    }

    #[tokio::test]
    async fn timings_add_up_and_show_reused_connections() {
        let server = test_server::spawn().await;
        let client = HttpClient::new();
        let first = client
            .send(&get(server.url("/echo")), Transport::default())
            .await
            .unwrap();
        let timings = first.timings.unwrap();
        assert!(!timings.reused);
        assert!(timings.connect_ms.unwrap() > 0.0, "{timings:?}");
        assert_eq!(timings.dns_ms, 0.0, "an IP address needs no lookup");
        let parts = timings.dns_ms
            + timings.connect_ms.unwrap()
            + timings.first_byte_ms
            + timings.download_ms;
        assert!((parts - first.time_ms).abs() < 0.1, "{timings:?} {first:?}");

        let second = client
            .send(&get(server.url("/echo")), Transport::default())
            .await
            .unwrap();
        let timings = second.timings.unwrap();
        assert!(timings.reused, "{timings:?}");
        assert_eq!(timings.connect_ms, Some(0.0));

        let named = client
            .send(
                &get(format!("http://localhost:{}/echo", server.port())),
                Transport::default(),
            )
            .await
            .unwrap();
        assert!(!named.timings.unwrap().reused);
    }

    #[tokio::test]
    async fn tls_shows_the_certificate_and_trusts_extra_authorities() {
        let server = test_server::spawn_tls(false).await;
        let client = HttpClient::new();
        let error = client
            .send(&get(server.url("/echo")), Transport::default())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("certificate"), "{error}");

        let response = client
            .send(&get(server.url("/echo")), trusting())
            .await
            .unwrap();
        let tls = response.tls.unwrap();
        assert_eq!(tls.version.as_deref(), Some("TLSv1.3"));
        let certificate = tls.certificate.unwrap();
        assert!(
            certificate.subject.contains("getman test server"),
            "{certificate:?}"
        );
        assert!(certificate.issuer.contains("Getman Test CA"));
        assert!(certificate.names.contains(&"127.0.0.1".to_owned()));
        assert!(certificate.not_after > crate::time::now_millis());

        let insecure = client
            .send(
                &get(server.url("/echo")),
                Transport {
                    verify_tls: false,
                    ..Transport::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(insecure.status, 200);
    }

    #[tokio::test]
    async fn client_certificates_are_presented() {
        let server = test_server::spawn_tls(true).await;
        let pki = test_server::pki();
        let client = HttpClient::new();
        assert!(
            client
                .send(&get(server.url("/echo")), trusting())
                .await
                .is_err(),
            "the server asks for a certificate"
        );

        let pem = format!("{}{}", pki.client_cert_pem, pki.client_key_pem);
        let identities = [
            ClientIdentity::from_pem(pem.into_bytes(), "client.pem").unwrap(),
            ClientIdentity::from_pkcs12(&pki.client_p12, test_server::P12_PASSPHRASE, "client.p12")
                .unwrap(),
        ];
        for identity in identities {
            let response = client
                .send(
                    &get(server.url("/echo")),
                    Transport {
                        client_identity: Some(identity),
                        ..trusting()
                    },
                )
                .await
                .unwrap();
            assert_eq!(response.status, 200);
        }

        let wrong = ClientIdentity::from_pkcs12(&pki.client_p12, "nope", "client.p12");
        assert!(
            wrong.unwrap_err().to_string().contains("passphrase"),
            "a wrong passphrase is named"
        );
        let missing_key =
            ClientIdentity::from_pem(pki.client_cert_pem.clone().into_bytes(), "cert.pem");
        assert!(missing_key.unwrap_err().to_string().contains("private key"));
    }

    #[tokio::test]
    async fn proxies_carry_credentials_and_honour_the_bypass_list() {
        let (proxy, mut heads) = test_server::spawn_raw(
            "HTTP/1.1 200 OK\r\nContent-Length: 9\r\nConnection: close\r\n\r\nvia proxy",
        )
        .await;
        let server = test_server::spawn().await;
        let client = HttpClient::new();
        let through = |bypass: &str| Transport {
            proxy: ProxyRoute::Url {
                url: format!("http://ada:p%40ss@{proxy}"),
                bypass: bypass.to_owned(),
            },
            ..Transport::default()
        };

        let response = client
            .send(&get("http://api.example.invalid/hello".into()), through(""))
            .await
            .unwrap();
        assert_eq!(response.text(), "via proxy");
        let head = heads.recv().await.unwrap();
        assert!(
            head.starts_with("GET http://api.example.invalid/hello HTTP/1.1\r\n"),
            "{head}"
        );
        let credentials = BASE64.encode("ada:p@ss");
        assert!(
            head.contains(&format!("Proxy-Authorization: Basic {credentials}\r\n")),
            "{head}"
        );

        let bypassed = client
            .send(&get(server.url("/echo")), through("127.0.0.1"))
            .await
            .unwrap();
        assert_eq!(bypassed.json().unwrap()["path"], "/echo");

        let direct = client
            .send(
                &get(server.url("/echo")),
                Transport {
                    proxy: ProxyRoute::Off,
                    ..Transport::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(direct.json().unwrap()["path"], "/echo");
        assert!(heads.try_recv().is_err(), "nothing else went to the proxy");
    }

    #[tokio::test]
    async fn requests_can_leave_the_cookie_jar_alone() {
        let server = test_server::spawn().await;
        let client = HttpClient::new();
        let without = Transport {
            cookies: false,
            ..Transport::default()
        };
        client
            .send(&get(server.url("/cookies/set?flavor=oat")), without.clone())
            .await
            .unwrap();
        assert_eq!(client.cookies(), [], "nothing was stored");

        client
            .send(
                &get(server.url("/cookies/set?flavor=oat")),
                Transport::default(),
            )
            .await
            .unwrap();
        let echo = client
            .send(&get(server.url("/echo")), without)
            .await
            .unwrap()
            .json()
            .unwrap();
        assert!(echo["headers"].get("cookie").is_none(), "{echo}");
    }

    #[tokio::test]
    async fn the_jar_survives_in_its_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(".getman").join(COOKIE_FILE);
        let server = test_server::spawn().await;
        let client = HttpClient::with_cookie_file(&file);
        client
            .send(
                &get(server.url("/cookies/set?flavor=oat")),
                Transport::default(),
            )
            .await
            .unwrap();
        assert!(file.exists());

        let again = HttpClient::with_cookie_file(&file);
        let cookies = again.cookies();
        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0].value, "oat");
        let echo = again
            .send(&get(server.url("/echo")), Transport::default())
            .await
            .unwrap()
            .json()
            .unwrap();
        assert_eq!(echo["headers"]["cookie"], "flavor=oat");

        // A cookie that expired while Getman was closed is not read back.
        let text = std::fs::read_to_string(&file).unwrap();
        let mut stored: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
        stored[0]["expires"] = serde_json::json!({ "AtUtc": "2001-01-01T00:00:00Z" });
        std::fs::write(&file, serde_json::to_string(&stored).unwrap()).unwrap();
        assert_eq!(HttpClient::with_cookie_file(&file).cookies(), []);
    }

    #[tokio::test]
    async fn cookies_can_be_added_and_edited() {
        let server = test_server::spawn().await;
        let client = HttpClient::new();
        let mut cookie = CookieInfo {
            domain: "127.0.0.1".into(),
            path: "/".into(),
            name: "session".into(),
            value: "abc".into(),
            expires: Some(crate::time::now_millis() + 3_600_000),
            secure: false,
            http_only: true,
            host_only: true,
        };
        client.set_cookie(&cookie).unwrap();
        cookie.value = "def".into();
        client.set_cookie(&cookie).unwrap();
        let cookies = client.cookies();
        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0].value, "def");
        assert!(cookies[0].host_only && cookies[0].http_only);
        assert!(cookies[0].expires.is_some());
        let echo = client
            .send(&get(server.url("/echo")), Transport::default())
            .await
            .unwrap()
            .json()
            .unwrap();
        assert_eq!(echo["headers"]["cookie"], "session=def");

        cookie.expires = Some(1000);
        assert!(client.set_cookie(&cookie).is_err());
        cookie.expires = None;
        cookie.domain = String::new();
        assert!(client.set_cookie(&cookie).is_err());
    }
}
