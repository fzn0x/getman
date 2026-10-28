//! Reaching the server of a WebSocket or Socket.IO connection: straight
//! to it, or through the proxy the request's settings call for.
//!
//! HTTP requests go through reqwest, which handles proxies. These
//! connections are opened here, so the same rules are applied by hand: a
//! proxy URL in the settings, `off`, the bypass list, and otherwise the
//! system's proxy settings and the `HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY`
//! and `NO_PROXY` environment variables. A proxy can be an HTTP or HTTPS
//! one, reached with `CONNECT` and optional Basic credentials, or a SOCKS5
//! one: `socks5://` resolves the server's name here, `socks5h://` leaves
//! that to the proxy.

use std::net::IpAddr;

use hyper_util::client::proxy::matcher::{Intercept, Matcher};
use rustls::pki_types::ServerName;
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use tokio::net::TcpStream;

use super::tls;
use crate::http::{ProxyRoute, Transport};

/// A stream a connection runs over: plain TCP or TLS.
pub(super) trait Io: AsyncRead + AsyncWrite + Unpin + Send {}

impl<T: AsyncRead + AsyncWrite + Unpin + Send> Io for T {}

/// Longest answer to a `CONNECT` the proxy may send before it is taken as
/// not being a proxy.
const MAX_ANSWER_BYTES: usize = 16 * 1024;

/// Where the next hop leads.
#[derive(Debug, PartialEq, Eq)]
enum Hop {
    /// Straight to the server.
    Direct,
    /// Through an HTTP or HTTPS proxy, with a tunnel.
    Http {
        host: String,
        port: u16,
        tls: bool,
        /// The value of `Proxy-Authorization`.
        auth: Option<String>,
    },
    /// Through a SOCKS5 proxy.
    Socks5 {
        host: String,
        port: u16,
        /// Whether the proxy resolves the server's name.
        remote_dns: bool,
        auth: Option<(String, String)>,
    },
}

/// Opens a stream to the host and port of `url` (`ws`, `wss`, `http` or
/// `https`), through the proxy `transport` asks for, if any. For `wss`
/// and `https` the stream is not yet encrypted; see [`secure`].
pub(super) async fn connect(url: &url::Url, transport: &Transport) -> Result<Box<dyn Io>, String> {
    let (host, port) = target(url)?;
    let hop = match &transport.proxy {
        ProxyRoute::Off => Hop::Direct,
        ProxyRoute::System => route(&Matcher::from_system(), url)?,
        ProxyRoute::Url { url: proxy, bypass } => {
            let matcher = Matcher::builder()
                .all(proxy.clone())
                .no(bypass.clone())
                .build();
            route(&matcher, url)?
        }
    };
    match hop {
        Hop::Direct => {
            let tcp = tcp(&host, port, "Could not connect").await?;
            Ok(Box::new(tcp))
        }
        Hop::Http {
            host: proxy_host,
            port: proxy_port,
            tls: proxy_tls,
            auth,
        } => {
            let tcp = tcp(
                &proxy_host,
                proxy_port,
                &format!("Could not connect to the proxy {proxy_host}:{proxy_port}"),
            )
            .await?;
            let mut stream: Box<dyn Io> = if proxy_tls {
                secure(Box::new(tcp), &proxy_host, transport.verify_tls).await?
            } else {
                Box::new(tcp)
            };
            http_tunnel(&mut stream, &host, port, auth.as_deref()).await?;
            Ok(stream)
        }
        Hop::Socks5 {
            host: proxy_host,
            port: proxy_port,
            remote_dns,
            auth,
        } => {
            let mut tcp = tcp(
                &proxy_host,
                proxy_port,
                &format!("Could not connect to the proxy {proxy_host}:{proxy_port}"),
            )
            .await?;
            socks5(&mut tcp, &host, port, remote_dns, auth.as_ref()).await?;
            Ok(Box::new(tcp))
        }
    }
}

