//! A local Socket.IO v4 server the realtime tests connect to, which can
//! allow WebSocket only, long polling only, or long polling with an
//! upgrade to WebSocket.
//!
//! A socket joins any namespace with the auth payload `{"token":
//! "secret"}` and is greeted with the events `other` and `welcome`. The
//! event `echo` is acknowledged and sent back, `binary` is answered with a
//! binary `file` event, and `kick` disconnects the socket. The server
//! pings every 100 ms, and tells what it saw in [`SocketIoServer::seen`]:
//! one line per request, such as `GET polling cookie=io=engine-1`.

use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine as _;
use futures_util::{SinkExt as _, StreamExt as _};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpStream;
use tokio::sync::{Notify, mpsc};
use tokio_tungstenite::accept_hdr_async;
use tokio_tungstenite::tungstenite::Message as Frame;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::StatusCode;

use super::test_servers::listen;

/// Which ways to the server are open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Transports {
    /// WebSocket only. Long polling is answered with 400.
    Websocket,
    /// Long polling only, with no upgrade on offer.
    Polling,
    /// Long polling, and an upgrade to WebSocket on offer.
    Upgrading,
    /// Long polling, and an upgrade on offer that is then refused.
    BrokenUpgrade,
}

pub(super) struct SocketIoServer {
    pub addr: SocketAddr,
    /// The requests the server saw, in order.
    pub seen: Arc<Mutex<Vec<String>>>,
}

impl SocketIoServer {
    /// How many requests were made over a transport and method, such as
    /// `("GET", "polling")`.
    pub(super) fn count(&self, method: &str, transport: &str) -> usize {
        let prefix = format!("{method} {transport} ");
        self.seen
            .lock()
            .unwrap()
            .iter()
            .filter(|line| line.starts_with(&prefix))
            .count()
    }
}

/// A WebSocket-only server, the address of which is all a test needs.
pub(super) async fn websocket_only() -> SocketAddr {
    server(Transports::Websocket).await.addr
}

struct Hub {
    transports: Transports,
    seen: Arc<Mutex<Vec<String>>>,
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    next: AtomicU32,
}

/// An Engine.IO session: what waits to be fetched by long polling, and
/// the WebSocket that took over, if one did.
#[derive(Default)]
struct Session {
    outbox: Mutex<VecDeque<String>>,
    wake: Notify,
    websocket: Mutex<Option<mpsc::UnboundedSender<Frame>>>,
}

enum Reply {
    Text(String),
    Binary(Vec<u8>),
}

impl Session {
    fn deliver(&self, reply: Reply) {
        if let Some(websocket) = self.websocket.lock().unwrap().as_ref() {
            let _ = websocket.send(match reply {
                Reply::Text(text) => Frame::text(text),
                Reply::Binary(bytes) => Frame::binary(bytes),
            });
            return;
        }
        let packet = match reply {
            Reply::Text(text) => text,
            Reply::Binary(bytes) => format!(
                "b{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            ),
        };
        self.outbox.lock().unwrap().push_back(packet);
        self.wake.notify_one();
    }
}

pub(super) async fn server(transports: Transports) -> SocketIoServer {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let hub = Arc::new(Hub {
        transports,
        seen: Arc::clone(&seen),
        sessions: Mutex::new(HashMap::new()),
        next: AtomicU32::new(1),
    });
    let addr = listen(move |stream| connection(stream, Arc::clone(&hub))).await;
    SocketIoServer { addr, seen }
}

impl Hub {
    fn open_packet(&self, sid: &str) -> String {
        let upgrades = match self.transports {
            Transports::Upgrading | Transports::BrokenUpgrade => json!(["websocket"]),
            _ => json!([]),
        };
        format!(
            "0{}",
            json!({
                "sid": sid,
                "upgrades": upgrades,
                "pingInterval": 300,
                "pingTimeout": 600,
                "maxPayload": 1_000_000,
            })
        )
    }

    fn create(&self) -> (String, Arc<Session>) {
        let sid = format!("engine-{}", self.next.fetch_add(1, Ordering::SeqCst));
        let session = Arc::new(Session::default());
        self.sessions
            .lock()
            .unwrap()
            .insert(sid.clone(), Arc::clone(&session));
        (sid, session)
    }

