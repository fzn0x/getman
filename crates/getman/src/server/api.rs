//! The JSON API the browser UI uses.

use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use axum::Json;
use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use getman_core::execute::{Cancel, Context, persist_changes, resolved_variables_with};
use getman_core::export::{codegen, openapi, postman};
use getman_core::http::{CookieInfo, Payload};
use getman_core::import::{self, SourceFile, curl};
use getman_core::model::{Auth, Example, Folder, OAuth2, Request};
use getman_core::runner::{self, RunEvent, RunOptions};
use getman_core::script::{ChangeScope, VarChange};
use getman_core::workspace::{EnvironmentFiles, LocalState};
use getman_core::{Error, Scope};
use serde::Deserialize;
use serde_json::{Value, json};

use super::AppState;
use super::presets::{self, Preset};
use crate::report::{About, Format};
use crate::runs::{self, StoredRun};

type Shared = State<Arc<AppState>>;

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/workspace", get(workspace))
        .route("/tree", get(tree))
        .route("/request/read", post(read_request))
        .route("/request/write", post(write_request))
        .route("/request/create", post(create_request))
        .route("/folder/read", post(read_folder))
        .route("/folder/write", post(write_folder))
        .route("/folder/create", post(create_folder))
        .route("/item/rename", post(rename))
        .route("/item/move", post(move_item))
        .route("/item/duplicate", post(duplicate))
        .route("/item/delete", post(delete))
        .route("/item/order", post(order))
        .route("/trash", get(trash))
        .route("/trash/restore", post(restore))
        .route("/trash/empty", post(empty_trash))
        .route("/environments", get(environments))
        .route("/environment/create", post(create_environment))
        .route("/environment/read", post(read_environment))
        .route("/environment/write", post(write_environment))
        .route("/environment/rename", post(rename_environment))
        .route("/environment/delete", post(delete_environment))
        .route("/environment/duplicate", post(duplicate_environment))
        .route("/globals", get(read_globals).put(write_globals))
        .route("/send", post(send))
        .route("/send/cancel", post(cancel_send))
        .route("/prepare", post(prepare))
        .route("/codegen", post(code))
        .route("/languages", get(languages))
        .route("/export", post(export))
        .route("/variables", post(variables))
        .route("/variables/set", post(set_variable))
        .route("/session/clear", post(clear_session))
        .route("/dynamic", get(dynamic))
        .route("/run", post(run))
        .route("/run/cancel", post(cancel_run))
        .route("/runs", get(list_runs))
        .route("/runs/read", post(read_run))
        .route("/runs/report", post(run_report))
        .route("/runs/delete", post(delete_run))
        .route(
            "/import",
            post(import).layer(DefaultBodyLimit::max(IMPORT_LIMIT)),
        )
        .route(
            "/import/files",
            post(import_files).layer(DefaultBodyLimit::max(IMPORT_LIMIT)),
        )
        .route("/parse-curl", post(parse_curl))
        .route("/cookies", get(cookies))
        .route("/cookies/set", post(set_cookie))
        .route("/cookies/delete", post(delete_cookie))
        .route("/cookies/clear", post(clear_cookies))
        .route("/history", get(history))
        .route("/history/read", post(read_history))
        .route("/history/clear", post(clear_history))
        .route("/oauth2/token", post(oauth2_token))
        .route("/examples", post(list_examples))
        .route("/example/save", post(save_example))
        .route("/example/body", post(example_body))
        .route("/example/rename", post(rename_example))
        .route("/example/delete", post(delete_example))
        .route("/mocks", get(list_mocks))
        .route("/mock/start", post(start_mock))
        .route("/mock/stop", post(stop_mock))
        .route(
            "/header-presets",
            get(read_header_presets).put(write_header_presets),
        )
        .route("/oauth2/authorize", post(oauth2_authorize))
        .route("/oauth2/wait", post(oauth2_wait))
        .route("/oauth2/cancel", post(oauth2_cancel))
        .route("/oauth2/status", post(oauth2_status))
        .route("/oauth2/clear", post(oauth2_clear))
}

/// An error answered as `{"error": "..."}` with a fitting status.
pub(super) struct ApiError(pub(super) StatusCode, pub(super) String);

