//! The loopback listener a browser sign-in returns to (RFC 8252).
//!
//! Anything on this machine can connect to it, so it trusts nothing it
//! reads: each connection gets a few seconds and a few kilobytes to send
//! its request line and headers, only `GET` on the callback path is
//! answered, and only an answer carrying the `state` Getman sent ends the
//! wait. Anything else gets an error page while Getman keeps listening.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use sha2::{Digest as _, Sha256};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;

use crate::error::{Error, Result};

/// Most bytes of request line and headers read from one connection.
const MAX_HEAD: usize = 16 * 1024;
/// Most connections served at once.
const MAX_CONNECTIONS: usize = 16;
/// How long one connection may take to send them, or to take the answer.
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(10);
/// The implicit grant puts the token after `#`, which browsers do not
/// send. This page sends it back as a query string.
const RELAY_SCRIPT: &str = r#"var h=location.hash.slice(1);if(h){location.replace(location.pathname+"?"+h)}else{document.getElementById("m").textContent="The provider sent no token. Go back to Getman and try again."}"#;

/// A listening callback.
#[derive(Debug)]
pub(super) struct Callback {
    listener: TcpListener,
    /// The redirect URI to give the provider.
    pub(super) redirect_uri: String,
    /// The path answers arrive on.
    path: String,
    /// Why the configured callback URL was not used, if it was not.
    pub(super) note: Option<String>,
}

impl Callback {
    /// Listens on the configured callback URL when it is a loopback `http`
    /// URL, else on a free port of 127.0.0.1 at `/callback`.
    pub(super) async fn bind(configured: &str) -> Result<Self> {
        let mut note = None;
        if !configured.is_empty() {
            match loopback(configured) {
                Some((address, path)) => {
                    let listener = bind_retrying(address).await.map_err(|e| {
                        Error::Http(format!(
                            "Could not listen for the OAuth 2.0 callback on {configured}: {e}"
                        ))
                    })?;
                    return Ok(Self {
                        listener,
                        redirect_uri: configured.to_owned(),
                        path,
                        note: None,
                    });
                }
                None => {
                    note = Some(format!(
                        "The callback URL {configured} is not a loopback http URL, so Getman cannot receive the answer there."
                    ));
                }
            }
        }
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(|e| {
                Error::Http(format!(
                    "Could not listen for the OAuth 2.0 callback on 127.0.0.1: {e}"
                ))
            })?;
        let port = listener
            .local_addr()
            .map_err(|e| Error::Http(e.to_string()))?
            .port();
        let redirect_uri = format!("http://127.0.0.1:{port}/callback");
        if let Some(text) = &mut note {
            text.push_str(&format!(
                " It listens on {redirect_uri} instead; your provider must allow that redirect URI."
            ));
        }
        Ok(Self {
            listener,
            redirect_uri,
            path: "/callback".into(),
            note,
        })
    }

    /// Waits for the browser to arrive with this `state`, and returns the
    /// parameters it carried. With `relay_fragment`, a visit without a
    /// query gets a page that resends the URL fragment as one.
    pub(super) async fn receive(
        &self,
        state: &str,
        relay_fragment: bool,
    ) -> Result<Vec<(String, String)>> {
        // Connections are served side by side: browsers open idle ones
        // ahead of time, which must not hold up the real one.
        let (sender, mut received) = tokio::sync::mpsc::channel(1);
        let slots = Arc::new(Semaphore::new(MAX_CONNECTIONS));
        let path: Arc<str> = Arc::from(self.path.as_str());
        let state: Arc<str> = Arc::from(state);
        loop {
            tokio::select! {
                accepted = self.listener.accept() => {
                    let (stream, _) = accepted.map_err(|e| {
                        Error::Http(format!("The OAuth 2.0 callback listener stopped: {e}"))
                    })?;
                    let Ok(slot) = Arc::clone(&slots).try_acquire_owned() else {
                        continue;
                    };
                    let (sender, path, state) =
                        (sender.clone(), Arc::clone(&path), Arc::clone(&state));
                    tokio::spawn(async move {
                        if let Some(params) = serve(stream, &path, &state, relay_fragment).await {
                            let _ = sender.send(params).await;
                        }
                        drop(slot);
                    });
                }
                Some(params) = received.recv() => return Ok(params),
            }
        }
    }
}

/// The address and path of a loopback `http` URL.
fn loopback(text: &str) -> Option<(SocketAddr, String)> {
    let url = url::Url::parse(text).ok()?;
    if url.scheme() != "http" {
        return None;
    }
    let ip: IpAddr = match url.host()? {
        url::Host::Domain(name) if name.eq_ignore_ascii_case("localhost") => {
            Ipv4Addr::LOCALHOST.into()
        }
        url::Host::Ipv4(ip) if ip.is_loopback() => ip.into(),
        url::Host::Ipv6(ip) if ip.is_loopback() => ip.into(),
        _ => return None,
    };
    Some((
        SocketAddr::new(ip, url.port_or_known_default()?),
        url.path().to_owned(),
    ))
}

