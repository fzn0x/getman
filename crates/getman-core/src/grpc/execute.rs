//! Running gRPC requests with the same steps as HTTP requests around the
//! call: pre-request scripts, variables, metadata from headers and
//! authentication, then post-response scripts, checks and captures.

use std::fmt::Write as _;
use std::iter;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use prost::Message as _;
use prost_reflect::{DescriptorPool, DynamicMessage, MessageDescriptor, MethodDescriptor};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};

use super::json::{from_json, split_messages, to_json};
use super::schema::Schema;
use super::status::{Code, Status};
use super::transport::{self, Connection, Head, Target, header_list};
use super::{CallType, Direction, GrpcCall, StreamMessage, metadata, reflected, reflection};
use crate::checks::{run_captures, run_checks};
use crate::error::{Error, Result};
use crate::execute::{Context, Execution, Executor, absorb, run_scripts, script_scopes, var_stack};
use crate::http::{Header, Outgoing, OutgoingBody, Payload, Response, Size, Transport};
use crate::model::{Folder, Request, Settings};
use crate::prepare::Effective;
use crate::script::{
    ChangeScope, Event, ScriptBody, ScriptHost, ScriptInfo, ScriptInput, ScriptRequest,
    ScriptResponse, VarChange,
};
use crate::vars::VarStack;
use crate::workspace::Workspace;

/// How long connecting may take, unless the deadline is shorter.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// Deadline of unary calls whose timeout is not set, the same as the
/// default timeout of HTTP requests.
const DEFAULT_DEADLINE: Duration = Duration::from_secs(30);
/// How long reflection may take.
const REFLECTION_TIMEOUT: Duration = Duration::from_secs(30);
/// Most messages kept for one call; later ones are counted only.
const MAX_KEPT_MESSAGES: usize = 10_000;

/// Steers a call while it runs, as the browser UI does: messages to send
/// one at a time, the end of the stream, or cancelling.
#[derive(Debug)]
pub struct Live {
    /// What to do next.
    pub commands: mpsc::UnboundedReceiver<Command>,
    /// What happens, as it happens.
    pub events: mpsc::UnboundedSender<LiveEvent>,
}

/// Something to do on a live call.
#[derive(Debug)]
pub enum Command {
    /// Send messages written as JSON, one after another, once variables
    /// are replaced. The reply says how many were sent, or why none were.
    Send {
        /// The messages.
        text: String,
        /// Gets the outcome.
        reply: oneshot::Sender<Result<usize, String>>,
    },
    /// End the client's side of the stream.
    End,
    /// Stop the call.
    Cancel,
}

/// Something that happened on a live call.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum LiveEvent {
    /// The call started.
    Started {
        /// The method's full name.
        method: String,
        /// Which sides stream.
        call_type: CallType,
        /// The server's address.
        address: String,
    },
    /// The response headers arrived.
    Headers {
        /// The headers.
        headers: Vec<Header>,
        /// Milliseconds from the start of the call.
        time_ms: f64,
    },
    /// A message was sent or arrived.
    Message {
        /// The message.
        message: StreamMessage,
    },
}

/// A message ready to send.
struct Encoded {
    json: Value,
    bytes: Vec<u8>,
}

/// How a call is steered and watched. By default every message the
/// request holds is sent at once, the stream ends, and nobody watches.
#[derive(Debug, Default)]
struct Control {
    commands: Option<mpsc::UnboundedReceiver<Command>>,
    events: Option<mpsc::UnboundedSender<LiveEvent>>,
}

impl Executor {
    /// Runs a gRPC request while `live` steers it: calls that stream from
    /// the client start without sending anything and send what
    /// [`Command::Send`] gives them. Other calls send the request's message
    /// at once.
    pub async fn execute_live(
        &self,
        path: Option<&str>,
        request: &Request,
        context: &mut Context,
        live: Live,
    ) -> Execution {
        let control = Control {
            commands: Some(live.commands),
            events: Some(live.events),
        };
        self.execute_grpc(path, request, context, control).await
    }