impl From<Error> for ApiError {
    fn from(error: Error) -> Self {
        let status = match &error {
            Error::NotFound(_) => StatusCode::NOT_FOUND,
            Error::AlreadyExists(_) => StatusCode::CONFLICT,
            Error::InvalidPath(_) | Error::Invalid(_) | Error::Yaml { .. } => {
                StatusCode::BAD_REQUEST
            }
            Error::Http(_) => StatusCode::BAD_GATEWAY,
            Error::Io { .. } | Error::Script(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        Self(status, error.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

fn bad(message: impl Into<String>) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, message.into())
}

pub(super) type ApiResult<T = Json<Value>> = Result<T, ApiError>;

fn done() -> Json<Value> {
    Json(json!({}))
}

// ---- Workspace and tree -----------------------------------------------------

async fn workspace(State(state): Shared) -> ApiResult {
    let ws = state.workspace();
    Ok(Json(json!({
        "name": ws.name(),
        "root": ws.root().display().to_string(),
        "version": env!("CARGO_PKG_VERSION"),
        "tree": ws.tree()?,
        "environments": ws.environments()?,
    })))
}

async fn tree(State(state): Shared) -> ApiResult {
    Ok(Json(json!(state.workspace().tree()?)))
}

#[derive(Deserialize)]
struct PathArg {
    #[serde(default)]
    path: String,
}

#[derive(Deserialize)]
struct WriteRequest {
    path: String,
    request: Request,
}

#[derive(Deserialize)]
struct CreateRequest {
    #[serde(default)]
    folder: String,
    name: String,
    #[serde(default)]
    request: Option<Request>,
}

#[derive(Deserialize)]
struct WriteFolder {
    #[serde(default)]
    path: String,
    folder: Folder,
}

#[derive(Deserialize)]
struct CreateFolder {
    #[serde(default)]
    parent: String,
    name: String,
}

#[derive(Deserialize)]
struct Rename {
    #[serde(default)]
    path: String,
    name: String,
}

#[derive(Deserialize)]
struct Move {
    path: String,
    #[serde(default)]
    to: String,
}

#[derive(Deserialize)]
struct Order {
    #[serde(default)]
    folder: String,
    order: Vec<String>,
}

async fn read_request(State(state): Shared, Json(arg): Json<PathArg>) -> ApiResult {
    Ok(Json(json!(state.workspace().read_request(&arg.path)?)))
}

async fn write_request(State(state): Shared, Json(arg): Json<WriteRequest>) -> ApiResult {
    let mut request = arg.request;
    // Examples change through their own calls; the editor does not hold
    // them, so keep the ones on disk.
    if let Ok(saved) = state.workspace().read_request(&arg.path) {
        request.examples = saved.examples;
    }
    state.workspace().write_request(&arg.path, &request)?;
    Ok(done())
}

async fn create_request(State(state): Shared, Json(arg): Json<CreateRequest>) -> ApiResult {
    let name = arg.name.trim();
    if name.is_empty() {
        return Err(bad("a request needs a name"));
    }
    let request = arg.request.unwrap_or_default();
    let path = state
        .workspace()
        .create_request(&arg.folder, name, &request)?;
    Ok(Json(json!({ "path": path })))
}

async fn read_folder(State(state): Shared, Json(arg): Json<PathArg>) -> ApiResult {
    Ok(Json(json!(state.workspace().read_folder(&arg.path)?)))
}

async fn write_folder(State(state): Shared, Json(arg): Json<WriteFolder>) -> ApiResult {
    state.workspace().write_folder(&arg.path, &arg.folder)?;
    Ok(done())
}

async fn create_folder(State(state): Shared, Json(arg): Json<CreateFolder>) -> ApiResult {
    let name = arg.name.trim();
    if name.is_empty() {
        return Err(bad("a folder needs a name"));
    }
    let path = state.workspace().create_folder(&arg.parent, name)?;
    Ok(Json(json!({ "path": path })))
}

async fn rename(State(state): Shared, Json(arg): Json<Rename>) -> ApiResult {
    let path = state.workspace().rename(&arg.path, &arg.name)?;
    Ok(Json(json!({ "path": path })))
}

async fn move_item(State(state): Shared, Json(arg): Json<Move>) -> ApiResult {
    let path = state.workspace().move_item(&arg.path, &arg.to)?;
    Ok(Json(json!({ "path": path })))
}

async fn duplicate(State(state): Shared, Json(arg): Json<PathArg>) -> ApiResult {
    let path = state.workspace().duplicate(&arg.path)?;
    Ok(Json(json!({ "path": path })))
}

/// Moves the item to the trash, from where it can be restored.
async fn delete(State(state): Shared, Json(arg): Json<PathArg>) -> ApiResult {
    Ok(Json(json!(state.workspace().trash(&arg.path)?)))
}

async fn order(State(state): Shared, Json(arg): Json<Order>) -> ApiResult {
    state.workspace().set_order(&arg.folder, arg.order)?;
    Ok(done())
}

// ---- Trash --------------------------------------------------------------------

#[derive(Deserialize)]
struct TrashId {
    id: String,
}

async fn trash(State(state): Shared) -> ApiResult {
    Ok(Json(json!(state.workspace().trash_list()?)))
}

async fn restore(State(state): Shared, Json(arg): Json<TrashId>) -> ApiResult {
    let path = state.workspace().restore(&arg.id)?;
    Ok(Json(json!({ "path": path })))
}

async fn empty_trash(State(state): Shared) -> ApiResult {
    state.workspace().empty_trash()?;
    Ok(done())
}

// ---- Environments and globals ---------------------------------------------

#[derive(Deserialize)]
struct EnvironmentId {
    id: String,
}

#[derive(Deserialize)]
struct NameArg {
    name: String,
}

#[derive(Deserialize)]
struct WriteEnvironment {
    id: String,
    environment: EnvironmentFiles,
}

#[derive(Deserialize)]
struct RenameEnvironment {
    id: String,
    name: String,
}

async fn environments(State(state): Shared) -> ApiResult {
    Ok(Json(json!(state.workspace().environments()?)))
}

async fn create_environment(State(state): Shared, Json(arg): Json<NameArg>) -> ApiResult {
    let name = arg.name.trim();
    if name.is_empty() {
        return Err(bad("an environment needs a name"));
    }
    let id = state.workspace().create_environment(name)?;
    Ok(Json(json!({ "id": id })))
}

async fn read_environment(State(state): Shared, Json(arg): Json<EnvironmentId>) -> ApiResult {
    Ok(Json(json!(state.workspace().read_environment(&arg.id)?)))
}

async fn write_environment(State(state): Shared, Json(arg): Json<WriteEnvironment>) -> ApiResult {
    state
        .workspace()
        .write_environment(&arg.id, &arg.environment)?;
    Ok(done())
}

async fn rename_environment(State(state): Shared, Json(arg): Json<RenameEnvironment>) -> ApiResult {
    let id = state.workspace().rename_environment(&arg.id, &arg.name)?;
    Ok(Json(json!({ "id": id })))
}

async fn delete_environment(State(state): Shared, Json(arg): Json<EnvironmentId>) -> ApiResult {
    state.workspace().delete_environment(&arg.id)?;
    Ok(done())
}

async fn duplicate_environment(State(state): Shared, Json(arg): Json<EnvironmentId>) -> ApiResult {
    let id = state.workspace().duplicate_environment(&arg.id)?;
    Ok(Json(json!({ "id": id })))
}

async fn read_globals(State(state): Shared) -> ApiResult {
    Ok(Json(json!(state.workspace().read_local_state()?)))
}

async fn write_globals(State(state): Shared, Json(local): Json<LocalState>) -> ApiResult {
    state.workspace().write_local_state(&local)?;
    Ok(done())
}

// ---- Sending and inspecting requests --------------------------------------

#[derive(Deserialize)]
struct SendArg {
    #[serde(default)]
    path: Option<String>,
    request: Request,
    #[serde(default)]
    environment: Option<String>,
    /// Save variables the request sets to the local files.
    #[serde(default = "yes")]
    persist: bool,
    /// An id the UI chose, to cancel the request with while it is sent.
    #[serde(default)]
    id: Option<String>,
}

fn yes() -> bool {
    true
}

/// Unregisters a request's cancel switch when its send ends, however it
/// ends.
struct SendGuard<'a> {
    state: &'a AppState,
    id: Option<String>,
}

impl Drop for SendGuard<'_> {
    fn drop(&mut self) {
        if let Some(id) = &self.id {
            self.state.sends.lock().expect("sends lock").remove(id);
        }
    }
}

fn context(environment: Option<&str>) -> Context {
    Context {
        environment: environment.filter(|e| !e.is_empty()).map(str::to_owned),
        ..Context::default()
    }
}

async fn send(State(state): Shared, Json(arg): Json<SendArg>) -> ApiResult {
    let mut context = context(arg.environment.as_deref());
    let id = arg.id.clone().filter(|id| !id.is_empty());
    if let Some(id) = &id {
        let cancel = Cancel::new();
        state
            .sends
            .lock()
            .expect("sends lock")
            .insert(id.clone(), cancel.clone());
        context.cancel = Some(cancel);
    }
    let _guard = SendGuard { state: &state, id };
    let path = arg.path.as_deref().filter(|p| !p.is_empty());
    let execution = state
        .executor
        .execute(path, &arg.request, &mut context)
        .await;
    let mut warnings = Vec::new();
    if arg.persist
        && let Err(e) = persist_changes(
            state.workspace(),
            context.environment.as_deref(),
            &execution.changes,
        )
    {
        warnings.push(format!("variables could not be saved: {e}"));
    }
    state
        .history
        .add(&arg.request, context.environment.clone(), &execution);
    let mut value = json!(execution);
    if let Some(list) = value["warnings"].as_array_mut() {
        list.extend(warnings.into_iter().map(Value::from));
    }
    Ok(Json(value))
}

#[derive(Deserialize)]
struct SendId {
    id: String,
}

/// Stops a request the UI is sending. Unknown ids are ignored: the
/// request may have finished already.
async fn cancel_send(State(state): Shared, Json(arg): Json<SendId>) -> ApiResult {
    if let Some(cancel) = state.sends.lock().expect("sends lock").get(&arg.id) {
        cancel.cancel();
    }
    Ok(done())
}

#[derive(Deserialize)]
struct PrepareArg {
    #[serde(default)]
    path: Option<String>,
    request: Request,
    #[serde(default)]
    environment: Option<String>,
    #[serde(default)]
    language: Option<String>,
    /// Keep `{{variables}}` as written in code.
    #[serde(default)]
    keep_vars: bool,
    /// `2`, `4` or `tab`; left out for each language's own habit.
    #[serde(default)]
    indent: Option<String>,
    /// Leave out the imports and `main` function.
    #[serde(default)]
    no_boilerplate: bool,
}

async fn prepare(State(state): Shared, Json(arg): Json<PrepareArg>) -> ApiResult {
    let context = context(arg.environment.as_deref());
    let prepared = state.executor.prepare(
        arg.path.as_deref().filter(|p| !p.is_empty()),
        &arg.request,
        &context,
    )?;
    Ok(Json(
        json!({ "request": prepared.outgoing, "missing": prepared.missing }),
    ))
}

async fn code(State(state): Shared, Json(arg): Json<PrepareArg>) -> ApiResult {
    let language = arg.language.clone().unwrap_or_else(|| "curl".into());
    let path = arg.path.as_deref().filter(|p| !p.is_empty());
    let prepared = if arg.keep_vars {
        state.executor.prepare_unresolved(path, &arg.request)?
    } else {
        let context = context(arg.environment.as_deref());
        state.executor.prepare(path, &arg.request, &context)?
    };
    let mut outgoing = prepared.outgoing;
    codegen::relativize(&mut outgoing, state.workspace().root());
    let options = codegen::Options {
        indent: match arg.indent.as_deref().filter(|i| !i.is_empty()) {
            Some(indent) => Some(indent.parse().map_err(bad)?),
            None => None,
        },
        boilerplate: !arg.no_boilerplate,
    };
    let code = codegen::generate_with(&language, &outgoing, &options)
        .ok_or_else(|| bad(format!("unknown language {language}")))?;
    Ok(Json(json!({ "code": code, "missing": prepared.missing })))
}

#[derive(Deserialize)]
struct ExportArg {
    /// `folder`, `environment` or `globals`.
    kind: String,
    /// The folder's path or the environment's id.
    #[serde(default)]
    path: String,
    /// `postman`, `openapi` or `openapi-yaml`.
    #[serde(default)]
    format: String,
    /// Include values from git-ignored local files.
    #[serde(default)]
    secrets: bool,
}

/// Writes a folder, environment or the globals as a file to download.
async fn export(State(state): Shared, Json(arg): Json<ExportArg>) -> ApiResult {
    let ws = state.workspace();
    let exported = match (arg.kind.as_str(), arg.format.as_str()) {
        ("folder", "postman") => postman::collection(ws, &arg.path)?,
        ("folder", "openapi") => openapi::document(ws, &arg.path, false)?,
        ("folder", "openapi-yaml") => openapi::document(ws, &arg.path, true)?,
        ("environment", _) => postman::environment(ws, &arg.path, arg.secrets)?,
        ("globals", _) => postman::globals(ws, arg.secrets)?,
        (kind, format) => return Err(bad(format!("cannot export a {kind} as {format:?}"))),
    };
    Ok(Json(json!(exported)))
}

async fn languages() -> Json<Value> {
    Json(json!(codegen::LANGUAGES))
}

/// The variables a request sees. Values from the environment that it marks
/// secret come with `"secret": true`, so the UI can mask them.
async fn variables(State(state): Shared, Json(arg): Json<PrepareArg>) -> ApiResult {
    let environment = arg.environment.as_deref().filter(|e| !e.is_empty());
    let resolved = resolved_variables_with(
        state.workspace(),
        arg.path.as_deref().filter(|p| !p.is_empty()),
        &arg.request,
        environment,
        &state.executor.session(),
    )?;
    let secrets = match environment {
        Some(id) => state.workspace().read_environment(id)?.shared.secrets,
        None => Vec::new(),
    };
    let list: Vec<Value> = resolved
        .into_iter()
        .map(|var| {
            let secret = var.scope == Scope::Environment && secrets.contains(&var.name);
            let mut value = json!(var);
            if secret {
                value["secret"] = Value::Bool(true);
            }
            value
        })
        .collect();
    Ok(Json(Value::Array(list)))
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum SetScope {
    Environment,
    Workspace,
    Global,
}

#[derive(Deserialize)]
struct SetVariable {
    scope: SetScope,
    name: String,
    value: String,
    #[serde(default)]
    environment: Option<String>,
}

/// Saves one variable where a script's `gm.environment.set`,
/// `gm.collectionVariables.set` or `gm.globals.set` would: the
/// environment's local file, or the workspace's `getman.local.yaml`.
async fn set_variable(State(state): Shared, Json(arg): Json<SetVariable>) -> ApiResult {
    let name = arg.name.trim();
    if name.is_empty() || name.contains(['{', '}', '\n', '"']) {
        return Err(bad(
            "A variable name cannot be empty or hold braces, quotes or line breaks.",
        ));
    }
    let environment = arg.environment.as_deref().filter(|e| !e.is_empty());
    let scope = match arg.scope {
        SetScope::Environment if environment.is_none() => {
            return Err(bad("Choose an environment to save the variable in."));
        }
        SetScope::Environment => ChangeScope::Environment,
        SetScope::Workspace => ChangeScope::Collection,
        SetScope::Global => ChangeScope::Global,
    };
    let change = VarChange {
        scope,
        name: name.to_owned(),
        value: Some(arg.value),
    };
    persist_changes(state.workspace(), environment, &[change])?;
    Ok(done())
}

/// Forgets the session variables scripts set.
async fn clear_session(State(state): Shared) -> ApiResult {
    state.executor.clear_session();
    Ok(done())
}

async fn dynamic() -> Json<Value> {
    Json(json!(
        getman_core::dynamic::NAMES
            .iter()
            .map(|(name, description)| json!({ "name": name, "description": description }))
            .collect::<Vec<_>>()
    ))
}

#[derive(Deserialize)]
struct OAuthArg {
    #[serde(default)]
    path: Option<String>,
    auth: Auth,
    #[serde(default)]
    environment: Option<String>,
}

/// How long a browser sign-in may take.
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// The OAuth 2.0 settings of an argument, and the variables they see.
fn oauth_settings(
    state: &AppState,
    arg: &OAuthArg,
) -> Result<(OAuth2, getman_core::VarStack), ApiError> {
    let Auth::Oauth2(settings) = &arg.auth else {
        return Err(bad("the authentication is not OAuth 2.0"));
    };
    let request = Request {
        auth: arg.auth.clone(),
        ..Request::default()
    };
    let path = arg.path.as_deref().filter(|p| !p.is_empty());
    let resolved = resolved_variables_with(
        state.workspace(),
        path,
        &request,
        arg.environment.as_deref().filter(|e| !e.is_empty()),
        &state.executor.session(),
    )?;
    let mut vars = getman_core::VarStack::new();
    vars.push(
        getman_core::Scope::Request,
        resolved.into_iter().map(|v| (v.name, v.value)).collect(),
    );
    Ok((settings.clone(), vars))
}

/// How token requests go out: the way the requests in the argument's
/// folder go, through the same proxy and with the same certificates.
fn oauth_transport(
    state: &AppState,
    arg: &OAuthArg,
) -> Result<getman_core::http::Transport, ApiError> {
    let request = Request {
        auth: arg.auth.clone(),
        ..Request::default()
    };
    let path = arg.path.as_deref().filter(|p| !p.is_empty());
    Ok(state
        .executor
        .transport(path, &request, &context(arg.environment.as_deref()))?)
}

async fn oauth2_token(State(state): Shared, Json(arg): Json<OAuthArg>) -> ApiResult {
    let (settings, vars) = oauth_settings(&state, &arg)?;
    let settings = &settings;
    let transport = oauth_transport(&state, &arg)?;
    let token = state
        .executor
        .authenticator
        .token(&state.executor.http, settings, &vars, transport, true)
        .await?;
    Ok(Json(json!(token)))
}

/// Starts a browser sign-in and opens the authorization URL in the
/// system's browser.
async fn oauth2_authorize(State(state): Shared, Json(arg): Json<OAuthArg>) -> ApiResult {
    let (settings, vars) = oauth_settings(&state, &arg)?;
    // The code is exchanged later, once the browser is back, the way the
    // folder's requests go.
    let transport = oauth_transport(&state, &arg)?;
    let authorization = state
        .executor
        .authenticator
        .authorize(&settings, &vars, transport)
        .await?;
    let opened = open::that_detached(&authorization.url).is_ok();
    let mut value = json!(authorization);
    value["opened"] = json!(opened);
    Ok(Json(value))
}

#[derive(Deserialize)]
struct SignInId {
    id: String,
}

/// Waits for a browser sign-in to finish and returns its token.
async fn oauth2_wait(State(state): Shared, Json(arg): Json<SignInId>) -> ApiResult {
    let token = state
        .executor
        .authenticator
        .wait_for_token(&state.executor.http, &arg.id, SIGN_IN_TIMEOUT)
        .await?;
    Ok(Json(json!(token)))
}

async fn oauth2_cancel(State(state): Shared, Json(arg): Json<SignInId>) -> ApiResult {
    state.executor.authenticator.cancel_authorization(&arg.id);
    Ok(done())
}

/// The cached token for these settings, if there is one.
async fn oauth2_status(State(state): Shared, Json(arg): Json<OAuthArg>) -> ApiResult {
    let (settings, vars) = oauth_settings(&state, &arg)?;
    let token = state.executor.authenticator.cached_token(&settings, &vars);
    Ok(Json(json!({ "token": token })))
}

async fn oauth2_clear(State(state): Shared, Json(arg): Json<OAuthArg>) -> ApiResult {
    let (settings, vars) = oauth_settings(&state, &arg)?;
    let cleared = state.executor.authenticator.clear_token(&settings, &vars)?;
    Ok(Json(json!({ "cleared": cleared })))
}

// ---- Runs -------------------------------------------------------------------

#[derive(Deserialize)]
struct RunArg {
    #[serde(default)]
    target: String,
    /// Folders and requests to run in this order, instead of `target`.
    #[serde(default)]
    targets: Vec<String>,
    #[serde(default)]
    environment: Option<String>,
    #[serde(default)]
    iterations: Option<usize>,
    /// CSV or JSON rows, as text.
    #[serde(default)]
    data: Option<String>,
    #[serde(default)]
    delay_ms: u64,
    #[serde(default)]
    bail: bool,
    #[serde(default)]
    persist: bool,
}

/// Starts a run and streams its events as JSON lines.
async fn run(State(state): Shared, Json(arg): Json<RunArg>) -> ApiResult<Response> {
    let data = match arg.data.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        Some(text) if text.starts_with('[') => getman_core::data::parse_json(text).map_err(bad)?,
        Some(text) => getman_core::data::parse_csv(text).map_err(bad)?,
        None => Vec::new(),
    };
    let environment = arg.environment.clone().filter(|e| !e.is_empty());
    let options = RunOptions {
        target: arg.target.clone(),
        targets: arg.targets.clone(),
        environment: environment.clone(),
        iterations: arg.iterations,
        data,
        delay: Duration::from_millis(arg.delay_ms),
        bail: arg.bail,
        ..RunOptions::default()
    };
    // Fail early, as a normal error, if a target does not exist.
    runner::planned(&state.workspace().tree()?, &options)?;

    let id = getman_core::dynamic::uuid_v4();
    let about = About {
        workspace: state.workspace().name(),
        targets: if options.targets.is_empty() {
            vec![options.target.clone()]
        } else {
            options.targets.clone()
        },
        environment: environment.as_ref().map(|id| {
            state
                .workspace()
                .read_environment(id)
                .map_or_else(|_| id.clone(), |files| files.shared.name)
        }),
        started: getman_core::time::now_millis(),
    };
    let mut stored = StoredRun::new(id.clone(), about);
    let cancel = Arc::new(AtomicBool::new(false));
    state
        .runs
        .lock()
        .expect("runs lock")
        .insert(id.clone(), Arc::clone(&cancel));

    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel::<Bytes>();
    let task_state = Arc::clone(&state);
    let task_id = id.clone();
    tokio::spawn(async move {
        let send = |event: &RunEvent| {
            let mut line = serde_json::to_vec(event).unwrap_or_default();
            line.push(b'\n');
            let _ = sender.send(Bytes::from(line));
        };
        let result = runner::run(&task_state.executor, &options, &cancel, |event| {
            send(&event);
            stored.record(&event);
        })
        .await;
        if result.is_ok()
            && let Err(e) = runs::save(task_state.workspace(), &stored)
        {
            crate::style::warn(&format!("the run could not be kept: {e}"));
        }
        match result {
            Ok(report) if arg.persist => {
                let overlay = &report.overlay;
                let mut changes = Vec::new();
                for (scope, map) in [
                    (ChangeScope::Global, &overlay.globals),
                    (ChangeScope::Collection, &overlay.collection),
                    (ChangeScope::Environment, &overlay.environment),
                ] {
                    changes.extend(map.iter().map(|(name, value)| VarChange {
                        scope,
                        name: name.clone(),
                        value: value.clone(),
                    }));
                }
                let _ = persist_changes(task_state.workspace(), environment.as_deref(), &changes);
            }
            Ok(_) => {}
            Err(e) => {
                let mut line =
                    serde_json::to_vec(&json!({ "event": "error", "error": e.to_string() }))
                        .unwrap_or_default();
                line.push(b'\n');
                let _ = sender.send(Bytes::from(line));
            }
        }
        task_state.runs.lock().expect("runs lock").remove(&task_id);
    });

    let stream = futures_util::stream::unfold(receiver, |mut receiver| async move {
        receiver
            .recv()
            .await
            .map(|bytes| (Ok::<_, Infallible>(bytes), receiver))
    });
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "application/x-ndjson")
        .header(header::CACHE_CONTROL, "no-cache")
        .header("x-run-id", id)
        .body(Body::from_stream(stream))
        .unwrap_or_default())
}

