//! Reaching a gRPC server through a proxy. A call is a long HTTP/2
//! connection, so it goes through an HTTP proxy with `CONNECT`, or through
//! a SOCKS5 proxy, and the proxy only relays bytes.
//!
//! Which proxy a call uses follows the HTTP client: the request's proxy
//! setting, or with the default the system's settings and the
//! `HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY` and `NO_PROXY` variables, and a
//! bypass list of hosts that are reached directly.

use std::net::IpAddr;

use hyper_util::client::proxy::matcher::{Intercept, Matcher};
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};

use super::transport::Target;
use crate::error::{Error, Result};
use crate::http::ProxyRoute;

/// Most bytes of a proxy's answer to `CONNECT` that are read.
const MAX_ANSWER: usize = 16 * 1024;

/// How a proxy is spoken to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    /// `CONNECT` over plain TCP.
    Http,
    /// `CONNECT` inside TLS to the proxy.
    Https,
    /// SOCKS5; `remote_dns` hands the proxy the host name to look up.
    Socks5 { remote_dns: bool },
}

/// A proxy to go through, with its credentials.
#[derive(Clone, PartialEq, Eq)]
pub(super) struct Proxy {
    pub(super) kind: Kind,
    pub(super) host: String,
    pub(super) port: u16,
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
    pub(super) fn authority(&self) -> String {
        if self.host.contains(':') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    fn from_intercept(intercept: &Intercept) -> Result<Self> {
        let uri = intercept.uri();
        let scheme = uri.scheme_str().unwrap_or("http");
        let (kind, default_port) = match scheme {
            "http" => (Kind::Http, 80),
            "https" => (Kind::Https, 443),
            "socks5" => (Kind::Socks5 { remote_dns: false }, 1080),
            "socks5h" => (Kind::Socks5 { remote_dns: true }, 1080),
            other => {
                return Err(Error::Invalid(format!(
                    "the proxy URL starts with {other}://; use http://, https://, socks5:// or socks5h://"
                )));
            }
        };
        let host = uri
            .host()
            .ok_or_else(|| Error::Invalid("the proxy URL has no host".into()))?
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

/// The proxy a call to `target` goes through, or `None` to connect
/// directly.
pub(super) fn route(target: &Target, proxy: &ProxyRoute) -> Result<Option<Proxy>> {
    match proxy {
        ProxyRoute::Off => Ok(None),
        ProxyRoute::System => choose(target, &Matcher::from_system()),
        ProxyRoute::Url { url, bypass } => choose(
            target,
            &Matcher::builder().all(url.as_str()).no(bypass).build(),
        ),
    }
}

fn choose(target: &Target, matcher: &Matcher) -> Result<Option<Proxy>> {
    let scheme = if target.tls { "https" } else { "http" };
    let Ok(destination) = format!("{scheme}://{}/", target.authority()).parse::<http::Uri>() else {
        return Ok(None);
    };
    matcher
        .intercept(&destination)
        .map(|intercept| Proxy::from_intercept(&intercept))
        .transpose()
}

/// Asks the proxy, over `stream`, to connect to the target.
pub(super) async fn tunnel<S>(
    stream: &mut S,
    proxy: &Proxy,
    target: &Target,
) -> std::result::Result<(), String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    match proxy.kind {
        Kind::Http | Kind::Https => connect(stream, proxy, target).await,
        Kind::Socks5 { remote_dns } => socks5(stream, proxy, target, remote_dns).await,
    }
}

/// `CONNECT host:port`, and a read of the proxy's answer.
async fn connect<S>(
    stream: &mut S,
    proxy: &Proxy,
    target: &Target,
) -> std::result::Result<(), String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let authority = target.authority();
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
/// `CONNECT` to the target.
async fn socks5<S>(
    stream: &mut S,
    proxy: &Proxy,
    target: &Target,
    remote_dns: bool,
) -> std::result::Result<(), String>
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

    let mut request = vec![0x05, 0x01, 0x00];
    if let Ok(ip) = target.host.parse::<IpAddr>() {
        put_ip(&mut request, ip);
    } else if remote_dns {
        let name = target.host.as_bytes();
        let len = u8::try_from(name.len())
            .map_err(|_| "the host name is too long for a SOCKS5 proxy".to_owned())?;
        request.push(0x03);
        request.push(len);
        request.extend_from_slice(name);
    } else {
        let found = tokio::net::lookup_host((target.host.as_str(), target.port))
            .await
            .map_err(|e| format!("could not look up {}: {e}", target.host))?
            .next()
            .ok_or_else(|| format!("{} has no address", target.host))?;
        put_ip(&mut request, found.ip());
    }
    request.extend_from_slice(&target.port.to_be_bytes());
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

    fn target(tls: bool) -> Target {
        Target::parse("api.example.com:50051", tls).unwrap()
    }

    #[test]
    fn the_route_follows_the_setting() {
        let url = |url: &str, bypass: &str| ProxyRoute::Url {
            url: url.into(),
            bypass: bypass.into(),
        };
        assert_eq!(route(&target(true), &ProxyRoute::Off).unwrap(), None);

        let http = route(&target(true), &url("http://ada:p%40ss@10.0.0.2:3128", ""))
            .unwrap()
            .unwrap();
        assert_eq!(http.kind, Kind::Http);
        assert_eq!(http.authority(), "10.0.0.2:3128");
        assert!(http.basic.is_some() && http.login.is_none());

        let socks = route(&target(false), &url("socks5h://ada:p%40ss@proxy.local", ""))
            .unwrap()
            .unwrap();
        assert_eq!(socks.kind, Kind::Socks5 { remote_dns: true });
        assert_eq!(socks.port, 1080);
        assert_eq!(socks.login, Some(("ada".into(), "p@ss".into())));
        assert!(!format!("{socks:?}").contains("p@ss"));

        let local = route(&target(false), &url("socks5://proxy.local:9", ""))
            .unwrap()
            .unwrap();
        assert_eq!(local.kind, Kind::Socks5 { remote_dns: false });

        let bypassed = url("http://10.0.0.2:3128", "localhost, .example.com");
        assert_eq!(route(&target(true), &bypassed).unwrap(), None);
        let other = Target::parse("api.other.org:443", true).unwrap();
        assert!(route(&other, &bypassed).unwrap().is_some());
    }

    #[test]
    fn the_system_matcher_uses_the_variables_as_http_does() {
        let matcher = Matcher::builder()
            .https("http://secure-proxy:8443")
            .no("internal.test")
            .build();
        let proxy = choose(&target(true), &matcher).unwrap().unwrap();
        assert_eq!(proxy.authority(), "secure-proxy:8443");
        // Only HTTPS_PROXY is set, so a plaintext call goes direct, as a
        // plaintext HTTP request would.
        assert_eq!(choose(&target(false), &matcher).unwrap(), None);
        let internal = Target::parse("https://svc.internal.test", false).unwrap();
        assert_eq!(choose(&internal, &matcher).unwrap(), None);
    }
}
