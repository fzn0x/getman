//! Reaching a server through a proxy, for the connections Getman opens
//! itself rather than through reqwest: gRPC calls, and WebSocket and
//! Socket.IO connections. They go through an HTTP or HTTPS proxy with
//! `CONNECT` and optional Basic credentials, or through a SOCKS5 proxy
//! with an optional user name and password, and the proxy only relays
//! bytes. `socks5://` looks the server's name up here, `socks5h://` leaves
//! that to the proxy.
//!
//! Which proxy a connection uses follows the HTTP client: the request's
//! proxy setting, or with the default the system's settings and the
//! `HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY` and `NO_PROXY` variables, and a
//! bypass list of hosts that are reached directly.
//!
//! Errors are lowercase reasons, to follow "Could not connect to ...";
//! [`failure`] writes that sentence.

use std::future::Future;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use hyper_util::client::proxy::matcher::{Intercept, Matcher};
use rustls::ClientConfig;
use rustls::pki_types::ServerName;
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use tokio::net::TcpStream;

use crate::http::ProxyRoute;

/// Most bytes of a proxy's answer to `CONNECT` that are read.
const MAX_ANSWER: usize = 16 * 1024;

/// A byte stream a connection runs over: a socket, or a tunnel and TLS
/// over one.
pub(crate) trait Io: AsyncRead + AsyncWrite + Unpin + Send {}

impl<T: AsyncRead + AsyncWrite + Unpin + Send> Io for T {}

/// The server a connection leads to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Destination<'a> {
    /// Whether the connection to the server is encrypted, which decides
    /// whether `HTTPS_PROXY` or `HTTP_PROXY` applies.
    pub(crate) tls: bool,
    /// Host name or IP address, without brackets.
    pub(crate) host: &'a str,
    /// TCP port.
    pub(crate) port: u16,
}

impl Destination<'_> {
    /// `host:port`, with brackets around an IPv6 address.
    pub(crate) fn authority(&self) -> String {
        authority(self.host, self.port)
    }
}

fn authority(host: &str, port: u16) -> String {
    if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

/// How a proxy is spoken to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// `CONNECT` over plain TCP.
    Http,
    /// `CONNECT` inside TLS to the proxy.
    Https,
    /// SOCKS5; `remote_dns` hands the proxy the host name to look up.
    Socks5 { remote_dns: bool },
}

/// A proxy to go through, with its credentials.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Proxy {
    kind: Kind,
    host: String,
    port: u16,
    /// The `Proxy-Authorization` header value, for HTTP proxies.
    basic: Option<String>,
    /// User name and password, for SOCKS5 proxies.
    login: Option<(String, String)>,
}

impl std::fmt::Debug for Proxy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Credentials stay out of logs.
        f.debug_struct("Proxy")
            .field("kind", &self.kind)
            .field("host", &self.host)
            .field("port", &self.port)
            .finish_non_exhaustive()
    }
}

impl Proxy {
    /// `host:port`, for messages.
    fn authority(&self) -> String {
        authority(&self.host, self.port)
    }

    fn from_intercept(intercept: &Intercept) -> Result<Self, String> {
        let uri = intercept.uri();
        let scheme = uri.scheme_str().unwrap_or("http");
        let (kind, default_port) = match scheme {
            "http" => (Kind::Http, 80),
            "https" => (Kind::Https, 443),
            "socks5" => (Kind::Socks5 { remote_dns: false }, 1080),
            "socks5h" => (Kind::Socks5 { remote_dns: true }, 1080),
            other => {
                return Err(format!(
                    "the proxy URL starts with {other}://; use http://, https://, socks5:// or socks5h://"
                ));
            }
        };
        let host = uri
            .host()
            .ok_or("the proxy URL has no host")?
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_owned();
        Ok(Self {
            kind,
            host,
            port: uri.port_u16().unwrap_or(default_port),
            basic: intercept
                .basic_auth()
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned),
            login: intercept
                .raw_auth()
                .map(|(user, password)| (user.to_owned(), password.to_owned())),
        })
    }
}

/// The proxy a connection to `destination` goes through, or `None` to
/// connect directly.
pub(crate) fn route(
    destination: Destination<'_>,
    proxy: &ProxyRoute,
) -> Result<Option<Proxy>, String> {
    match proxy {
        ProxyRoute::Off => Ok(None),
        ProxyRoute::System => choose(destination, &Matcher::from_system()),
        ProxyRoute::Url { url, bypass } => choose(
            destination,
            &Matcher::builder().all(url.as_str()).no(bypass).build(),
        ),
    }
}