#[derive(Deserialize)]
struct RunId {
    id: String,
}

async fn cancel_run(State(state): Shared, Json(arg): Json<RunId>) -> ApiResult {
    if let Some(cancel) = state.runs.lock().expect("runs lock").get(&arg.id) {
        cancel.store(true, Ordering::Relaxed);
    }
    Ok(done())
}

// ---- Past runs ------------------------------------------------------------------

async fn list_runs(State(state): Shared) -> Json<Value> {
    Json(json!(runs::list(state.workspace())))
}

async fn read_run(State(state): Shared, Json(arg): Json<RunId>) -> ApiResult {
    let run = runs::read(state.workspace(), &arg.id).map_err(not_found)?;
    Ok(Json(json!(run)))
}

#[derive(Deserialize)]
struct ReportArg {
    id: String,
    format: String,
}

/// A past run's report as a file to download.
async fn run_report(State(state): Shared, Json(arg): Json<ReportArg>) -> ApiResult<Response> {
    let format = Format::from_name(&arg.format).ok_or_else(|| {
        bad(format!(
            "{:?} is not a report format; use one of {}",
            arg.format,
            Format::ALL.map(Format::name).join(", ")
        ))
    })?;
    let run = runs::read(state.workspace(), &arg.id).map_err(not_found)?;
    let bytes = format
        .render(&run.recorded())
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let name = runs::report_name(&run, format.extension());
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, format.content_type())
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{name}\""),
        )
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(bytes))
        .unwrap_or_default())
}