    /// Runs a gRPC request like [`Executor::execute`], reporting headers
    /// and messages to `events` as they happen.
    pub async fn execute_watched(
        &self,
        path: Option<&str>,
        request: &Request,
        context: &mut Context,
        events: mpsc::UnboundedSender<LiveEvent>,
    ) -> Execution {
        let control = Control {
            commands: None,
            events: Some(events),
        };
        self.execute_grpc(path, request, context, control).await
    }

    async fn execute_grpc(
        &self,
        path: Option<&str>,
        request: &Request,
        context: &mut Context,
        control: Control,
    ) -> Execution {
        let mut execution = Execution {
            path: path.map(str::to_owned),
            name: request.name.clone(),
            ..Execution::default()
        };
        if let Err(e) = execute_with(self, path, request, context, &mut execution, control).await {
            execution.error = Some(e.to_string());
        }
        execution
    }
}

/// Runs a gRPC request from start to finish: every message the request
/// holds is sent and the stream ends.
pub(crate) async fn execute(
    executor: &Executor,
    path: Option<&str>,
    request: &Request,
    context: &mut Context,
    execution: &mut Execution,
) -> Result<()> {
    execute_with(
        executor,
        path,
        request,
        context,
        execution,
        Control::default(),
    )
    .await
}

async fn execute_with(
    executor: &Executor,
    path: Option<&str>,
    request: &Request,
    context: &mut Context,
    execution: &mut Execution,
    control: Control,
) -> Result<()> {
    let mut call = GrpcCall {
        method: request.method.trim().trim_start_matches('/').to_owned(),
        tls: request.grpc.tls,
        ..GrpcCall::default()
    };
    let result = run(
        executor, path, request, context, execution, control, &mut call,
    )
    .await;
    execution.grpc = Some(call);
    result
}

