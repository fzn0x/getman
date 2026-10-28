//! Requests that stay open and carry messages: WebSocket, Server-Sent
//! Events and Socket.IO.
//!
//! [`connect`] prepares such a request the way an HTTP request is prepared
//! (inherited headers, authentication, variables) and starts a task that
//! holds the connection. The task reports everything that happens as
//! [`StreamEvent`]s on the [`Session`], and takes messages to send and the
//! order to close through the session's [`Control`]. Scripts, checks and
//! captures do not run for these requests.

mod settings;
mod socketio;
mod sse;
#[cfg(test)]
mod test_servers;
#[cfg(test)]
mod tests;
mod tls;
mod websocket;

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{Notify, mpsc};

use crate::auth::Authenticator;
use crate::error::{Error, Result};
use crate::execute::{Context, Executor};
use crate::http::{Header, HttpClient, Outgoing, Transport, header_list, set_header};
use crate::model::{Auth, Body, KeyValue, Request, RequestKind};
use crate::prepare::prepare;
use crate::vars::VarStack;

pub use settings::{
    MessageFormat, SavedEmit, SavedMessage, SocketIoSettings, SseSettings, WebSocketSettings,
};

/// Messages longer than this are reported cut short; their `size` still
/// gives the full length. Keeps a stream of large messages from filling
/// the memory of whoever shows them.
pub const MAX_SHOWN_BYTES: usize = 256 * 1024;

/// The largest message Getman reads. A server that sends a larger one is
/// disconnected.
pub const MAX_MESSAGE_BYTES: usize = 16 * 1024 * 1024;

/// Events held for a reader that has fallen behind.
const EVENT_BUFFER: usize = 256;

/// Messages waiting to be sent.
const COMMAND_BUFFER: usize = 64;

/// Something that happened on a connection.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StreamEvent {
    /// When it happened, in milliseconds since the Unix epoch.
    pub at: i64,
    /// What happened.
    #[serde(flatten)]
    pub kind: EventKind,
}

impl StreamEvent {
    fn now(kind: EventKind) -> Self {
        Self {
            at: crate::time::now_millis(),
            kind,
        }
    }
}

/// What happened on a connection. The tag is `type`, since messages have
/// an `event` of their own.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    /// The connection is being opened with this request.
    Connecting {
        /// HTTP method of the request that opens the connection.
        method: String,
        /// The URL connected to.
        url: String,
        /// Headers sent with it.
        headers: Vec<Header>,
        /// Referenced variables that are not defined.
        missing: Vec<String>,
    },
    /// The connection is open, and this is how the server answered.
    Open {
        /// Status code: 101 for WebSocket, usually 200 for event streams.
        status: u16,
        /// Response headers.
        headers: Vec<Header>,
        /// The WebSocket subprotocol the server chose.
        #[serde(skip_serializing_if = "Option::is_none")]
        protocol: Option<String>,
        /// Milliseconds the connection took to open.
        time_ms: f64,
    },
    /// A message, sent or received.
    Message(Message),
    /// Something worth knowing, such as a reconnection.
    Note {
        /// What happened.
        message: String,
    },
    /// A problem that leaves the connection open, such as a message that
    /// could not be sent.
    Warning {
        /// What went wrong.
        message: String,
    },
    /// A problem that ends the connection. A `closed` event follows.
    Error {
        /// What went wrong.
        message: String,
    },
    /// The connection closed. Always the last event.
    Closed {
        /// WebSocket close code, if the closing side sent one.
        code: Option<u16>,
        /// Close reason, if any.
        reason: String,
        /// Whether Getman closed it, because it was asked to.
        by_client: bool,
    },
}

