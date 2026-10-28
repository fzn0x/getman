//! Running pre-request and post-response scripts.
//!
//! Scripts are JavaScript, run by an embedded `QuickJS` engine with the `gm`
//! API (`pm` works too, so scripts written for other tools keep working).
//! Each request gets a fresh engine: a snapshot of the request, the
//! response and the variables goes in as JSON, and what the scripts did
//! comes back the same way: test results, logs, variable changes and edits
//! to the request.
//!
//! Scripts can also use the `bru`, `req` and `res` globals of scripts
//! written for Bruno, and `require` the `.js` files of the workspace.
//!
//! Scripts are stopped after [`DEFAULT_TIMEOUT`] and may use 256 MiB of
//! memory.

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use hmac::digest::KeyInit;
use hmac::{Hmac, Mac};
use rquickjs::context::EvalOptions;
use rquickjs::promise::PromiseState;
use rquickjs::{
    CatchResultExt, CaughtError, Context, Ctx, Exception, Function, Object, Promise, Runtime,
};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use sha2::Digest as _;

use crate::checks::TestResult;
use crate::http::{CookieInfo, Header, HttpClient, Outgoing, Response, Transport};
use crate::model::{FormField, KeyValue, Vars};
use crate::prepare::Script;

const PRELUDE: &str = include_str!("prelude.js");

/// How long one script may run before it is stopped.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
const MEMORY_LIMIT: usize = 256 * 1024 * 1024;
const STACK_LIMIT: usize = 1024 * 1024;
const MAX_TICKS: usize = 100_000;
/// The largest module a script may `require`.
const MODULE_LIMIT: u64 = 256 * 1024;

/// When a script runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    /// Before the request is sent.
    PreRequest,
    /// After the response arrives.
    PostResponse,
}

/// About the request whose scripts run.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ScriptInfo {
    /// Display name of the request.
    pub request_name: String,
    /// Path of the request in the workspace.
    pub request_path: String,
    /// Iteration of the run, from 0.
    pub iteration: usize,
    /// Number of iterations in the run.
    pub iteration_count: usize,
}

/// Variables as scripts see them, by scope.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ScriptScopes {
    /// Global variables.
    pub globals: Vars,
    /// Workspace variables.
    pub collection: Vars,
    /// The selected environment's variables.
    pub environment: Vars,
    /// The selected environment's name.
    pub environment_name: Option<String>,
    /// Variables of the enclosing folders, merged.
    pub folder: Vars,
    /// The request's variables.
    pub request: Vars,
    /// The current row of a data file, with its JSON types.
    pub data: serde_json::Map<String, Value>,
    /// Variables set by scripts for the whole session.
    pub session: Vars,
    /// Variables set by scripts for the rest of the run.
    pub local: Vars,
}

/// The request as scripts see it, before variables are replaced.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ScriptRequest {
    /// HTTP method.
    pub method: String,
    /// URL, with `{{variables}}` still in place.
    pub url: String,
    /// Headers, including disabled ones.
    pub headers: Vec<KeyValue>,
    /// The body, for text bodies.
    pub body: Option<ScriptBody>,
}

/// A request body as scripts see it.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ScriptBody {
    /// `raw` for text bodies, else the body type.
    pub mode: String,
    /// The text, for text bodies.
    pub raw: Option<String>,
    /// The fields of a URL-encoded form.
    pub urlencoded: Option<Vec<KeyValue>>,
    /// The fields of a multipart form.
    pub formdata: Option<Vec<FormField>>,
}

/// The response as scripts see it.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ScriptResponse {
    /// Status code.
    pub code: u16,
    /// Reason phrase.
    pub status: String,
    /// Headers.
    pub headers: Vec<Header>,
    /// Body as text.
    pub body: String,
    /// Milliseconds the request took.
    pub response_time: f64,
    /// Body size in bytes.
    pub response_size: u64,
}

impl From<&Response> for ScriptResponse {
    fn from(response: &Response) -> Self {
        Self {
            code: response.status,
            status: response.status_text.clone(),
            headers: response.headers.clone(),
            body: response.text().into_owned(),
            response_time: response.time_ms,
            response_size: response.size.body,
        }
    }
}

