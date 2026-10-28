//! The local server behind the browser UI.
//!
//! It listens on 127.0.0.1 only and guards against other web pages: every
//! request must name a loopback host (stopping DNS rebinding), requests
//! from other origins are refused, and API calls must carry a token that
//! only the UI opened from the terminal knows. Without these checks any
//! website could use the server to send requests or read files.
//!
//! `getman` runs it in the foreground with `serve`; the desktop app runs
//! it in the background of its own process with [`Server`].

mod api;
mod assets;
mod background;
mod grpc;
mod history;
mod presets;
mod realtime;
mod token;

pub use background::{Options, Server};

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use getman_core::Workspace;
use getman_core::execute::{Cancel, Executor};

use crate::style;

/// Port the UI is served on unless another is asked for or it is taken.
pub(crate) const DEFAULT_PORT: u16 = 7878;

/// Shared by every request the server handles.
pub(crate) struct AppState {
    pub(crate) executor: Executor,
    pub(crate) token: String,
    pub(crate) port: u16,
    pub(crate) runs: Mutex<HashMap<String, Arc<AtomicBool>>>,
    /// Requests being sent from the UI, by the id it gave them.
    pub(crate) sends: Mutex<HashMap<String, Cancel>>,
    pub(crate) history: history::History,
    /// Mock servers started from the UI, by folder.
    pub(crate) mocks: Mutex<HashMap<String, crate::mock::Mock>>,
    /// Live WebSocket, SSE and Socket.IO connections of the browser UI.
    pub(crate) realtime: realtime::Hub,
    /// Live gRPC calls of the browser UI.
    pub(crate) grpc: grpc::Calls,
}

impl AppState {
    pub(crate) fn workspace(&self) -> &Workspace {
        &self.executor.workspace
    }

    /// Ends what this state has under way, for when the server stops or
    /// serves another workspace: live connections and gRPC calls (their
    /// event streams never end by themselves), requests being sent, and
    /// runs.
    pub(crate) fn shutdown(&self) {
        self.realtime.close_all();
        self.grpc.cancel_all();
        for cancel in self.sends.lock().expect("sends lock").values() {
            cancel.cancel();
        }
        for run in self.runs.lock().expect("runs lock").values() {
            run.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

/// Serves the UI until Ctrl+C.
pub(crate) async fn serve(
    workspace: Workspace,
    port: Option<u16>,
    open_browser: bool,
) -> Result<(), String> {
    let token = token::load_or_create()?;
    let listener = bind(port).await?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let state = state(workspace, token, port);
    let app = router(Arc::clone(&state));

    let address = format!("http://127.0.0.1:{port}/");
    let with_token = format!("{address}#token={}", state.token);
    anstream::println!(
        "{} {} at {}",
        style::paint(style::CYAN.bold(), "getman"),
        style::paint(style::BOLD, &state.workspace().name()),
        style::paint(style::BOLD, &address)
    );
    style::note(&format!(
        "Workspace: {}",
        state.workspace().root().display()
    ));
    if !assets::built() {
        style::warn(
            "this build has no browser UI; build it with `npm run build` in web/, then rebuild getman",
        );
    }
    if open_browser && open::that_detached(&with_token).is_ok() {
        style::note(&format!(
            "Opened {with_token} in your browser. Press Ctrl+C to stop."
        ));
    } else {
        style::note(&format!(
            "Open {with_token} in your browser. Press Ctrl+C to stop."
        ));
    }

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            // Live connections' event streams never end by themselves.
            state.shutdown();
        })
        .await
        .map_err(|e| e.to_string())
}

/// The state of a server on `port` for `workspace`.
fn state(workspace: Workspace, token: String, port: u16) -> Arc<AppState> {
    Arc::new(AppState {
        history: history::History::open(&workspace),
        executor: Executor::with_saved_cookies(workspace),
        token,
        port,
        runs: Mutex::new(HashMap::new()),
        mocks: Mutex::new(HashMap::new()),
        sends: Mutex::new(HashMap::new()),
        realtime: realtime::Hub::default(),
        grpc: grpc::Calls::default(),
    })
}

async fn bind(port: Option<u16>) -> Result<tokio::net::TcpListener, String> {
    let localhost = |port| SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    if let Some(port) = port {
        return tokio::net::TcpListener::bind(localhost(port))
            .await
            .map_err(|e| format!("cannot listen on port {port}: {e}"));
    }
    for port in DEFAULT_PORT..DEFAULT_PORT + 20 {
        if let Ok(listener) = tokio::net::TcpListener::bind(localhost(port)).await {
            return Ok(listener);
        }
    }
    tokio::net::TcpListener::bind(localhost(0))
        .await
        .map_err(|e| format!("cannot listen on 127.0.0.1: {e}"))
}

pub(crate) fn router(state: Arc<AppState>) -> Router {
    router_with(state, None)
}

/// The router, with the routes of a program that embeds the server served
/// under `/api/app/`, behind the same guard and token as the rest.
fn router_with(state: Arc<AppState>, app: Option<&Router>) -> Router {
    let mut router = Router::new().nest(
        "/api",
        api::routes()
            .merge(realtime::routes())
            .merge(grpc::routes()),
    );
    if let Some(app) = app {
        router = router.nest_service("/api/app", app.clone());
    }
    router
        .fallback(assets::serve)
        .layer(middleware::from_fn_with_state(Arc::clone(&state), guard))
        .with_state(state)
}

/// Refuses requests that do not come from the UI on this machine, and adds
/// security headers to every response.
async fn guard(State(state): State<Arc<AppState>>, request: Request, next: Next) -> Response {
    let allowed = [
        format!("127.0.0.1:{}", state.port),
        format!("localhost:{}", state.port),
        format!("[::1]:{}", state.port),
    ];
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or_default();
    if !allowed.iter().any(|a| a.eq_ignore_ascii_case(host)) {
        return refuse("Getman only answers requests addressed to 127.0.0.1 or localhost");
    }
    if let Some(origin) = request.headers().get(header::ORIGIN) {
        let origin = origin.to_str().unwrap_or_default();
        let same = allowed
            .iter()
            .any(|a| origin.eq_ignore_ascii_case(&format!("http://{a}")));
        if !same {
            return refuse("Getman does not answer requests from other web pages");
        }
    }
    if request.uri().path().starts_with("/api/") {
        let given = request
            .headers()
            .get("x-getman-token")
            .and_then(|h| h.to_str().ok())
            .unwrap_or_default();
        if !token::matches(given, &state.token) {
            return (
                StatusCode::UNAUTHORIZED,
                axum::Json(serde_json::json!({
                    "error": "This page is not signed in to Getman. Open the link Getman printed in your terminal.",
                    "code": "token",
                })),
            )
                .into_response();
        }
    }
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        "cross-origin-resource-policy",
        HeaderValue::from_static("same-origin"),
    );
    response
}