/// A message sent or received on a connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Message {
    /// Whether Getman sent it or received it.
    pub direction: Direction,
    /// The content: text as it is, binary data in Base64. For Socket.IO,
    /// the event's arguments as a JSON array.
    pub data: String,
    /// Whether the message is binary, so `data` is Base64.
    #[serde(skip_serializing_if = "is_false")]
    pub binary: bool,
    /// Size of the whole message in bytes.
    pub size: u64,
    /// Whether `data` was cut short at [`MAX_SHOWN_BYTES`].
    #[serde(skip_serializing_if = "is_false")]
    pub truncated: bool,
    /// Name of the saved message it was sent from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Event type of a Server-Sent Event, or the name of a Socket.IO event.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
    /// Id of the last Server-Sent Event that set one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Reconnection time a Server-Sent Event asked for, in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry: Option<u64>,
    /// Acknowledgement id of a Socket.IO event that asked for one, or of
    /// the acknowledgement itself.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ack: Option<u64>,
}

impl Message {
    /// A message of these bytes, cut short at [`MAX_SHOWN_BYTES`].
    pub(crate) fn new(direction: Direction, mut bytes: Vec<u8>, binary: bool) -> Self {
        let size = bytes.len() as u64;
        let truncated = bytes.len() > MAX_SHOWN_BYTES;
        if truncated {
            let mut end = MAX_SHOWN_BYTES;
            // Cut text between characters, not inside one.
            while !binary && end > 0 && bytes[end] & 0xC0 == 0x80 {
                end -= 1;
            }
            bytes.truncate(end);
        }
        let data = if binary {
            use base64::Engine as _;
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        } else {
            String::from_utf8_lossy(&bytes).into_owned()
        };
        Self {
            direction,
            data,
            binary,
            size,
            truncated,
            name: None,
            event: None,
            id: None,
            retry: None,
            ack: None,
        }
    }

    /// A text message.
    pub(crate) fn text(direction: Direction, text: String) -> Self {
        Self::new(direction, text.into_bytes(), false)
    }
}

/// Which way a message went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Getman sent it.
    Sent,
    /// The server sent it.
    Received,
}

/// A message to send on an open connection: a saved WebSocket message or
/// Socket.IO event, or one written on the spot.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outbound {
    /// A WebSocket message.
    Message(SavedMessage),
    /// A Socket.IO event.
    Emit(SavedEmit),
}

/// How a session reports what it receives.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Options {
    /// When whoever reads the events falls behind, skip received messages
    /// instead of waiting, so a fast stream cannot stall the connection.
    /// A warning says how many were skipped.
    pub lossy: bool,
}

/// An open connection, or one being opened.
#[derive(Debug)]
pub struct Session {
    events: mpsc::Receiver<StreamEvent>,
    control: Control,
}

impl Session {
    /// The next event, or `None` after the connection closed.
    pub async fn next(&mut self) -> Option<StreamEvent> {
        self.events.recv().await
    }

    /// A handle that sends messages and closes the connection.
    pub fn control(&self) -> Control {
        self.control.clone()
    }

    /// The handle and the events, to use apart.
    pub fn into_parts(self) -> (Control, mpsc::Receiver<StreamEvent>) {
        (self.control, self.events)
    }
}

/// Sends messages on a session and closes it.
#[derive(Debug, Clone)]
pub struct Control {
    commands: mpsc::Sender<Outbound>,
    close: Arc<Notify>,
}

impl Control {
    /// Queues a message. It is sent once the connection is open; what was
    /// sent, or why it could not be, arrives as an event.
    pub fn send(&self, message: Outbound) -> Result<()> {
        self.commands.try_send(message).map_err(|e| match e {
            TrySendError::Full(_) => Error::Invalid(
                "Too many messages are waiting to be sent; try again in a moment".into(),
            ),
            TrySendError::Closed(_) => Error::Invalid("The connection is closed".into()),
        })
    }

    /// Closes the connection. A `closed` event follows.
    pub fn close(&self) {
        self.close.notify_one();
    }

    /// Whether the connection has closed.
    pub fn is_closed(&self) -> bool {
        self.commands.is_closed()
    }
}