fn choose(destination: Destination<'_>, matcher: &Matcher) -> Result<Option<Proxy>, String> {
    let scheme = if destination.tls { "https" } else { "http" };
    let Ok(uri) = format!("{scheme}://{}/", destination.authority()).parse::<http::Uri>() else {
        return Ok(None);
    };
    matcher
        .intercept(&uri)
        .map(|intercept| Proxy::from_intercept(&intercept))
        .transpose()
}

/// "Could not connect to ..." for a connection to `destination` that
/// failed for `reason`, naming the proxy if it went through one.
pub(crate) fn failure(destination: Destination<'_>, proxy: Option<&Proxy>, reason: &str) -> String {
    match proxy {
        Some(proxy) => format!(
            "Could not connect to {} through the proxy {}: {reason}",
            destination.authority(),
            proxy.authority()
        ),
        None => format!("Could not connect to {}: {reason}", destination.authority()),
    }
}

/// Opens a stream to `destination`: straight to it, or to `proxy` and
/// through the tunnel it sets up. The stream is not yet encrypted for a
/// TLS destination; see [`secure`]. `proxy_tls` gives the TLS settings for
/// talking to an HTTPS proxy. `timeout` bounds connecting, the TLS
/// handshake with the proxy and the tunnel, each on its own.
pub(crate) async fn open(
    destination: Destination<'_>,
    proxy: Option<&Proxy>,
    proxy_tls: impl FnOnce() -> Result<Arc<ClientConfig>, String>,
    timeout: Option<Duration>,
) -> Result<Box<dyn Io>, String> {
    let (host, port) = match proxy {
        Some(proxy) => (proxy.host.as_str(), proxy.port),
        None => (destination.host, destination.port),
    };
    let tcp = bounded(
        timeout,
        |seconds| format!("no answer within {seconds} s"),
        async {
            TcpStream::connect((host, port))
                .await
                .map_err(|e| e.to_string())
        },
    )
    .await?;
    let _ = tcp.set_nodelay(true);
    let mut stream: Box<dyn Io> = Box::new(tcp);

    if let Some(proxy) = proxy {
        if proxy.kind == Kind::Https {
            stream = secure(stream, &proxy.host, proxy_tls()?, timeout).await?;
        }
        bounded(
            timeout,
            |seconds| format!("the proxy did not set up the tunnel within {seconds} s"),
            tunnel(&mut stream, proxy, destination),
        )
        .await?;
    }
    Ok(stream)
}

/// Runs the TLS handshake with `host` over `stream`.
pub(crate) async fn secure(
    stream: Box<dyn Io>,
    host: &str,
    config: Arc<ClientConfig>,
    timeout: Option<Duration>,
) -> Result<Box<dyn Io>, String> {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let name = ServerName::try_from(host.to_owned())
        .map_err(|e| format!("{host} is not a valid server name: {e}"))?;
    let connector = tokio_rustls::TlsConnector::from(config);
    let stream = bounded(
        timeout,
        |seconds| format!("the TLS handshake took longer than {seconds} s"),
        async {
            connector
                .connect(name, stream)
                .await
                .map_err(|e| format!("TLS failed: {e}"))
        },
    )
    .await?;
    Ok(Box::new(stream))
}

/// `future`, within `limit` if there is one; `timed_out` words the
/// failure, given the limit in seconds.
async fn bounded<T>(
    limit: Option<Duration>,
    timed_out: impl FnOnce(u64) -> String,
    future: impl Future<Output = Result<T, String>>,
) -> Result<T, String> {
    match limit {
        None => future.await,
        Some(limit) => tokio::time::timeout(limit, future)
            .await
            .unwrap_or_else(|_| Err(timed_out(limit.as_secs()))),
    }
}

/// Asks the proxy, over `stream`, to connect to the destination.
async fn tunnel<S>(
    stream: &mut S,
    proxy: &Proxy,
    destination: Destination<'_>,
) -> Result<(), String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    match proxy.kind {
        Kind::Http | Kind::Https => connect(stream, proxy, destination).await,
        Kind::Socks5 { remote_dns } => socks5(stream, proxy, destination, remote_dns).await,
    }
}