fn refuse(message: &str) -> Response {
    Response::builder()
        .status(StatusCode::FORBIDDEN)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from(message.to_owned()))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request as HttpRequest;
    use tower::ServiceExt as _;

    fn app() -> (tempfile::TempDir, Router) {
        let dir = tempfile::tempdir().unwrap();
        let workspace = Workspace::init(dir.path(), "Test").unwrap();
        let state = Arc::new(AppState {
            history: history::History::open(&workspace),
            executor: Executor::new(workspace),
            token: "secret-token".into(),
            port: 7878,
            runs: Mutex::new(HashMap::new()),
            mocks: Mutex::new(HashMap::new()),
            sends: Mutex::new(HashMap::new()),
            realtime: realtime::Hub::default(),
            grpc: grpc::Calls::default(),
        });
        (dir, router(state))
    }

    fn get(path: &str, host: &str) -> HttpRequest<Body> {
        HttpRequest::get(path)
            .header("host", host)
            .body(Body::empty())
            .unwrap()
    }

    #[tokio::test]
    async fn other_hosts_are_refused() {
        let (_dir, app) = app();
        let response = app.oneshot(get("/", "evil.example:7878")).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn the_api_needs_the_token() {
        let (_dir, app) = app();
        let response = app
            .clone()
            .oneshot(get("/api/workspace", "127.0.0.1:7878"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let mut request = get("/api/workspace", "localhost:7878");
        request
            .headers_mut()
            .insert("x-getman-token", HeaderValue::from_static("secret-token"));
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["x-frame-options"], "DENY");
    }

    #[tokio::test]
    async fn other_origins_are_refused_even_with_the_token() {
        let (_dir, app) = app();
        let mut request = get("/api/workspace", "127.0.0.1:7878");
        request
            .headers_mut()
            .insert("x-getman-token", HeaderValue::from_static("secret-token"));
        request
            .headers_mut()
            .insert("origin", HeaderValue::from_static("https://evil.example"));
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    /// Calls the API as the UI does, with GET when there is no body, and
    /// returns the JSON answer.
    async fn call(app: &Router, path: &str, body: &str) -> (StatusCode, serde_json::Value) {
        let builder = if body.is_empty() {
            HttpRequest::get(path)
        } else {
            HttpRequest::post(path)
        };
        let request = builder
            .header("host", "127.0.0.1:7878")
            .header("x-getman-token", "secret-token")
            .header("content-type", "application/json")
            .body(Body::from(body.to_owned()))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or_default())
    }

    #[tokio::test]
    async fn deleting_moves_items_to_the_trash() {
        let (_dir, app) = app();
        let (_, created) = call(&app, "/api/request/create", r#"{"name":"Ping"}"#).await;
        assert_eq!(created["path"], "ping.yaml");
        let (status, item) = call(&app, "/api/item/delete", r#"{"path":"ping.yaml"}"#).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(item["name"], "Ping");
        let (_, tree) = call(&app, "/api/tree", "").await;
        assert_eq!(tree, serde_json::json!([]));

        let id = item["id"].as_str().unwrap();
        let (_, restored) = call(&app, "/api/trash/restore", &format!(r#"{{"id":"{id}"}}"#)).await;
        assert_eq!(restored["path"], "ping.yaml");
        let (status, _) = call(&app, "/api/trash/empty", "{}").await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn variables_set_from_the_ui_land_in_local_files_and_secrets_are_marked() {
        let (_dir, app) = app();
        let (_, env) = call(&app, "/api/environment/create", r#"{"name":"Dev"}"#).await;
        assert_eq!(env["id"], "dev");
        let files = r#"{"id":"dev","environment":{"shared":{"variables":{"token":""},"secrets":["token"]},"local":{"token":"s3cret"}}}"#;
        call(&app, "/api/environment/write", files).await;

        let set = r#"{"scope":"environment","name":"id","value":"42","environment":"dev"}"#;
        let (status, _) = call(&app, "/api/variables/set", set).await;
        assert_eq!(status, StatusCode::OK);
        let (_, read) = call(&app, "/api/environment/read", r#"{"id":"dev"}"#).await;
        assert_eq!(read["local"]["id"], "42");

        let ask = r#"{"request":{"name":"x","method":"GET","url":""},"environment":"dev"}"#;
        let (_, vars) = call(&app, "/api/variables", ask).await;
        let token = vars
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["name"] == "token")
            .unwrap();
        assert_eq!(token["secret"], true);

        let nowhere = r#"{"scope":"environment","name":"id","value":"42"}"#;
        let (status, _) = call(&app, "/api/variables/set", nowhere).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let global = r#"{"scope":"global","name":"who","value":"me"}"#;
        call(&app, "/api/variables/set", global).await;
        let (_, globals) = call(&app, "/api/globals", "").await;
        assert_eq!(globals["globals"]["who"], "me");
    }

    #[tokio::test]
    async fn dropped_folders_are_imported() {
        let (dir, app) = app();
        let body = serde_json::json!({
            "files": [
                { "path": "shop/bruno.json", "text": "{\"name\": \"Shop\"}" },
                { "path": "shop/ping.bru", "text": "meta {\n  name: Ping\n}\n\nget {\n  url: https://x.test/ping\n}\n" },
            ],
        });
        let request = HttpRequest::post("/api/import/files")
            .header("host", "127.0.0.1:7878")
            .header("x-getman-token", "secret-token")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(dir.path().join("shop/ping.yaml").is_file());
    }

    #[tokio::test]
    async fn the_page_is_served_without_the_token() {
        let (_dir, app) = app();
        let response = app.oneshot(get("/", "127.0.0.1:7878")).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response.headers()["content-security-policy"]
                .to_str()
                .unwrap()
                .contains("default-src 'self'")
        );
    }

    /// A call to the API with the token, and JSON when a body is given.
    pub(super) fn api(path: &str, body: Option<serde_json::Value>) -> HttpRequest<Body> {
        let builder = HttpRequest::builder()
            .method(if body.is_some() { "POST" } else { "GET" })
            .uri(path)
            .header("host", "127.0.0.1:7878")
            .header("x-getman-token", "secret-token");
        match body {
            Some(body) => builder
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
            None => builder.body(Body::empty()).unwrap(),
        }
    }

    pub(super) async fn read(response: Response) -> (StatusCode, serde_json::Value) {
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or_default())
    }

    #[tokio::test]
    async fn runs_are_kept_and_give_reports() {
        use getman_core::model::{Request, Scripts};
        use serde_json::json;

        let (dir, app) = app();
        let workspace = Workspace::open(dir.path()).unwrap();
        let skipped = Request {
            url: "http://127.0.0.1:9/".into(),
            scripts: Scripts {
                pre_request: "gm.execution.skipRequest();".into(),
                post_response: String::new(),
            },
            ..Request::default()
        };
        workspace.create_request("", "Skip me", &skipped).unwrap();
        workspace.create_request("", "Me too", &skipped).unwrap();

        let response = app
            .clone()
            .oneshot(api(
                "/api/run",
                Some(json!({ "targets": ["me-too.yaml", "skip-me.yaml"] })),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let id = response.headers()["x-run-id"].to_str().unwrap().to_owned();
        let lines = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert!(String::from_utf8_lossy(&lines).contains("\"finish\""));

        let (_, list) = read(app.clone().oneshot(api("/api/runs", None)).await.unwrap()).await;
        assert_eq!(list[0]["id"], id.as_str());
        assert_eq!(list[0]["requests"], 2);
        let (_, run) = read(
            app.clone()
                .oneshot(api("/api/runs/read", Some(json!({ "id": id }))))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(run["results"][0]["execution"]["name"], "Me too");
        assert_eq!(run["items"][1]["path"], "skip-me.yaml");

        let response = app
            .clone()
            .oneshot(api(
                "/api/runs/report",
                Some(json!({ "id": id, "format": "csv" })),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response.headers()["content-disposition"]
                .to_str()
                .unwrap()
                .ends_with(".csv\"")
        );
        let csv = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert!(String::from_utf8_lossy(&csv).contains(",skipped,"));

        let (status, _) = read(
            app.clone()
                .oneshot(api(
                    "/api/runs/report",
                    Some(json!({ "id": id, "format": "pdf" })),
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, _) = read(
            app.clone()
                .oneshot(api("/api/runs/read", Some(json!({ "id": "../x" }))))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn examples_are_saved_listed_and_kept_on_save() {
        use serde_json::json;

        let (dir, app) = app();
        let workspace = Workspace::open(dir.path()).unwrap();
        let path = workspace
            .create_request("", "Get user", &getman_core::model::Request::default())
            .unwrap();
        let call = |route: &str, body: serde_json::Value| {
            let app = app.clone();
            let route = route.to_owned();
            async move { read(app.oneshot(api(&route, Some(body))).await.unwrap()).await }
        };

        let (status, list) = call(
            "/api/example/save",
            json!({
                "path": path,
                "example": {
                    "name": "Found",
                    "status": 200,
                    "headers": [
                        { "name": "Content-Type", "value": "image/png" },
                        { "name": "Set-Cookie", "value": "session=s3cret" }
                    ],
                    "request": { "method": "GET", "url": "{{baseUrl}}/users/:id" }
                },
                "body": { "base64": "iVBORw0K" }
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{list}");
        assert_eq!(list[0]["body_file"], "found.png");
        assert_eq!(list[0]["headers"].as_array().unwrap().len(), 1);

        let (_, body) = call(
            "/api/example/body",
            json!({ "path": path, "name": "Found" }),
        )
        .await;
        assert_eq!(body["body"]["base64"], "iVBORw0K");

        // Saving the request from the editor keeps its examples.
        let (status, _) = call(
            "/api/request/write",
            json!({ "path": path, "request": { "name": "Get user", "method": "GET", "url": "x" } }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(workspace.read_request(&path).unwrap().examples.len(), 1);

        let (_, list) = call(
            "/api/example/rename",
            json!({ "path": path, "from": "Found", "to": "Here" }),
        )
        .await;
        assert_eq!(list[0]["name"], "Here");
        let (_, list) = call(
            "/api/example/delete",
            json!({ "path": path, "name": "Here" }),
        )
        .await;
        assert_eq!(list, json!([]));
        let (status, _) = call(
            "/api/example/delete",
            json!({ "path": path, "name": "Here" }),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn mock_servers_start_and_stop_per_folder() {
        use getman_core::http::{HttpClient, Outgoing, OutgoingBody, Transport};
        use getman_core::model::{Example, Request};
        use serde_json::json;

        let (dir, app) = app();
        let workspace = Workspace::open(dir.path()).unwrap();
        let folder = workspace.create_folder("", "Users").unwrap();
        workspace
            .create_request(
                &folder,
                "Me",
                &Request {
                    url: "{{baseUrl}}/me".into(),
                    examples: vec![Example {
                        name: "Ada".into(),
                        body: "ada".into(),
                        ..Example::default()
                    }],
                    ..Request::default()
                },
            )
            .unwrap();

        let (status, started) = read(
            app.clone()
                .oneshot(api(
                    "/api/mock/start",
                    Some(json!({ "folder": folder, "port": 0 })),
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{started}");
        assert_eq!(started["mocked"][0]["path"], "/me");
        let url = started["url"].as_str().unwrap().to_owned();

        let outgoing = Outgoing {
            method: "GET".into(),
            url: format!("{url}/me"),
            headers: Vec::new(),
            body: OutgoingBody::None,
        };
        let answer = HttpClient::new()
            .send(&outgoing, Transport::default())
            .await
            .unwrap();
        assert_eq!(answer.text(), "ada");

        let (_, list) = read(app.clone().oneshot(api("/api/mocks", None)).await.unwrap()).await;
        assert_eq!(list[0]["folder"], "users");
        assert_eq!(list[0]["hits"][0]["path"], "/me");

        let (status, _) = read(
            app.clone()
                .oneshot(api("/api/mock/start", Some(json!({ "folder": "nope" }))))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        read(
            app.clone()
                .oneshot(api("/api/mock/stop", Some(json!({ "folder": "users" }))))
                .await
                .unwrap(),
        )
        .await;
        let (_, list) = read(app.clone().oneshot(api("/api/mocks", None)).await.unwrap()).await;
        assert_eq!(list, json!([]));
    }

    async fn call_json(app: &Router, path: &str, body: serde_json::Value) -> serde_json::Value {
        let request = HttpRequest::post(path)
            .header("host", "127.0.0.1:7878")
            .header("x-getman-token", "secret-token")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_send_can_be_cancelled() {
        let (_dir, app) = app();
        // A server that accepts the connection and never answers.
        let silent = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = silent.local_addr().unwrap();
        tokio::spawn(async move {
            let mut open = Vec::new();
            while let Ok((stream, _)) = silent.accept().await {
                open.push(stream);
            }
        });
        let sending = {
            let app = app.clone();
            tokio::spawn(async move {
                call_json(
                    &app,
                    "/api/send",
                    serde_json::json!({
                        "id": "send-1",
                        "request": { "url": format!("http://{address}/") },
                    }),
                )
                .await
            })
        };
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        call_json(
            &app,
            "/api/send/cancel",
            serde_json::json!({ "id": "send-1" }),
        )
        .await;
        let execution = tokio::time::timeout(std::time::Duration::from_secs(5), sending)
            .await
            .expect("the send stopped")
            .unwrap();
        assert_eq!(execution["cancelled"], true, "{execution}");
        assert_eq!(execution["error"], "The request was cancelled.");
    }

    #[tokio::test]
    async fn cookies_can_be_added_and_renamed() {
        let (_dir, app) = app();
        let cookie = |name: &str, value: &str| {
            serde_json::json!({
                "domain": "api.example.com",
                "path": "/",
                "name": name,
                "value": value,
                "host_only": true,
            })
        };
        call_json(
            &app,
            "/api/cookies/set",
            serde_json::json!({ "cookie": cookie("a", "1") }),
        )
        .await;
        call_json(
            &app,
            "/api/cookies/set",
            serde_json::json!({
                "cookie": cookie("b", "2"),
                "previous": { "domain": "api.example.com", "path": "/", "name": "a" },
            }),
        )
        .await;
        let request = {
            let mut request = get("/api/cookies", "127.0.0.1:7878");
            request
                .headers_mut()
                .insert("x-getman-token", HeaderValue::from_static("secret-token"));
            request
        };
        let response = app.clone().oneshot(request).await.unwrap();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let cookies: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(cookies.as_array().unwrap().len(), 1, "{cookies}");
        assert_eq!(cookies[0]["name"], "b");
        assert_eq!(cookies[0]["value"], "2");

        let refused = call_json(
            &app,
            "/api/cookies/set",
            serde_json::json!({ "cookie": { "domain": "", "name": "x" } }),
        )
        .await;
        assert!(refused["error"].as_str().unwrap().contains("domain"));
    }
}
