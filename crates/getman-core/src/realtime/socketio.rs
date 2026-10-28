//! Socket.IO v4 over WebSocket: the Engine.IO handshake and heartbeat, and
//! the packets of one namespace.
//!
//! Engine.IO carries one packet per WebSocket text frame, its type in the
//! first character: `0` open, `1` close, `2` ping, `3` pong, `4` message.
//! A message holds a Socket.IO packet, such as `2/chat,7["event",1]`: the
//! type (`2` event), the namespace unless it is `/`, an acknowledgement id
//! if one is wanted, then JSON. Binary packets (`5`, `6`) say how many
//! binary frames follow; their data refers to each with a placeholder.

use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use futures_util::{SinkExt as _, StreamExt as _};
use serde_json::Value;
use tokio_tungstenite::tungstenite::Message as Frame;

use super::websocket::{self, Socket};
use super::{
    Direction, EventKind, Job, Message, Outbound, SavedEmit, Sink, SocketIoSettings, header_list,
    within,
};
use crate::error::{Error, Result};
use crate::vars::VarStack;

/// Event names Socket.IO keeps for itself.
const RESERVED: [&str; 6] = [
    "connect",
    "connect_error",
    "disconnect",
    "disconnecting",
    "newListener",
    "removeListener",
];

/// Socket.IO packet types.
const CONNECT: u8 = 0;
const DISCONNECT: u8 = 1;
const EVENT: u8 = 2;
const ACK: u8 = 3;
const CONNECT_ERROR: u8 = 4;
const BINARY_EVENT: u8 = 5;
const BINARY_ACK: u8 = 6;

/// The WebSocket URL of the Engine.IO endpoint for a request's URL, and
/// the namespace to join.
pub(super) fn endpoint(
    raw: &str,
    settings: &SocketIoSettings,
    vars: &VarStack,
) -> Result<(String, String)> {
    let mut url = url::Url::parse(raw)
        .map_err(|e| Error::Invalid(format!("{raw:?} is not a valid URL: {e}")))?;
    let scheme = match url.scheme() {
        "http" | "ws" => "ws",
        "https" | "wss" => "wss",
        other => {
            return Err(Error::Invalid(format!(
                "a Socket.IO URL starts with http://, https://, ws:// or wss://, not {other}://"
            )));
        }
    };
    if url.set_scheme(scheme).is_err() {
        return Err(Error::Invalid(format!(
            "{raw:?} is not a valid Socket.IO URL"
        )));
    }
    let named = vars.render(settings.namespace.trim());
    let namespace = normalize(if named.is_empty() { url.path() } else { &named });
    let path = vars.render(settings.path.trim());
    let path = if path.is_empty() {
        "/socket.io/".to_owned()
    } else {
        format!("/{}/", path.trim_matches('/'))
    };
    url.set_path(&path);
    url.query_pairs_mut()
        .append_pair("EIO", "4")
        .append_pair("transport", "websocket");
    Ok((url.to_string(), namespace))
}

/// A namespace as Socket.IO writes it: a leading slash, no trailing one.
fn normalize(namespace: &str) -> String {
    let trimmed = namespace.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        "/".to_owned()
    } else if trimmed.starts_with('/') {
        trimmed.to_owned()
    } else {
        format!("/{trimmed}")
    }
}

/// A Socket.IO packet.
#[derive(Debug, Clone, PartialEq)]
struct Packet {
    kind: u8,
    attachments: usize,
    namespace: String,
    id: Option<u64>,
    data: Option<Value>,
}