#[allow(clippy::too_many_arguments)]
async fn run(
    executor: &Executor,
    path: Option<&str>,
    request: &Request,
    context: &mut Context,
    execution: &mut Execution,
    control: Control,
    call: &mut GrpcCall,
) -> Result<()> {
    let chain = folder_chain(&executor.workspace, path)?;
    let files = executor.file_vars(context.environment.as_deref())?;
    let mut request = request.clone();
    let effective = Effective::new(&chain, &request);
    let transport = executor.transport(path, &request, context)?;
    let mut headers = effective.headers.clone();
    let info = ScriptInfo {
        request_name: request.name.clone(),
        request_path: path.unwrap_or_default().to_owned(),
        iteration: context.iteration,
        iteration_count: context.iteration_count,
    };
    let host = ScriptHost {
        http: Some((
            Arc::clone(&executor.http),
            tokio::runtime::Handle::current(),
        )),
        transport: transport.clone(),
        timeout: context.script_timeout,
    };

    // Pre-request scripts. The message is the body they see and edit.
    if !effective.pre_request.is_empty() {
        let input = ScriptInput {
            event: Event::PreRequest,
            info: info.clone(),
            scopes: script_scopes(&files, &chain, &request, context),
            request: script_request(&request, &headers),
            response: None,
            cookies: Vec::new(),
        };
        let output = run_scripts(host.clone(), effective.pre_request.clone(), input).await;
        if let Some(edited) = &output.request {
            request.method.clone_from(&edited.method);
            request.url.clone_from(&edited.url);
            headers.clone_from(&edited.headers);
            if let Some(text) = &edited.body {
                request.grpc.message.clone_from(text);
            }
        }
        if absorb(output, context, execution) {
            execution.error = Some("a pre-request script failed, so the call was not made".into());
            return Ok(());
        }
        if execution.skipped {
            return Ok(());
        }
    }

    // Variables, the address and metadata.
    let vars = var_stack(&files, &chain, &request, context);
    let mut missing = Vec::new();
    let address = render(&vars, &request.url, &mut missing);
    let message_text = render(&vars, &request.grpc.message, &mut missing);
    let target = Target::parse(&address, request.grpc.tls)?;
    call.address = target.authority();
    call.tls = target.tls;
    let metadata = metadata::build(
        &headers,
        &effective.auth,
        &vars,
        &mut missing,
        Some((&executor.authenticator, &executor.http, transport.clone())),
    )
    .await;
    execution.missing_variables = missing;
    let metadata = metadata?;
    call.method = request.method.trim().trim_start_matches('/').to_owned();
    execution.request = Some(Outgoing {
        method: "GRPC".into(),
        url: target.url(&format!("/{}", call.method)),
        headers: metadata.clone(),
        body: if message_text.trim().is_empty() {
            OutgoingBody::None
        } else {
            OutgoingBody::Bytes {
                data: Payload(message_text.clone().into_bytes()),
            }
        },
    });

    // The schema, the method and the messages.
    let proto_schema = match request.grpc.proto.trim() {
        "" => None,
        proto => Some(compile(executor.workspace.root(), proto).await?),
    };
    let mut connection = connect(&target, &transport, &chain, &request).await?;
    let schema = match proto_schema {
        Some(schema) => schema,
        None => reflect(&target, &mut connection, &metadata, false).await?,
    };
    let method = schema.method(&request.method)?;
    let call_type = CallType::of(&method);
    call.method = format!("{}/{}", method.parent_service().full_name(), method.name());
    call.call_type = call_type;
    let mut initial = encode(&method.input(), &message_text)?;
    if !call_type.client_streams() {
        match initial.len() {
            0 => initial.push(encode_value(
                &method.input(),
                &Value::Object(serde_json::Map::new()),
            )?),
            1 => {}
            n => {
                return Err(Error::Invalid(format!(
                    "{} takes one message, but the request holds {n}",
                    call.method
                )));
            }
        }
    } else if control.commands.is_some() {
        initial.clear();
    }
    if let Some(outgoing) = &mut execution.request {
        outgoing.url = target.url(&format!("/{}", call.method));
    }

    // The call.
    let deadline = deadline(&chain, &request, call_type);
    drive(
        &mut connection,
        call,
        &metadata,
        deadline,
        initial,
        control,
        &method,
        &vars,
        schema.pool(),
    )
    .await?;

    // Post-response scripts, then checks and captures, on the call as a
    // response.
    let response = response_view(call, &target);
    if !effective.post_response.is_empty() {
        let input = ScriptInput {
            event: Event::PostResponse,
            info,
            scopes: script_scopes(&files, &chain, &request, context),
            request: script_request(&request, &headers),
            response: Some(ScriptResponse::from(&response)),
            cookies: Vec::new(),
        };
        let output = run_scripts(host, effective.post_response.clone(), input).await;
        absorb(output, context, execution);
    }
    execution.checks = run_checks(&request.checks, &response);
    let captured = run_captures(&request.captures, &response);
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
    if call.dropped_messages > 0 {
        execution.warnings.push(format!(
            "{} messages arrived after the first {MAX_KEPT_MESSAGES} and were not kept",
            call.dropped_messages
        ));
    }
    execution.response = Some(response);
    Ok(())
}

/// The schema a request names, for choosing a method. Reflected schemas
/// are kept until `refresh` asks for them again.
pub async fn load_schema(
    executor: &Executor,
    path: Option<&str>,
    request: &Request,
    context: &Context,
    refresh: bool,
) -> Result<Schema> {
    let proto = request.grpc.proto.trim();
    if !proto.is_empty() {
        return compile(executor.workspace.root(), proto).await;
    }
    let chain = folder_chain(&executor.workspace, path)?;
    let files = executor.file_vars(context.environment.as_deref())?;
    let effective = Effective::new(&chain, request);
    let transport = executor.transport(path, request, context)?;
    let vars = var_stack(&files, &chain, request, context);
    let mut missing = Vec::new();
    let target = Target::parse(&render(&vars, &request.url, &mut missing), request.grpc.tls)?;
    if !refresh && let Some(schema) = cached(&target) {
        return Ok(schema);
    }
    let metadata = metadata::build(
        &effective.headers,
        &effective.auth,
        &vars,
        &mut missing,
        Some((&executor.authenticator, &executor.http, transport.clone())),
    )
    .await?;
    let mut connection = connect(&target, &transport, &chain, request).await?;
    reflect(&target, &mut connection, &metadata, true).await
}