/// Opens a WebSocket, Server-Sent Events or Socket.IO request. `path`
/// places it in the workspace, for what it inherits. Problems with the
/// request itself, such as an invalid URL, are returned; everything after,
/// including failing to connect, arrives as events.
pub fn connect(
    executor: &Executor,
    path: Option<&str>,
    request: &Request,
    context: &Context,
    options: Options,
) -> Result<Session> {
    if !request.kind.is_realtime() {
        return Err(Error::Invalid(format!(
            "{} requests are sent, not connected to",
            request.kind.label()
        )));
    }
    let (effective, vars) = executor.resolve(path, request, context)?;
    let mut headers = effective.headers.clone();
    let mut stored = request.clone();
    if request.kind == RequestKind::Sse {
        // As browsers do, unless the request says otherwise.
        for (name, value) in [
            ("Accept", "text/event-stream"),
            ("Cache-Control", "no-cache"),
        ] {
            if !headers.iter().any(|h| h.name.eq_ignore_ascii_case(name)) {
                headers.push(KeyValue::new(name, value));
            }
        }
    } else {
        stored.body = Body::None;
        if matches!(effective.auth, Auth::Digest { .. }) {
            return Err(Error::Invalid(format!(
                "Digest authentication cannot open a {} connection; use Basic, Bearer, an API key or OAuth 2.0",
                request.kind.label()
            )));
        }
    }
    let prepared = prepare(
        &stored,
        &headers,
        &effective.auth,
        &vars,
        executor.workspace.root(),
    )?;
    let mut outgoing = prepared.outgoing;
    let mut namespace = String::new();
    match request.kind {
        RequestKind::Websocket => {
            outgoing.method = "GET".into();
            outgoing.url = websocket::url(&outgoing.url)?;
        }
        RequestKind::Socketio => {
            outgoing.method = "GET".into();
            (outgoing.url, namespace) =
                socketio::endpoint(&outgoing.url, &request.socketio, &vars)?;
        }
        RequestKind::Sse => sse::check_url(&outgoing.url)?,
        RequestKind::Http => {}
    }

    let transport = crate::prepare::transport(
        &context.settings.clone().or(effective.settings.clone()),
        &vars,
        executor.workspace.root(),
    )?;
    let (events, receiver) = mpsc::channel(EVENT_BUFFER);
    let (commands, command_receiver) = mpsc::channel(COMMAND_BUFFER);
    let close = Arc::new(Notify::new());
    let job = Job {
        kind: request.kind,
        outgoing,
        missing: prepared.missing,
        auth: effective.auth,
        vars,
        transport,
        request: request.clone(),
        namespace,
        http: Arc::clone(&executor.http),
        authenticator: Arc::clone(&executor.authenticator),
        commands: command_receiver,
        close: Arc::clone(&close),
    };
    let sink = Sink {
        events,
        lossy: options.lossy,
        skipped: 0,
    };
    tokio::spawn(job.run(sink));
    Ok(Session {
        events: receiver,
        control: Control { commands, close },
    })
}

/// Everything the task holding a connection needs.
struct Job {
    kind: RequestKind,
    outgoing: Outgoing,
    missing: Vec<String>,
    auth: Auth,
    vars: VarStack,
    transport: Transport,
    request: Request,
    /// The Socket.IO namespace.
    namespace: String,
    http: Arc<HttpClient>,
    authenticator: Arc<Authenticator>,
    commands: mpsc::Receiver<Outbound>,
    close: Arc<Notify>,
}

impl Job {
    async fn run(mut self, mut sink: Sink) {
        if let Auth::Oauth2(settings) = &self.auth {
            let token = self
                .authenticator
                .token(
                    &self.http,
                    settings,
                    &self.vars,
                    self.transport.clone(),
                    false,
                )
                .await;
            match token {
                Ok(token) => {
                    let scheme = if token.token_type.is_empty()
                        || token.token_type.eq_ignore_ascii_case("bearer")
                    {
                        "Bearer"
                    } else {
                        token.token_type.as_str()
                    };
                    set_header(
                        &mut self.outgoing.headers,
                        "Authorization",
                        format!("{scheme} {}", token.access_token),
                    );
                }
                Err(e) => {
                    sink.fail(e.to_string()).await;
                    return;
                }
            }
        }
        let connecting = EventKind::Connecting {
            method: self.outgoing.method.clone(),
            url: self.outgoing.url.clone(),
            headers: self.outgoing.headers.clone(),
            missing: std::mem::take(&mut self.missing),
        };
        if !sink.emit(connecting).await {
            return;
        }
        match self.kind {
            RequestKind::Websocket => websocket::run(&mut self, &mut sink).await,
            RequestKind::Sse => sse::run(&mut self, &mut sink).await,
            RequestKind::Socketio => socketio::run(&mut self, &mut sink).await,
            RequestKind::Http => {}
        }
        // Refuse messages before the events end, so a reader who saw the
        // end cannot queue one that is never sent.
        self.commands.close();
        drop(sink);
    }

