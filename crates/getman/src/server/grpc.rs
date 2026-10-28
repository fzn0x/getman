//! The JSON API for gRPC requests: schemas, live calls that stream their
//! events as JSON lines, and generating requests.
//!
//! A call starts with `/grpc/invoke`, whose response streams the call's
//! events and ends with the whole execution. While it runs, `/grpc/send`
//! sends messages on a stream from the client, `/grpc/end` ends that
//! stream and `/grpc/cancel` stops the call. Closing the event stream
//! cancels the call too.

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};

use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::Response;
use axum::routing::post;
use axum::{Json, Router};
use getman_core::execute::{Context, persist_changes};
use getman_core::grpc::{self, Command, Live, LiveEvent};
use getman_core::model::{Request, RequestKind};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};

use super::AppState;
use super::api::{ApiError, ApiResult};

type Shared = State<Arc<AppState>>;

/// Live calls, by id, and how to steer them.
#[derive(Debug, Default)]
pub(crate) struct Calls(Mutex<HashMap<String, mpsc::UnboundedSender<Command>>>);

impl Calls {
    fn insert(&self, id: String, commands: mpsc::UnboundedSender<Command>) {
        self.0.lock().expect("calls lock").insert(id, commands);
    }

    fn remove(&self, id: &str) {
        self.0.lock().expect("calls lock").remove(id);
    }

    fn get(&self, id: &str) -> Result<mpsc::UnboundedSender<Command>, ApiError> {
        self.0
            .lock()
            .expect("calls lock")
            .get(id)
            .cloned()
            .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "The call has ended.".into()))
    }
}

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/grpc/protos", post(protos))
        .route("/grpc/schema", post(schema))
        .route("/grpc/invoke", post(invoke))
        .route("/grpc/send", post(send))
        .route("/grpc/end", post(end))
        .route("/grpc/cancel", post(cancel))
        .route("/grpc/code", post(code))
        .route("/grpc/generate", post(generate))
}

#[derive(Deserialize)]
struct RequestArg {
    #[serde(default)]
    path: Option<String>,
    request: Request,
    #[serde(default)]
    environment: Option<String>,
    /// Ask the server again instead of using its schema read before.
    #[serde(default)]
    refresh: bool,
    /// Save variables the call sets to the local files.
    #[serde(default = "yes")]
    persist: bool,
}

fn yes() -> bool {
    true
}

impl RequestArg {
    fn path(&self) -> Option<&str> {
        self.path.as_deref().filter(|p| !p.is_empty())
    }

    fn context(&self) -> Context {
        Context {
            environment: self.environment.clone().filter(|e| !e.is_empty()),
            ..Context::default()
        }
    }

    fn check(&self) -> Result<(), ApiError> {
        if self.request.kind == RequestKind::Grpc {
            Ok(())
        } else {
            Err(ApiError(
                StatusCode::BAD_REQUEST,
                "the request is not a gRPC request".into(),
            ))
        }
    }
}

/// Every `.proto` file in the workspace, for choosing a schema.
async fn protos(State(state): Shared) -> Json<Value> {
    let root = state.workspace().root().to_path_buf();
    let files = tokio::task::spawn_blocking(move || grpc::proto_files(&root))
        .await
        .unwrap_or_default();
    Json(json!({ "files": files }))
}

/// The services and methods a request can call, with example messages.
async fn schema(State(state): Shared, Json(arg): Json<RequestArg>) -> ApiResult {
    arg.check()?;
    let schema = grpc::load_schema(
        &state.executor,
        arg.path(),
        &arg.request,
        &arg.context(),
        arg.refresh,
    )
    .await?;
    Ok(Json(json!({ "services": schema.services() })))
}