/// A request as a `grpcurl` command, run from the workspace root, and the
/// variables it uses that have no value. OAuth 2.0 tokens are left out,
/// as they need a round trip.
pub fn grpcurl(
    executor: &Executor,
    path: Option<&str>,
    request: &Request,
    context: &Context,
) -> Result<(String, Vec<String>)> {
    let chain = folder_chain(&executor.workspace, path)?;
    let files = executor.file_vars(context.environment.as_deref())?;
    let effective = Effective::new(&chain, request);
    let vars = var_stack(&files, &chain, request, context);
    let mut missing = Vec::new();
    let target = Target::parse(&render(&vars, &request.url, &mut missing), request.grpc.tls)?;
    let message = render(&vars, &request.grpc.message, &mut missing);
    let auth = match &effective.auth {
        crate::model::Auth::Oauth2(_) => crate::model::Auth::None,
        other => other.clone(),
    };
    let metadata = futures_now(metadata::build(
        &effective.headers,
        &auth,
        &vars,
        &mut missing,
        None,
    ))?;

    let mut parts = vec!["grpcurl".to_owned()];
    if !target.tls {
        parts.push("-plaintext".into());
    } else if !context
        .settings
        .clone()
        .or(effective.settings.clone())
        .verify_tls
        .unwrap_or(true)
    {
        parts.push("-insecure".into());
    }
    let proto = request.grpc.proto.trim().trim_end_matches('/');
    if !proto.is_empty() {
        let root = executor.workspace.root();
        let full = root.join(proto);
        if full.is_dir() {
            parts.push(format!("-import-path {}", sh(proto)));
            for name in super::schema::proto_files(&full) {
                parts.push(format!("-proto {}", sh(&name)));
            }
        } else {
            let (dir, name) = proto.rsplit_once('/').unwrap_or((".", proto));
            parts.push(format!("-import-path {}", sh(dir)));
            if dir != "." {
                parts.push("-import-path .".into());
            }
            parts.push(format!("-proto {}", sh(name)));
        }
    }
    for header in &metadata {
        parts.push(format!(
            "-H {}",
            sh(&format!("{}: {}", header.name, header.value))
        ));
    }
    let message = message.trim();
    if !message.is_empty() {
        parts.push(format!("-d {}", sh(message)));
    }
    let timeout = chain
        .iter()
        .map(|f| f.settings.clone())
        .chain(iter::once(request.settings.clone()))
        .fold(Settings::default(), |parent, level| level.or(parent))
        .timeout_ms;
    if let Some(ms) = timeout.filter(|ms| *ms > 0) {
        let mut seconds = format!("{:.3}", Duration::from_millis(ms).as_secs_f64());
        while seconds.ends_with('0') {
            seconds.pop();
        }
        if seconds.ends_with('.') {
            seconds.pop();
        }
        parts.push(format!("-max-time {seconds}"));
    }
    let mut last = sh(&target.authority());
    let method = request.method.trim().trim_start_matches('/');
    if !method.is_empty() {
        let _ = write!(last, " {}", sh(method));
    }
    parts.push(last);
    Ok((parts.join(" \\\n  "), missing))
}

/// Runs a future that never waits, such as building metadata without
/// fetching tokens.
fn futures_now<T>(future: impl std::future::Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    match future.as_mut().poll(&mut context) {
        std::task::Poll::Ready(value) => value,
        std::task::Poll::Pending => unreachable!("building metadata without tokens never waits"),
    }
}

