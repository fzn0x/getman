//! The transports under Socket.IO: Engine.IO v4 over WebSocket, and over
//! HTTP long polling.
//!
//! A [`Link`] carries Engine.IO packets, whichever transport it runs on:
//! the Socket.IO code above it reads [`Incoming`] packets and sends text
//! packets, and does not care how they travel.
//!
//! Long polling is plain HTTP, so it goes through the HTTP client and
//! with it the proxy settings, the cookie jar and the TLS settings of the
//! request. The client keeps one GET open at a time, which the server
//! answers when it has packets to deliver, and sends packets with POST.
//! Several packets in one body are separated by the character U+001E, and
//! a binary packet is written as `b` and its Base64.
//!
//! Like the official client, Getman starts on long polling and, if the
//! server offers it, upgrades to WebSocket: it opens a WebSocket with the
//! session's id, probes it with `2probe`, waits for `3probe`, lets the open
//! GET end, and sends `5` to say the WebSocket takes over.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt as _, StreamExt as _};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message as Frame;

use super::websocket::{self, Socket};
use super::{Job, MAX_MESSAGE_BYTES, SocketIoTransport, header_list, within};
use crate::http::{
    Header, HttpClient, Incoming as HttpResponse, Outgoing, OutgoingBody, Payload, Transport,
    set_header,
};

/// Separates the packets of a long-polling body.
const SEPARATOR: char = '\u{1e}';

/// How long the upgrade waits for the open GET to end once the WebSocket
/// has answered the probe.
const PAUSE_WAIT: Duration = Duration::from_secs(5);

/// The ways to carry Engine.IO.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Wire {
    Polling,
    Websocket,
}

/// The URL of the Engine.IO endpoint for a transport. `base` is the
/// endpoint with a `ws` or `wss` scheme and no `EIO` or `transport`
/// parameter; see [`super::socketio::endpoint`].
pub(super) fn wire_url(base: &str, wire: Wire, sid: Option<&str>) -> String {
    let Ok(mut url) = url::Url::parse(base) else {
        return base.to_owned();
    };
    let secure = url.scheme() == "wss" || url.scheme() == "https";
    let scheme = match (wire, secure) {
        (Wire::Polling, false) => "http",
        (Wire::Polling, true) => "https",
        (Wire::Websocket, false) => "ws",
        (Wire::Websocket, true) => "wss",
    };
    let _ = url.set_scheme(scheme);
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("EIO", "4");
        query.append_pair(
            "transport",
            match wire {
                Wire::Polling => "polling",
                Wire::Websocket => "websocket",
            },
        );
        if let Some(sid) = sid {
            query.append_pair("sid", sid);
        }
    }
    url.to_string()
}

/// Something that arrived on the connection.
#[derive(Debug)]
pub(super) enum Incoming {
    /// A packet: its type in the first character.
    Text(String),
    /// A binary attachment.
    Binary(Vec<u8>),
    /// The server closed the transport.
    Closed { code: Option<u16>, reason: String },
    /// The transport broke.
    Failed(String),
}

/// A connection to a Socket.IO server's Engine.IO endpoint.
pub(super) struct Link {
    inner: Inner,
    /// Packets read ahead, before the transport is read again.
    pending: VecDeque<Incoming>,
}

enum Inner {
    Websocket {
        writer: SplitSink<Socket, Frame>,
        reader: SplitStream<Socket>,
    },
    Polling(Box<Poll>),
}

/// What opening the connection gave.
pub(super) struct Opened {
    pub link: Link,
    /// Status of the handshake: 101 for a WebSocket, else the answer to
    /// the first long-polling request.
    pub status: u16,
    pub headers: Vec<Header>,
    /// Whether long polling was refused and a WebSocket used instead.
    pub fell_back: bool,
    /// Whether the server offers to upgrade to WebSocket.
    pub can_upgrade: bool,
}

impl Link {
    /// Whether this runs on long polling.
    pub(super) fn is_polling(&self) -> bool {
        matches!(self.inner, Inner::Polling(_))
    }

