//! What the terminal UI shows for requests that stay open: the log of a
//! WebSocket, event stream, Socket.IO or gRPC connection, and the events
//! that fill it.

use std::collections::VecDeque;

use getman_core::execute::Execution;
use getman_core::grpc::{self, CallType, LiveEvent};
use getman_core::http::Header;
use getman_core::model::{Request, RequestKind};
use getman_core::realtime::{
    Control, Direction, EventKind, Message, MessageFormat, Outbound, SavedEmit, SavedMessage,
    StreamEvent,
};
use getman_core::vars::missing_reason;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use tokio::sync::mpsc::UnboundedSender;

use super::text::{json_line, size};

/// Most entries a log keeps; the oldest are dropped first.
const MAX_ENTRIES: usize = 2_000;
/// Most lines a log keeps, so a scroll position always fits.
const MAX_LINES: usize = 20_000;
/// Longest a single entry is shown.
const MAX_ENTRY_LINES: usize = 1_000;
/// Width of what comes before an entry's text: the time, the arrow and
/// the spaces around them.
const GUTTER: usize = 12;

/// Who an entry is about, which decides how it looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tone {
    /// Getman sent it.
    Sent,
    /// The server sent it.
    Received,
    /// Something worth knowing.
    Note,
    /// A problem that leaves the connection open.
    Warning,
    /// A problem that ends it.
    Error,
}

/// One line of the log, or several when a message is pretty-printed.
#[derive(Debug, Clone)]
pub(crate) struct Entry {
    pub(crate) tone: Tone,
    /// Milliseconds since the connection began.
    pub(crate) at_ms: i64,
    /// An event name or a saved message's name, shown before the text.
    pub(crate) label: String,
    pub(crate) text: String,
    /// Whether the text is pretty-printed JSON, to colour.
    json: bool,
    height: usize,
}

impl Entry {
    fn new(tone: Tone, at_ms: i64, label: &str, text: &str, json: bool) -> Self {
        let mut text = clean(text);
        if text.lines().count() > MAX_ENTRY_LINES {
            let kept: Vec<&str> = text.lines().take(MAX_ENTRY_LINES).collect();
            text = format!("{}\n… cut short", kept.join("\n"));
        }
        let height = text.lines().count().max(1);
        Self {
            tone,
            at_ms,
            label: clean(label),
            text,
            json,
            height,
        }
    }

    /// The entry as lines of the log: the time and an arrow before the
    /// first, and the same width of space before the others.
    pub(crate) fn lines(&self) -> Vec<Line<'static>> {
        let (glyph, color) = match self.tone {
            Tone::Sent => ("→", Color::Cyan),
            Tone::Received => ("←", Color::Green),
            Tone::Note => ("•", Color::DarkGray),
            Tone::Warning => ("!", Color::Yellow),
            Tone::Error => ("✗", Color::Red),
        };
        let body = match self.tone {
            Tone::Sent | Tone::Received => Style::new(),
            Tone::Note => Style::new().fg(Color::DarkGray),
            Tone::Warning => Style::new().fg(Color::Yellow),
            Tone::Error => Style::new().fg(Color::Red),
        };
        let texts: Vec<&str> = if self.text.is_empty() {
            vec![""]
        } else {
            self.text.lines().collect()
        };
        texts
            .into_iter()
            .enumerate()
            .map(|(i, text)| {
                let mut spans = if i == 0 {
                    vec![
                        Span::styled(
                            format!("{:>9} ", elapsed(self.at_ms)),
                            Style::new().fg(Color::DarkGray),
                        ),
                        Span::styled(format!("{glyph} "), Style::new().fg(color)),
                    ]
                } else {
                    vec![Span::raw(" ".repeat(GUTTER))]
                };
                if i == 0 && !self.label.is_empty() {
                    spans.push(Span::styled(
                        format!("{} ", self.label),
                        Style::new().add_modifier(Modifier::BOLD),
                    ));
                }
                if self.json {
                    spans.extend(json_line(text).spans);
                } else {
                    spans.push(Span::styled(text.to_owned(), body));
                }
                Line::from(spans)
            })
            .collect()
    }
}

