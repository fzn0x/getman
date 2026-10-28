//! WebSocket connections, and the WebSocket transport under Socket.IO.

use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use futures_util::{SinkExt as _, StreamExt as _};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;
use tokio_tungstenite::tungstenite::error::{ProtocolError, SubProtocolError};
use tokio_tungstenite::tungstenite::handshake::client::Response as Handshake;
use tokio_tungstenite::tungstenite::http::{HeaderName, HeaderValue};
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, WebSocketConfig};
use tokio_tungstenite::tungstenite::{self, Message as Frame};

use super::dial::{self, Io};
use super::{
    Direction, EventKind, Job, MAX_MESSAGE_BYTES, Message, MessageFormat, Outbound, SavedMessage,
    Sink, header_list, within,
};
use crate::error::{Error, Result};
use crate::http::{Header, HttpClient, Transport};

/// An open WebSocket.
pub(super) type Socket = WebSocketStream<Box<dyn Io>>;

/// Headers the handshake writes itself; a request's own are left out.
const HANDSHAKE_HEADERS: [&str; 5] = [
    "host",
    "connection",
    "upgrade",
    "sec-websocket-version",
    "sec-websocket-key",
];

/// How long to wait for the server to answer a close.
const CLOSE_WAIT: Duration = Duration::from_secs(2);

/// The WebSocket URL for a request's URL: `ws` and `wss` as they are,
/// `http` and `https` as their WebSocket equivalents.
pub(super) fn url(raw: &str) -> Result<String> {
    let mut url = url::Url::parse(raw)
        .map_err(|e| Error::Invalid(format!("{raw:?} is not a valid URL: {e}")))?;
    let scheme = match url.scheme() {
        "ws" | "http" => "ws",
        "wss" | "https" => "wss",
        other => {
            return Err(Error::Invalid(format!(
                "a WebSocket URL starts with ws:// or wss://, not {other}://"
            )));
        }
    };
    if url.set_scheme(scheme).is_err() {
        return Err(Error::Invalid(format!(
            "{raw:?} is not a valid WebSocket URL"
        )));
    }
    Ok(url.to_string())
}

/// Opens a WebSocket: TCP, through the proxy the transport settings call
/// for, TLS for `wss`, then the handshake with these headers, offering
/// these subprotocols. The cookie jar's cookies for the URL go with the
/// handshake, and those the server sets are stored, unless the settings
/// turn cookies off.
pub(super) async fn open(
    http: &HttpClient,
    transport: &Transport,
    url: &str,
    headers: &[Header],
    subprotocols: &[String],
) -> Result<(Socket, Handshake), String> {
    let parsed = url::Url::parse(url).map_err(|e| format!("{url:?} is not a valid URL: {e}"))?;
    let secure = parsed.scheme() == "wss";
    let host = parsed
        .host_str()
        .ok_or_else(|| format!("{url:?} has no host"))?
        .to_owned();
    let tcp = dial::connect(&parsed, transport).await?;
    let stream: Box<dyn Io> = if secure {
        dial::secure(tcp, &host, transport.verify_tls).await?
    } else {
        tcp
    };

    let mut request = url
        .into_client_request()
        .map_err(|e| format!("{url:?} cannot be opened as a WebSocket: {e}"))?;
    let offered: Vec<&str> = subprotocols
        .iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();
    for header in headers {
        let name = header.name.trim();
        let ours = HANDSHAKE_HEADERS
            .iter()
            .any(|h| name.eq_ignore_ascii_case(h))
            || (!offered.is_empty() && name.eq_ignore_ascii_case("sec-websocket-protocol"));
        if ours {
            continue;
        }
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| format!("{:?} is not a valid header name", header.name))?;
        let value = HeaderValue::from_str(&header.value).map_err(|_| {
            format!(
                "the value of header {} contains characters headers cannot carry",
                header.name
            )
        })?;
        request.headers_mut().append(name, value);
    }
    let jar_url = jar_url(&parsed);
    if transport.cookies
        && !headers
            .iter()
            .any(|h| h.name.trim().eq_ignore_ascii_case("cookie"))
        && let Some(cookies) = jar_url.as_ref().and_then(|url| http.cookie_header(url))
        && let Ok(value) = HeaderValue::from_str(&cookies)
    {
        request.headers_mut().insert("cookie", value);
    }
    if !offered.is_empty() {
        let value = HeaderValue::from_str(&offered.join(", "))
            .map_err(|_| "a subprotocol contains characters headers cannot carry".to_owned())?;
        request
            .headers_mut()
            .insert("sec-websocket-protocol", value);
    }
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_MESSAGE_BYTES))
        .max_frame_size(Some(MAX_MESSAGE_BYTES));
    let answered = tokio_tungstenite::client_async_with_config(request, stream, Some(config)).await;
    if transport.cookies
        && let Some(jar_url) = &jar_url
    {
        // A refusal can set cookies too, such as a session to sign in with.
        let headers = match &answered {
            Ok((_, handshake)) => Some(handshake.headers()),
            Err(tungstenite::Error::Http(response)) => Some(response.headers()),
            Err(_) => None,
        };
        if let Some(headers) = headers {
            http.store_cookies(
                jar_url,
                headers
                    .get_all("set-cookie")
                    .iter()
                    .filter_map(|v| v.to_str().ok()),
            );
        }
    }
    answered.map_err(|e| describe(&e, &offered))
}