    /// Replaces the variables in text to send, warning about missing ones.
    async fn render(&self, text: &str, sink: &mut Sink) -> String {
        let rendered = self.vars.interpolate(text);
        if !rendered.missing.is_empty() {
            let names: Vec<String> = rendered
                .missing
                .iter()
                .map(|name| format!("{{{{{name}}}}}"))
                .collect();
            sink.emit(EventKind::Warning {
                message: format!(
                    "Not defined: {}. They were sent as written.",
                    names.join(", ")
                ),
            })
            .await;
        }
        rendered.text
    }

    /// How long opening the connection may take; `None` waits forever.
    fn connect_timeout(&self) -> Option<Duration> {
        (self.transport.timeout_ms > 0).then(|| Duration::from_millis(self.transport.timeout_ms))
    }
}

/// Where a connection's events go.
struct Sink {
    events: mpsc::Sender<StreamEvent>,
    lossy: bool,
    skipped: u64,
}

impl Sink {
    /// Reports an event. Returns whether anyone still listens.
    async fn emit(&mut self, kind: EventKind) -> bool {
        if !self.report_skipped().await {
            return false;
        }
        self.events.send(StreamEvent::now(kind)).await.is_ok()
    }

    /// Reports a received message, or skips it when the reader is behind
    /// and the session is lossy. Returns whether anyone still listens.
    async fn received(&mut self, message: Message) -> bool {
        if !self.lossy {
            return self.emit(EventKind::Message(message)).await;
        }
        if self.skipped > 0 && self.events.capacity() > 1 && !self.report_skipped().await {
            return false;
        }
        match self
            .events
            .try_send(StreamEvent::now(EventKind::Message(message)))
        {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => {
                self.skipped += 1;
                true
            }
            Err(TrySendError::Closed(_)) => false,
        }
    }

    async fn report_skipped(&mut self) -> bool {
        if self.skipped == 0 {
            return true;
        }
        let count = std::mem::take(&mut self.skipped);
        let message = format!(
            "Skipped {count} received {} that arrived faster than they could be shown",
            if count == 1 { "message" } else { "messages" }
        );
        self.events
            .send(StreamEvent::now(EventKind::Warning { message }))
            .await
            .is_ok()
    }

    /// Reports an error that ends the connection, and the end.
    async fn fail(&mut self, message: String) {
        if self.emit(EventKind::Error { message }).await {
            self.closed(None, String::new(), false).await;
        }
    }

    /// Reports the end of the connection.
    async fn closed(&mut self, code: Option<u16>, reason: String, by_client: bool) {
        self.emit(EventKind::Closed {
            code,
            reason,
            by_client,
        })
        .await;
    }

    /// A handle that tells when no one listens anymore.
    fn watch(&self) -> mpsc::Sender<StreamEvent> {
        self.events.clone()
    }
}

/// Waits for `future`, giving up after `limit`.
async fn within<T>(
    limit: Option<Duration>,
    future: impl Future<Output = Result<T, String>>,
) -> Result<T, String> {
    match limit {
        Some(limit) => tokio::time::timeout(limit, future).await.unwrap_or_else(|_| {
            Err(format!(
                "The connection timed out after {} ms; raise the timeout in Settings if the server is slow",
                limit.as_millis()
            ))
        }),
        None => future.await,
    }
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(value: &bool) -> bool {
    !*value
}