    fn session(&self, sid: &str) -> Option<Arc<Session>> {
        self.sessions.lock().unwrap().get(sid).cloned()
    }
}

async fn connection(stream: TcpStream, hub: Arc<Hub>) {
    // Look at the request without taking it, to tell WebSocket from HTTP.
    let mut peeked = vec![0_u8; 8192];
    let head = loop {
        let Ok(read) = stream.peek(&mut peeked).await else {
            return;
        };
        if read == 0 {
            return;
        }
        let text = String::from_utf8_lossy(&peeked[..read]).into_owned();
        if let Some(end) = text.find("\r\n\r\n") {
            break text[..end].to_owned();
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    };
    if head.to_ascii_lowercase().contains("upgrade: websocket") {
        websocket_session(stream, hub).await;
    } else {
        http_session(stream, hub).await;
    }
}

// ---- Long polling ------------------------------------------------------------------

struct Http {
    method: String,
    query: HashMap<String, String>,
    cookie: String,
    body: String,
}

async fn read_request(stream: &mut TcpStream) -> Option<Http> {
    let mut head = Vec::new();
    let mut byte = [0_u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte).await.ok()? == 0 {
            return None;
        }
        head.push(byte[0]);
    }
    let head = String::from_utf8_lossy(&head).into_owned();
    let mut lines = head.lines();
    let mut first = lines.next()?.split(' ');
    let method = first.next()?.to_owned();
    let target = first.next()?;
    let url = url::Url::parse(&format!("http://test{target}")).ok()?;
    let mut cookie = String::new();
    let mut length = 0;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        match name.to_ascii_lowercase().as_str() {
            "cookie" => value.trim().clone_into(&mut cookie),
            "content-length" => length = value.trim().parse().unwrap_or(0),
            _ => {}
        }
    }
    let mut body = vec![0_u8; length];
    stream.read_exact(&mut body).await.ok()?;
    Some(Http {
        method,
        query: url.query_pairs().into_owned().collect(),
        cookie,
        body: String::from_utf8_lossy(&body).into_owned(),
    })
}

async fn respond(stream: &mut TcpStream, status: &str, extra: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=UTF-8\r\nContent-Length: {}\r\n{extra}\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
}

async fn http_session(mut stream: TcpStream, hub: Arc<Hub>) {
    while let Some(request) = read_request(&mut stream).await {
        let transport = request
            .query
            .get("transport")
            .map_or("none", String::as_str)
            .to_owned();
        hub.seen.lock().unwrap().push(format!(
            "{} {transport} cookie={}",
            request.method, request.cookie
        ));
        if hub.transports == Transports::Websocket || transport != "polling" {
            respond(
                &mut stream,
                "400 Bad Request",
                "Connection: close\r\n",
                r#"{"code":0,"message":"Transport unknown"}"#,
            )
            .await;
            return;
        }
        let Some(sid) = request.query.get("sid") else {
            let (sid, _) = hub.create();
            let cookie = format!("Set-Cookie: io={sid}; Path=/\r\n");
            respond(&mut stream, "200 OK", &cookie, &hub.open_packet(&sid)).await;
            continue;
        };
        let Some(session) = hub.session(sid) else {
            respond(
                &mut stream,
                "400 Bad Request",
                "",
                r#"{"code":1,"message":"Session ID unknown"}"#,
            )
            .await;
            continue;
        };
        if request.method == "POST" {
            for packet in request.body.split('\u{1e}') {
                let (replies, _) = react(packet);
                for reply in replies {
                    session.deliver(reply);
                }
            }
            respond(&mut stream, "200 OK", "", "ok").await;
            continue;
        }
        // Hold the request until there is something to say, or a ping is due.
        if session.outbox.lock().unwrap().is_empty() {
            let _ = tokio::time::timeout(Duration::from_millis(100), session.wake.notified()).await;
        }
        let packets: Vec<String> = session.outbox.lock().unwrap().drain(..).collect();
        let body = if packets.is_empty() {
            "2".to_owned()
        } else {
            packets.join("\u{1e}")
        };
        respond(&mut stream, "200 OK", "", &body).await;
    }
}

// ---- WebSocket ---------------------------------------------------------------------

