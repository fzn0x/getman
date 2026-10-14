//! The local server behind the browser UI.
//!
//! It listens on 127.0.0.1 only and guards against other web pages: every
//! request must name a loopback host (stopping DNS rebinding), requests
//! from other origins are refused, and API calls must carry a token that
//! only the UI opened from the terminal knows. Without these checks any
//! website could use the server to send requests or read files.

mod api;
mod assets;
mod history;
mod token;

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
use getman_core::execute::Executor;

use crate::style;

/// Port the UI is served on unless another is asked for or it is taken.
pub(crate) const DEFAULT_PORT: u16 = 7878;

/// Shared by every request the server handles.
pub(crate) struct AppState {
    pub(crate) executor: Executor,
    pub(crate) token: String,
    pub(crate) port: u16,
    pub(crate) runs: Mutex<HashMap<String, Arc<AtomicBool>>>,
    pub(crate) history: history::History,
}

impl AppState {
    pub(crate) fn workspace(&self) -> &Workspace {
        &self.executor.workspace
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
    let state = Arc::new(AppState {
        history: history::History::open(&workspace),
        executor: Executor::new(workspace),
        token,
        port,
        runs: Mutex::new(HashMap::new()),
    });
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
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .map_err(|e| e.to_string())
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
    Router::new()
        .nest("/api", api::routes())
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
}