/// Quotes a word for POSIX shells.
fn sh(text: &str) -> String {
    let plain = !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:@%+=,".contains(c));
    if plain {
        text.to_owned()
    } else {
        format!("'{}'", text.replace('\'', r"'\''"))
    }
}

fn folder_chain(workspace: &Workspace, path: Option<&str>) -> Result<Vec<Folder>> {
    match path {
        Some(path) => workspace.folder_chain(path),
        None => Ok(vec![workspace.read_folder("")?]),
    }
}

fn render(vars: &VarStack, text: &str, missing: &mut Vec<String>) -> String {
    let out = vars.interpolate(text);
    for name in out.missing {
        if !missing.contains(&name) {
            missing.push(name);
        }
    }
    out.text
}

/// The request as scripts see it: the message is the body.
fn script_request(request: &Request, headers: &[crate::model::KeyValue]) -> ScriptRequest {
    ScriptRequest {
        method: request.method.clone(),
        url: request.url.clone(),
        headers: headers.to_vec(),
        body: Some(ScriptBody {
            mode: "raw".into(),
            raw: Some(request.grpc.message.clone()),
        }),
    }
}

async fn compile(root: &Path, proto: &str) -> Result<Schema> {
    let (root, proto) = (root.to_path_buf(), proto.to_owned());
    tokio::task::spawn_blocking(move || Schema::from_protos(&root, &proto))
        .await
        .map_err(|e| Error::Invalid(format!("Could not read the .proto files: {e}")))?
}

fn cache_key(target: &Target) -> String {
    format!("{}|{}", target.tls, target.authority())
}

fn cached(target: &Target) -> Option<Schema> {
    reflected()
        .lock()
        .expect("schema cache lock")
        .get(&cache_key(target))
        .cloned()
}

async fn reflect(
    target: &Target,
    connection: &mut Connection,
    metadata: &[Header],
    refresh: bool,
) -> Result<Schema> {
    if !refresh && let Some(schema) = cached(target) {
        return Ok(schema);
    }
    let pool = reflection::reflect(connection, metadata, REFLECTION_TIMEOUT).await?;
    let schema = Schema::from_pool(pool);
    reflected()
        .lock()
        .expect("schema cache lock")
        .insert(cache_key(target), schema.clone());
    Ok(schema)
}

async fn connect(
    target: &Target,
    transport: &Transport,
    chain: &[Folder],
    request: &Request,
) -> Result<Connection> {
    let timeout = deadline(chain, request, CallType::Unary)
        .map_or(CONNECT_TIMEOUT, |d| d.min(CONNECT_TIMEOUT));
    transport::connect(target, transport, timeout).await
}

/// The deadline: the timeout set on the request or its folders, `None`
/// for 0. Unary calls without a timeout get the default; streams get none,
/// as they may last.
fn deadline(chain: &[Folder], request: &Request, call_type: CallType) -> Option<Duration> {
    let settings = chain
        .iter()
        .map(|f| f.settings.clone())
        .chain(iter::once(request.settings.clone()))
        .fold(Settings::default(), |parent, level| level.or(parent));
    match settings.timeout_ms {
        Some(0) => None,
        Some(ms) => Some(Duration::from_millis(ms)),
        None if call_type == CallType::Unary => Some(DEFAULT_DEADLINE),
        None => None,
    }
}

fn encode(input: &MessageDescriptor, text: &str) -> Result<Vec<Encoded>> {
    split_messages(text)?
        .iter()
        .map(|value| encode_value(input, value))
        .collect()
}

fn encode_value(input: &MessageDescriptor, value: &Value) -> Result<Encoded> {
    let message = from_json(input, value)?;
    Ok(Encoded {
        json: to_json(&message).unwrap_or_else(|_| value.clone()),
        bytes: message.encode_to_vec(),
    })
}