async fn delete_run(State(state): Shared, Json(arg): Json<RunId>) -> ApiResult {
    runs::delete(state.workspace(), &arg.id).map_err(bad)?;
    Ok(done())
}

fn not_found(message: String) -> ApiError {
    ApiError(StatusCode::NOT_FOUND, message)
}

// ---- Examples -------------------------------------------------------------------

#[derive(Deserialize)]
struct SaveExample {
    path: String,
    example: Example,
    /// The body, as `{"text": ...}` or `{"base64": ...}`.
    #[serde(default)]
    body: Payload,
}

#[derive(Deserialize)]
struct ExampleName {
    path: String,
    name: String,
}

#[derive(Deserialize)]
struct RenameExample {
    path: String,
    from: String,
    to: String,
}

fn examples_of(state: &AppState, path: &str) -> ApiResult {
    Ok(Json(json!(state.workspace().read_request(path)?.examples)))
}

async fn list_examples(State(state): Shared, Json(arg): Json<PathArg>) -> ApiResult {
    examples_of(&state, &arg.path)
}

async fn save_example(State(state): Shared, Json(arg): Json<SaveExample>) -> ApiResult {
    let mut example = arg.example;
    // Session cookies do not belong in a file that is committed.
    example
        .headers
        .retain(|h| !h.name.eq_ignore_ascii_case("set-cookie"));
    getman_core::examples::save(state.workspace(), &arg.path, example, &arg.body.0)?;
    examples_of(&state, &arg.path)
}