/// A message received on a connection that stays open, as scripts see it
/// in `gm.messages`.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ScriptMessage {
    /// `received`.
    pub direction: String,
    /// The text, or Base64 for a binary message.
    pub data: String,
    /// Milliseconds after the connection opened.
    pub time: f64,
    /// Size in bytes.
    pub size: u64,
    /// Whether `data` is Base64.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub binary: bool,
    /// Event type of a Server-Sent Event, or name of a Socket.IO event.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
    /// Id of a Server-Sent Event.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Acknowledgement id of a Socket.IO event or acknowledgement.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ack: Option<u64>,
}

/// Everything scripts can see.
#[derive(Debug, Clone, Serialize)]
pub struct ScriptInput {
    /// Which scripts run.
    pub event: Event,
    /// About the request.
    pub info: ScriptInfo,
    /// Variables by scope.
    pub scopes: ScriptScopes,
    /// The request.
    pub request: ScriptRequest,
    /// The response, after it arrived. For a connection that stays open,
    /// the handshake.
    pub response: Option<ScriptResponse>,
    /// The messages received on a connection that stayed open, once it
    /// ended; `None` for other requests.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub messages: Option<Vec<ScriptMessage>>,
    /// Cookies the client holds for the request's host.
    pub cookies: Vec<CookieInfo>,
}

/// A line a script logged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntry {
    /// `info`, `warn`, `error` or `debug`.
    pub level: String,
    /// The message.
    pub message: String,
    /// The script that logged it.
    pub origin: String,
}

/// The scope a script changed a variable in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeScope {
    /// `gm.environment`.
    Environment,
    /// `gm.collectionVariables`.
    Collection,
    /// `gm.globals`.
    Global,
    /// `gm.session`: while the UI server runs, or for a whole run.
    Session,
    /// `gm.variables`: for the rest of the run only.
    Local,
}

/// A variable a script set or removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VarChange {
    /// Where the variable lives.
    pub scope: ChangeScope,
    /// Its name.
    pub name: String,
    /// The new value, or `None` when removed.
    pub value: Option<String>,
}

/// The request after pre-request scripts edited it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct EditedRequest {
    /// HTTP method.
    pub method: String,
    /// URL.
    pub url: String,
    /// Headers.
    pub headers: Vec<KeyValue>,
    /// The body text, for text bodies.
    pub body: Option<String>,
    /// The fields of a URL-encoded form, if the body is one.
    #[serde(default)]
    pub urlencoded: Option<Vec<KeyValue>>,
    /// The fields of a multipart form, if the body is one.
    #[serde(default)]
    pub formdata: Option<Vec<FormField>>,
}

/// A script that failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptError {
    /// The script, such as `request` or `folder "Users"`.
    pub origin: String,
    /// What went wrong.
    pub message: String,
}

/// What the scripts did.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ScriptOutput {
    /// Results of `gm.test`.
    pub tests: Vec<TestResult>,
    /// Console output.
    pub logs: Vec<LogEntry>,
    /// Variables set or removed, in order.
    pub changes: Vec<VarChange>,
    /// The request as edited, after pre-request scripts.
    pub request: Option<EditedRequest>,
    /// Where a run goes next, if a script said.
    #[serde(deserialize_with = "next_request")]
    pub next_request: Option<NextRequest>,
    /// Whether a pre-request script skipped the request.
    pub skip_request: bool,
    /// Whether a script asked for the run to stop after this request.
    pub stop_run: bool,
    /// The first script that failed; later scripts did not run.
    #[serde(skip)]
    pub error: Option<ScriptError>,
}

/// Where a run goes after a request, as set by
/// `gm.execution.setNextRequest`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "name", rename_all = "snake_case")]
pub enum NextRequest {
    /// End the run.
    Stop,
    /// Continue with the request of this name.
    Jump(String),
}

/// Reads `null` as [`NextRequest::Stop`]; a missing field stays `None`.
fn next_request<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<NextRequest>, D::Error> {
    Ok(Some(
        Option::<String>::deserialize(deserializer)?.map_or(NextRequest::Stop, NextRequest::Jump),
    ))
}