/// Wraps a stream in TLS toward `host`, checking its certificate unless
/// `verify` is off.
pub(super) async fn secure(
    stream: Box<dyn Io>,
    host: &str,
    verify: bool,
) -> Result<Box<dyn Io>, String> {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let name = match host.parse::<IpAddr>() {
        Ok(address) => ServerName::IpAddress(address.into()),
        Err(_) => ServerName::try_from(host.to_owned())
            .map_err(|e| format!("{host:?} is not a valid host name: {e}"))?,
    };
    let connector = tokio_rustls::TlsConnector::from(tls::config(verify)?);
    let tls = connector
        .connect(name, stream)
        .await
        .map_err(|e| format!("Could not connect securely: {e}"))?;
    Ok(Box::new(tls))
}

/// The host and port a URL leads to.
fn target(url: &url::Url) -> Result<(String, u16), String> {
    let host = url
        .host_str()
        .ok_or_else(|| format!("{url:?} has no host"))?
        .to_owned();
    let secure = matches!(url.scheme(), "wss" | "https");
    let port = url
        .port_or_known_default()
        .unwrap_or(if secure { 443 } else { 80 });
    Ok((host, port))
}

/// Which way to the server `matcher` chooses. WebSocket URLs count as the
/// HTTP ones they upgrade.
fn route(matcher: &Matcher, url: &url::Url) -> Result<Hop, String> {
    let (host, port) = target(url)?;
    let scheme = if matches!(url.scheme(), "wss" | "https") {
        "https"
    } else {
        "http"
    };
    let Ok(uri) = format!("{scheme}://{host}:{port}/").parse::<http::Uri>() else {
        return Ok(Hop::Direct);
    };
    let Some(proxy) = matcher.intercept(&uri) else {
        return Ok(Hop::Direct);
    };
    hop(&proxy)
}

fn hop(proxy: &Intercept) -> Result<Hop, String> {
    let uri = proxy.uri();
    let host = uri.host().unwrap_or_default().to_owned();
    match uri.scheme_str().unwrap_or("http") {
        scheme @ ("http" | "https") => Ok(Hop::Http {
            host,
            port: uri
                .port_u16()
                .unwrap_or(if scheme == "https" { 443 } else { 80 }),
            tls: scheme == "https",
            auth: proxy
                .basic_auth()
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned),
        }),
        scheme @ ("socks5" | "socks5h") => Ok(Hop::Socks5 {
            host,
            port: uri.port_u16().unwrap_or(1080),
            remote_dns: scheme == "socks5h",
            auth: proxy
                .raw_auth()
                .map(|(user, password)| (user.to_owned(), password.to_owned())),
        }),
        other => Err(format!(
            "The proxy URL starts with {other}://, which WebSocket and Socket.IO connections cannot use; use http://, https://, socks5:// or socks5h://"
        )),
    }
}

async fn tcp(host: &str, port: u16, failure: &str) -> Result<TcpStream, String> {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let stream = TcpStream::connect((host, port))
        .await
        .map_err(|e| format!("{failure}: {e}"))?;
    let _ = stream.set_nodelay(true);
    Ok(stream)
}

/// Asks an HTTP proxy to tunnel to `host:port`.
async fn http_tunnel(
    stream: &mut Box<dyn Io>,
    host: &str,
    port: u16,
    auth: Option<&str>,
) -> Result<(), String> {
    let authority = format!("{host}:{port}");
    let mut request = format!("CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\n");
    if let Some(auth) = auth {
        request.push_str(&format!("Proxy-Authorization: {auth}\r\n"));
    }
    request.push_str("\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|e| format!("The proxy closed the connection: {e}"))?;

    // Read the answer up to the blank line, and not a byte beyond: what
    // follows belongs to the server.
    let mut answer = Vec::new();
    let mut byte = [0_u8; 1];
    while !answer.ends_with(b"\r\n\r\n") {
        if answer.len() >= MAX_ANSWER_BYTES {
            return Err("The proxy answered with more than a proxy should".into());
        }
        let read = stream
            .read(&mut byte)
            .await
            .map_err(|e| format!("The proxy closed the connection: {e}"))?;
        if read == 0 {
            return Err("The proxy closed the connection before it answered".into());
        }
        answer.push(byte[0]);
    }
    let text = String::from_utf8_lossy(&answer);
    let status_line = text.lines().next().unwrap_or_default();
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok());
    match status {
        Some(200..=299) => Ok(()),
        Some(407) => Err(
            "The proxy asked for credentials (407); give the proxy a user name and password in Settings"
                .into(),
        ),
        _ => Err(format!(
            "The proxy would not open a tunnel to {authority}: {}",
            if status_line.is_empty() {
                "it did not answer like an HTTP proxy"
            } else {
                status_line
            }
        )),
    }
}