async fn example_body(State(state): Shared, Json(arg): Json<ExampleName>) -> ApiResult {
    let request = state.workspace().read_request(&arg.path)?;
    let example = request
        .examples
        .iter()
        .find(|e| e.name == arg.name)
        .ok_or_else(|| Error::NotFound(format!("the example {:?}", arg.name)))?;
    let body = getman_core::examples::body(state.workspace(), &arg.path, example)?;
    Ok(Json(json!({ "body": Payload(body) })))
}

async fn rename_example(State(state): Shared, Json(arg): Json<RenameExample>) -> ApiResult {
    getman_core::examples::rename(state.workspace(), &arg.path, &arg.from, &arg.to)?;
    examples_of(&state, &arg.path)
}

async fn delete_example(State(state): Shared, Json(arg): Json<ExampleName>) -> ApiResult {
    getman_core::examples::delete(state.workspace(), &arg.path, &arg.name)?;
    examples_of(&state, &arg.path)
}

// ---- Mock servers -----------------------------------------------------------------

#[derive(Deserialize)]
struct StartMock {
    #[serde(default)]
    folder: String,
    #[serde(default)]
    port: Option<u16>,
    #[serde(default)]
    delay_ms: u64,
}

#[derive(Deserialize)]
struct FolderArg {
    #[serde(default)]
    folder: String,
}