/// What scripts may reach outside the engine.
#[derive(Debug, Clone)]
pub struct ScriptHost {
    /// Client and runtime for `gm.sendRequest`, which is unavailable
    /// without them.
    pub http: Option<(Arc<HttpClient>, tokio::runtime::Handle)>,
    /// Transport settings for `gm.sendRequest`.
    pub transport: Transport,
    /// How long one script may run.
    pub timeout: Duration,
    /// The workspace's folder, where `require` finds modules. Without it,
    /// scripts cannot require files.
    pub root: Option<PathBuf>,
}

impl Default for ScriptHost {
    fn default() -> Self {
        Self {
            http: None,
            transport: Transport::default(),
            timeout: DEFAULT_TIMEOUT,
            root: None,
        }
    }
}

/// Runs scripts in order, stopping at the first that fails. Blocks; call it
/// from a thread that may block, such as one from
/// `tokio::task::spawn_blocking`.
pub fn run(host: &ScriptHost, scripts: &[Script], input: &ScriptInput) -> ScriptOutput {
    if scripts.iter().all(|s| s.code.trim().is_empty()) {
        return ScriptOutput::default();
    }
    match run_engine(host, scripts, input) {
        Ok(output) => output,
        Err(message) => ScriptOutput {
            error: Some(ScriptError {
                origin: "getman".into(),
                message,
            }),
            ..ScriptOutput::default()
        },
    }
}

fn run_engine(
    host: &ScriptHost,
    scripts: &[Script],
    input: &ScriptInput,
) -> Result<ScriptOutput, String> {
    let runtime = Runtime::new().map_err(|e| e.to_string())?;
    runtime.set_memory_limit(MEMORY_LIMIT);
    runtime.set_max_stack_size(STACK_LIMIT);
    let deadline = Rc::new(Cell::new(Instant::now() + host.timeout));
    {
        let deadline = Rc::clone(&deadline);
        runtime.set_interrupt_handler(Some(Box::new(move || Instant::now() > deadline.get())));
    }
    let context = Context::full(&runtime).map_err(|e| e.to_string())?;
    let input_json = serde_json::to_string(input).map_err(|e| e.to_string())?;

    context.with(|ctx| {
        let fail = |e: rquickjs::Error| e.to_string();
        let globals = ctx.globals();
        globals.set("__gm_input", input_json).map_err(fail)?;
        globals
            .set("__gm_native", natives(&ctx, host, &deadline).map_err(fail)?)
            .map_err(fail)?;
        if let Err(caught) = ctx.eval::<(), _>(PRELUDE).catch(&ctx) {
            return Err(format!(
                "the script runtime failed to start: {}",
                describe(&ctx, caught, "prelude", host)
            ));
        }
        let begin: Function<'_> = globals.get("__gm_begin").map_err(fail)?;
        let tick: Function<'_> = globals.get("__gm_tick").map_err(fail)?;
        let finish: Function<'_> = globals.get("__gm_finish").map_err(fail)?;

        let mut error = None;
        for script in scripts.iter().filter(|s| !s.code.trim().is_empty()) {
            deadline.set(Instant::now() + host.timeout);
            begin
                .call::<_, ()>((script.origin.clone(),))
                .map_err(fail)?;
            // The wrapper starts on the script's first line, so line numbers
            // in errors match the editor.
            let source = format!("(async () => {{{}\n}})()", script.code);
            let mut options = EvalOptions::default();
            options.strict = false;
            options.filename = Some(script.origin.clone());
            let outcome = match ctx
                .eval_with_options::<Promise<'_>, _>(source, options)
                .catch(&ctx)
            {
                Ok(promise) => settle(&ctx, &promise, &tick, &script.origin, host),
                Err(caught) => Err(describe(&ctx, caught, &script.origin, host)),
            };
            if let Err(message) = outcome {
                error = Some(ScriptError {
                    origin: script.origin.clone(),
                    message,
                });
                break;
            }
        }

        deadline.set(Instant::now() + host.timeout);
        let json: String = finish
            .call(())
            .catch(&ctx)
            .map_err(|caught| describe(&ctx, caught, "getman", host))?;
        let mut output: ScriptOutput = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        output.error = error;
        Ok(output)
    })
}