/// Reads a Socket.IO packet: the text after Engine.IO's `4`.
fn decode(text: &str) -> Result<Packet, String> {
    let bad = || format!("The server sent a packet Getman cannot read: {text:.80}");
    let mut rest = text;
    let kind = rest
        .chars()
        .next()
        .and_then(|c| c.to_digit(10))
        .and_then(|d| u8::try_from(d).ok())
        .filter(|kind| *kind <= BINARY_ACK)
        .ok_or_else(bad)?;
    rest = &rest[1..];
    let mut attachments = 0;
    if kind == BINARY_EVENT || kind == BINARY_ACK {
        let (count, after) = rest.split_once('-').ok_or_else(bad)?;
        attachments = count.parse().map_err(|_| bad())?;
        rest = after;
    }
    let mut namespace = "/".to_owned();
    if rest.starts_with('/') {
        let end = rest.find(',').unwrap_or(rest.len());
        namespace = rest[..end].to_owned();
        rest = rest.get(end + 1..).unwrap_or_default();
    }
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    let id = if digits > 0 {
        Some(rest[..digits].parse().map_err(|_| bad())?)
    } else {
        None
    };
    rest = &rest[digits..];
    let data = if rest.is_empty() {
        None
    } else {
        Some(serde_json::from_str(rest).map_err(|_| bad())?)
    };
    Ok(Packet {
        kind,
        attachments,
        namespace,
        id,
        data,
    })
}

/// Writes a Socket.IO packet as an Engine.IO message.
fn encode(kind: u8, namespace: &str, id: Option<u64>, data: Option<&Value>) -> String {
    let mut out = format!("4{kind}");
    if namespace != "/" {
        out.push_str(namespace);
        out.push(',');
    }
    if let Some(id) = id {
        out.push_str(&id.to_string());
    }
    if let Some(data) = data {
        out.push_str(&data.to_string());
    }
    out
}

/// Puts binary attachments where a packet's placeholders refer to them,
/// as `{"base64": "..."}`.
fn fill_placeholders(value: &mut Value, attachments: &[Vec<u8>]) {
    match value {
        Value::Object(map) => {
            let placeholder = map.get("_placeholder").and_then(Value::as_bool) == Some(true);
            let number = map
                .get("num")
                .and_then(Value::as_u64)
                .and_then(|n| usize::try_from(n).ok());
            if placeholder && let Some(bytes) = number.and_then(|n| attachments.get(n)) {
                *value = serde_json::json!({ "base64": BASE64.encode(bytes) });
                return;
            }
            for item in map.values_mut() {
                fill_placeholders(item, attachments);
            }
        }
        Value::Array(items) => {
            for item in items {
                fill_placeholders(item, attachments);
            }
        }
        _ => {}
    }
}

/// Engine.IO's heartbeat, from its open packet.
#[derive(Debug, Clone, Copy)]
struct Heartbeat {
    interval: Duration,
    timeout: Duration,
}

/// What joining the namespace ended in.
enum Joined {
    Yes { sid: String, heartbeat: Heartbeat },
    No(String),
}