    fn websocket(socket: Socket) -> Self {
        let (writer, reader) = socket.split();
        Self {
            inner: Inner::Websocket { writer, reader },
            pending: VecDeque::new(),
        }
    }

    /// The next thing to arrive. Safe to drop while waiting.
    pub(super) async fn next(&mut self) -> Incoming {
        loop {
            if let Some(incoming) = self.pending.pop_front() {
                return incoming;
            }
            match &mut self.inner {
                Inner::Websocket { reader, .. } => match reader.next().await {
                    Some(Ok(Frame::Text(text))) => return Incoming::Text(text.as_str().to_owned()),
                    Some(Ok(Frame::Binary(bytes))) => return Incoming::Binary(bytes.to_vec()),
                    Some(Ok(Frame::Close(frame))) => {
                        let (code, reason) = frame.map_or((None, String::new()), |f| {
                            (Some(u16::from(f.code)), f.reason.to_string())
                        });
                        return Incoming::Closed { code, reason };
                    }
                    // Pings are answered by the WebSocket library.
                    Some(Ok(_)) => {}
                    Some(Err(e)) => return Incoming::Failed(websocket::describe(&e, &[])),
                    None => {
                        return Incoming::Closed {
                            code: None,
                            reason: String::new(),
                        };
                    }
                },
                Inner::Polling(poll) => match poll.rx.recv().await {
                    Some(PollMessage::Packets(packets)) => self.pending.extend(packets),
                    Some(PollMessage::Ended) | None => {
                        return Incoming::Closed {
                            code: None,
                            reason: String::new(),
                        };
                    }
                },
            }
        }
    }

    /// Sends a packet.
    pub(super) async fn send(&mut self, packet: String) -> Result<(), String> {
        match &mut self.inner {
            Inner::Websocket { writer, .. } => writer
                .send(Frame::text(packet))
                .await
                .map_err(|e| websocket::describe(&e, &[])),
            Inner::Polling(poll) => poll.client.post(packet).await,
        }
    }

    /// Closes the transport politely, and returns the close code and
    /// reason it ended with.
    pub(super) async fn close(&mut self) -> (Option<u16>, String) {
        match &mut self.inner {
            Inner::Websocket { writer, reader } => websocket::close(writer, reader).await,
            Inner::Polling(poll) => {
                let _ = poll.client.post("1".into()).await;
                poll.stop();
                (None, String::new())
            }
        }
    }

    /// Lets go of the transport without waiting for anyone.
    pub(super) async fn abandon(&mut self) {
        match &mut self.inner {
            Inner::Websocket { writer, .. } => {
                let _ = writer.send(Frame::Close(None)).await;
            }
            Inner::Polling(poll) => poll.stop(),
        }
    }
}