/// Runs queued promise jobs and timers until the script's promise settles
/// and nothing is left to do.
fn settle<'js>(
    ctx: &Ctx<'js>,
    promise: &Promise<'js>,
    tick: &Function<'js>,
    origin: &str,
    host: &ScriptHost,
) -> Result<(), String> {
    for _ in 0..MAX_TICKS {
        while ctx.execute_pending_job() {}
        match tick.call::<_, bool>(()).catch(ctx) {
            Ok(true) => {}
            Ok(false) => break,
            Err(caught) => return Err(describe(ctx, caught, origin, host)),
        }
    }
    match promise.state() {
        PromiseState::Resolved => Ok(()),
        PromiseState::Rejected => match promise.result::<rquickjs::Value<'_>>().map(|r| r.catch(ctx)) {
            Some(Err(caught)) => Err(describe(ctx, caught, origin, host)),
            _ => Err("the script failed".into()),
        },
        PromiseState::Pending => Err(
            "the script never finished: it awaits something that never settles, or a timer that keeps firing"
                .into(),
        ),
    }
}

/// One line describing a JavaScript error, with its line number.
fn describe<'js>(
    ctx: &Ctx<'js>,
    caught: CaughtError<'js>,
    origin: &str,
    host: &ScriptHost,
) -> String {
    match caught {
        CaughtError::Exception(exception) => {
            let message = exception.message().unwrap_or_default();
            if message == "interrupted" {
                return format!(
                    "the script ran longer than {} s and was stopped",
                    host.timeout.as_secs_f64()
                );
            }
            let name: String = exception
                .as_object()
                .get("name")
                .unwrap_or_else(|_| "Error".to_owned());
            let line = exception.stack().and_then(|stack| line_in(&stack, origin));
            match line {
                Some(line) => format!("{name}: {message} (line {line})"),
                None => format!("{name}: {message}"),
            }
        }
        CaughtError::Value(value) => {
            let text = ctx
                .globals()
                .get::<_, Function<'_>>("__gm_describe")
                .and_then(|describe| describe.call::<_, String>((value,)))
                .unwrap_or_else(|_| "a value that is not an Error was thrown".into());
            format!("Uncaught {text}")
        }
        CaughtError::Error(error) => error.to_string(),
    }
}

/// The line of the first stack frame in the given script.
fn line_in(stack: &str, origin: &str) -> Option<u32> {
    let marker = format!("{origin}:");
    let at = stack.find(&marker)? + marker.len();
    stack[at..]
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()
}