async fn websocket_session(stream: TcpStream, hub: Arc<Hub>) {
    let mut sid = None;
    let seen = Arc::clone(&hub.seen);
    let transports = hub.transports;
    #[allow(clippy::result_large_err)]
    let callback = |request: &Request, response: Response| {
        let query: HashMap<String, String> =
            url::Url::parse(&format!("http://test{}", request.uri()))
                .map(|url| url.query_pairs().into_owned().collect())
                .unwrap_or_default();
        let cookie = request
            .headers()
            .get("cookie")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        seen.lock()
            .unwrap()
            .push(format!("GET websocket cookie={cookie}"));
        let usable = request.uri().path() == "/socket.io/"
            && query.get("EIO").is_some_and(|v| v == "4")
            && query.get("transport").is_some_and(|v| v == "websocket");
        if !usable || matches!(transports, Transports::Polling | Transports::BrokenUpgrade) {
            let mut refusal = ErrorResponse::new(Some("no WebSocket here".into()));
            *refusal.status_mut() = StatusCode::BAD_REQUEST;
            return Err(refusal);
        }
        sid = query.get("sid").cloned();
        Ok(response)
    };
    let Ok(socket) = accept_hdr_async(stream, callback).await else {
        return;
    };
    let (mut tx, mut rx) = socket.split();
    let (sender, mut outgoing) = mpsc::unbounded_channel();

    let session = if let Some(sid) = sid {
        // An upgrade: probe, let the open GET go, then take over.
        let Some(session) = hub.session(&sid) else {
            return;
        };
        loop {
            let Some(Ok(Frame::Text(text))) = rx.next().await else {
                return;
            };
            match text.as_str() {
                "2probe" => {
                    if tx.send(Frame::text("3probe")).await.is_err() {
                        return;
                    }
                    session.outbox.lock().unwrap().push_back("6".into());
                    session.wake.notify_one();
                }
                "5" => break,
                _ => {}
            }
        }
        *session.websocket.lock().unwrap() = Some(sender);
        let leftover: Vec<String> = session.outbox.lock().unwrap().drain(..).collect();
        for packet in leftover {
            session.deliver(Reply::Text(packet));
        }
        session
    } else {
        let (sid, session) = hub.create();
        *session.websocket.lock().unwrap() = Some(sender);
        if tx.send(Frame::text(hub.open_packet(&sid))).await.is_err() {
            return;
        }
        session
    };

    let mut ping = tokio::time::interval(Duration::from_millis(100));
    loop {
        tokio::select! {
            _ = ping.tick() => {
                if tx.send(Frame::text("2")).await.is_err() {
                    return;
                }
            }
            Some(frame) = outgoing.recv() => {
                if tx.send(frame).await.is_err() {
                    return;
                }
            }
            frame = rx.next() => {
                let Some(Ok(frame)) = frame else {
                    return;
                };
                let Frame::Text(text) = frame else {
                    continue;
                };
                let (replies, close) = react(text.as_str());
                for reply in replies {
                    session.deliver(reply);
                }
                if close {
                    return;
                }
            }
        }
    }
}

// ---- Socket.IO ---------------------------------------------------------------------

/// What the server does about an Engine.IO packet: the packets it answers
/// with, and whether the client has gone.
fn react(packet: &str) -> (Vec<Reply>, bool) {
    let Some(packet) = packet.strip_prefix('4') else {
        return (Vec::new(), false);
    };
    let (kind, rest) = packet.split_at(1);
    let (namespace, rest) = match rest.strip_prefix('/') {
        Some(_) => {
            let end = rest.find(',').unwrap_or(rest.len());
            (&rest[..end], rest.get(end + 1..).unwrap_or_default())
        }
        None => ("/", rest),
    };
    let prefix = if namespace == "/" {
        String::new()
    } else {
        format!("{namespace},")
    };
    let mut replies = Vec::new();
    match kind {
        "0" => {
            let auth: Value = serde_json::from_str(rest).unwrap_or_default();
            if auth["token"] == "secret" {
                replies.push(Reply::Text(format!(r#"40{prefix}{{"sid":"socket-1"}}"#)));
                replies.push(Reply::Text(format!(r#"42{prefix}["other",1]"#)));
                replies.push(Reply::Text(format!(
                    r#"42{prefix}["welcome",{{"hi":true}}]"#
                )));
            } else {
                replies.push(Reply::Text(format!(
                    r#"44{prefix}{{"message":"bad token"}}"#
                )));
            }
        }
        "2" => {
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            let id = &rest[..digits];
            let array: Vec<Value> = serde_json::from_str(&rest[digits..]).unwrap_or_default();
            let args = Value::Array(array.iter().skip(1).cloned().collect());
            match array.first().and_then(Value::as_str) {
                Some("echo") => {
                    if !id.is_empty() {
                        replies.push(Reply::Text(format!("43{prefix}{id}{args}")));
                    }
                    let mut event = vec![json!("echo")];
                    event.extend(array.iter().skip(1).cloned());
                    replies.push(Reply::Text(format!("42{prefix}{}", Value::Array(event))));
                }
                Some("binary") => {
                    replies.push(Reply::Text(format!(
                        r#"451-{prefix}["file",{{"_placeholder":true,"num":0}}]"#
                    )));
                    replies.push(Reply::Binary(vec![1_u8, 2, 3]));
                }
                Some("kick") => replies.push(Reply::Text(format!("41{prefix}"))),
                _ => {}
            }
        }
        "1" => return (replies, true),
        _ => {}
    }
    (replies, false)
}