/// The URL the cookie jar knows a WebSocket URL by: the HTTP one it
/// upgrades.
fn jar_url(url: &url::Url) -> Option<url::Url> {
    let mut jar = url.clone();
    jar.set_scheme(if url.scheme() == "wss" {
        "https"
    } else {
        "http"
    })
    .ok()?;
    Some(jar)
}

/// Explains a WebSocket error in one line.
pub(super) fn describe(error: &tungstenite::Error, offered: &[&str]) -> String {
    match error {
        tungstenite::Error::Http(response) => {
            let status = response.status();
            let mut message = format!(
                "The server answered {} {} instead of opening a WebSocket",
                status.as_u16(),
                status.canonical_reason().unwrap_or_default()
            );
            let body = response
                .body()
                .as_deref()
                .map(String::from_utf8_lossy)
                .unwrap_or_default();
            let body = body.trim();
            if !body.is_empty() {
                let excerpt: String = body.chars().take(200).collect();
                message.push_str(": ");
                message.push_str(&excerpt);
            }
            message
        }
        tungstenite::Error::Protocol(ProtocolError::SecWebSocketSubProtocolError(
            SubProtocolError::NoSubProtocol,
        )) => format!(
            "The server accepted none of the subprotocols offered ({})",
            offered.join(", ")
        ),
        tungstenite::Error::Protocol(ProtocolError::SecWebSocketSubProtocolError(_)) => {
            "The server chose a subprotocol that was not offered".into()
        }
        tungstenite::Error::Capacity(e) => format!(
            "The server sent more than Getman reads at once ({e}); messages may be up to {} MB",
            MAX_MESSAGE_BYTES / 1024 / 1024
        ),
        tungstenite::Error::Io(e) => format!("The connection failed: {e}"),
        tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed => {
            "The connection is closed".into()
        }
        other => format!("The WebSocket failed: {other}"),
    }
}