/// Functions the prelude calls into Rust for.
fn natives<'js>(
    ctx: &Ctx<'js>,
    host: &ScriptHost,
    deadline: &Rc<Cell<Instant>>,
) -> rquickjs::Result<Object<'js>> {
    let native = Object::new(ctx.clone())?;
    native.set(
        "digest",
        Function::new(
            ctx.clone(),
            |ctx: Ctx<'_>, algorithm: String, data: String| {
                let data =
                    hex::decode(data).map_err(|_| Exception::throw_type(&ctx, "invalid data"))?;
                digest(&algorithm, &data)
                    .ok_or_else(|| Exception::throw_type(&ctx, "unknown algorithm"))
            },
        )?,
    )?;
    native.set(
        "hmac",
        Function::new(
            ctx.clone(),
            |ctx: Ctx<'_>, algorithm: String, key: String, data: String| {
                let key =
                    hex::decode(key).map_err(|_| Exception::throw_type(&ctx, "invalid key"))?;
                let data =
                    hex::decode(data).map_err(|_| Exception::throw_type(&ctx, "invalid data"))?;
                hmac(&algorithm, &key, &data)
                    .ok_or_else(|| Exception::throw_type(&ctx, "unknown algorithm"))
            },
        )?,
    )?;
    native.set("uuid", Function::new(ctx.clone(), crate::dynamic::uuid_v4)?)?;
    native.set(
        "dynamic",
        Function::new(ctx.clone(), |name: String| crate::dynamic::generate(&name))?,
    )?;
    native.set(
        "env",
        Function::new(ctx.clone(), |name: String| {
            crate::vars::read_process_env(&name)
        })?,
    )?;
    let root = host.root.clone();
    native.set(
        "module",
        Function::new(ctx.clone(), move |path: String| -> String {
            match read_module(root.as_deref(), &path) {
                Ok(source) => serde_json::json!({ "source": source }),
                Err(error) => serde_json::json!({ "error": error }),
            }
            .to_string()
        })?,
    )?;
    let deadline = Rc::clone(deadline);
    native.set(
        "sleep",
        // Never sleeps past the script's time limit: the script is stopped
        // as if it had been interrupted there.
        Function::new(ctx.clone(), move |ctx: Ctx<'_>, ms: f64| {
            let wanted = Duration::try_from_secs_f64(ms.clamp(0.0, 3_600_000.0) / 1000.0)
                .unwrap_or_default();
            let left = deadline.get().saturating_duration_since(Instant::now());
            std::thread::sleep(wanted.min(left));
            if wanted > left {
                return Err(Exception::throw_message(&ctx, "interrupted"));
            }
            Ok(())
        })?,
    )?;
    let http = host.http.clone();
    let transport = host.transport.clone();
    native.set(
        "send",
        Function::new(ctx.clone(), move |json: String| -> String {
            let reply = match (&http, serde_json::from_str::<Outgoing>(&json)) {
                (None, _) => serde_json::json!({ "error": "gm.sendRequest is not available here" }),
                (_, Err(e)) => {
                    serde_json::json!({ "error": format!("the request is not valid: {e}") })
                }
                (Some((client, handle)), Ok(outgoing)) => {
                    match handle.block_on(client.send(&outgoing, transport.clone())) {
                        Ok(response) => {
                            serde_json::json!({ "response": ScriptResponse::from(&response) })
                        }
                        Err(e) => serde_json::json!({ "error": e.to_string() }),
                    }
                }
            };
            reply.to_string()
        })?,
    )?;
    Ok(native)
}

/// The source of a `.js` file inside the workspace.
fn read_module(root: Option<&std::path::Path>, path: &str) -> Result<String, String> {
    let Some(root) = root else {
        return Err("files cannot be required here".into());
    };
    if std::path::Path::new(path).extension() != Some(std::ffi::OsStr::new("js")) {
        return Err("only .js files can be required".into());
    }
    let file = crate::prepare::resolve_file(root, path).map_err(|e| e.to_string())?;
    let size = std::fs::metadata(&file).map_err(|e| e.to_string())?.len();
    if size > MODULE_LIMIT {
        return Err(format!(
            "{path} is larger than {} KiB, the most a module may be",
            MODULE_LIMIT / 1024
        ));
    }
    std::fs::read_to_string(&file).map_err(|e| format!("{path} cannot be read: {e}"))
}

fn digest(algorithm: &str, data: &[u8]) -> Option<String> {
    Some(match algorithm {
        "md5" => hex::encode(md5::Md5::digest(data)),
        "sha1" => hex::encode(sha1::Sha1::digest(data)),
        "sha256" => hex::encode(sha2::Sha256::digest(data)),
        "sha512" => hex::encode(sha2::Sha512::digest(data)),
        _ => return None,
    })
}

fn hmac(algorithm: &str, key: &[u8], data: &[u8]) -> Option<String> {
    fn sign<M: Mac + KeyInit>(key: &[u8], data: &[u8]) -> String {
        let mut mac = <M as KeyInit>::new_from_slice(key).expect("HMAC accepts any key length");
        mac.update(data);
        hex::encode(mac.finalize().into_bytes())
    }
    Some(match algorithm {
        "md5" => sign::<Hmac<md5::Md5>>(key, data),
        "sha1" => sign::<Hmac<sha1::Sha1>>(key, data),
        "sha256" => sign::<Hmac<sha2::Sha256>>(key, data),
        "sha512" => sign::<Hmac<sha2::Sha512>>(key, data),
        _ => return None,
    })
}

#[cfg(test)]
mod tests;