fn describe_mock(mock: &crate::mock::Mock) -> Value {
    json!({
        "folder": mock.folder,
        "url": mock.url(),
        "port": mock.port,
        "delay_ms": mock.delay_ms,
        "mocked": mock.mocked(),
        "hits": mock.hits(),
    })
}

async fn list_mocks(State(state): Shared) -> Json<Value> {
    let mocks = state.mocks.lock().expect("mocks lock");
    let mut list: Vec<Value> = mocks.values().map(describe_mock).collect();
    list.sort_by(|a, b| a["folder"].as_str().cmp(&b["folder"].as_str()));
    Json(json!(list))
}

async fn start_mock(State(state): Shared, Json(arg): Json<StartMock>) -> ApiResult {
    let folder = arg.folder.trim_matches('/').to_owned();
    if let Some(running) = state.mocks.lock().expect("mocks lock").get(&folder) {
        return Ok(Json(describe_mock(running)));
    }
    let listener = crate::mock::bind(arg.port).await.map_err(bad)?;
    let label = if folder.is_empty() {
        state.workspace().name()
    } else {
        folder.clone()
    };
    let mock = crate::mock::start(
        state.workspace().clone(),
        &folder,
        listener,
        Duration::from_millis(arg.delay_ms),
        Box::new(move |hit: &crate::mock::Hit| {
            crate::style::note(&format!(
                "mock {label}: {} {} -> {}{}",
                hit.method,
                hit.path,
                hit.status,
                hit.example
                    .as_ref()
                    .map(|name| format!(" ({name})"))
                    .unwrap_or_default()
            ));
        }),
    )
    .map_err(|e| ApiError(StatusCode::NOT_FOUND, e))?;
    let described = describe_mock(&mock);
    state.mocks.lock().expect("mocks lock").insert(folder, mock);
    Ok(Json(described))
}

