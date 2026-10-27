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
    clients: Mutex<HashMap<ClientKey, reqwest::Client>>,
    /// Where the jar is kept between runs, if anywhere, and what was last
    /// written there.
    cookie_file: Option<(PathBuf, Mutex<String>)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(clippy::struct_excessive_bools)]
struct ClientKey {
    follow_redirects: bool,
    max_redirects: u32,
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
            follow_redirects: transport.follow_redirects,
            max_redirects: transport.max_redirects,
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

    fn client(&self, transport: &Transport) -> Result<reqwest::Client> {
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

    fn build_client(&self, key: &ClientKey) -> Result<reqwest::Client> {
        let redirect = if key.follow_redirects {
            reqwest::redirect::Policy::limited(key.max_redirects as usize)
        } else {
            reqwest::redirect::Policy::none()
        };
        let mut builder = reqwest::Client::builder()
            .redirect(redirect)
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
            HttpVersion::Auto => builder,
            HttpVersion::Http1 => builder.http1_only(),
            HttpVersion::Http2 => builder.http2_prior_knowledge(),
            #[cfg(reqwest_unstable)]
            HttpVersion::Http3 => builder.http3_prior_knowledge(),
            #[cfg(not(reqwest_unstable))]
            HttpVersion::Http3 => return Err(Error::Invalid(NO_HTTP3.into())),
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
        builder.build().map_err(|e| Error::Http(describe(&e)))
    }

    /// Sends a request and reads the whole response, up to the size
    /// limit.
    pub async fn send(&self, outgoing: &Outgoing, transport: Transport) -> Result<Response> {
        check_route(outgoing, &transport)?;
        let client = self.client(&transport)?;
        let request = build(&client, outgoing, &transport).await?;
        let probe = Arc::new(Probe::default());
        let exchange = timing::measure(Arc::clone(&probe), exchange(&client, request, &transport));
        let result = exchange.await;
        if transport.cookies {
            self.save_cookies_quietly();
        }
        let mut response = result.map_err(|e| {
            let mut message = describe(&e);
            if transport.http_version == HttpVersion::Http3 {
                message.push_str(
                    ". The server did not answer over HTTP/3 (QUIC, on UDP); if it does not support HTTP/3, set the HTTP version to auto",
                );
            }
            Error::Http(message)
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
            reused: if over_quic {
                dns.is_zero() && !is_ip_host(&response.url)
            } else {
                connects == 0
            },
        });
        response.http3_available = transport.http_version == HttpVersion::Auto
            && !over_quic
            && advertises_http3(&response.headers);
        Ok(response)
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
        std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
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

/// Said when HTTP/3 is asked of a build made without reqwest's unstable
/// cfg (see `.cargo/config.toml`).
const NO_HTTP3: &str = "this build of Getman has no HTTP/3 support; build it with RUSTFLAGS=\"--cfg reqwest_unstable\"";

/// Refuses combinations that cannot work before trying them.
fn check_route(outgoing: &Outgoing, transport: &Transport) -> Result<()> {
    if transport.http_version != HttpVersion::Http3 {
        return Ok(());
    }
    if !cfg!(reqwest_unstable) {
        return Err(Error::Invalid(NO_HTTP3.into()));
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

/// Sends the request and reads the response, inside the timing scope.
async fn exchange(
    client: &reqwest::Client,
    request: reqwest::Request,
    transport: &Transport,
) -> std::result::Result<Response, reqwest::Error> {
    let started = Instant::now();
    let mut response = client.execute(request).await?;
    let headers_ms = millis(started.elapsed());

    let status = response.status();
    let http_version = version_text(response.version());
    let url = response.url().to_string();
    let remote_addr = response.remote_addr().map(|a| a.to_string());
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

    let limit = match usize::try_from(transport.max_body_bytes) {
        Ok(0) | Err(_) => usize::MAX,
        Ok(limit) => limit,
    };
    let mut body = Vec::new();
    let mut truncated = false;
    while let Some(chunk) = response.chunk().await? {
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
        remote_addr,
        timings: None,
        tls,
        http3_available: false,
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

fn is_ip_host(url: &str) -> bool {
    matches!(
        url::Url::parse(url)
            .ok()
            .and_then(|u| u.host().map(|h| h.to_owned())),
        Some(url::Host::Ipv4(_) | url::Host::Ipv6(_))
    )
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(value: &bool) -> bool {
    !*value
}

async fn build(
    client: &reqwest::Client,
    outgoing: &Outgoing,
    transport: &Transport,
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
    #[cfg(reqwest_unstable)]
    if transport.http_version == HttpVersion::Http3 {
        builder = builder.version(reqwest::Version::HTTP_3);
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

    #[cfg(reqwest_unstable)]
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

    #[cfg(reqwest_unstable)]
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

    #[cfg(not(reqwest_unstable))]
    #[tokio::test]
    async fn builds_without_http3_say_so() {
        let error = HttpClient::new()
            .send(
                &get("https://127.0.0.1:9/".into()),
                with_version(HttpVersion::Http3),
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("no HTTP/3 support"), "{error}");
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
