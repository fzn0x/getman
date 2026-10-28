//! Reaching the server of a WebSocket or Socket.IO connection: straight
//! to it, or through the proxy the request's settings call for, with TLS
//! on top for `wss` and `https`.
//!
//! HTTP requests go through reqwest, which handles proxies. These
//! connections are opened here, with the rules the [`tunnel`] module
//! applies to every connection Getman opens itself.

use url::Url;

use super::tls;
use crate::http::Transport;
use crate::tunnel::{self, Destination, Io};

/// Opens a stream to the host and port of `url` (`ws`, `wss`, `http` or
/// `https`), through the proxy `transport` asks for, if any, and for
/// `wss` and `https` encrypted.
pub(super) async fn connect(url: &Url, transport: &Transport) -> Result<Box<dyn Io>, String> {
    let host = url
        .host_str()
        .ok_or_else(|| format!("{url:?} has no host"))?
        .trim_start_matches('[')
        .trim_end_matches(']');
    let secure = matches!(url.scheme(), "wss" | "https");
    // WebSocket URLs count as the HTTP ones they upgrade.
    let destination = Destination {
        tls: secure,
        host,
        port: url
            .port_or_known_default()
            .unwrap_or(if secure { 443 } else { 80 }),
    };
    let proxy = tunnel::route(destination, &transport.proxy)
        .map_err(|reason| format!("Could not use the proxy: {reason}"))?;
    let failed = |reason: String| tunnel::failure(destination, proxy.as_ref(), &reason);
    // The timeout is the connection's, which the caller applies.
    let stream = tunnel::open(
        destination,
        proxy.as_ref(),
        || tls::config(transport.verify_tls),
        None,
    )
    .await
    .map_err(failed)?;
    if !secure {
        return Ok(stream);
    }
    tunnel::secure(stream, host, tls::config(transport.verify_tls)?, None)
        .await
        .map_err(failed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::ProxyRoute;

    #[tokio::test]
    async fn a_proxy_url_that_cannot_be_used_is_named() {
        let transport = Transport {
            proxy: ProxyRoute::Url {
                url: "socks4://proxy.test".into(),
                bypass: String::new(),
            },
            ..Transport::default()
        };
        let url = Url::parse("ws://chat.test/").unwrap();
        let Err(error) = connect(&url, &transport).await else {
            panic!("expected an error");
        };
        assert!(
            error.starts_with("Could not use the proxy: the proxy URL starts with socks4://"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn a_connection_that_fails_names_the_server() {
        let transport = Transport {
            proxy: ProxyRoute::Off,
            ..Transport::default()
        };
        // Nothing listens on port 1 of the loopback address.
        let url = Url::parse("ws://[::1]:1/").unwrap();
        let Err(error) = connect(&url, &transport).await else {
            panic!("expected an error");
        };
        assert!(
            error.starts_with("Could not connect to [::1]:1: "),
            "{error}"
        );
    }
}
