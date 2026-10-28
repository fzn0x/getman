//! Running a WebSocket, Server-Sent Events or Socket.IO request from start
//! to finish, as an HTTP request is: pre-request scripts before connecting,
//! then connecting, sending the saved messages and collecting what arrives
//! until a limit, then post-response scripts, checks and captures.
//!
//! The handshake plays the part of the response, and the messages received
//! are what scripts read as `gm.messages` and checks as `messages`. The
//! pieces are public so the browser UI, which holds the connection open
//! for as long as a person likes, can use them around its own session:
//! [`Executor::before_connect`], a [`Recorder`] fed with the session's
//! events, and [`Executor::after_connect`].

use serde::{Deserialize, Serialize};
use tokio::time::Instant;

use super::{
    Direction, EventKind, Limits, Message, Options, Outbound, Session, StreamEvent, connect_with,
};
use crate::checks::{messages_value, run_captures_on, run_checks_on};
use crate::error::Result;
use crate::execute::{
    Context, Execution, Executor, absorb, run_scripts, script_request, script_scopes, var_stack,
};
use crate::http::{Header, Outgoing, OutgoingBody, Payload, Response, Size};
use crate::model::{KeyValue, Request, RequestKind};
use crate::prepare::{Effective, transport};
use crate::script::{
    ChangeScope, Event, ScriptHost, ScriptInfo, ScriptInput, ScriptMessage, ScriptResponse,
    VarChange,
};

/// Most messages kept for one connection; later ones are counted only.
const MAX_KEPT_MESSAGES: usize = 10_000;

/// What to do on a connection that nobody steers: the messages to send
/// once it is open, and when to end it.
#[derive(Debug, Clone, Default)]
pub struct Plan {
    /// Messages to send, in order, as soon as the connection is open.
    pub send: Vec<Outbound>,
    /// When to end the connection.
    pub limits: Limits,
}

impl Plan {
    /// What `getman run` does with a request: send its saved messages,
    /// and end as its `until` says, or after the defaults.
    pub fn of(request: &Request) -> Self {
        Self {
            send: saved_messages(request),
            limits: request.until.limits(),
        }
    }
}

/// The messages or events saved with a request, in order.
pub fn saved_messages(request: &Request) -> Vec<Outbound> {
    match request.kind {
        RequestKind::Websocket => request
            .websocket
            .messages
            .iter()
            .cloned()
            .map(Outbound::Message)
            .collect(),
        RequestKind::Socketio => request
            .socketio
            .messages
            .iter()
            .cloned()
            .map(Outbound::Emit)
            .collect(),
        _ => Vec::new(),
    }
}

/// A message of a connection, with when it went.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// Whether Getman sent it or received it.
    pub direction: Direction,
    /// The content: text as it is, binary data in Base64. For Socket.IO,
    /// the event's arguments as a JSON array.
    pub data: String,
    /// Milliseconds after the connection opened.
    pub time: f64,
    /// Size of the whole message in bytes.
    pub size: u64,
    /// Whether `data` is Base64.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub binary: bool,
    /// Name of the saved message it was sent from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Event type of a Server-Sent Event, or name of a Socket.IO event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
    /// Id of a Server-Sent Event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Acknowledgement id of a Socket.IO event or acknowledgement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ack: Option<u64>,
}

/// How a connection ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Closed {
    /// WebSocket close code, if the closing side sent one.
    pub code: Option<u16>,
    /// Close reason, if any.
    pub reason: String,
    /// Whether Getman closed it.
    pub by_client: bool,
}

/// What happened on a connection, from the handshake to the end.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RealtimeCall {
    /// The URL connected to.
    pub url: String,
    /// Headers sent with the handshake.
    pub request_headers: Vec<Header>,
    /// Variables the request referenced that are not defined.
    pub missing: Vec<String>,
    /// Whether the connection opened.
    pub opened: bool,
    /// The handshake's status: 101 for WebSocket, usually 200 otherwise.
    pub status: u16,
    /// The handshake's response headers.
    pub headers: Vec<Header>,
    /// The WebSocket subprotocol the server chose.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
    /// Milliseconds the connection took to open.
    pub open_ms: f64,
    /// Milliseconds the connection stayed open.
    pub duration_ms: f64,
    /// Messages sent and received, in order.
    pub messages: Vec<Entry>,
    /// Messages after the first ones kept, which were counted only.
    pub dropped: u64,
    /// Bytes received.
    pub received_bytes: u64,
    /// Things worth knowing, such as an upgrade.
    pub notes: Vec<String>,
    /// Problems that left the connection open.
    pub warnings: Vec<String>,
    /// The problem that ended the connection, if one did.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// How it ended.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closed: Option<Closed>,
}

