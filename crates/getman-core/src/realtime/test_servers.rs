//! Local WebSocket and Socket.IO servers the realtime tests connect to.

use std::net::SocketAddr;
use std::time::Duration;

use futures_util::{SinkExt as _, StreamExt as _};
use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::accept_hdr_async;
use tokio_tungstenite::tungstenite::Message as Frame;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::{HeaderValue, StatusCode};
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

pub(super) async fn listen<F, Fut>(session: F) -> SocketAddr
where
    F: Fn(TcpStream) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(session(stream));
        }
    });
    addr
}

/// A WebSocket server. It first sends what the handshake carried, as JSON,
/// then echoes every message, except these texts: `close` closes with
/// 4001 "bye", `ping` sends a ping (the pong is reported back), `flood`
/// sends 2000 messages at once and `done?` answers `done`. The path
/// `/refuse` is refused with 403 "go away", and the subprotocol `chat.v2`
/// is accepted when offered.
pub(super) async fn websocket() -> SocketAddr {
    listen(echo).await
}

async fn echo(stream: TcpStream) {
    let mut seen = Value::Null;
    #[allow(clippy::result_large_err)]
    let callback = |request: &Request, mut response: Response| {
        let header = |name: &str| {
            request
                .headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default()
                .to_owned()
        };
        if request.uri().path() == "/refuse" {
            let mut refusal = ErrorResponse::new(Some("go away".into()));
            *refusal.status_mut() = StatusCode::FORBIDDEN;
            return Err(refusal);
        }
        let offered = header("sec-websocket-protocol");
        if offered.split(',').any(|p| p.trim() == "chat.v2") {
            response.headers_mut().insert(
                "sec-websocket-protocol",
                HeaderValue::from_static("chat.v2"),
            );
        }
        if request.uri().path() == "/session" {
            response.headers_mut().append(
                "set-cookie",
                HeaderValue::from_static("session=abc123; Path=/"),
            );
        }
        seen = json!({
            "path": request.uri().path(),
            "query": request.uri().query().unwrap_or_default(),
            "authorization": header("authorization"),
            "x_test": header("x-test"),
            "protocol": offered,
            "cookie": header("cookie"),
        });
        Ok(response)
    };
    let Ok(mut socket) = accept_hdr_async(stream, callback).await else {
        return;
    };
    let _ = socket.send(Frame::text(seen.to_string())).await;
    while let Some(Ok(frame)) = socket.next().await {
        let reply = match frame {
            Frame::Text(text) => match text.as_str() {
                "close" => {
                    let frame = CloseFrame {
                        code: CloseCode::from(4001),
                        reason: "bye".into(),
                    };
                    let _ = socket.close(Some(frame)).await;
                    continue;
                }
                "ping" => Frame::Ping("are you there".into()),
                "flood" => {
                    for n in 0..2000 {
                        let _ = socket.send(Frame::text(format!("flood {n}"))).await;
                    }
                    continue;
                }
                "done?" => Frame::text("done"),
                _ => Frame::Text(text),
            },
            Frame::Binary(bytes) => Frame::Binary(bytes),
            Frame::Pong(payload) => {
                Frame::text(format!("pong: {}", String::from_utf8_lossy(&payload)))
            }
            _ => continue,
        };
        if socket.send(reply).await.is_err() {
            return;
        }
    }
}

/// A Socket.IO v4 server, over WebSocket only. It pings every 100 ms. A
/// socket joins any namespace with the auth payload `{"token": "secret"}`
/// and is greeted with the events `other` and `welcome`. The event `echo`
/// is acknowledged and sent back, `binary` is answered with a binary
/// `file` event, and `kick` disconnects the socket.
pub(super) async fn socketio() -> SocketAddr {
    listen(socketio_session).await
}

async fn socketio_session(stream: TcpStream) {
    let mut engine = false;
    #[allow(clippy::result_large_err)]
    let callback = |request: &Request, response: Response| {
        let query = request.uri().query().unwrap_or_default();
        engine = request.uri().path() == "/socket.io/"
            && query.contains("EIO=4")
            && query.contains("transport=websocket");
        Ok(response)
    };
    let Ok(socket) = accept_hdr_async(stream, callback).await else {
        return;
    };
    let (mut tx, mut rx) = socket.split();
    if !engine {
        let _ = tx.send(Frame::Close(None)).await;
        return;
    }
    let open = r#"0{"sid":"engine-1","upgrades":[],"pingInterval":100,"pingTimeout":200,"maxPayload":1000000}"#;
    let _ = tx.send(Frame::text(open)).await;
    let mut ping = tokio::time::interval(Duration::from_millis(100));
    loop {
        let frame = tokio::select! {
            _ = ping.tick() => {
                if tx.send(Frame::text("2")).await.is_err() {
                    return;
                }
                continue;
            }
            frame = rx.next() => frame,
        };
        let Some(Ok(frame)) = frame else {
            return;
        };
        let Frame::Text(text) = frame else {
            continue;
        };
        let Some(packet) = text.as_str().strip_prefix('4') else {
            continue;
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
                    replies.push(Frame::text(format!(r#"40{prefix}{{"sid":"socket-1"}}"#)));
                    replies.push(Frame::text(format!(r#"42{prefix}["other",1]"#)));
                    replies.push(Frame::text(format!(
                        r#"42{prefix}["welcome",{{"hi":true}}]"#
                    )));
                } else {
                    replies.push(Frame::text(format!(
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
                            replies.push(Frame::text(format!("43{prefix}{id}{args}")));
                        }
                        let mut event = vec![json!("echo")];
                        event.extend(array.iter().skip(1).cloned());
                        replies.push(Frame::text(format!("42{prefix}{}", Value::Array(event))));
                    }
                    Some("binary") => {
                        replies.push(Frame::text(format!(
                            r#"451-{prefix}["file",{{"_placeholder":true,"num":0}}]"#
                        )));
                        replies.push(Frame::binary(vec![1_u8, 2, 3]));
                    }
                    Some("kick") => replies.push(Frame::text(format!("41{prefix}"))),
                    _ => {}
                }
            }
            "1" => return,
            _ => {}
        }
        for reply in replies {
            if tx.send(reply).await.is_err() {
                return;
            }
        }
    }
}