/// Holds a Socket.IO connection until it closes.
pub(super) async fn run(job: &mut Job, sink: &mut Sink) {
    let started = Instant::now();
    let gone = sink.watch();
    let namespace = job.namespace.clone();
    let auth = job.vars.interpolate(job.request.socketio.auth.trim());
    let auth = if auth.text.is_empty() {
        None
    } else {
        match serde_json::from_str::<Value>(&auth.text) {
            Ok(value @ Value::Object(_)) => Some(value),
            _ => {
                sink.fail(
                    "The Socket.IO auth payload must be a JSON object, such as {\"token\": \"...\"}"
                        .into(),
                )
                .await;
                return;
            }
        }
    };
    let opening = within(job.connect_timeout(), async {
        let (mut socket, handshake) = websocket::open(
            &job.http,
            &job.transport,
            &job.outgoing.url,
            &job.outgoing.headers,
            &[],
        )
        .await?;
        let joined = join(&mut socket, &namespace, auth.as_ref()).await?;
        Ok((socket, handshake, joined))
    });
    let opened = tokio::select! {
        opened = opening => opened,
        () = job.close.notified() => {
            sink.closed(None, String::new(), true).await;
            return;
        }
        () = gone.closed() => return,
    };
    let (socket, handshake, joined) = match opened {
        Ok(opened) => opened,
        Err(message) => {
            sink.fail(message).await;
            return;
        }
    };
    let (sid, heartbeat) = match joined {
        Joined::Yes { sid, heartbeat } => (sid, heartbeat),
        Joined::No(message) => {
            sink.fail(format!(
                "The server refused to let Getman join {namespace}: {message}"
            ))
            .await;
            return;
        }
    };
    let open = EventKind::Open {
        status: handshake.status().as_u16(),
        headers: header_list(handshake.headers()),
        protocol: None,
        time_ms: (started.elapsed().as_secs_f64() * 100_000.0).round() / 100.0,
    };
    if !sink.emit(open).await {
        return;
    }
    let mut joined_note = format!("Joined namespace {namespace}");
    if !sid.is_empty() {
        joined_note.push_str(&format!(" as socket {sid}"));
    }
    if !sink
        .emit(EventKind::Note {
            message: joined_note,
        })
        .await
    {
        return;
    }

    let events = job.request.socketio.events.clone();
    let (mut writer, mut reader) = socket.split();
    let mut next_ack = 0_u64;
    let mut last_ping = Instant::now();
    let mut pending: Option<(Packet, Vec<Vec<u8>>)> = None;
    loop {
        let deadline = last_ping + heartbeat.interval + heartbeat.timeout;
        tokio::select! {
            () = job.close.notified() => {
                let _ = writer.send(Frame::text(encode(DISCONNECT, &namespace, None, None))).await;
                let (code, reason) = websocket::close(&mut writer, &mut reader).await;
                sink.closed(code, reason, true).await;
                return;
            }
            command = job.commands.recv() => {
                let Some(command) = command else {
                    let _ = writer.send(Frame::Close(None)).await;
                    return;
                };
                let saved = match command {
                    Outbound::Emit(saved) => saved,
                    Outbound::Message(_) => {
                        sink.emit(EventKind::Warning {
                            message: "This is a Socket.IO connection: emit an event instead of sending a raw message".into(),
                        })
                        .await;
                        continue;
                    }
                };
                match emit(job, sink, &saved, &namespace, &mut next_ack).await {
                    Ok(Some((packet, message))) => {
                        if let Err(e) = writer.send(Frame::text(packet)).await {
                            sink.fail(websocket::describe(&e, &[])).await;
                            return;
                        }
                        if !sink.emit(EventKind::Message(message)).await {
                            return;
                        }
                    }
                    Ok(None) => {}
                    Err(problem) => {
                        sink.emit(EventKind::Warning { message: problem }).await;
                    }
                }
            }
            frame = reader.next() => {
                let frame = match frame {
                    Some(Ok(frame)) => frame,
                    Some(Err(e)) => {
                        sink.fail(websocket::describe(&e, &[])).await;
                        return;
                    }
                    None => {
                        sink.closed(None, String::new(), false).await;
                        return;
                    }
                };
                let packet = match frame {
                    Frame::Text(text) => {
                        let text = text.as_str();
                        match text.as_bytes().first() {
                            Some(b'2') => {
                                last_ping = Instant::now();
                                let pong = format!("3{}", &text[1..]);
                                if writer.send(Frame::text(pong)).await.is_err() {
                                    sink.fail("The connection broke while answering a ping".into()).await;
                                    return;
                                }
                                continue;
                            }
                            Some(b'1') => {
                                sink.closed(None, "The server closed the connection".into(), false).await;
                                return;
                            }
                            Some(b'4') => match decode(&text[1..]) {
                                Ok(packet) => packet,
                                Err(problem) => {
                                    sink.emit(EventKind::Warning { message: problem }).await;
                                    continue;
                                }
                            },
                            _ => continue,
                        }
                    }
                    Frame::Binary(bytes) => {
                        let Some((packet, mut attachments)) = pending.take() else {
                            continue;
                        };
                        attachments.push(bytes.to_vec());
                        if attachments.len() < packet.attachments {
                            pending = Some((packet, attachments));
                            continue;
                        }
                        let mut packet = packet;
                        if let Some(data) = packet.data.as_mut() {
                            fill_placeholders(data, &attachments);
                        }
                        packet.kind = if packet.kind == BINARY_EVENT { EVENT } else { ACK };
                        packet
                    }
                    Frame::Close(frame) => {
                        let (code, reason) = frame
                            .map_or((None, String::new()), |f| (Some(u16::from(f.code)), f.reason.to_string()));
                        sink.closed(code, reason, false).await;
                        return;
                    }
                    _ => continue,
                };
                if packet.namespace != namespace {
                    continue;
                }
                if packet.attachments > 0 && (packet.kind == BINARY_EVENT || packet.kind == BINARY_ACK) {
                    pending = Some((packet, Vec::new()));
                    continue;
                }
                match packet.kind {
                    EVENT => {
                        let Some(Value::Array(mut items)) = packet.data else {
                            continue;
                        };
                        if items.is_empty() {
                            continue;
                        }
                        let name = match items.remove(0) {
                            Value::String(name) => name,
                            other => other.to_string(),
                        };
                        if !events.is_empty() && !events.iter().any(|e| e.trim() == name) {
                            continue;
                        }
                        let mut message = Message::text(Direction::Received, Value::Array(items).to_string());
                        message.event = Some(name);
                        message.ack = packet.id;
                        if !sink.received(message).await {
                            let _ = writer.send(Frame::Close(None)).await;
                            return;
                        }
                    }
                    ACK => {
                        let data = packet.data.unwrap_or(Value::Array(Vec::new()));
                        let mut message = Message::text(Direction::Received, data.to_string());
                        message.ack = packet.id;
                        if !sink.received(message).await {
                            let _ = writer.send(Frame::Close(None)).await;
                            return;
                        }
                    }
                    DISCONNECT => {
                        sink.closed(None, format!("The server disconnected the socket from {namespace}"), false).await;
                        let _ = writer.send(Frame::Close(None)).await;
                        return;
                    }
                    CONNECT_ERROR => {
                        sink.fail(format!("The server refused the socket: {}", refusal(packet.data.as_ref()))).await;
                        return;
                    }
                    _ => {}
                }
            }
            () = tokio::time::sleep_until(deadline.into()) => {
                sink.fail(format!(
                    "The server sent no heartbeat for {} ms, so the connection is taken as lost",
                    (heartbeat.interval + heartbeat.timeout).as_millis()
                ))
                .await;
                let _ = writer.send(Frame::Close(None)).await;
                return;
            }
            () = gone.closed() => {
                let _ = writer.send(Frame::Close(None)).await;
                return;
            }
        }
    }
}