impl RealtimeCall {
    /// The messages received, as scripts see them.
    pub fn received(&self) -> Vec<ScriptMessage> {
        self.messages
            .iter()
            .filter(|m| m.direction == Direction::Received)
            .map(|m| ScriptMessage {
                direction: "received".into(),
                data: m.data.clone(),
                time: m.time,
                size: m.size,
                binary: m.binary,
                event: m.event.clone(),
                id: m.id.clone(),
                ack: m.ack,
            })
            .collect()
    }
}

/// Collects the events of a session into a [`RealtimeCall`].
#[derive(Debug, Default)]
pub struct Recorder {
    call: RealtimeCall,
    opened_at: Option<i64>,
}

impl Recorder {
    /// A recorder with nothing in it.
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes in an event.
    #[allow(clippy::cast_precision_loss)]
    pub fn record(&mut self, event: &StreamEvent) {
        let call = &mut self.call;
        match &event.kind {
            EventKind::Connecting {
                url,
                headers,
                missing,
                ..
            } => {
                url.clone_into(&mut call.url);
                call.request_headers.clone_from(headers);
                call.missing.clone_from(missing);
            }
            EventKind::Open {
                status,
                headers,
                protocol,
                time_ms,
            } => {
                call.opened = true;
                call.status = *status;
                call.headers.clone_from(headers);
                call.protocol.clone_from(protocol);
                call.open_ms = *time_ms;
                self.opened_at = Some(event.at);
            }
            EventKind::Message(message) => {
                if message.direction == Direction::Received {
                    call.received_bytes += message.size;
                }
                if call.messages.len() < MAX_KEPT_MESSAGES {
                    call.messages.push(entry(
                        message,
                        event.at,
                        self.opened_at.unwrap_or(event.at),
                    ));
                } else {
                    call.dropped += 1;
                }
            }
            EventKind::Note { message } => call.notes.push(message.clone()),
            EventKind::Warning { message } => call.warnings.push(message.clone()),
            EventKind::Error { message } => call.error = Some(message.clone()),
            EventKind::Closed {
                code,
                reason,
                by_client,
            } => {
                call.closed = Some(Closed {
                    code: *code,
                    reason: reason.clone(),
                    by_client: *by_client,
                });
                if let Some(opened) = self.opened_at {
                    call.duration_ms = (event.at - opened).max(0) as f64;
                }
            }
        }
    }

    /// Notes a problem that is not an event of the session.
    pub fn warn(&mut self, message: String) {
        self.call.warnings.push(message);
    }

    /// The connection as it is so far.
    pub fn finish(self) -> RealtimeCall {
        self.call
    }
}

#[allow(clippy::cast_precision_loss)]
fn entry(message: &Message, at: i64, opened: i64) -> Entry {
    Entry {
        direction: message.direction,
        data: message.data.clone(),
        time: (at - opened).max(0) as f64,
        size: message.size,
        binary: message.binary,
        name: message.name.clone(),
        event: message.event.clone(),
        id: message.id.clone(),
        ack: message.ack,
    }
}

/// What a pre-request script left of a request to connect with.
#[derive(Debug, Clone)]
pub struct Ready {
    /// The request, with the URL a script set.
    pub request: Request,
    /// The headers the scripts left, when scripts ran; otherwise the
    /// request inherits and sets its own.
    pub headers: Option<Vec<KeyValue>>,
}

impl Executor {
    /// Runs a WebSocket, Server-Sent Events or Socket.IO request: pre-request
    /// scripts, connecting, sending the plan's messages and collecting
    /// what arrives until its limits, then post-response scripts, checks
    /// and captures. `on_event` sees each event as it happens; it returns
    /// false to end the connection. Setting `context.cancel` ends it too.
    pub async fn execute_realtime(
        &self,
        path: Option<&str>,
        request: &Request,
        context: &mut Context,
        plan: Plan,
        on_event: &mut dyn FnMut(&StreamEvent) -> bool,
    ) -> Execution {
        let mut execution = Execution {
            path: path.map(str::to_owned),
            name: request.name.clone(),
            ..Execution::default()
        };
        if let Err(e) = execute(self, path, request, context, &mut execution, plan, on_event).await
        {
            execution.error = Some(e.to_string());
        }
        execution
    }