/// Starts a call and streams its events as JSON lines, ending with
/// `{"event": "done", "execution": ...}`.
async fn invoke(State(state): Shared, Json(arg): Json<RequestArg>) -> ApiResult<Response> {
    arg.check()?;
    let id = getman_core::dynamic::uuid_v4();
    let (commands, receiver) = mpsc::unbounded_channel();
    let (events, mut event_receiver) = mpsc::unbounded_channel::<LiveEvent>();
    state.grpc.insert(id.clone(), commands.clone());

    let (lines, line_receiver) = mpsc::unbounded_channel::<Bytes>();
    let task_state = Arc::clone(&state);
    let task_id = id.clone();
    tokio::spawn(async move {
        let send = |value: &Value| {
            let mut line = serde_json::to_vec(value).unwrap_or_default();
            line.push(b'\n');
            lines.send(Bytes::from(line)).is_ok()
        };
        let mut context = arg.context();
        let live = Live {
            commands: receiver,
            events,
        };
        let run = task_state
            .executor
            .execute_live(arg.path(), &arg.request, &mut context, live);
        let forward = async {
            while let Some(event) = event_receiver.recv().await {
                // Nobody is listening any more: stop the call.
                if !send(&json!(event)) {
                    let _ = commands.send(Command::Cancel);
                }
            }
        };
        let (execution, ()) = tokio::join!(run, forward);
        task_state.grpc.remove(&task_id);

        let mut warnings = Vec::new();
        if arg.persist
            && let Err(e) = persist_changes(
                task_state.workspace(),
                context.environment.as_deref(),
                &execution.changes,
            )
        {
            warnings.push(format!("variables could not be saved: {e}"));
        }
        task_state
            .history
            .add(&arg.request, context.environment.clone(), &execution);
        let mut value = json!(execution);
        if let Some(list) = value["warnings"].as_array_mut() {
            list.extend(warnings.into_iter().map(Value::from));
        }
        send(&json!({ "event": "done", "execution": value }));
    });

    let stream = futures_util::stream::unfold(line_receiver, |mut receiver| async move {
        receiver
            .recv()
            .await
            .map(|bytes| (Ok::<_, Infallible>(bytes), receiver))
    });
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "application/x-ndjson")
        .header(header::CACHE_CONTROL, "no-cache")
        .header("x-call-id", id)
        .body(Body::from_stream(stream))
        .unwrap_or_default())
}

#[derive(Deserialize)]
struct SendArg {
    id: String,
    text: String,
}

/// Sends messages on a live call's stream.
async fn send(State(state): Shared, Json(arg): Json<SendArg>) -> ApiResult {
    let commands = state.grpc.get(&arg.id)?;
    let (reply, answer) = oneshot::channel();
    commands
        .send(Command::Send {
            text: arg.text,
            reply,
        })
        .map_err(|_| ended())?;
    match answer.await.map_err(|_| ended())? {
        Ok(sent) => Ok(Json(json!({ "sent": sent }))),
        Err(message) => Err(ApiError(StatusCode::BAD_REQUEST, message)),
    }
}

#[derive(Deserialize)]
struct CallId {
    id: String,
}

/// Ends the client's side of a live call's stream.
async fn end(State(state): Shared, Json(arg): Json<CallId>) -> ApiResult {
    let commands = state.grpc.get(&arg.id)?;
    commands.send(Command::End).map_err(|_| ended())?;
    Ok(Json(json!({})))
}

/// Stops a live call. Stopping one that has ended is not an error.
async fn cancel(State(state): Shared, Json(arg): Json<CallId>) -> ApiResult {
    if let Ok(commands) = state.grpc.get(&arg.id) {
        let _ = commands.send(Command::Cancel);
    }
    Ok(Json(json!({})))
}

fn ended() -> ApiError {
    ApiError(StatusCode::NOT_FOUND, "The call has ended.".into())
}

/// The request as a grpcurl command.
async fn code(State(state): Shared, Json(arg): Json<RequestArg>) -> ApiResult {
    arg.check()?;
    let (code, missing) = grpc::grpcurl(&state.executor, arg.path(), &arg.request, &arg.context())?;
    Ok(Json(json!({ "code": code, "missing": missing })))
}