/// Collects what happens on a call, and passes it on to a live caller.
struct Recorder<'a> {
    call: &'a mut GrpcCall,
    started: Instant,
    events: Option<mpsc::UnboundedSender<LiveEvent>>,
    output: MessageDescriptor,
}

impl Recorder<'_> {
    fn elapsed(&self) -> f64 {
        millis(self.started.elapsed())
    }

    fn emit(&self, event: LiveEvent) {
        if let Some(events) = &self.events {
            let _ = events.send(event);
        }
    }

    fn sent(&mut self, message: Encoded) {
        self.push(StreamMessage {
            direction: Direction::Sent,
            message: message.json,
            time_ms: self.elapsed(),
            size: message.bytes.len() as u64,
            error: None,
        });
    }

    fn received(&mut self, bytes: &[u8]) {
        let (message, error) = match DynamicMessage::decode(self.output.clone(), bytes) {
            Ok(message) => match to_json(&message) {
                Ok(json) => (json, None),
                Err(e) => (Value::Null, Some(format!("could not show it as JSON: {e}"))),
            },
            Err(e) => (
                Value::Null,
                Some(format!(
                    "it is not a valid {}: {e}",
                    self.output.full_name()
                )),
            ),
        };
        self.push(StreamMessage {
            direction: Direction::Received,
            message,
            time_ms: self.elapsed(),
            size: bytes.len() as u64,
            error,
        });
    }

    fn push(&mut self, message: StreamMessage) {
        self.emit(LiveEvent::Message {
            message: message.clone(),
        });
        if self.call.messages.len() < MAX_KEPT_MESSAGES {
            self.call.messages.push(message);
        } else {
            self.call.dropped_messages += 1;
        }
    }

    fn headers(&mut self, head: Option<&Head>) {
        if self.call.headers_ms.is_some() {
            return;
        }
        let Some(head) = head else {
            return;
        };
        let time_ms = self.elapsed();
        self.call.headers = header_list(&head.headers);
        self.call.headers_ms = Some(time_ms);
        self.emit(LiveEvent::Headers {
            headers: self.call.headers.clone(),
            time_ms,
        });
    }
}

/// Makes the call: sends the first messages, then reads replies while
/// following live commands, until the server ends the call, the deadline
/// passes or the call is cancelled.
#[allow(clippy::too_many_arguments)]
async fn drive(
    connection: &mut Connection,
    call: &mut GrpcCall,
    metadata: &[Header],
    deadline: Option<Duration>,
    initial: Vec<Encoded>,
    control: Control,
    method: &MethodDescriptor,
    vars: &VarStack,
    pool: &DescriptorPool,
) -> Result<()> {
    let path = format!("/{}", call.method);
    let Control {
        mut commands,
        events,
    } = control;
    let started = Instant::now();
    let mut stream = connection.start(&path, metadata, deadline).await?;
    let mut recorder = Recorder {
        call,
        started,
        events,
        output: method.output(),
    };
    recorder.emit(LiveEvent::Started {
        method: recorder.call.method.clone(),
        call_type: recorder.call.call_type,
        address: recorder.call.address.clone(),
    });
    for message in initial {
        stream.send(&message.bytes)?;
        recorder.sent(message);
    }
    if commands.is_none() || !recorder.call.call_type.client_streams() {
        stream.end();
    }

    let mut expiry = std::pin::pin!(async move {
        match deadline {
            Some(after) => tokio::time::sleep(after).await,
            None => std::future::pending().await,
        }
    });
    let status = loop {
        tokio::select! {
            biased;
            () = &mut expiry => {
                break Status::new(
                    Code::DeadlineExceeded,
                    format!("the deadline of {} passed", seconds(deadline.unwrap_or_default())),
                );
            }
            command = next_command(&mut commands) => match command {
                Some(Command::Send { text, reply }) => {
                    let outcome = send_text(&stream, &mut recorder, vars, &method.input(), &text);
                    let _ = reply.send(outcome.map_err(|e| e.to_string()));
                }
                Some(Command::End) => stream.end(),
                Some(Command::Cancel) | None => {
                    break Status::new(Code::Cancelled, "the call was cancelled");
                }
            },
            next = stream.next() => {
                recorder.headers(stream.head());
                match next {
                    Ok(Some(bytes)) => recorder.received(&bytes),
                    Ok(None) => break stream.status(),
                    Err(e) if stream.head().is_none() => return Err(e),
                    Err(e) => break Status::new(Code::Internal, e.to_string()),
                }
            }
        }
    };
    recorder.headers(stream.head());
    let mut status = status;
    status.decode_details(Some(pool));
    let call = recorder.call;
    call.trailers = stream.trailers().map(header_list).unwrap_or_default();
    call.status = Some(status);
    call.time_ms = millis(started.elapsed());
    Ok(())
}