    /// Runs the pre-request scripts of a request that is about to connect.
    /// Returns what to connect with, or `None` when a script failed or
    /// skipped the request; `execution` says which.
    pub async fn before_connect(
        &self,
        path: Option<&str>,
        request: &Request,
        context: &mut Context,
        execution: &mut Execution,
    ) -> Result<Option<Ready>> {
        let chain = match path {
            Some(path) => self.workspace.folder_chain(path)?,
            None => vec![self.workspace.read_folder("")?],
        };
        let mut request = request.clone();
        let effective = Effective::new(&chain, &request);
        if effective.pre_request.is_empty() {
            return Ok(Some(Ready {
                request,
                headers: None,
            }));
        }
        let files = self.file_vars(context.environment.as_deref())?;
        let settings = context.settings.clone().or(effective.settings.clone());
        let early = transport(
            &settings,
            &var_stack(&files, &chain, &request, context),
            self.workspace.root(),
        );
        let host = ScriptHost {
            http: Some((
                std::sync::Arc::clone(&self.http),
                tokio::runtime::Handle::current(),
            )),
            transport: early.unwrap_or_default(),
            timeout: context.script_timeout,
        };
        let mut headers = effective.headers.clone();
        let input = ScriptInput {
            event: Event::PreRequest,
            info: info(path, &request, context),
            scopes: script_scopes(&files, &chain, &request, context),
            request: script_request(&request, &headers),
            response: None,
            messages: None,
            cookies: self.cookies_for(&request.url, &files, &chain, context),
        };
        let output = run_scripts(host, effective.pre_request.clone(), input).await;
        if let Some(edited) = &output.request {
            request.url.clone_from(&edited.url);
            headers.clone_from(&edited.headers);
        }
        if absorb(output, context, execution) {
            execution.error =
                Some("a pre-request script failed, so the connection was not made".into());
            return Ok(None);
        }
        if execution.skipped {
            return Ok(None);
        }
        Ok(Some(Ready {
            request,
            headers: Some(headers),
        }))
    }

    /// Finishes a request whose connection has ended: the handshake is the
    /// response, and the messages received are what post-response scripts,
    /// checks and captures read. Fills in `execution`.
    pub async fn after_connect(
        &self,
        path: Option<&str>,
        request: &Request,
        context: &mut Context,
        call: RealtimeCall,
        execution: &mut Execution,
    ) {
        if let Err(e) = self
            .finish_connection(path, request, context, call, execution)
            .await
        {
            execution.error.get_or_insert_with(|| e.to_string());
        }
    }

    async fn finish_connection(
        &self,
        path: Option<&str>,
        request: &Request,
        context: &mut Context,
        call: RealtimeCall,
        execution: &mut Execution,
    ) -> Result<()> {
        execution.request = Some(Outgoing {
            method: request.kind.as_str().to_ascii_uppercase(),
            url: call.url.clone(),
            headers: call.request_headers.clone(),
            body: OutgoingBody::None,
        });
        execution.error = call.error.clone();
        execution.missing_variables.clone_from(&call.missing);
        if call.dropped > 0 {
            execution.warnings.push(format!(
                "{} messages arrived after the first {MAX_KEPT_MESSAGES} and were not kept",
                call.dropped
            ));
        }
        if !call.opened {
            if execution.error.is_none() {
                execution.error = Some("The connection did not open.".into());
            }
            execution.realtime = Some(call);
            return Ok(());
        }

        let chain = match path {
            Some(path) => self.workspace.folder_chain(path)?,
            None => vec![self.workspace.read_folder("")?],
        };
        let files = self.file_vars(context.environment.as_deref())?;
        let effective = Effective::new(&chain, request);
        let response = handshake_view(&call);
        let received = call.received();
        if !effective.post_response.is_empty() {
            let settings = context.settings.clone().or(effective.settings.clone());
            let early = transport(
                &settings,
                &var_stack(&files, &chain, request, context),
                self.workspace.root(),
            );
            let host = ScriptHost {
                http: Some((
                    std::sync::Arc::clone(&self.http),
                    tokio::runtime::Handle::current(),
                )),
                transport: early.unwrap_or_default(),
                timeout: context.script_timeout,
            };
            let input = ScriptInput {
                event: Event::PostResponse,
                info: info(path, request, context),
                scopes: script_scopes(&files, &chain, request, context),
                request: script_request(request, &effective.headers),
                response: Some(ScriptResponse::from(&response)),
                messages: Some(received.clone()),
                cookies: self.cookies_for(&request.url, &files, &chain, context),
            };
            let output = run_scripts(host, effective.post_response.clone(), input).await;
            absorb(output, context, execution);
        }
        let messages = messages_value(&received);
        execution.checks = run_checks_on(&request.checks, &response, Some(&messages));
        let captured = run_captures_on(&request.captures, &response, Some(&messages));
        let scope = if context.environment.is_some() {
            ChangeScope::Environment
        } else {
            ChangeScope::Collection
        };
        for (name, value) in captured.values {
            let change = VarChange {
                scope,
                name,
                value: Some(value),
            };
            context.overlay.apply(&change);
            execution.changes.push(change);
        }
        execution.warnings.extend(captured.failures);
        execution.warnings.extend(call.warnings.iter().cloned());
        execution.response = Some(response);
        execution.realtime = Some(call);
        Ok(())
    }
}

