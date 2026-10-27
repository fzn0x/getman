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
use getman_core::Error;
use getman_core::execute::{Context, persist_changes, resolved_variables};
use getman_core::export::codegen;
use getman_core::import::{self, SourceFile, curl};
use getman_core::model::{Auth, Folder, Request};
use getman_core::runner::{self, RunEvent, RunOptions};
use getman_core::script::{ChangeScope, VarChange};
use getman_core::workspace::{EnvironmentFiles, LocalState};
use serde::Deserialize;
use serde_json::{Value, json};

use super::AppState;

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
        .route("/environments", get(environments))
        .route("/environment/create", post(create_environment))
        .route("/environment/read", post(read_environment))
        .route("/environment/write", post(write_environment))
        .route("/environment/rename", post(rename_environment))
        .route("/environment/delete", post(delete_environment))
        .route("/globals", get(read_globals).put(write_globals))
        .route("/send", post(send))
        .route("/prepare", post(prepare))
        .route("/codegen", post(code))
        .route("/languages", get(languages))
        .route("/variables", post(variables))
        .route("/dynamic", get(dynamic))
        .route("/run", post(run))
        .route("/run/cancel", post(cancel_run))
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
        .route("/cookies/delete", post(delete_cookie))
        .route("/cookies/clear", post(clear_cookies))
        .route("/history", get(history))
        .route("/history/clear", post(clear_history))
        .route("/oauth2/token", post(oauth2_token))
}

/// An error answered as `{"error": "..."}` with a fitting status.
pub(super) struct ApiError(StatusCode, String);

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

type ApiResult<T = Json<Value>> = Result<T, ApiError>;

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
    state.workspace().write_request(&arg.path, &arg.request)?;
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

async fn delete(State(state): Shared, Json(arg): Json<PathArg>) -> ApiResult {
    state.workspace().delete(&arg.path)?;
    Ok(done())
}

async fn order(State(state): Shared, Json(arg): Json<Order>) -> ApiResult {
    state.workspace().set_order(&arg.folder, arg.order)?;
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
}

fn yes() -> bool {
    true
}

fn context(environment: Option<&str>) -> Context {
    Context {
        environment: environment.filter(|e| !e.is_empty()).map(str::to_owned),
        ..Context::default()
    }
}

async fn send(State(state): Shared, Json(arg): Json<SendArg>) -> ApiResult {
    let mut context = context(arg.environment.as_deref());
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
struct PrepareArg {
    #[serde(default)]
    path: Option<String>,
    request: Request,
    #[serde(default)]
    environment: Option<String>,
    #[serde(default)]
    language: Option<String>,
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
    let context = context(arg.environment.as_deref());
    let prepared = state.executor.prepare(
        arg.path.as_deref().filter(|p| !p.is_empty()),
        &arg.request,
        &context,
    )?;
    let mut outgoing = prepared.outgoing;
    codegen::relativize(&mut outgoing, state.workspace().root());
    let code = codegen::generate(&language, &outgoing)
        .ok_or_else(|| bad(format!("unknown language {language}")))?;
    Ok(Json(json!({ "code": code, "missing": prepared.missing })))
}

async fn languages() -> Json<Value> {
    Json(json!(codegen::LANGUAGES))
}

async fn variables(State(state): Shared, Json(arg): Json<PrepareArg>) -> ApiResult {
    let resolved = resolved_variables(
        state.workspace(),
        arg.path.as_deref().filter(|p| !p.is_empty()),
        &arg.request,
        arg.environment.as_deref().filter(|e| !e.is_empty()),
    )?;
    Ok(Json(json!(resolved)))
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

async fn oauth2_token(State(state): Shared, Json(arg): Json<OAuthArg>) -> ApiResult {
    let Auth::Oauth2(settings) = &arg.auth else {
        return Err(bad("the authentication is not OAuth 2.0"));
    };
    let request = Request {
        auth: arg.auth.clone(),
        ..Request::default()
    };
    let resolved = resolved_variables(
        state.workspace(),
        arg.path.as_deref().filter(|p| !p.is_empty()),
        &request,
        arg.environment.as_deref().filter(|e| !e.is_empty()),
    )?;
    let mut vars = getman_core::VarStack::new();
    vars.push(
        getman_core::Scope::Request,
        resolved.into_iter().map(|v| (v.name, v.value)).collect(),
    );
    let token = state
        .executor
        .authenticator
        .token(
            &state.executor.http,
            settings,
            &vars,
            getman_core::http::Transport::default(),
            true,
        )
        .await?;
    Ok(Json(json!(token)))
}

// ---- Runs -------------------------------------------------------------------

#[derive(Deserialize)]
struct RunArg {
    #[serde(default)]
    target: String,
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
        environment: environment.clone(),
        iterations: arg.iterations,
        data,
        delay: Duration::from_millis(arg.delay_ms),
        bail: arg.bail,
        ..RunOptions::default()
    };
    // Fail early, as a normal error, if the target does not exist.
    runner::requests_in(&state.workspace().tree()?, &options.target)?;

    let id = getman_core::dynamic::uuid_v4();
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
        })
        .await;
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

async fn clear_history(State(state): Shared) -> ApiResult {
    state.history.clear();
    Ok(done())
}