#[derive(Deserialize)]
struct GenerateArg {
    #[serde(flatten)]
    template: RequestArg,
    /// Full name of the service.
    service: String,
    /// Folder to create the requests' folder in.
    #[serde(default)]
    folder: String,
}

/// Creates a folder with a request for each method of a service.
async fn generate(State(state): Shared, Json(arg): Json<GenerateArg>) -> ApiResult {
    arg.template.check()?;
    let schema = grpc::load_schema(
        &state.executor,
        arg.template.path(),
        &arg.template.request,
        &arg.template.context(),
        false,
    )
    .await?;
    let service = schema
        .services()
        .into_iter()
        .find(|s| s.name == arg.service)
        .ok_or_else(|| {
            ApiError(
                StatusCode::BAD_REQUEST,
                format!("the schema has no service {}", arg.service),
            )
        })?;
    let (folder, requests) = grpc::generate_requests(
        state.workspace(),
        &arg.folder,
        &arg.template.request,
        &service,
    )?;
    Ok(Json(json!({ "folder": folder, "requests": requests })))
}

#[cfg(test)]
mod tests {
    use super::super::router;
    use super::*;
    use axum::http::{HeaderValue, Request as HttpRequest};
    use getman_core::Workspace;
    use getman_core::execute::Executor;
    use tower::ServiceExt as _;

    fn app() -> (tempfile::TempDir, Router) {
        let dir = tempfile::tempdir().unwrap();
        let workspace = Workspace::init(dir.path(), "Test").unwrap();
        std::fs::create_dir(dir.path().join("protos")).unwrap();
        std::fs::write(
            dir.path().join("protos/shop.proto"),
            "syntax = \"proto3\";\npackage shop;\nservice Orders { rpc Get(Id) returns (Id); }\nmessage Id { string id = 1; }\n",
        )
        .unwrap();
        let state = Arc::new(AppState {
            history: super::super::history::History::open(&workspace),
            executor: Executor::new(workspace),
            token: "secret-token".into(),
            port: 7878,
            runs: Mutex::new(HashMap::new()),
            mocks: Mutex::new(HashMap::new()),
            sends: Mutex::new(HashMap::new()),
            realtime: super::super::realtime::Hub::default(),
            grpc: Calls::default(),
        });
        (dir, router(state))
    }

    async fn post(app: &Router, path: &str, body: Value) -> (StatusCode, Value) {
        let mut request = HttpRequest::post(path)
            .header("host", "127.0.0.1:7878")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        request
            .headers_mut()
            .insert("x-getman-token", HeaderValue::from_static("secret-token"));
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    #[tokio::test]
    async fn schemas_list_methods_and_generate_requests() {
        let (dir, app) = app();
        let (status, body) = post(&app, "/api/grpc/protos", json!({})).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["files"], json!(["protos/shop.proto"]));

        let request = json!({
            "name": "Get",
            "kind": "grpc",
            "method": "shop.Orders/Get",
            "url": "localhost:1",
            "grpc": { "proto": "protos/shop.proto" },
        });
        let (status, body) = post(&app, "/api/grpc/schema", json!({ "request": request })).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["services"][0]["methods"][0]["path"], "shop.Orders/Get");
        assert_eq!(
            body["services"][0]["methods"][0]["example"],
            json!({"id": ""})
        );

        let (status, body) = post(&app, "/api/grpc/code", json!({ "request": request })).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(body["code"].as_str().unwrap().starts_with("grpcurl"));

        let (status, body) = post(
            &app,
            "/api/grpc/generate",
            json!({ "request": request, "service": "shop.Orders" }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body, json!({ "folder": "orders", "requests": 1 }));
        assert!(dir.path().join("orders/get.yaml").is_file());

        let (status, _) = post(
            &app,
            "/api/grpc/send",
            json!({ "id": "nope", "text": "{}" }),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, body) = post(
            &app,
            "/api/grpc/schema",
            json!({ "request": { "method": "GET", "url": "http://x" } }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    }
}