fn info(path: Option<&str>, request: &Request, context: &Context) -> ScriptInfo {
    ScriptInfo {
        request_name: request.name.clone(),
        request_path: path.unwrap_or_default().to_owned(),
        iteration: context.iteration,
        iteration_count: context.iteration_count,
    }
}

/// The handshake dressed as a response: its status and headers, an empty
/// body, the time the connection took to open, and the bytes received.
fn handshake_view(call: &RealtimeCall) -> Response {
    let status_text = http::StatusCode::from_u16(call.status)
        .ok()
        .and_then(|status| status.canonical_reason())
        .unwrap_or_default()
        .to_owned();
    let header_bytes = call
        .headers
        .iter()
        .map(|h| h.name.len() + h.value.len() + 4)
        .sum::<usize>();
    Response {
        status: call.status,
        status_text,
        http_version: "HTTP/1.1".into(),
        url: call.url.clone(),
        headers: call.headers.clone(),
        body: Payload(Vec::new()),
        truncated: false,
        time_ms: call.open_ms,
        headers_ms: call.open_ms,
        size: Size {
            headers: header_bytes as u64,
            body: call.received_bytes,
        },
        remote_addr: None,
        timings: None,
        tls: None,
        http3_available: false,
        stream_open: false,
    }
}

pub(crate) async fn execute(
    executor: &Executor,
    path: Option<&str>,
    request: &Request,
    context: &mut Context,
    execution: &mut Execution,
    plan: Plan,
    on_event: &mut dyn FnMut(&StreamEvent) -> bool,
) -> Result<()> {
    let Some(ready) = executor
        .before_connect(path, request, context, execution)
        .await?
    else {
        return Ok(());
    };
    let session = connect_with(
        executor,
        path,
        &ready.request,
        context,
        Options::default(),
        ready.headers,
    )?;
    let cancel = context.cancel.clone();
    let mut recorder = Recorder::new();
    drive(session, plan, cancel, &mut recorder, on_event).await;
    let call = recorder.finish();
    executor
        .after_connect(path, &ready.request, context, call, execution)
        .await;
    Ok(())
}

/// Reads a session's events until it closes, sending the plan's messages
/// once it opens and ending it when a limit is reached.
async fn drive(
    mut session: Session,
    plan: Plan,
    cancel: Option<crate::execute::Cancel>,
    recorder: &mut Recorder,
    on_event: &mut dyn FnMut(&StreamEvent) -> bool,
) {
    let control = session.control();
    let mut outbound = plan.send;
    let mut received = 0_u64;
    let mut deadline: Option<Instant> = None;
    let (mut opened, mut closing) = (false, false);
    loop {
        let event = tokio::select! {
            event = session.next() => match event {
                Some(event) => event,
                None => break,
            },
            () = async {
                match deadline {
                    Some(deadline) => tokio::time::sleep_until(deadline).await,
                    None => std::future::pending().await,
                }
            }, if !closing => {
                closing = true;
                control.close();
                continue;
            }
            () = async {
                match &cancel {
                    Some(cancel) => cancel.cancelled().await,
                    None => std::future::pending().await,
                }
            }, if !closing => {
                closing = true;
                control.close();
                continue;
            }
        };
        // What arrives while the connection is being closed is past the
        // limit that closed it.
        if closing
            && matches!(&event.kind, EventKind::Message(m) if m.direction == Direction::Received)
        {
            continue;
        }
        recorder.record(&event);
        if !on_event(&event) && !closing {
            closing = true;
            control.close();
        }
        match &event.kind {
            EventKind::Open { .. } if !opened => {
                opened = true;
                deadline = plan.limits.time.map(|time| Instant::now() + time);
                for message in outbound.drain(..) {
                    if let Err(e) = control.send(message) {
                        recorder.warn(e.to_string());
                    }
                }
                if !closing && plan.limits.messages == Some(0) {
                    closing = true;
                    control.close();
                }
            }
            EventKind::Message(message) if message.direction == Direction::Received => {
                received += 1;
                if !closing && plan.limits.messages.is_some_and(|most| received >= most) {
                    closing = true;
                    control.close();
                }
            }
            _ => {}
        }
    }
}