/// Opens the transport `mode` asks for. When long polling is first and
/// the server turns it down with a 400, which is what a server limited to
/// WebSocket answers, a WebSocket is tried instead.
pub(super) async fn open(job: &Job, mode: SocketIoTransport) -> Result<Opened, String> {
    if mode != SocketIoTransport::Websocket {
        match Client::handshake(job).await {
            Ok((client, open, status, headers)) => {
                let can_upgrade = mode == SocketIoTransport::Auto
                    && open
                        .upgrades
                        .iter()
                        .any(|upgrade| upgrade.eq_ignore_ascii_case("websocket"));
                let mut link = Link {
                    inner: Inner::Polling(Box::new(Poll::start(client))),
                    pending: VecDeque::new(),
                };
                link.pending.push_back(Incoming::Text(open.packet));
                return Ok(Opened {
                    link,
                    status,
                    headers,
                    fell_back: false,
                    can_upgrade,
                });
            }
            Err(PollError::Refused { status: 400, .. }) if mode == SocketIoTransport::Auto => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    let url = wire_url(&job.endpoint, Wire::Websocket, None);
    let (socket, handshake) =
        websocket::open(&job.http, &job.transport, &url, &job.outgoing.headers, &[]).await?;
    Ok(Opened {
        link: Link::websocket(socket),
        status: handshake.status().as_u16(),
        headers: header_list(handshake.headers()),
        fell_back: mode == SocketIoTransport::Auto,
        can_upgrade: false,
    })
}

/// Why an upgrade did not happen.
pub(super) enum UpgradeFailure {
    /// The connection is as it was, on long polling.
    Kept(String),
    /// The connection is lost.
    Lost(String),
}

/// Moves a long-polling connection to a WebSocket.
pub(super) async fn upgrade(link: &mut Link, job: &Job) -> Result<(), UpgradeFailure> {
    let Link { inner, pending } = link;
    let Inner::Polling(poll) = inner else {
        return Ok(());
    };
    let url = wire_url(&job.endpoint, Wire::Websocket, Some(&poll.client.sid));
    let mut socket = within(job.connect_timeout(), async {
        let (mut socket, _) =
            websocket::open(&job.http, &job.transport, &url, &job.outgoing.headers, &[]).await?;
        socket
            .send(Frame::text("2probe"))
            .await
            .map_err(|e| websocket::describe(&e, &[]))?;
        loop {
            match socket.next().await {
                Some(Ok(Frame::Text(text))) if text.as_str() == "3probe" => return Ok(socket),
                Some(Ok(Frame::Close(_))) | None => {
                    return Err(
                        "the server closed the WebSocket instead of answering the probe".to_owned(),
                    );
                }
                Some(Err(e)) => return Err(websocket::describe(&e, &[])),
                Some(Ok(_)) => {}
            }
        }
    })
    .await
    .map_err(UpgradeFailure::Kept)?;

    // The server ends the open GET once it has answered the probe. Take
    // what it carried, and let no new one start.
    poll.paused.store(true, Ordering::SeqCst);
    let drained = tokio::time::timeout(PAUSE_WAIT, async {
        loop {
            match poll.rx.recv().await {
                Some(PollMessage::Packets(packets)) => {
                    for packet in packets {
                        if let Incoming::Failed(message) = packet {
                            return Err(message);
                        }
                        pending.push_back(packet);
                    }
                }
                Some(PollMessage::Ended) | None => return Ok(()),
            }
        }
    })
    .await;
    match drained {
        Ok(Ok(())) => {}
        Ok(Err(message)) => return Err(UpgradeFailure::Lost(message)),
        Err(_) => {
            return Err(UpgradeFailure::Lost(
                "The server did not end its open long-polling request when the WebSocket took over"
                    .into(),
            ));
        }
    }
    socket
        .send(Frame::text("5"))
        .await
        .map_err(|e| UpgradeFailure::Lost(websocket::describe(&e, &[])))?;
    let (writer, reader) = socket.split();
    *inner = Inner::Websocket { writer, reader };
    Ok(())
}

// ---- Long polling --------------------------------------------------------------------

/// What the polling task reports.
enum PollMessage {
    Packets(Vec<Incoming>),
    /// The task stopped because polling was paused.
    Ended,
}

/// The running long-polling GETs.
struct Poll {
    client: Client,
    rx: mpsc::Receiver<PollMessage>,
    task: JoinHandle<()>,
    paused: Arc<AtomicBool>,
}

impl Poll {
    fn start(client: Client) -> Self {
        let (tx, rx) = mpsc::channel(16);
        let paused = Arc::new(AtomicBool::new(false));
        let task = tokio::spawn(poll_loop(client.clone(), tx, Arc::clone(&paused)));
        Self {
            client,
            rx,
            task,
            paused,
        }
    }

    fn stop(&self) {
        self.task.abort();
    }
}

impl Drop for Poll {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Keeps one GET open, one after another, until polling is paused or fails.
async fn poll_loop(client: Client, tx: mpsc::Sender<PollMessage>, paused: Arc<AtomicBool>) {
    loop {
        if paused.load(Ordering::SeqCst) {
            let _ = tx.send(PollMessage::Ended).await;
            return;
        }
        match client.get().await {
            Ok(packets) => {
                if tx.send(PollMessage::Packets(packets)).await.is_err() {
                    return;
                }
            }
            Err(e) => {
                let _ = tx
                    .send(PollMessage::Packets(vec![Incoming::Failed(e.to_string())]))
                    .await;
                return;
            }
        }
    }
}

/// Why a long-polling request failed.
#[derive(Debug)]
enum PollError {
    /// The server answered with a status other than success.
    Refused { status: u16, message: String },
    /// The request could not be made, or the answer not read.
    Failed(String),
}

impl std::fmt::Display for PollError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused { message, .. } | Self::Failed(message) => f.write_str(message),
        }
    }
}