/// `CONNECT host:port`, and a read of the proxy's answer.
async fn connect<S>(
    stream: &mut S,
    proxy: &Proxy,
    destination: Destination<'_>,
) -> Result<(), String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let authority = destination.authority();
    let mut request = format!("CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\n");
    if let Some(value) = &proxy.basic {
        request.push_str(&format!("Proxy-Authorization: {value}\r\n"));
    }
    request.push_str("\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|e| format!("could not ask the proxy to connect: {e}"))?;

    // Byte by byte, so nothing after the answer, which belongs to the
    // server, is read.
    let mut answer = Vec::new();
    while !answer.ends_with(b"\r\n\r\n") {
        if answer.len() >= MAX_ANSWER {
            return Err("the proxy's answer is too long".into());
        }
        let byte = stream
            .read_u8()
            .await
            .map_err(|e| format!("the proxy closed the connection before it answered ({e})"))?;
        answer.push(byte);
    }
    let text = String::from_utf8_lossy(&answer);
    let status_line = text.lines().next().unwrap_or_default();
    let mut words = status_line.splitn(3, ' ');
    let version = words.next().unwrap_or_default();
    let code = words.next().unwrap_or_default();
    if !version.starts_with("HTTP/") || code.len() != 3 {
        return Err(format!(
            "the proxy did not answer like an HTTP proxy ({status_line:?})"
        ));
    }
    if code.starts_with('2') {
        return Ok(());
    }
    let hint = match code {
        "407" => "; check the proxy's user name and password",
        "403" => "; the proxy does not allow this destination",
        _ => "",
    };
    Err(format!("the proxy answered {status_line}{hint}"))
}

/// The SOCKS5 handshake (RFC 1928, with RFC 1929 passwords) for a
/// `CONNECT` to the destination.
async fn socks5<S>(
    stream: &mut S,
    proxy: &Proxy,
    destination: Destination<'_>,
    remote_dns: bool,
) -> Result<(), String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let io = |e: std::io::Error| format!("the SOCKS proxy stopped answering ({e})");

    let methods: &[u8] = if proxy.login.is_some() {
        &[0x00, 0x02]
    } else {
        &[0x00]
    };
    let mut hello = vec![0x05, u8::try_from(methods.len()).expect("two methods")];
    hello.extend_from_slice(methods);
    stream.write_all(&hello).await.map_err(io)?;
    let mut choice = [0_u8; 2];
    stream.read_exact(&mut choice).await.map_err(io)?;
    if choice[0] != 0x05 {
        return Err("the proxy is not a SOCKS5 proxy".into());
    }
    match choice[1] {
        0x00 => {}
        0x02 => {
            let Some((user, password)) = &proxy.login else {
                return Err("the SOCKS proxy wants a user name and password".into());
            };
            let (Ok(user_len), Ok(password_len)) =
                (u8::try_from(user.len()), u8::try_from(password.len()))
            else {
                return Err(
                    "the SOCKS5 user name and password may be at most 255 bytes each".into(),
                );
            };
            let mut login = vec![0x01, user_len];
            login.extend_from_slice(user.as_bytes());
            login.push(password_len);
            login.extend_from_slice(password.as_bytes());
            stream.write_all(&login).await.map_err(io)?;
            let mut verdict = [0_u8; 2];
            stream.read_exact(&mut verdict).await.map_err(io)?;
            if verdict[1] != 0x00 {
                return Err("the SOCKS proxy rejected the user name or password".into());
            }
        }
        0xff => {
            return Err(if proxy.login.is_some() {
                "the SOCKS proxy accepts none of the ways Getman can sign in".into()
            } else {
                "the SOCKS proxy wants a user name and password, and the proxy URL has none".into()
            });
        }
        other => {
            return Err(format!(
                "the SOCKS proxy chose an unknown way to sign in ({other:#04x})"
            ));
        }
    }

    let host = destination.host;
    let mut request = vec![0x05, 0x01, 0x00];
    if let Ok(ip) = host.parse::<IpAddr>() {
        put_ip(&mut request, ip);
    } else if remote_dns {
        let name = host.as_bytes();
        let len = u8::try_from(name.len())
            .map_err(|_| "the host name is too long for a SOCKS5 proxy".to_owned())?;
        request.push(0x03);
        request.push(len);
        request.extend_from_slice(name);
    } else {
        // A proxy is asked for one address, so take an IPv4 one when there
        // is one: those are the ones proxies reach most reliably.
        let found: Vec<_> = tokio::net::lookup_host((host, destination.port))
            .await
            .map_err(|e| format!("could not look up {host}: {e}"))?
            .collect();
        let found = found
            .iter()
            .find(|address| address.is_ipv4())
            .or(found.first())
            .ok_or_else(|| format!("{host} has no address"))?;
        put_ip(&mut request, found.ip());
    }
    request.extend_from_slice(&destination.port.to_be_bytes());
    stream.write_all(&request).await.map_err(io)?;

    let mut head = [0_u8; 4];
    stream.read_exact(&mut head).await.map_err(io)?;
    if head[1] != 0x00 {
        return Err(format!(
            "the SOCKS proxy could not connect: {}",
            socks_reply(head[1])
        ));
    }
    // The address the proxy bound; it is read to get past it.
    let rest = match head[3] {
        0x01 => 4,
        0x04 => 16,
        0x03 => usize::from(stream.read_u8().await.map_err(io)?),
        _ => return Err("the SOCKS proxy sent an address Getman does not know".into()),
    };
    let mut skipped = vec![0_u8; rest + 2];
    stream.read_exact(&mut skipped).await.map_err(io)?;
    Ok(())
}