/// How far a connection has come.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    /// Being opened, or a call not yet started.
    Connecting,
    /// Open, or a call under way.
    Open,
    /// Closed, or the call finished.
    Ended,
}

/// How to reach a connection that is running.
#[derive(Debug)]
pub(crate) enum Link {
    Realtime(Control),
    Grpc(UnboundedSender<grpc::Command>),
}

/// A connection or call, and what it has reported.
#[derive(Debug)]
pub(crate) struct Stream {
    /// Tells this connection's events from an older one's.
    pub(crate) serial: u64,
    pub(crate) kind: RequestKind,
    pub(crate) link: Link,
    pub(crate) phase: Phase,
    pub(crate) entries: VecDeque<Entry>,
    dropped: usize,
    lines: usize,
    origin: Option<i64>,
    /// What the server answered when the connection opened.
    pub(crate) connected: Option<String>,
    /// How it ended, and whether that was well.
    pub(crate) result: Option<(String, bool)>,
    pub(crate) headers: Vec<Header>,
    pub(crate) trailers: Vec<Header>,
    pub(crate) sent: usize,
    pub(crate) received: usize,
    /// Which sides of a gRPC call stream, once the call has started.
    pub(crate) call_type: Option<CallType>,
    /// Keep the end of the log in view as it grows.
    pub(crate) follow: bool,
    /// Whether the user asked to stop it.
    pub(crate) stopping: bool,
    failed: bool,
}

impl Stream {
    pub(crate) fn new(serial: u64, kind: RequestKind, link: Link) -> Self {
        Self {
            serial,
            kind,
            link,
            phase: Phase::Connecting,
            entries: VecDeque::new(),
            dropped: 0,
            lines: 0,
            origin: None,
            connected: None,
            result: None,
            headers: Vec::new(),
            trailers: Vec::new(),
            sent: 0,
            received: 0,
            call_type: None,
            follow: true,
            stopping: false,
            failed: false,
        }
    }

    /// Whether it is still connecting, open or running.
    pub(crate) fn is_live(&self) -> bool {
        self.phase != Phase::Ended
    }

    /// Asks it to stop: closes the connection, or cancels the call. A
    /// `closed` event, or the call's status, follows.
    pub(crate) fn stop(&mut self) {
        if !self.is_live() || self.stopping {
            return;
        }
        self.stopping = true;
        match &self.link {
            Link::Realtime(control) => control.close(),
            Link::Grpc(commands) => {
                let _ = commands.send(grpc::Command::Cancel);
            }
        }
    }

    /// Lines in the log, counting the note about dropped entries.
    pub(crate) fn total_lines(&self) -> usize {
        self.lines + usize::from(self.dropped > 0)
    }