/// Binds, retrying briefly while an earlier sign-in releases the port.
async fn bind_retrying(address: SocketAddr) -> std::io::Result<TcpListener> {
    let mut attempts = 0;
    loop {
        match TcpListener::bind(address).await {
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse && attempts < 20 => {
                attempts += 1;
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            result => return result,
        }
    }
}

/// Answers one connection, returning the parameters if they end the wait.
/// Slow or broken connections are dropped.
async fn serve(
    mut stream: TcpStream,
    path: &str,
    state: &str,
    relay_fragment: bool,
) -> Option<Vec<(String, String)>> {
    let head = tokio::time::timeout(CONNECTION_TIMEOUT, read_head(&mut stream))
        .await
        .ok()?
        .ok()?;
    let answer = match head {
        Some(head) => answer(&head, path, state, relay_fragment),
        None => Answer::page(
            431,
            "Request too large",
            "Getman did not read this request.",
        ),
    };
    let _ = tokio::time::timeout(CONNECTION_TIMEOUT, write(&mut stream, &answer)).await;
    answer.params
}

/// Reads up to the end of the headers: `None` when they are too long.
async fn read_head(stream: &mut TcpStream) -> std::io::Result<Option<String>> {
    let mut head = Vec::new();
    let mut chunk = [0_u8; 2048];
    loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        head.extend_from_slice(&chunk[..read]);
        if let Some(end) = head.windows(4).position(|w| w == b"\r\n\r\n") {
            head.truncate(end);
            return Ok(Some(String::from_utf8_lossy(&head).into_owned()));
        }
        if head.len() > MAX_HEAD {
            return Ok(None);
        }
    }
}

/// A reply, and the parameters that end the wait, if it ends.
#[derive(Debug)]
struct Answer {
    status: u16,
    reason: &'static str,
    body: String,
    /// The page's script, allowed by its hash.
    script: Option<&'static str>,
    params: Option<Vec<(String, String)>>,
}

impl Answer {
    fn page(status: u16, reason: &'static str, message: &str) -> Self {
        Self {
            status,
            reason,
            body: format!(
                "<!doctype html><meta charset=\"utf-8\"><title>Getman</title><p>{}</p>",
                escape(message)
            ),
            script: None,
            params: None,
        }
    }
}

/// What to answer to a request with this head.
fn answer(head: &str, path: &str, state: &str, relay_fragment: bool) -> Answer {
    let line = head.lines().next().unwrap_or_default();
    let mut parts = line.split(' ');
    let (method, target) = (
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
    );
    if method != "GET" {
        return Answer::page(405, "Method Not Allowed", "Getman only answers GET here.");
    }
    let (target_path, query) = target.split_once('?').unwrap_or((target, ""));
    if target_path != path {
        return Answer::page(404, "Not Found", "Nothing here.");
    }
    if query.is_empty() && relay_fragment {
        return Answer {
            status: 200,
            reason: "OK",
            body: format!(
                "<!doctype html><meta charset=\"utf-8\"><title>Getman</title><p id=\"m\">Passing the token to Getman…</p><script>{RELAY_SCRIPT}</script>"
            ),
            script: Some(RELAY_SCRIPT),
            params: None,
        };
    }
    let params: Vec<(String, String)> = url::form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect();
    let sent = params
        .iter()
        .find(|(name, _)| name == "state")
        .map(|(_, v)| v.as_str());
    if sent != Some(state) {
        return Answer::page(
            400,
            "Bad Request",
            "This answer does not belong to the sign-in Getman started. Start it again from Getman.",
        );
    }
    let error = params
        .iter()
        .find(|(name, _)| name == "error")
        .map(|(_, v)| v.clone());
    let mut reply = match error {
        Some(error) => Answer::page(
            200,
            "OK",
            &format!("The sign-in did not succeed ({error}). Go back to Getman to see why."),
        ),
        None => Answer::page(
            200,
            "OK",
            "Signed in. You can close this tab and go back to Getman.",
        ),
    };
    reply.params = Some(params);
    reply
}