/// Asks a SOCKS5 proxy to connect to `host:port` (RFC 1928), logging in
/// with a user name and password if it asks (RFC 1929).
async fn socks5(
    stream: &mut TcpStream,
    host: &str,
    port: u16,
    remote_dns: bool,
    auth: Option<&(String, String)>,
) -> Result<(), String> {
    let lost = |e: std::io::Error| format!("The SOCKS proxy closed the connection: {e}");
    // The methods we can do: none, and a login if we have one.
    if auth.is_some() {
        stream.write_all(&[5, 2, 0, 2]).await.map_err(lost)?;
    } else {
        stream.write_all(&[5, 1, 0]).await.map_err(lost)?;
    }
    let mut choice = [0_u8; 2];
    stream.read_exact(&mut choice).await.map_err(lost)?;
    if choice[0] != 5 {
        return Err("The proxy is not a SOCKS5 proxy".into());
    }
    match choice[1] {
        0 => {}
        2 => {
            let Some((user, password)) = auth else {
                return Err(
                    "The SOCKS proxy asks for a user name and password; give them in Settings"
                        .into(),
                );
            };
            let (Ok(user_length), Ok(password_length)) =
                (u8::try_from(user.len()), u8::try_from(password.len()))
            else {
                return Err("A SOCKS user name or password may be at most 255 bytes".into());
            };
            let mut login = vec![1, user_length];
            login.extend_from_slice(user.as_bytes());
            login.push(password_length);
            login.extend_from_slice(password.as_bytes());
            stream.write_all(&login).await.map_err(lost)?;
            let mut answer = [0_u8; 2];
            stream.read_exact(&mut answer).await.map_err(lost)?;
            if answer[1] != 0 {
                return Err("The SOCKS proxy refused the user name and password".into());
            }
        }
        0xFF => {
            return Err("The SOCKS proxy accepts none of the ways to log in Getman offers".into());
        }
        other => {
            return Err(format!(
                "The SOCKS proxy chose an unknown login method ({other})"
            ));
        }
    }

    let plain = host.trim_start_matches('[').trim_end_matches(']');
    let mut request = vec![5, 1, 0];
    if let Ok(address) = plain.parse::<IpAddr>() {
        push_address(&mut request, address);
    } else if remote_dns {
        let Ok(length) = u8::try_from(host.len()) else {
            return Err(format!("{host:?} is too long a name for a SOCKS proxy"));
        };
        request.push(3);
        request.push(length);
        request.extend_from_slice(host.as_bytes());
    } else {
        // A proxy is asked for one address, so take an IPv4 one when there
        // is one: those are the ones proxies reach most reliably.
        let addresses: Vec<_> = tokio::net::lookup_host((plain, port))
            .await
            .map_err(|e| format!("Could not find {host}: {e}"))?
            .collect();
        let address = addresses
            .iter()
            .find(|a| a.is_ipv4())
            .or(addresses.first())
            .ok_or_else(|| format!("Could not find {host}"))?;
        push_address(&mut request, address.ip());
    }
    request.extend_from_slice(&port.to_be_bytes());
    stream.write_all(&request).await.map_err(lost)?;

    let mut head = [0_u8; 4];
    stream.read_exact(&mut head).await.map_err(lost)?;
    if head[1] != 0 {
        return Err(format!(
            "The SOCKS proxy could not connect to {host}:{port}: {}",
            match head[1] {
                1 => "general failure",
                2 => "not allowed by its rules",
                3 => "network unreachable",
                4 => "host unreachable",
                5 => "connection refused",
                6 => "TTL expired",
                7 => "command not supported",
                8 => "address type not supported",
                _ => "unknown error",
            }
        ));
    }
    // Skip the bound address.
    let skip = match head[3] {
        1 => 4,
        4 => 16,
        3 => {
            let mut length = [0_u8; 1];
            stream.read_exact(&mut length).await.map_err(lost)?;
            usize::from(length[0])
        }
        _ => return Err("The SOCKS proxy answered with an address Getman cannot read".into()),
    };
    let mut rest = vec![0_u8; skip + 2];
    stream.read_exact(&mut rest).await.map_err(lost)?;
    Ok(())
}