async fn stop_mock(State(state): Shared, Json(arg): Json<FolderArg>) -> ApiResult {
    let folder = arg.folder.trim_matches('/');
    let mock = state.mocks.lock().expect("mocks lock").remove(folder);
    if let Some(mock) = mock {
        mock.stop();
    }
    Ok(done())
}

// ---- Import -------------------------------------------------------------------

/// Largest import accepted: big API specs run to tens of megabytes.
const IMPORT_LIMIT: usize = 64 * 1024 * 1024;

#[derive(Deserialize)]
struct ImportArg {
    text: String,
    /// File name, which tells `.bru` files apart.
    #[serde(default)]
    name: String,
    #[serde(default)]
    folder: String,
    #[serde(default)]
    root: bool,
}

/// Imports one text in any format Getman reads.
async fn import(State(state): Shared, Json(arg): Json<ImportArg>) -> ApiResult {
    let report = import::import_text(
        state.workspace(),
        &arg.folder,
        &arg.name,
        &arg.text,
        arg.root,
    )?;
    Ok(Json(json!(report)))
}

#[derive(Deserialize)]
struct ImportFilesArg {
    files: Vec<SourceFile>,
    #[serde(default)]
    folder: String,
    #[serde(default)]
    root: bool,
}

/// Imports dropped files, or a folder such as a Bruno collection.
async fn import_files(State(state): Shared, Json(arg): Json<ImportFilesArg>) -> ApiResult {
    let reports = import::import_files(state.workspace(), &arg.folder, &arg.files, arg.root)?;
    Ok(Json(json!(reports)))
}