/// Waits for Engine.IO to open, then joins the namespace.
async fn join(
    socket: &mut Socket,
    namespace: &str,
    auth: Option<&Value>,
) -> Result<Joined, String> {
    let mut heartbeat = None;
    while let Some(frame) = socket.next().await {
        let frame = frame.map_err(|e| websocket::describe(&e, &[]))?;
        let Frame::Text(text) = frame else {
            if let Frame::Close(_) = frame {
                break;
            }
            continue;
        };
        let text = text.as_str();
        match text.as_bytes().first() {
            Some(b'0') => {
                let open: Value = serde_json::from_str(&text[1..]).unwrap_or_default();
                let ms = |name: &str, default: u64| {
                    Duration::from_millis(open[name].as_u64().unwrap_or(default))
                };
                heartbeat = Some(Heartbeat {
                    interval: ms("pingInterval", 25_000),
                    timeout: ms("pingTimeout", 20_000),
                });
                socket
                    .send(Frame::text(encode(CONNECT, namespace, None, auth)))
                    .await
                    .map_err(|e| websocket::describe(&e, &[]))?;
            }
            Some(b'2') => {
                socket
                    .send(Frame::text(format!("3{}", &text[1..])))
                    .await
                    .map_err(|e| websocket::describe(&e, &[]))?;
            }
            Some(b'4') => {
                let packet = decode(&text[1..])?;
                if packet.namespace != namespace {
                    continue;
                }
                match packet.kind {
                    CONNECT => {
                        let sid = packet
                            .data
                            .as_ref()
                            .and_then(|d| d["sid"].as_str())
                            .unwrap_or_default()
                            .to_owned();
                        let heartbeat = heartbeat.ok_or_else(|| {
                            "The server joined the namespace before opening Engine.IO".to_owned()
                        })?;
                        return Ok(Joined::Yes { sid, heartbeat });
                    }
                    CONNECT_ERROR => return Ok(Joined::No(refusal(packet.data.as_ref()))),
                    _ => {}
                }
            }
            Some(b'1') => break,
            _ => {}
        }
    }
    Err("The server closed the connection before the socket joined its namespace".into())
}