fn push_address(request: &mut Vec<u8>, address: IpAddr) {
    match address {
        IpAddr::V4(v4) => {
            request.push(1);
            request.extend_from_slice(&v4.octets());
        }
        IpAddr::V6(v6) => {
            request.push(4);
            request.extend_from_slice(&v6.octets());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matcher(all: &str, no: &str) -> Matcher {
        Matcher::builder()
            .all(all.to_owned())
            .no(no.to_owned())
            .build()
    }

    fn url(raw: &str) -> url::Url {
        url::Url::parse(raw).unwrap()
    }

    #[test]
    fn a_proxy_url_chooses_the_way() {
        let http = matcher("http://ada:p%40ss@proxy.test:3128", "");
        let Hop::Http {
            host,
            port,
            tls,
            auth,
        } = route(&http, &url("ws://chat.test/socket")).unwrap()
        else {
            panic!("expected an HTTP proxy");
        };
        assert_eq!((host.as_str(), port, tls), ("proxy.test", 3128, false));
        // ada:p@ss
        assert_eq!(auth.as_deref(), Some("Basic YWRhOnBAc3M="));

        let socks = matcher("socks5h://bob:secret@proxy.test:1081", "");
        assert_eq!(
            route(&socks, &url("wss://chat.test/")).unwrap(),
            Hop::Socks5 {
                host: "proxy.test".into(),
                port: 1081,
                remote_dns: true,
                auth: Some(("bob".into(), "secret".into())),
            }
        );
        let Hop::Socks5 { remote_dns, .. } =
            route(&matcher("socks5://proxy.test", ""), &url("ws://a.test")).unwrap()
        else {
            panic!("expected a SOCKS proxy");
        };
        assert!(!remote_dns);
        assert!(route(&matcher("socks4://proxy.test", ""), &url("ws://a.test")).is_err());
    }

    #[test]
    fn the_bypass_list_goes_direct() {
        let m = matcher("http://proxy.test:3128", "internal.test, 10.0.0.0/8");
        assert_eq!(
            route(&m, &url("ws://api.internal.test/x")).unwrap(),
            Hop::Direct
        );
        assert_eq!(route(&m, &url("ws://10.1.2.3:9000/")).unwrap(), Hop::Direct);
        assert!(matches!(
            route(&m, &url("ws://elsewhere.test/")).unwrap(),
            Hop::Http { .. }
        ));
    }

    #[test]
    fn the_environment_sets_the_system_proxy() {
        // What `ProxyRoute::System` reads, built the way the environment
        // would give it.
        let m = Matcher::builder()
            .https("http://secure-proxy.test:8443".to_owned())
            .no("localhost".to_owned())
            .build();
        assert!(matches!(
            route(&m, &url("wss://chat.test/")).unwrap(),
            Hop::Http { port: 8443, .. }
        ));
        assert_eq!(route(&m, &url("ws://chat.test/")).unwrap(), Hop::Direct);
        assert_eq!(route(&m, &url("wss://localhost/")).unwrap(), Hop::Direct);
    }
}