#[derive(Deserialize)]
struct TextArg {
    text: String,
}

async fn parse_curl(Json(arg): Json<TextArg>) -> ApiResult {
    Ok(Json(json!(curl::parse(&arg.text)?)))
}

// ---- Cookies and history ------------------------------------------------------

#[derive(Deserialize)]
struct CookieArg {
    domain: String,
    path: String,
    name: String,
}

async fn cookies(State(state): Shared) -> Json<Value> {
    Json(json!(state.executor.http.cookies()))
}

#[derive(Deserialize)]
struct SetCookie {
    cookie: CookieInfo,
    /// The cookie being edited, when its domain, path or name changes.
    #[serde(default)]
    previous: Option<CookieArg>,
}

async fn set_cookie(State(state): Shared, Json(arg): Json<SetCookie>) -> ApiResult {
    let http = &state.executor.http;
    http.set_cookie(&arg.cookie)?;
    if let Some(previous) = &arg.previous {
        let same = previous.domain == arg.cookie.domain
            && previous.path == arg.cookie.path
            && previous.name == arg.cookie.name;
        if !same {
            http.remove_cookie(&previous.domain, &previous.path, &previous.name);
        }
    }
    Ok(done())
}

async fn delete_cookie(State(state): Shared, Json(arg): Json<CookieArg>) -> ApiResult {
    state
        .executor
        .http
        .remove_cookie(&arg.domain, &arg.path, &arg.name);
    Ok(done())
}

async fn clear_cookies(State(state): Shared) -> ApiResult {
    state.executor.http.clear_cookies();
    Ok(done())
}

async fn history(State(state): Shared) -> Json<Value> {
    Json(json!(state.history.list()))
}

#[derive(Deserialize)]
struct HistoryId {
    id: String,
}

/// An entry with the response kept with it.
async fn read_history(State(state): Shared, Json(arg): Json<HistoryId>) -> ApiResult {
    let (entry, stored) = state
        .history
        .read(&arg.id)
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "That history entry is gone.".into()))?;
    let mut value = json!(entry);
    value["response"] = stored
        .as_ref()
        .map_or(Value::Null, |stored| json!(stored.response));
    value["body_cut"] = json!(stored.is_some_and(|stored| stored.body_cut));
    Ok(Json(value))
}

async fn clear_history(State(state): Shared) -> ApiResult {
    state.history.clear();
    Ok(done())
}

// ---- Header presets -------------------------------------------------------------

async fn read_header_presets(State(state): Shared) -> ApiResult {
    Ok(Json(json!(presets::read(state.workspace())?)))
}

async fn write_header_presets(State(state): Shared, Json(list): Json<Vec<Preset>>) -> ApiResult {
    presets::write(state.workspace(), &list)?;
    Ok(done())
}