    /// `height` lines of the log from line `from`.
    pub(crate) fn window(&self, from: usize, height: usize) -> Vec<Line<'static>> {
        let mut out = Vec::new();
        let mut at = 0;
        if self.dropped > 0 {
            if from == 0 {
                out.push(Line::styled(
                    format!("… {} earlier entries dropped", self.dropped),
                    Style::new().fg(Color::DarkGray),
                ));
            }
            at = 1;
        }
        for entry in &self.entries {
            if out.len() >= height {
                break;
            }
            let end = at + entry.height;
            if end > from {
                out.extend(
                    entry
                        .lines()
                        .into_iter()
                        .skip(from.saturating_sub(at))
                        .take(height - out.len()),
                );
            }
            at = end;
        }
        out
    }

    fn since(&mut self, at: i64) -> i64 {
        (at - *self.origin.get_or_insert(at)).max(0)
    }

    fn push(&mut self, tone: Tone, at_ms: i64, label: &str, text: &str, json: bool) {
        let entry = Entry::new(tone, at_ms, label, text, json);
        self.lines += entry.height;
        self.entries.push_back(entry);
        while self.entries.len() > 1 && (self.entries.len() > MAX_ENTRIES || self.lines > MAX_LINES)
        {
            if let Some(old) = self.entries.pop_front() {
                self.lines -= old.height;
                self.dropped += 1;
            }
        }
    }

    fn say(&mut self, tone: Tone, at_ms: i64, text: &str) {
        self.push(tone, at_ms, "", text, false);
    }

    /// Notes something the user did, such as an error sending a message.
    pub(crate) fn note(&mut self, tone: Tone, text: &str) {
        let at = self.entries.back().map_or(0, |e| e.at_ms);
        self.say(tone, at, text);
    }

    // ---- WebSocket, event streams, Socket.IO ---------------------------------

    /// Takes in what a WebSocket, event stream or Socket.IO connection
    /// reported.
    pub(crate) fn on_realtime(&mut self, event: StreamEvent) {
        let at = self.since(event.at);
        match event.kind {
            EventKind::Connecting { url, missing, .. } => {
                for name in missing {
                    self.say(Tone::Warning, at, &missing_reason(&name));
                }
                self.say(Tone::Note, at, &format!("Connecting to {url}"));
            }
            EventKind::Open {
                status,
                headers,
                protocol,
                time_ms,
            } => {
                self.phase = Phase::Open;
                self.headers = headers;
                let mut line = format!("{status} · {time_ms:.0} ms");
                if let Some(protocol) = protocol {
                    line.push_str(&format!(" · subprotocol {protocol}"));
                }
                self.say(Tone::Note, at, &format!("Connected · {line}"));
                self.connected = Some(line);
            }
            EventKind::Message(message) => {
                match message.direction {
                    Direction::Sent => self.sent += 1,
                    Direction::Received => self.received += 1,
                }
                let (tone, label, text, json) = message_parts(&message);
                self.push(tone, at, &label, &text, json);
            }
            EventKind::Note { message } => self.say(Tone::Note, at, &message),
            EventKind::Warning { message } => self.say(Tone::Warning, at, &message),
            EventKind::Error { message } => {
                self.failed = true;
                self.say(Tone::Error, at, &message);
            }
            EventKind::Closed {
                code,
                reason,
                by_client,
            } => {
                let mut line = String::from(if by_client {
                    "Disconnected"
                } else {
                    "The server closed the connection"
                });
                if let Some(code) = code {
                    line.push_str(&format!(" ({code})"));
                }
                if !reason.is_empty() {
                    line.push_str(&format!(": {reason}"));
                }
                self.say(Tone::Note, at, &line);
                self.phase = Phase::Ended;
                let title = if self.failed {
                    "Failed"
                } else if by_client {
                    "Disconnected"
                } else {
                    "Closed"
                };
                self.result = Some((title.to_owned(), !self.failed));
            }
        }
    }

    /// The events stopped without a `closed` event.
    pub(crate) fn on_ended(&mut self) {
        if self.is_live() {
            let at = self.entries.back().map_or(0, |e| e.at_ms);
            self.say(Tone::Note, at, "The connection ended.");
            self.phase = Phase::Ended;
            self.result = Some(("Closed".to_owned(), !self.failed));
        }
    }

    // ---- gRPC ----------------------------------------------------------------

    /// Takes in what a gRPC call reported while it ran.
    pub(crate) fn on_grpc(&mut self, event: LiveEvent) {
        match event {
            LiveEvent::Started {
                method,
                call_type,
                address,
            } => {
                self.phase = Phase::Open;
                self.call_type = Some(call_type);
                self.say(
                    Tone::Note,
                    0,
                    &format!("Calling {method} at {address} · {}", call_label(call_type)),
                );
            }
            LiveEvent::Headers { headers, .. } => self.headers = headers,
            LiveEvent::Message { message } => {
                let at = millis(message.time_ms);
                if let Some(error) = &message.error {
                    self.say(
                        Tone::Warning,
                        at,
                        &format!("A message could not be shown: {error}"),
                    );
                    return;
                }
                let tone = match message.direction {
                    grpc::Direction::Sent => {
                        self.sent += 1;
                        Tone::Sent
                    }
                    grpc::Direction::Received => {
                        self.received += 1;
                        Tone::Received
                    }
                };
                let text = serde_json::to_string_pretty(&message.message).unwrap_or_default();
                self.push(tone, at, &size(message.size), &text, true);
            }
        }
    }

    /// Takes in how the call ended: its status, trailers and problems.
    pub(crate) fn on_grpc_done(&mut self, execution: &Execution) {
        self.phase = Phase::Ended;
        let mut at = self.entries.back().map_or(0, |e| e.at_ms);
        let mut ok = execution.error.is_none();
        let mut title = String::from("No status");
        if let Some(call) = &execution.grpc {
            at = at.max(millis(call.time_ms));
            self.headers.clone_from(&call.headers);
            self.trailers.clone_from(&call.trailers);
            if let Some(status) = &call.status {
                ok = status.is_ok();
                title = format!(
                    "{} {} · {} ms",
                    status.code,
                    status.name,
                    millis(call.time_ms)
                );
                let mut line = format!("Status {} {}", status.code, status.name);
                if !status.message.is_empty() {
                    line.push_str(&format!(": {}", status.message));
                }
                self.say(if ok { Tone::Note } else { Tone::Error }, at, &line);
                if let Some(details) = &status.details {
                    let text = serde_json::to_string_pretty(details).unwrap_or_default();
                    self.push(Tone::Error, at, "details", &text, true);
                }
            }
            if call.dropped_messages > 0 {
                self.say(
                    Tone::Warning,
                    at,
                    &format!("{} later messages were not kept", call.dropped_messages),
                );
            }
        }
        if let Some(error) = &execution.error {
            let tone = if execution.cancelled {
                Tone::Note
            } else {
                Tone::Error
            };
            self.say(tone, at, error);
            if execution.cancelled {
                title = "Cancelled".to_owned();
            } else if execution.grpc.as_ref().is_none_or(|c| c.status.is_none()) {
                title = "Failed".to_owned();
            }
        }
        self.result = Some((title, ok));
    }
}

