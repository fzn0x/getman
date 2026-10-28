//! The local WebSocket server the realtime tests connect to.

use std::net::SocketAddr;

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
