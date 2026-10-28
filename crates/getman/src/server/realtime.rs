//! Live connections for the browser UI: WebSocket, Server-Sent Events and
//! Socket.IO requests.
//!
//! The server holds the connections. A page opens one stream of events,
//! newline-delimited JSON that carries the events of every connection the
//! page opens, then connects, sends and closes with ordinary API calls.
//! One stream per page, whatever the number of connections, keeps the
//! browser's few connections per host free for the rest of the API. When
//! the page goes away its stream ends, and every connection it opened is
//! closed.
//!
//! Pre-request scripts run before a connection is made, and when it ends
//! the post-response scripts and checks run on the handshake and the
//! messages received; their results arrive on the stream as a `result`
//! event.

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use getman_core::execute::{Context, Execution, persist_changes};
use getman_core::model::Request;
use getman_core::realtime::{self, Control, Options, Outbound, Recorder};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::{mpsc, watch};

use super::AppState;

/// Pages that may have a stream open at once.
const MAX_PAGES: usize = 32;
/// Connections one page may have open at once.
const MAX_SESSIONS: usize = 32;
/// Lines held for a page that reads slowly.
const LINE_BUFFER: usize = 256;
/// How often an idle stream says it is alive, which also tells the server
/// soon when the page has gone.
const HEARTBEAT: Duration = Duration::from_secs(15);

type Shared = State<Arc<AppState>>;

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/realtime/events", post(events))
        .route("/realtime/connect", post(connect))
        .route("/realtime/send", post(send))
        .route("/realtime/close", post(close))
}

/// The pages with a stream open, and their connections.
#[derive(Debug)]
pub(crate) struct Hub {
    inner: Mutex<Inner>,
    /// Set when the server stops, which ends every page's stream.
    stopping: watch::Sender<bool>,
}

impl Default for Hub {
    fn default() -> Self {
        Self {
            inner: Mutex::default(),
            stopping: watch::channel(false).0,
        }
    }
}

#[derive(Debug, Default)]
struct Inner {
    pages: HashMap<String, mpsc::Sender<Bytes>>,
    sessions: HashMap<String, Session>,
}

#[derive(Debug)]
struct Session {
    page: String,
    control: Control,
}

impl Hub {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("realtime lock")
    }

    /// Forgets a page and closes its connections.
    fn drop_page(&self, page: &str) {
        let mut inner = self.lock();
        inner.pages.remove(page);
        inner.sessions.retain(|_, session| {
            let keep = session.page != page;
            if !keep {
                session.control.close();
            }
            keep
        });
    }

    /// Closes every connection and ends every page's stream, for when the
    /// server stops: it waits for open responses, and these never end.
    pub(crate) fn close_all(&self) {
        self.stopping.send_replace(true);
        let mut inner = self.lock();
        for session in inner.sessions.values() {
            session.control.close();
        }
        inner.sessions.clear();
        inner.pages.clear();
    }
}

fn line(value: &Value) -> Bytes {
    let mut line = serde_json::to_vec(value).unwrap_or_default();
    line.push(b'\n');
    Bytes::from(line)
}

/// An error answered as `{"error": "..."}`, like the rest of the API.
pub(super) struct ApiError(StatusCode, String);