/// Text from a server, made safe to print: control characters, which a
/// terminal would act on, are dropped.
pub(crate) fn clean(text: &str) -> String {
    text.replace('\t', "    ")
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .collect()
}

/// A short phrase for which sides of a call stream.
pub(crate) fn call_label(call_type: CallType) -> &'static str {
    match call_type {
        CallType::Unary => "unary",
        CallType::ServerStreaming => "server stream",
        CallType::ClientStreaming => "client stream",
        CallType::BidiStreaming => "bidirectional stream",
    }
}

#[allow(clippy::cast_possible_truncation)]
fn millis(ms: f64) -> i64 {
    ms.round() as i64
}

/// A time since the start as `+1.234s`, `+2m05s` or `+1h02m`.
pub(crate) fn elapsed(ms: i64) -> String {
    let ms = ms.max(0);
    if ms < 60_000 {
        format!("+{}.{:03}s", ms / 1000, ms % 1000)
    } else if ms < 3_600_000 {
        format!("+{}m{:02}s", ms / 60_000, ms % 60_000 / 1000)
    } else {
        format!("+{}h{:02}m", ms / 3_600_000, ms % 3_600_000 / 60_000)
    }
}

/// JSON text pretty-printed, if it is an object or an array.
fn pretty(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if !(trimmed.starts_with('{') || trimmed.starts_with('[')) {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(trimmed).ok()?;
    serde_json::to_string_pretty(&value).ok()
}

/// How a message is shown: its tone, label and text, and whether the
/// text is JSON.
fn message_parts(message: &Message) -> (Tone, String, String, bool) {
    let tone = match message.direction {
        Direction::Sent => Tone::Sent,
        Direction::Received => Tone::Received,
    };
    let label = match (&message.event, message.ack, &message.name) {
        (Some(event), _, _) => event.clone(),
        (None, Some(ack), _) => format!("ack {ack}"),
        (None, None, Some(name)) => name.clone(),
        (None, None, None) => String::new(),
    };
    let (mut text, json) = if message.binary {
        (format!("binary · {}", size(message.size)), false)
    } else if let Some(text) = pretty(&message.data) {
        (text, true)
    } else {
        (message.data.clone(), false)
    };
    if message.truncated {
        text.push_str(&format!("\n… {} in all", size(message.size)));
    }
    (tone, label, text, json)
}

// ---- Messages to send ----------------------------------------------------------

/// A message or event saved with a request.
#[derive(Debug, Clone)]
pub(crate) struct Saved {
    /// Its name, or what it holds when it has none.
    pub(crate) label: String,
    /// What it sends, on one line.
    pub(crate) preview: String,
    pub(crate) outbound: Outbound,
}

/// The messages a WebSocket request or the events a Socket.IO request
/// saves, in order.
pub(crate) fn saved_messages(request: &Request) -> Vec<Saved> {
    let one_line = |text: &str| {
        let line: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.chars().count() > 60 {
            format!("{}…", line.chars().take(60).collect::<String>())
        } else {
            line
        }
    };
    match request.kind {
        RequestKind::Websocket => request
            .websocket
            .messages
            .iter()
            .map(|m| Saved {
                label: if m.name.trim().is_empty() {
                    one_line(&m.content)
                } else {
                    m.name.trim().to_owned()
                },
                preview: one_line(&m.content),
                outbound: Outbound::Message(m.clone()),
            })
            .collect(),
        RequestKind::Socketio => request
            .socketio
            .messages
            .iter()
            .map(|m| Saved {
                label: if m.name.trim().is_empty() {
                    m.event.clone()
                } else {
                    m.name.trim().to_owned()
                },
                preview: format!("{} {}", m.event, one_line(&m.args))
                    .trim_end()
                    .to_owned(),
                outbound: Outbound::Emit(m.clone()),
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// A message typed on the spot. A WebSocket message is sent as written;
/// a Socket.IO event is its name, then its arguments as JSON.
pub(crate) fn typed(kind: RequestKind, text: &str) -> Result<Outbound, String> {
    match kind {
        RequestKind::Websocket => Ok(Outbound::Message(SavedMessage {
            name: String::new(),
            format: MessageFormat::Text,
            content: text.to_owned(),
        })),
        RequestKind::Socketio => {
            let text = text.trim();
            let (event, args) = text
                .split_once(char::is_whitespace)
                .map_or((text, ""), |(event, args)| (event, args.trim()));
            if event.is_empty() {
                return Err(
                    "Type the event's name, then its arguments as JSON, such as: chat [\"hi\"]"
                        .into(),
                );
            }
            Ok(Outbound::Emit(SavedEmit {
                name: String::new(),
                event: event.to_owned(),
                args: args.to_owned(),
                ack: false,
            }))
        }
        other => Err(format!("{} requests only receive", other.label())),
    }
}

#[cfg(test)]
mod tests {
    use getman_core::grpc::{GrpcCall, Status, StreamMessage};
    use serde_json::json;

    use super::*;

    fn stream(kind: RequestKind) -> Stream {
        let (commands, _receiver) = tokio::sync::mpsc::unbounded_channel();
        Stream::new(1, kind, Link::Grpc(commands))
    }

    fn event(at: i64, kind: EventKind) -> StreamEvent {
        StreamEvent { at, kind }
    }

    fn received(data: &str) -> Message {
        Message {
            direction: Direction::Received,
            data: data.to_owned(),
            binary: false,
            size: data.len() as u64,
            truncated: false,
            name: None,
            event: None,
            id: None,
            retry: None,
            ack: None,
        }
    }

    #[test]
    fn messages_are_logged_with_direction_and_time_and_json_is_pretty() {
        let mut log = stream(RequestKind::Websocket);
        log.on_realtime(event(
            1_000,
            EventKind::Connecting {
                method: "GET".into(),
                url: "ws://x.test/chat".into(),
                headers: Vec::new(),
                missing: vec!["token".into()],
            },
        ));
        log.on_realtime(event(
            1_040,
            EventKind::Open {
                status: 101,
                headers: vec![Header::new("upgrade", "websocket")],
                protocol: None,
                time_ms: 40.0,
            },
        ));
        log.on_realtime(event(
            2_500,
            EventKind::Message(received(r#"{"type":"hello","n":[1,2]}"#)),
        ));
        assert_eq!(log.phase, Phase::Open);
        assert_eq!(log.connected.as_deref(), Some("101 · 40 ms"));
        assert_eq!(log.received, 1);

        let entries: Vec<_> = log.entries.iter().collect();
        assert_eq!(entries[0].tone, Tone::Warning);
        assert!(entries[0].text.contains("{{token}} is not defined"));
        let message = entries.last().unwrap();
        assert_eq!(message.tone, Tone::Received);
        assert_eq!(message.at_ms, 1_500);
        assert!(message.text.starts_with("{\n"), "{}", message.text);
        let rendered: Vec<String> = message
            .lines()
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert!(rendered[0].starts_with("  +1.500s ← {"), "{rendered:?}");
        assert!(rendered[1].starts_with("             "), "{rendered:?}");
    }

    #[test]
    fn the_log_ends_with_how_the_connection_closed() {
        let mut log = stream(RequestKind::Sse);
        log.on_realtime(event(
            0,
            EventKind::Error {
                message: "connection refused".into(),
            },
        ));
        log.on_realtime(event(
            5,
            EventKind::Closed {
                code: None,
                reason: String::new(),
                by_client: false,
            },
        ));
        assert_eq!(log.phase, Phase::Ended);
        assert_eq!(log.result, Some(("Failed".to_owned(), false)));

        let mut log = stream(RequestKind::Websocket);
        log.on_realtime(event(
            0,
            EventKind::Closed {
                code: Some(1000),
                reason: "bye".into(),
                by_client: true,
            },
        ));
        assert_eq!(log.result, Some(("Disconnected".to_owned(), true)));
        assert_eq!(log.entries[0].text, "Disconnected (1000): bye");
    }

    #[test]
    fn text_from_a_server_cannot_steer_the_terminal() {
        let mut log = stream(RequestKind::Websocket);
        log.on_realtime(event(
            0,
            EventKind::Message(received("hi\u{1b}[2J\u{7}there\tx\r\nnext")),
        ));
        assert_eq!(log.entries[0].text, "hi[2Jthere    x\nnext");
    }

    #[test]
    fn a_long_log_drops_its_oldest_entries_and_scrolls_by_line() {
        let mut log = stream(RequestKind::Sse);
        for i in 0..(MAX_ENTRIES + 5) {
            log.on_realtime(event(
                0,
                EventKind::Message(received(&format!("{{\"i\": {i}}}"))),
            ));
        }
        assert_eq!(log.entries.len(), MAX_ENTRIES);
        assert!(log.total_lines() > MAX_ENTRIES);
        let top = log.window(0, 2);
        assert!(top[0].spans[0].content.contains("earlier entries dropped"));
        // Each message is three lines: `{`, the member and `}`.
        let text = |lines: &[Line<'_>]| -> String {
            lines
                .iter()
                .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
                .collect()
        };
        let middle = log.window(2, 2);
        assert_eq!(middle.len(), 2);
        assert!(text(&middle).contains("\"i\""), "{}", text(&middle));
        let end = log.window(log.total_lines() - 1, 5);
        assert_eq!(end.len(), 1);
        assert!(text(&end).contains('}'));
    }

    #[test]
    fn grpc_events_fill_the_log_and_the_status_closes_it() {
        let mut log = stream(RequestKind::Grpc);
        log.on_grpc(LiveEvent::Started {
            method: "shop.Orders/Watch".into(),
            call_type: CallType::ServerStreaming,
            address: "localhost:50051".into(),
        });
        log.on_grpc(LiveEvent::Message {
            message: StreamMessage {
                direction: grpc::Direction::Received,
                message: json!({"state": "SHIPPED"}),
                time_ms: 12.4,
                size: 9,
                error: None,
            },
        });
        assert_eq!(log.call_type, Some(CallType::ServerStreaming));
        assert_eq!(log.received, 1);
        assert_eq!(log.entries[1].at_ms, 12);
        assert!(log.entries[1].text.contains("\"state\": \"SHIPPED\""));

        let status: Status = serde_json::from_value(json!({
            "code": 5,
            "name": "NOT_FOUND",
            "message": "no such order",
            "details": {"reason": "GONE"},
        }))
        .unwrap();
        let execution = Execution {
            grpc: Some(GrpcCall {
                status: Some(status),
                headers: vec![Header::new("x-request-id", "r1")],
                trailers: vec![Header::new("x-trace", "t1")],
                time_ms: 30.0,
                ..GrpcCall::default()
            }),
            ..Execution::default()
        };
        log.on_grpc_done(&execution);
        assert_eq!(log.phase, Phase::Ended);
        assert_eq!(log.result, Some(("5 NOT_FOUND · 30 ms".to_owned(), false)));
        assert_eq!(log.trailers[0].name, "x-trace");
        assert_eq!(log.headers[0].value, "r1");
        let last: Vec<_> = log.entries.iter().rev().take(2).collect();
        assert!(last[1].text.contains("Status 5 NOT_FOUND: no such order"));
        assert_eq!(last[1].tone, Tone::Error);
        assert!(last[0].text.contains("GONE"));
    }

    #[test]
    fn a_call_that_never_started_reports_why() {
        let mut log = stream(RequestKind::Grpc);
        log.on_grpc_done(&Execution {
            error: Some("connection refused".into()),
            ..Execution::default()
        });
        assert_eq!(log.result, Some(("Failed".to_owned(), false)));
        assert_eq!(log.entries[0].tone, Tone::Error);

        let mut log = stream(RequestKind::Grpc);
        log.on_grpc_done(&Execution {
            error: Some(getman_core::execute::CANCELLED.into()),
            cancelled: true,
            ..Execution::default()
        });
        assert_eq!(log.result, Some(("Cancelled".to_owned(), false)));
        assert_eq!(log.entries[0].tone, Tone::Note);
    }

    #[test]
    fn typed_messages_follow_the_kind() {
        let Outbound::Message(message) = typed(RequestKind::Websocket, "hi there").unwrap() else {
            panic!("a WebSocket message");
        };
        assert_eq!(message.content, "hi there");
        assert_eq!(message.format, MessageFormat::Text);

        let Outbound::Emit(emit) = typed(RequestKind::Socketio, "chat [\"hi\", 1]").unwrap() else {
            panic!("a Socket.IO event");
        };
        assert_eq!(
            (emit.event.as_str(), emit.args.as_str()),
            ("chat", "[\"hi\", 1]")
        );
        let Outbound::Emit(emit) = typed(RequestKind::Socketio, "ping").unwrap() else {
            panic!("a Socket.IO event");
        };
        assert_eq!((emit.event.as_str(), emit.args.as_str()), ("ping", ""));
        assert!(typed(RequestKind::Socketio, "  ").is_err());
        assert!(
            typed(RequestKind::Sse, "x")
                .unwrap_err()
                .contains("only receive")
        );
    }

    #[test]
    fn saved_messages_are_listed_by_name() {
        let request: Request = serde_yaml_ng::from_str(
            "kind: websocket\nurl: ws://x.test\nwebsocket:\n  messages:\n    - name: Hello\n      content: '{\"a\": 1}'\n    - content: ping\n",
        )
        .unwrap();
        let saved = saved_messages(&request);
        assert_eq!(saved[0].label, "Hello");
        assert_eq!(saved[0].preview, "{\"a\": 1}");
        assert_eq!(saved[1].label, "ping");

        let request: Request = serde_yaml_ng::from_str(
            "kind: socketio\nurl: http://x.test\nsocketio:\n  messages:\n    - event: subscribe\n      args: '[\"eu\"]'\n",
        )
        .unwrap();
        let saved = saved_messages(&request);
        assert_eq!(saved[0].label, "subscribe");
        assert_eq!(saved[0].preview, "subscribe [\"eu\"]");
        assert!(saved_messages(&Request::default()).is_empty());
    }

    #[test]
    fn times_are_short() {
        assert_eq!(elapsed(0), "+0.000s");
        assert_eq!(elapsed(12_345), "+12.345s");
        assert_eq!(elapsed(125_000), "+2m05s");
        assert_eq!(elapsed(3_720_000), "+1h02m");
    }
}