/// The next command, or never when there are no commands.
async fn next_command(commands: &mut Option<mpsc::UnboundedReceiver<Command>>) -> Option<Command> {
    match commands {
        Some(receiver) => receiver.recv().await,
        None => std::future::pending().await,
    }
}

fn send_text(
    stream: &transport::Call,
    recorder: &mut Recorder<'_>,
    vars: &VarStack,
    input: &MessageDescriptor,
    text: &str,
) -> Result<usize> {
    if !stream.sending() {
        return Err(Error::Invalid(
            "the stream was ended, so no more messages can be sent".into(),
        ));
    }
    let messages = encode(input, &vars.render(text))?;
    if messages.is_empty() {
        return Err(Error::Invalid("write a message to send".into()));
    }
    let count = messages.len();
    for message in messages {
        stream.send(&message.bytes)?;
        recorder.sent(message);
    }
    Ok(count)
}

/// The call as an HTTP response, for scripts, checks and captures: the
/// status code is the gRPC code, the headers are followed by the trailers,
/// and the body is the response message, or an array of messages when the
/// server streams.
fn response_view(call: &GrpcCall, target: &Target) -> Response {
    let status = call
        .status
        .clone()
        .unwrap_or_else(|| Status::new(Code::Unknown, "the call did not end"));
    let received: Vec<&StreamMessage> = call
        .messages
        .iter()
        .filter(|m| m.direction == Direction::Received)
        .collect();
    let body = if call.call_type.server_streams() {
        let all: Vec<&Value> = received.iter().map(|m| &m.message).collect();
        serde_json::to_string_pretty(&all).unwrap_or_default()
    } else {
        received
            .first()
            .and_then(|m| serde_json::to_string_pretty(&m.message).ok())
            .unwrap_or_default()
    };
    let headers: Vec<Header> = call.headers.iter().chain(&call.trailers).cloned().collect();
    let header_bytes = headers
        .iter()
        .map(|h| h.name.len() + h.value.len() + 4)
        .sum::<usize>();
    Response {
        status: u16::try_from(status.code).unwrap_or(u16::MAX),
        status_text: status.name,
        http_version: "HTTP/2".into(),
        url: target.url(&format!("/{}", call.method)),
        headers,
        body: Payload(body.into_bytes()),
        truncated: call.dropped_messages > 0,
        time_ms: call.time_ms,
        headers_ms: call.headers_ms.unwrap_or(call.time_ms),
        size: Size {
            headers: header_bytes as u64,
            body: received.iter().map(|m| m.size).sum(),
        },
        remote_addr: None,
        timings: None,
        tls: None,
        http3_available: false,
        stream_open: false,
    }
}

fn millis(duration: Duration) -> f64 {
    (duration.as_secs_f64() * 1000.0 * 100.0).round() / 100.0
}

fn seconds(duration: Duration) -> String {
    let ms = duration.as_millis();
    if ms < 1000 {
        format!("{ms} ms")
    } else {
        format!("{:.1} s", duration.as_secs_f64())
    }
}