impl From<getman_core::Error> for ApiError {
    fn from(error: getman_core::Error) -> Self {
        use getman_core::Error;
        let status = match &error {
            Error::NotFound(_) => StatusCode::NOT_FOUND,
            Error::InvalidPath(_) | Error::Invalid(_) | Error::Yaml { .. } => {
                StatusCode::BAD_REQUEST
            }
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        Self(status, error.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

fn failure(status: StatusCode, message: impl Into<String>) -> ApiError {
    ApiError(status, message.into())
}

/// Opens the page's stream of events.
async fn events(State(state): Shared) -> Result<Response, ApiError> {
    let page = getman_core::dynamic::uuid_v4();
    let (lines, receiver) = mpsc::channel::<Bytes>(LINE_BUFFER);
    {
        let mut inner = state.realtime.lock();
        inner.pages.retain(|_, lines| !lines.is_closed());
        if inner.pages.len() >= MAX_PAGES {
            return Err(failure(
                StatusCode::TOO_MANY_REQUESTS,
                "Too many pages are connected to Getman; close some tabs",
            ));
        }
        inner.pages.insert(page.clone(), lines.clone());
    }
    let _ = lines.try_send(line(&json!({ "type": "ready", "page": page })));

    // Say something now and then, and let go of the page's connections
    // once nobody reads its stream.
    let watched = Arc::clone(&state);
    let watched_page = page.clone();
    tokio::spawn(async move {
        let mut beat = tokio::time::interval(HEARTBEAT);
        beat.tick().await;
        loop {
            tokio::select! {
                _ = beat.tick() => {
                    if lines.send(line(&json!({ "type": "ping" }))).await.is_err() {
                        break;
                    }
                }
                () = lines.closed() => break,
            }
        }
        watched.realtime.drop_page(&watched_page);
    });

    let stopping = state.realtime.stopping.subscribe();
    let stream = futures_util::stream::unfold(
        (receiver, stopping),
        |(mut receiver, mut stopping)| async move {
            if *stopping.borrow() {
                return None;
            }
            tokio::select! {
                bytes = receiver.recv() => bytes.map(|bytes| (Ok::<_, Infallible>(bytes), (receiver, stopping))),
                _ = stopping.changed() => None,
            }
        },
    );
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "application/x-ndjson")
        .header(header::CACHE_CONTROL, "no-cache")
        .header("x-page-id", page)
        .body(Body::from_stream(stream))
        .unwrap_or_default())
}

#[derive(Deserialize)]
struct ConnectArg {
    page: String,
    #[serde(default)]
    path: Option<String>,
    request: Request,
    #[serde(default)]
    environment: Option<String>,
    /// Save variables the scripts set to the local files.
    #[serde(default = "yes")]
    persist: bool,
}

fn yes() -> bool {
    true
}

/// Opens a connection whose events go to the page's stream.
async fn connect(
    State(state): Shared,
    Json(arg): Json<ConnectArg>,
) -> Result<Json<Value>, ApiError> {
    let lines = {
        let inner = state.realtime.lock();
        let lines = inner
            .pages
            .get(&arg.page)
            .filter(|lines| !lines.is_closed())
            .cloned()
            .ok_or_else(|| {
                failure(
                    StatusCode::NOT_FOUND,
                    "This page lost its connection to Getman; reload it",
                )
            })?;
        let open = inner
            .sessions
            .values()
            .filter(|s| s.page == arg.page)
            .count();
        if open >= MAX_SESSIONS {
            return Err(failure(
                StatusCode::TOO_MANY_REQUESTS,
                format!("A page may hold {MAX_SESSIONS} connections at once; disconnect some"),
            ));
        }
        lines
    };
    let mut context = Context {
        environment: arg.environment.filter(|e| !e.is_empty()),
        ..Context::default()
    };
    let path = arg.path.filter(|p| !p.is_empty());
    let mut execution = Execution {
        path: path.clone(),
        name: arg.request.name.clone(),
        ..Execution::default()
    };
    let ready = state
        .executor
        .before_connect(path.as_deref(), &arg.request, &mut context, &mut execution)
        .await?;
    let Some(ready) = ready else {
        let reason = execution
            .script_errors
            .first()
            .map(|e| format!("The script in {} failed: {}", e.origin, e.message))
            .or(execution.error.clone())
            .unwrap_or_else(|| "A pre-request script skipped the connection".into());
        return Err(failure(StatusCode::BAD_REQUEST, reason));
    };
    let session = realtime::connect_with(
        &state.executor,
        path.as_deref(),
        &ready.request,
        &context,
        Options { lossy: true },
        ready.headers,
    )?;
    let request = ready.request;
    let id = getman_core::dynamic::uuid_v4();
    let (control, mut events) = session.into_parts();
    state.realtime.lock().sessions.insert(
        id.clone(),
        Session {
            page: arg.page.clone(),
            control,
        },
    );

    let forward = Arc::clone(&state);
    let session_id = id.clone();
    let persist = arg.persist;
    tokio::spawn(async move {
        let mut recorder = Recorder::new();
        let mut listening = true;
        while let Some(event) = events.recv().await {
            recorder.record(&event);
            if !listening {
                continue;
            }
            let mut value = json!(event);
            value["session"] = Value::from(session_id.as_str());
            // Nobody reads the page's stream any more; the session ends
            // by itself, and the scripts still run over what it carried.
            listening = lines.send(line(&value)).await.is_ok();
        }
        forward.realtime.lock().sessions.remove(&session_id);

        let call = recorder.finish();
        let opened = call.opened;
        forward
            .executor
            .after_connect(
                path.as_deref(),
                &request,
                &mut context,
                call,
                &mut execution,
            )
            .await;
        if persist
            && let Err(e) = persist_changes(
                forward.workspace(),
                context.environment.as_deref(),
                &execution.changes,
            )
        {
            execution
                .warnings
                .push(format!("variables could not be saved: {e}"));
        }
        let ran = !(execution.tests.is_empty()
            && execution.checks.is_empty()
            && execution.logs.is_empty()
            && execution.script_errors.is_empty()
            && execution.changes.is_empty());
        if opened && ran && listening {
            let result = json!({
                "type": "result",
                "session": session_id,
                "tests": execution.tests,
                "checks": execution.checks,
                "logs": execution.logs,
                "script_errors": execution.script_errors,
                "warnings": execution.warnings,
                "changes": execution.changes,
            });
            let _ = lines.send(line(&result)).await;
        }
    });
    Ok(Json(json!({ "id": id })))
}

#[derive(Deserialize)]
struct SendArg {
    id: String,
    message: Outbound,
}

fn control(state: &AppState, id: &str) -> Result<Control, ApiError> {
    state
        .realtime
        .lock()
        .sessions
        .get(id)
        .map(|s| s.control.clone())
        .ok_or_else(|| failure(StatusCode::NOT_FOUND, "The connection is closed"))
}

/// Sends a message on a connection.
async fn send(State(state): Shared, Json(arg): Json<SendArg>) -> Result<Json<Value>, ApiError> {
    control(&state, &arg.id)?.send(arg.message)?;
    Ok(Json(json!({})))
}

#[derive(Deserialize)]
struct CloseArg {
    id: String,
}

/// Closes a connection. Closing one that already closed is not an error.
async fn close(State(state): Shared, Json(arg): Json<CloseArg>) -> Json<Value> {
    if let Ok(control) = control(&state, &arg.id) {
        control.close();
    }
    Json(json!({}))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use axum::http::Request as HttpRequest;
    use futures_util::StreamExt as _;
    use getman_core::Workspace;
    use getman_core::execute::Executor;
    use tower::ServiceExt as _;

    use super::*;

    fn app() -> (tempfile::TempDir, Arc<AppState>, Router) {
        let dir = tempfile::tempdir().unwrap();
        let workspace = Workspace::init(dir.path(), "Test").unwrap();
        let state = Arc::new(AppState {
            history: super::super::history::History::open(&workspace),
            executor: Executor::new(workspace),
            token: "secret-token".into(),
            port: 7878,
            runs: Mutex::new(HashMap::new()),
            mocks: Mutex::new(HashMap::new()),
            sends: Mutex::new(HashMap::new()),
            realtime: Hub::default(),
            grpc: super::super::grpc::Calls::default(),
        });
        let router = super::super::router(Arc::clone(&state));
        (dir, state, router)
    }

    fn post(path: &str, body: &Value) -> HttpRequest<Body> {
        HttpRequest::post(path)
            .header("host", "127.0.0.1:7878")
            .header("x-getman-token", "secret-token")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    async fn call(router: &Router, path: &str, body: &Value) -> (StatusCode, Value) {
        let response = router.clone().oneshot(post(path, body)).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or_default())
    }

    /// Reads a page's stream line by line.
    struct Lines {
        body: axum::body::BodyDataStream,
        buffer: String,
    }

    impl Lines {
        async fn until(&mut self, done: impl Fn(&Value) -> bool) -> Value {
            let read = tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    if let Some(at) = self.buffer.find('\n') {
                        let line: String = self.buffer.drain(..=at).collect();
                        let value: Value = serde_json::from_str(&line).unwrap();
                        if done(&value) {
                            return value;
                        }
                        continue;
                    }
                    let chunk = self.body.next().await.unwrap().unwrap();
                    self.buffer.push_str(std::str::from_utf8(&chunk).unwrap());
                }
            });
            read.await.expect("the line never came")
        }
    }

    /// A server sending an event every 20 ms.
    async fn ticking() -> String {
        let app = Router::new().route(
            "/ticks",
            axum::routing::get(|| async {
                let ticks = futures_util::stream::unfold(0_u32, |n| async move {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    Some((Ok::<_, Infallible>(format!("data: tick {n}\n\n")), n + 1))
                });
                Response::builder()
                    .header(header::CONTENT_TYPE, "text/event-stream")
                    .body(Body::from_stream(ticks))
                    .unwrap()
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}/ticks")
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_page_streams_its_connections_until_it_goes_away() {
        let url = ticking().await;
        let (_dir, state, router) = app();
        let response = router
            .clone()
            .oneshot(post("/api/realtime/events", &json!({})))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let page = response.headers()["x-page-id"].to_str().unwrap().to_owned();
        let mut lines = Lines {
            body: response.into_body().into_data_stream(),
            buffer: String::new(),
        };
        let ready = lines.until(|_| true).await;
        assert_eq!(ready, json!({ "type": "ready", "page": page }));

        let request = json!({ "kind": "sse", "url": url });
        let (status, opened) = call(
            &router,
            "/api/realtime/connect",
            &json!({ "page": page, "request": request }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{opened}");
        let id = opened["id"].as_str().unwrap().to_owned();
        let tick = lines.until(|v| v["type"] == "message").await;
        assert_eq!(tick["session"], id.as_str());
        assert_eq!(tick["direction"], "received");
        assert!(tick["data"].as_str().unwrap().starts_with("tick "));

        let (status, _) = call(
            &router,
            "/api/realtime/send",
            &json!({ "id": id, "message": { "message": { "content": "hi" } } }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        lines.until(|v| v["type"] == "warning").await;
        call(&router, "/api/realtime/close", &json!({ "id": id })).await;
        let closed = lines.until(|v| v["type"] == "closed").await;
        assert_eq!(closed["by_client"], true);
        let (status, _) = call(
            &router,
            "/api/realtime/send",
            &json!({ "id": id, "message": { "message": { "content": "hi" } } }),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        // An HTTP request cannot be connected to.
        let (status, error) = call(
            &router,
            "/api/realtime/connect",
            &json!({ "page": page, "request": { "url": url } }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(
            error["error"]
                .as_str()
                .unwrap()
                .contains("not connected to")
        );

        // When the page goes away, so do its connections.
        let (_, opened) = call(
            &router,
            "/api/realtime/connect",
            &json!({ "page": page, "request": request }),
        )
        .await;
        lines.until(|v| v["type"] == "message").await;
        drop(lines);
        let left = tokio::time::timeout(Duration::from_secs(5), async {
            while !state.realtime.lock().sessions.is_empty() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await;
        assert!(left.is_ok(), "the connection outlived its page");
        assert!(!state.realtime.lock().pages.contains_key(&page));
        let (status, _) = call(
            &router,
            "/api/realtime/close",
            &json!({ "id": opened["id"] }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (status, _) = call(
            &router,
            "/api/realtime/connect",
            &json!({ "page": page, "request": request }),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn scripts_and_checks_run_around_a_connection() {
        let url = ticking().await;
        let (dir, state, router) = app();
        let response = router
            .clone()
            .oneshot(post("/api/realtime/events", &json!({})))
            .await
            .unwrap();
        let page = response.headers()["x-page-id"].to_str().unwrap().to_owned();
        let mut lines = Lines {
            body: response.into_body().into_data_stream(),
            buffer: String::new(),
        };
        lines.until(|v| v["type"] == "ready").await;

        let request = json!({
            "kind": "sse",
            "url": format!("{url}?{{{{stamp}}}}"),
            "scripts": {
                "pre_request": "gm.variables.set('stamp', 'from-pre'); gm.request.headers.add({ key: 'X-Pre', value: 'yes' });",
                "post_response": "gm.test('ticks arrived', () => gm.expect(gm.messages.length).to.be.above(1)); gm.collectionVariables.set('first', gm.messages[0].data); console.log('done');",
            },
            "checks": ["status == 200", "messages.length > 1", "messages[0].data == tick 0"],
            "until": { "messages": 3 },
        });
        let (status, opened) = call(
            &router,
            "/api/realtime/connect",
            &json!({ "page": page, "request": request }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{opened}");

        // The page reads the connection as before, and then the results.
        let connecting = lines.until(|v| v["type"] == "connecting").await;
        assert!(connecting["url"].as_str().unwrap().ends_with("?from-pre"));
        let header = |name: &str| {
            connecting["headers"]
                .as_array()
                .unwrap()
                .iter()
                .any(|h| h["name"] == name)
        };
        assert!(header("X-Pre"), "{connecting}");

        // The connection runs until the page closes it; scripts run then.
        lines.until(|v| v["type"] == "message").await;
        lines.until(|v| v["type"] == "message").await;
        lines.until(|v| v["type"] == "message").await;
        call(
            &router,
            "/api/realtime/close",
            &json!({ "id": opened["id"] }),
        )
        .await;
        let result = lines.until(|v| v["type"] == "result").await;
        assert_eq!(result["session"], opened["id"]);
        assert_eq!(result["tests"][0]["name"], "ticks arrived");
        assert_eq!(result["tests"][0]["passed"], true, "{result}");
        let checks = result["checks"].as_array().unwrap();
        assert_eq!(checks.len(), 3);
        assert!(checks.iter().all(|c| c["passed"] == true), "{result}");
        assert_eq!(result["logs"][0]["message"], "done");

        // What the post-response script set is saved with the local variables.
        let local = state.workspace().read_local_state().unwrap();
        assert_eq!(
            local.variables.get("first").map(String::as_str),
            Some("tick 0")
        );

        // A pre-request script that fails stops the connection, and says why.
        let broken = json!({
            "kind": "sse",
            "url": url,
            "scripts": { "pre_request": "throw new Error('nope');" },
        });
        let (status, error) = call(
            &router,
            "/api/realtime/connect",
            &json!({ "page": page, "request": broken }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let message = error["error"].as_str().unwrap();
        assert!(
            message.contains("nope") && message.contains("script"),
            "{message}"
        );
        drop(dir);
    }

    #[tokio::test]
    async fn the_stream_needs_the_token() {
        let (_dir, _state, router) = app();
        let mut request = post("/api/realtime/events", &json!({}));
        request.headers_mut().remove("x-getman-token");
        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