fn put_ip(request: &mut Vec<u8>, ip: IpAddr) {
    match ip {
        IpAddr::V4(ip) => {
            request.push(0x01);
            request.extend_from_slice(&ip.octets());
        }
        IpAddr::V6(ip) => {
            request.push(0x04);
            request.extend_from_slice(&ip.octets());
        }
    }
}

fn socks_reply(code: u8) -> &'static str {
    match code {
        0x01 => "general failure",
        0x02 => "the rules of the proxy do not allow it",
        0x03 => "the network is unreachable",
        0x04 => "the host is unreachable",
        0x05 => "the server refused the connection",
        0x06 => "the TTL expired",
        0x07 => "the proxy does not support the command",
        0x08 => "the proxy does not support the address type",
        _ => "unknown error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn destination(host: &str, tls: bool) -> Destination<'_> {
        Destination {
            tls,
            host,
            port: if tls { 443 } else { 50051 },
        }
    }

    fn url(url: &str, bypass: &str) -> ProxyRoute {
        ProxyRoute::Url {
            url: url.into(),
            bypass: bypass.into(),
        }
    }

    fn proxy_for(host: &str, tls: bool, route_setting: &ProxyRoute) -> Option<Proxy> {
        route(destination(host, tls), route_setting).unwrap()
    }

    #[test]
    fn the_route_follows_the_setting() {
        let host = "api.example.com";
        assert_eq!(proxy_for(host, true, &ProxyRoute::Off), None);

        let http = proxy_for(host, true, &url("http://ada:p%40ss@10.0.0.2:3128", "")).unwrap();
        assert_eq!(http.kind, Kind::Http);
        assert_eq!(http.authority(), "10.0.0.2:3128");
        assert!(http.login.is_none());
        // ada:p@ss
        assert_eq!(http.basic.as_deref(), Some("Basic YWRhOnBAc3M="));

        let secure = proxy_for(host, true, &url("https://proxy.local", "")).unwrap();
        assert_eq!((secure.kind, secure.port), (Kind::Https, 443));

        let socks = proxy_for(host, false, &url("socks5h://ada:p%40ss@proxy.local", "")).unwrap();
        assert_eq!(socks.kind, Kind::Socks5 { remote_dns: true });
        assert_eq!(socks.port, 1080);
        assert_eq!(socks.login, Some(("ada".into(), "p@ss".into())));
        assert!(socks.basic.is_none());
        assert!(!format!("{socks:?}").contains("p@ss"));

        let local = proxy_for(host, false, &url("socks5://proxy.local:9", "")).unwrap();
        assert_eq!(local.kind, Kind::Socks5 { remote_dns: false });

        let error = route(destination(host, true), &url("socks4://proxy.local", "")).unwrap_err();
        assert!(error.contains("socks4://"), "{error}");
    }

    #[test]
    fn the_bypass_list_goes_direct() {
        let bypassed = url(
            "http://10.0.0.2:3128",
            "localhost, .example.com, internal.test, 10.0.0.0/8",
        );
        assert_eq!(proxy_for("api.example.com", true, &bypassed), None);
        assert_eq!(proxy_for("api.internal.test", true, &bypassed), None);
        assert_eq!(proxy_for("10.1.2.3", false, &bypassed), None);
        assert!(proxy_for("api.other.org", true, &bypassed).is_some());
    }

    #[test]
    fn the_system_matcher_uses_the_variables_as_http_does() {
        let matcher = Matcher::builder()
            .https("http://secure-proxy:8443")
            .no("internal.test")
            .build();
        let proxy = choose(destination("chat.test", true), &matcher)
            .unwrap()
            .unwrap();
        assert_eq!(proxy.authority(), "secure-proxy:8443");
        // Only HTTPS_PROXY is set, so a plaintext connection goes direct,
        // as a plaintext HTTP request would.
        assert_eq!(choose(destination("chat.test", false), &matcher), Ok(None));
        let internal = destination("svc.internal.test", true);
        assert_eq!(choose(internal, &matcher), Ok(None));
    }

    #[test]
    fn an_ipv6_address_is_written_in_brackets() {
        assert_eq!(destination("::1", false).authority(), "[::1]:50051");
        assert_eq!(destination("a.test", true).authority(), "a.test:443");
        let proxy = proxy_for("a.test", false, &url("http://[::1]:3128", "")).unwrap();
        assert_eq!(proxy.host, "::1");
        assert_eq!(
            failure(destination("::1", false), Some(&proxy), "no"),
            "Could not connect to [::1]:50051 through the proxy [::1]:3128: no"
        );
        assert_eq!(
            failure(destination("a.test", true), None, "no"),
            "Could not connect to a.test:443: no"
        );
    }
}