/// The server's answer to the first long-polling request.
struct OpenPacket {
    upgrades: Vec<String>,
    /// The packet as received, `0` and its JSON.
    packet: String,
}

/// Makes the long-polling requests of one session.
#[derive(Clone)]
struct Client {
    http: Arc<HttpClient>,
    /// For the handshake and for sending packets.
    transport: Transport,
    /// For the open GET, which the server may hold for a whole heartbeat.
    long: Transport,
    headers: Vec<Header>,
    /// The endpoint, with the session's id once there is one.
    url: String,
    sid: String,
}

impl Client {
    /// Asks for a session. Returns the client for it, the open packet, and
    /// the status and headers of the answer.
    async fn handshake(job: &Job) -> Result<(Self, OpenPacket, u16, Vec<Header>), PollError> {
        let mut client = Self {
            http: Arc::clone(&job.http),
            transport: job.transport.clone(),
            long: job.transport.clone(),
            headers: job.outgoing.headers.clone(),
            url: wire_url(&job.endpoint, Wire::Polling, None),
            sid: String::new(),
        };
        let (response, _) = client.request("GET", &client.transport, None).await?;
        let status = response.status().as_u16();
        let headers = header_list(response.headers());
        let packets = decode(&read(response, &client.transport).await?);
        let Some(Incoming::Text(first)) = packets.first() else {
            return Err(PollError::Failed(
                "The server answered the long-polling request with nothing".into(),
            ));
        };
        let json: Value = first
            .strip_prefix('0')
            .and_then(|json| serde_json::from_str(json).ok())
            .ok_or_else(|| {
                PollError::Failed(format!(
                    "The server answered the long-polling request with something that is not an Engine.IO session: {first:.80}"
                ))
            })?;
        let sid = json["sid"].as_str().unwrap_or_default().to_owned();
        if sid.is_empty() {
            return Err(PollError::Failed(
                "The server opened an Engine.IO session without an id".into(),
            ));
        }
        let ms = |name: &str, default: u64| {
            Duration::from_millis(json[name].as_u64().unwrap_or(default))
        };
        let open = OpenPacket {
            upgrades: json["upgrades"]
                .as_array()
                .map(|list| {
                    list.iter()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default(),
            packet: first.clone(),
        };
        client.url = wire_url(&job.endpoint, Wire::Polling, Some(&sid));
        client.sid = sid;
        // Between two pings the server holds the GET; allow for its timeout.
        let wait = ms("pingInterval", 25_000) + ms("pingTimeout", 20_000) + Duration::from_secs(2);
        client.long.timeout_ms = u64::try_from(wait.as_millis()).unwrap_or(u64::MAX);
        Ok((client, open, status, headers))
    }

    async fn request(
        &self,
        method: &str,
        transport: &Transport,
        body: Option<String>,
    ) -> Result<(HttpResponse, f64), PollError> {
        let mut headers = self.headers.clone();
        let body = match body {
            Some(text) => {
                set_header(&mut headers, "Content-Type", "text/plain;charset=UTF-8");
                OutgoingBody::Bytes {
                    data: Payload(text.into_bytes()),
                }
            }
            None => OutgoingBody::None,
        };
        let outgoing = Outgoing {
            method: method.to_owned(),
            url: self.url.clone(),
            headers,
            body,
        };
        let (response, ms) = self
            .http
            .open(&outgoing, transport.clone())
            .await
            .map_err(|e| PollError::Failed(e.to_string()))?;
        let status = response.status();
        if status.is_success() {
            return Ok((response, ms));
        }
        let body = read(response, transport).await.unwrap_or_default();
        let body = body.trim();
        let mut message = format!(
            "The server answered {} {} to a long-polling request",
            status.as_u16(),
            status.canonical_reason().unwrap_or_default()
        );
        if !body.is_empty() {
            let excerpt: String = body.chars().take(200).collect();
            message.push_str(": ");
            message.push_str(&excerpt);
        }
        Err(PollError::Refused {
            status: status.as_u16(),
            message,
        })
    }

    /// Waits for the server to have packets.
    async fn get(&self) -> Result<Vec<Incoming>, PollError> {
        let (response, _) = self.request("GET", &self.long, None).await?;
        Ok(decode(&read(response, &self.long).await?))
    }

    /// Sends a packet.
    async fn post(&self, packet: String) -> Result<(), String> {
        let (response, _) = self
            .request("POST", &self.transport, Some(packet))
            .await
            .map_err(|e| e.to_string())?;
        read(response, &self.transport)
            .await
            .map(drop)
            .map_err(|e| e.to_string())
    }
}

/// Reads a response's body as text, up to the largest message Getman
/// reads, within the transport's timeout.
async fn read(mut response: HttpResponse, transport: &Transport) -> Result<String, PollError> {
    let reading = async {
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| PollError::Failed(e.to_string()))?
        {
            bytes.extend_from_slice(&chunk);
            if bytes.len() > MAX_MESSAGE_BYTES {
                return Err(PollError::Failed(format!(
                    "The server sent more than Getman reads at once; messages may be up to {} MB",
                    MAX_MESSAGE_BYTES / 1024 / 1024
                )));
            }
        }
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    };
    if transport.timeout_ms == 0 {
        return reading.await;
    }
    tokio::time::timeout(Duration::from_millis(transport.timeout_ms), reading)
        .await
        .unwrap_or_else(|_| {
            Err(PollError::Failed(
                "The long-polling request timed out".into(),
            ))
        })
}

/// Splits a long-polling body into packets: `b` and Base64 is a binary
/// attachment, anything else a text packet.
fn decode(body: &str) -> Vec<Incoming> {
    body.split(SEPARATOR)
        .filter(|packet| !packet.is_empty())
        .map(|packet| match packet.strip_prefix('b') {
            Some(base64) => BASE64
                .decode(base64)
                .map_or_else(|_| Incoming::Text(packet.to_owned()), Incoming::Binary),
            None => Incoming::Text(packet.to_owned()),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_urls_follow_the_transport() {
        assert_eq!(
            wire_url(
                "ws://localhost:3000/socket.io/?token=x",
                Wire::Polling,
                None
            ),
            "http://localhost:3000/socket.io/?token=x&EIO=4&transport=polling"
        );
        assert_eq!(
            wire_url("wss://example.com/rt/", Wire::Websocket, Some("abc")),
            "wss://example.com/rt/?EIO=4&transport=websocket&sid=abc"
        );
        assert_eq!(
            wire_url("wss://example.com/rt/", Wire::Polling, Some("a b")),
            "https://example.com/rt/?EIO=4&transport=polling&sid=a+b"
        );
    }

    #[test]
    fn polling_bodies_hold_packets_and_attachments() {
        let packets = decode("42[\"a\",1]\u{1e}bAQID\u{1e}2");
        assert!(matches!(&packets[0], Incoming::Text(t) if t == "42[\"a\",1]"));
        assert!(matches!(&packets[1], Incoming::Binary(b) if b == &[1, 2, 3]));
        assert!(matches!(&packets[2], Incoming::Text(t) if t == "2"));
        assert!(decode("").is_empty());
    }
}