async fn write(stream: &mut TcpStream, answer: &Answer) -> std::io::Result<()> {
    let csp = match answer.script {
        Some(script) => format!(
            "default-src 'none'; script-src 'sha256-{}'",
            BASE64.encode(Sha256::digest(script.as_bytes()))
        ),
        None => "default-src 'none'".to_owned(),
    };
    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: {csp}\r\nConnection: close\r\n\r\n{}",
        answer.status,
        answer.reason,
        answer.body.len(),
        answer.body
    );
    stream.write_all(response.as_bytes()).await?;
    stream.shutdown().await
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_urls_are_recognized() {
        assert_eq!(
            loopback("http://127.0.0.1:8765/cb"),
            Some(("127.0.0.1:8765".parse().unwrap(), "/cb".into()))
        );
        assert_eq!(
            loopback("http://localhost:9000/"),
            Some(("127.0.0.1:9000".parse().unwrap(), "/".into()))
        );
        assert!(loopback("http://[::1]:9000/x").is_some());
        assert!(loopback("https://127.0.0.1:9000/x").is_none());
        assert!(loopback("http://example.com/x").is_none());
        assert!(loopback("http://10.0.0.1/x").is_none());
    }

    #[test]
    fn only_the_right_state_ends_the_wait() {
        let answer = |head: &str| super::answer(head, "/callback", "s1", false);
        assert_eq!(answer("POST /callback?state=s1 HTTP/1.1").status, 405);
        assert_eq!(answer("GET /favicon.ico HTTP/1.1").status, 404);
        let wrong = answer("GET /callback?code=c&state=s2 HTTP/1.1");
        assert_eq!(wrong.status, 400);
        assert!(wrong.params.is_none());
        let right = answer("GET /callback?code=c%2B1&state=s1 HTTP/1.1\r\nHost: x");
        assert_eq!(right.status, 200);
        assert!(
            right
                .params
                .unwrap()
                .contains(&("code".into(), "c+1".into()))
        );
        let refused = answer("GET /callback?error=access_denied&state=s1 HTTP/1.1");
        assert!(refused.body.contains("access_denied"));
        assert!(refused.params.is_some());
    }

    #[test]
    fn the_implicit_grant_gets_a_relay_page() {
        let relay = answer("GET /callback HTTP/1.1", "/callback", "s1", true);
        assert_eq!(relay.status, 200);
        assert!(relay.body.contains(RELAY_SCRIPT));
        assert!(relay.params.is_none());
        let without = answer("GET /callback HTTP/1.1", "/callback", "s1", false);
        assert_eq!(without.status, 400);
        let escaped = Answer::page(200, "OK", "<script>");
        assert!(escaped.body.contains("&lt;script&gt;"));
    }

    #[tokio::test]
    async fn oversized_and_wrong_requests_do_not_end_the_wait() {
        let callback = Callback::bind("").await.unwrap();
        assert!(callback.redirect_uri.starts_with("http://127.0.0.1:"));
        assert!(callback.redirect_uri.ends_with("/callback"));
        let address = callback.redirect_uri["http://".len()..]
            .split('/')
            .next()
            .unwrap()
            .to_owned();
        let browser = tokio::spawn(async move {
            let send = |text: Vec<u8>| {
                let address = address.clone();
                async move {
                    let mut stream = TcpStream::connect(address).await.unwrap();
                    stream.write_all(&text).await.unwrap();
                    let mut reply = String::new();
                    let _ = stream.read_to_string(&mut reply).await;
                    reply
                }
            };
            let big = send(vec![b'a'; MAX_HEAD + 4096]).await;
            // The listener may reset the connection before all is sent.
            assert!(big.is_empty() || big.starts_with("HTTP/1.1 431"), "{big}");
            let wrong = send(b"GET /callback?state=nope HTTP/1.1\r\n\r\n".to_vec()).await;
            assert!(wrong.starts_with("HTTP/1.1 400"), "{wrong}");
            let right = send(b"GET /callback?code=abc&state=s1 HTTP/1.1\r\n\r\n".to_vec()).await;
            assert!(right.starts_with("HTTP/1.1 200"), "{right}");
            assert!(right.contains("default-src 'none'"));
        });
        let params = tokio::time::timeout(Duration::from_secs(10), callback.receive("s1", false))
            .await
            .unwrap()
            .unwrap();
        browser.await.unwrap();
        assert_eq!(params[0], ("code".into(), "abc".into()));
    }

    #[tokio::test]
    async fn an_idle_connection_does_not_hold_up_the_answer() {
        let callback = Callback::bind("").await.unwrap();
        let address = callback.redirect_uri["http://".len()..]
            .split('/')
            .next()
            .unwrap()
            .to_owned();
        let idle = TcpStream::connect(&address).await.unwrap();
        let mut stream = TcpStream::connect(&address).await.unwrap();
        stream
            .write_all(b"GET /callback?code=abc&state=s1 HTTP/1.1\r\n\r\n")
            .await
            .unwrap();
        let params = tokio::time::timeout(Duration::from_secs(3), callback.receive("s1", false))
            .await
            .expect("the idle connection held up the answer")
            .unwrap();
        assert_eq!(params[0].1, "abc");
        drop(idle);
    }

    #[tokio::test]
    async fn a_configured_loopback_callback_is_used() {
        let probe = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = probe.local_addr().unwrap().port();
        drop(probe);
        let url = format!("http://localhost:{port}/oauth/done");
        let callback = Callback::bind(&url).await.unwrap();
        assert_eq!(callback.redirect_uri, url);
        assert_eq!(callback.path, "/oauth/done");
        assert!(callback.note.is_none());

        let elsewhere = Callback::bind("https://app.example.com/callback")
            .await
            .unwrap();
        assert!(elsewhere.redirect_uri.starts_with("http://127.0.0.1:"));
        assert!(elsewhere.note.unwrap().contains("not a loopback"));
    }
}