/// Holds a WebSocket connection until it closes.
pub(super) async fn run(job: &mut Job, sink: &mut Sink) {
    let started = Instant::now();
    let gone = sink.watch();
    let opening = within(
        job.connect_timeout(),
        open(
            &job.http,
            &job.transport,
            &job.outgoing.url,
            &job.outgoing.headers,
            &job.request.websocket.subprotocols,
        ),
    );
    let opened = tokio::select! {
        opened = opening => opened,
        () = job.close.notified() => {
            sink.closed(None, String::new(), true).await;
            return;
        }
        () = gone.closed() => return,
    };
    let (socket, handshake) = match opened {
        Ok(opened) => opened,
        Err(message) => {
            sink.fail(message).await;
            return;
        }
    };
    let open = EventKind::Open {
        status: handshake.status().as_u16(),
        headers: header_list(handshake.headers()),
        protocol: handshake
            .headers()
            .get("sec-websocket-protocol")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned),
        time_ms: millis(started.elapsed()),
    };
    if !sink.emit(open).await {
        return;
    }

    let (mut writer, mut reader) = socket.split();
    loop {
        tokio::select! {
            () = job.close.notified() => {
                let (code, reason) = close(&mut writer, &mut reader).await;
                sink.closed(code, reason, true).await;
                return;
            }
            command = job.commands.recv() => {
                let Some(command) = command else {
                    let _ = writer.send(Frame::Close(None)).await;
                    return;
                };
                let saved = match command {
                    Outbound::Message(saved) => saved,
                    Outbound::Emit(_) => {
                        sink.emit(EventKind::Warning {
                            message: "Socket.IO events can be emitted only on Socket.IO requests; this is a plain WebSocket".into(),
                        })
                        .await;
                        continue;
                    }
                };
                let text = job.render(&saved.content, sink).await;
                match frame(&saved, text) {
                    Ok((frame, message)) => {
                        if let Err(e) = writer.send(frame).await {
                            sink.fail(describe(&e, &[])).await;
                            return;
                        }
                        if !sink.emit(EventKind::Message(message)).await {
                            return;
                        }
                    }
                    Err(problem) => {
                        sink.emit(EventKind::Warning { message: problem }).await;
                    }
                }
            }
            frame = reader.next() => {
                let message = match frame {
                    Some(Ok(Frame::Text(text))) => {
                        Message::new(Direction::Received, text.as_bytes().to_vec(), false)
                    }
                    Some(Ok(Frame::Binary(bytes))) => {
                        Message::new(Direction::Received, bytes.to_vec(), true)
                    }
                    Some(Ok(Frame::Close(frame))) => {
                        let (code, reason) = close_details(frame.as_ref());
                        // Let the reply to the close go out.
                        let _ = tokio::time::timeout(CLOSE_WAIT, writer.close()).await;
                        sink.closed(code, reason, false).await;
                        return;
                    }
                    // Pings are answered by the WebSocket library.
                    Some(Ok(_)) => continue,
                    Some(Err(e)) => {
                        sink.fail(describe(&e, &[])).await;
                        return;
                    }
                    None => {
                        sink.closed(None, String::new(), false).await;
                        return;
                    }
                };
                if !sink.received(message).await {
                    let _ = writer.send(Frame::Close(None)).await;
                    return;
                }
            }
            () = gone.closed() => {
                let _ = writer.send(Frame::Close(None)).await;
                return;
            }
        }
    }
}

/// Closes normally and waits a little for the server's answer, which
/// carries the code and reason the connection ends with.
pub(super) async fn close(
    writer: &mut futures_util::stream::SplitSink<Socket, Frame>,
    reader: &mut futures_util::stream::SplitStream<Socket>,
) -> (Option<u16>, String) {
    let frame = CloseFrame {
        code: CloseCode::Normal,
        reason: "".into(),
    };
    if writer.send(Frame::Close(Some(frame))).await.is_err() {
        return (Some(1000), String::new());
    }
    let answer = tokio::time::timeout(CLOSE_WAIT, async {
        while let Some(Ok(frame)) = reader.next().await {
            if let Frame::Close(frame) = frame {
                return Some(close_details(frame.as_ref()));
            }
        }
        None
    })
    .await;
    match answer {
        Ok(Some((code, reason))) => (code.or(Some(1000)), reason),
        _ => (Some(1000), String::new()),
    }
}

fn close_details(frame: Option<&CloseFrame>) -> (Option<u16>, String) {
    frame.map_or((None, String::new()), |frame| {
        (Some(u16::from(frame.code)), frame.reason.to_string())
    })
}

/// The frame for a message and how it is reported, or why it cannot be
/// sent. `text` is its content with the variables replaced.
fn frame(saved: &SavedMessage, text: String) -> Result<(Frame, Message), String> {
    let (frame, mut message) = match saved.format {
        MessageFormat::Text => (
            Frame::text(text.clone()),
            Message::text(Direction::Sent, text),
        ),
        MessageFormat::Json => {
            if let Err(e) = serde_json::from_str::<serde_json::Value>(&text) {
                return Err(format!(
                    "The message is not valid JSON ({e}), so it was not sent"
                ));
            }
            (
                Frame::text(text.clone()),
                Message::text(Direction::Sent, text),
            )
        }
        MessageFormat::Binary => {
            let compact: String = text.split_whitespace().collect();
            let bytes = BASE64.decode(compact).map_err(|e| {
                format!("The message is not valid Base64 ({e}), so it was not sent")
            })?;
            (
                Frame::binary(bytes.clone()),
                Message::new(Direction::Sent, bytes, true),
            )
        }
    };
    message.name = Some(saved.name.trim().to_owned()).filter(|name| !name.is_empty());
    Ok((frame, message))
}

fn millis(duration: Duration) -> f64 {
    (duration.as_secs_f64() * 1000.0 * 100.0).round() / 100.0
}