/// The message of a refused connection.
fn refusal(data: Option<&Value>) -> String {
    match data {
        Some(Value::Object(map)) => map
            .get("message")
            .and_then(Value::as_str)
            .map_or_else(|| Value::Object(map.clone()).to_string(), str::to_owned),
        Some(Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
        None => "no reason given".into(),
    }
}

/// The packet for a saved event and how it is reported, `None` when there
/// is nothing to send, or why it cannot be sent.
async fn emit(
    job: &Job,
    sink: &mut Sink,
    saved: &SavedEmit,
    namespace: &str,
    next_ack: &mut u64,
) -> Result<Option<(String, Message)>, String> {
    let event = job.render(saved.event.trim(), sink).await;
    if event.is_empty() {
        return Err("The event has no name, so it was not sent".into());
    }
    if RESERVED.contains(&event.as_str()) {
        return Err(format!(
            "{event:?} is reserved by Socket.IO and cannot be emitted"
        ));
    }
    let text = job.render(&saved.args, sink).await;
    let args = if text.trim().is_empty() {
        Vec::new()
    } else {
        match serde_json::from_str::<Value>(&text) {
            Ok(Value::Array(items)) => items,
            Ok(value) => vec![value],
            Err(e) => {
                return Err(format!(
                    "The arguments are not valid JSON ({e}), so the event was not sent"
                ));
            }
        }
    };
    let ack = saved.ack.then(|| {
        let id = *next_ack;
        *next_ack += 1;
        id
    });
    let mut array = vec![Value::String(event.clone())];
    array.extend(args.iter().cloned());
    let packet = encode(EVENT, namespace, ack, Some(&Value::Array(array)));
    let mut message = Message::text(Direction::Sent, Value::Array(args).to_string());
    message.event = Some(event);
    message.ack = ack;
    message.name = Some(saved.name.trim().to_owned()).filter(|name| !name.is_empty());
    Ok(Some((packet, message)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn packets_read_and_write() {
        assert_eq!(
            decode(r#"2/chat,12["hello",{"a":1}]"#).unwrap(),
            Packet {
                kind: EVENT,
                attachments: 0,
                namespace: "/chat".into(),
                id: Some(12),
                data: Some(json!(["hello", {"a": 1}])),
            }
        );
        let connect = decode("0").unwrap();
        assert_eq!((connect.kind, connect.namespace.as_str()), (CONNECT, "/"));
        let binary = decode(r#"51-["up",{"_placeholder":true,"num":0}]"#).unwrap();
        assert_eq!(binary.attachments, 1);
        assert!(decode("9").is_err());
        assert!(decode("2[oops").is_err());

        assert_eq!(
            encode(EVENT, "/", Some(3), Some(&json!(["a"]))),
            r#"423["a"]"#
        );
        assert_eq!(
            encode(CONNECT, "/admin", None, Some(&json!({"token": "t"}))),
            r#"40/admin,{"token":"t"}"#
        );
        assert_eq!(encode(DISCONNECT, "/", None, None), "41");
    }

    #[test]
    fn placeholders_take_the_attachments() {
        let mut data = json!(["file", {"_placeholder": true, "num": 0}, {"nested": {"_placeholder": true, "num": 1}}]);
        fill_placeholders(&mut data, &[vec![1, 2], vec![255]]);
        assert_eq!(
            data,
            json!(["file", {"base64": "AQI="}, {"nested": {"base64": "/w=="}}])
        );
    }

    #[test]
    fn the_endpoint_follows_the_client() {
        let vars = VarStack::new();
        let settings = SocketIoSettings::default();
        let (url, namespace) =
            endpoint("http://localhost:3000/admin?token=x", &settings, &vars).unwrap();
        assert_eq!(
            url,
            "ws://localhost:3000/socket.io/?token=x&EIO=4&transport=websocket"
        );
        assert_eq!(namespace, "/admin");

        let settings = SocketIoSettings {
            namespace: "orders/".into(),
            path: "/rt".into(),
            ..SocketIoSettings::default()
        };
        let (url, namespace) = endpoint("https://example.com", &settings, &vars).unwrap();
        assert_eq!(url, "wss://example.com/rt/?EIO=4&transport=websocket");
        assert_eq!(namespace, "/orders");
        assert!(endpoint("ftp://example.com", &settings, &vars).is_err());
    }
}
