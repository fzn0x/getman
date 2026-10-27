//! The mock server: answers requests with the saved examples of a folder
//! or the whole workspace, on 127.0.0.1.
//!
//! `getman mock` runs one in the foreground, and the browser UI starts and
//! stops them per folder. The examples are read again at most once a
//! second, so an example saved or edited while the server runs is served
//! right away.

use std::collections::VecDeque;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderName, HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use getman_core::Workspace;
use getman_core::mock::{Answer, EXAMPLE_HEADER, Routes};
use serde::Serialize;
use serde_json::json;

/// Port a mock server listens on unless another is asked for or it is
/// taken.
pub(crate) const DEFAULT_PORT: u16 = 7979;

/// How many recent requests a running mock remembers.
const HITS_KEPT: usize = 50;

/// A request the mock server answered.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Hit {
    /// Milliseconds since the Unix epoch.
    pub(crate) at: i64,
    pub(crate) method: String,
    /// Path and query, as asked.
    pub(crate) path: String,
    pub(crate) status: u16,
    /// The request file whose example answered.
    pub(crate) request: Option<String>,
    /// The example that answered.
    pub(crate) example: Option<String>,
}

type Log = Box<dyn Fn(&Hit) + Send + Sync>;

struct MockState {
    workspace: Workspace,
    folder: String,
    port: u16,
    delay: Duration,
    routes: Mutex<(Instant, Arc<Routes>)>,
    hits: Arc<Mutex<VecDeque<Hit>>>,
    log: Log,
}

impl MockState {
    /// The routes, read again when they are more than a second old.
    fn routes(&self) -> Arc<Routes> {
        let mut cache = self.routes.lock().expect("mock routes lock");
        if cache.0.elapsed() >= Duration::from_secs(1) {
            if let Ok(routes) = Routes::load(&self.workspace, &self.folder) {
                cache.1 = Arc::new(routes);
            }
            cache.0 = Instant::now();
        }
        Arc::clone(&cache.1)
    }
}

/// A mock server running in the background.
pub(crate) struct Mock {
    pub(crate) folder: String,
    pub(crate) port: u16,
    pub(crate) delay_ms: u64,
    hits: Arc<Mutex<VecDeque<Hit>>>,
    state: Arc<MockState>,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
}

impl Mock {
    /// The address requests go to.
    pub(crate) fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// The most recent requests, newest first.
    pub(crate) fn hits(&self) -> Vec<Hit> {
        self.hits
            .lock()
            .expect("mock hits lock")
            .iter()
            .cloned()
            .collect()
    }

    /// What it serves.
    pub(crate) fn mocked(&self) -> Vec<getman_core::mock::Mocked> {
        self.state.routes().summary()
    }

    /// Stops the server.
    pub(crate) fn stop(mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
    }
}

impl Drop for Mock {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
    }
}

/// Listens on `port` on 127.0.0.1, or else on the first free port from
/// [`DEFAULT_PORT`].
pub(crate) async fn bind(port: Option<u16>) -> Result<tokio::net::TcpListener, String> {
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

/// Starts serving a folder's examples in the background. Fails when the
/// folder does not exist.
pub(crate) fn start(
    workspace: Workspace,
    folder: &str,
    listener: tokio::net::TcpListener,
    delay: Duration,
    log: Log,
) -> Result<Mock, String> {
    let routes = Routes::load(&workspace, folder).map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let hits = Arc::new(Mutex::new(VecDeque::new()));
    let state = Arc::new(MockState {
        workspace,
        folder: folder.to_owned(),
        port,
        delay,
        routes: Mutex::new((Instant::now(), Arc::new(routes))),
        hits: Arc::clone(&hits),
        log,
    });
    let app = Router::new()
        .fallback(answer)
        .with_state(Arc::clone(&state));
    let (shutdown, stopped) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await;
    });
    Ok(Mock {
        folder: folder.to_owned(),
        port,
        delay_ms: u64::try_from(delay.as_millis()).unwrap_or(u64::MAX),
        hits,
        state,
        shutdown: Some(shutdown),
    })
}

async fn answer(State(state): State<Arc<MockState>>, request: Request) -> Response {
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
        return (
            StatusCode::FORBIDDEN,
            "Getman's mock server only answers requests addressed to 127.0.0.1 or localhost",
        )
            .into_response();
    }

    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let query = request.uri().query().map(str::to_owned);
    let wanted = request
        .headers()
        .get(EXAMPLE_HEADER)
        .and_then(|h| h.to_str().ok())
        .map(str::to_owned);
    let shown_path = match &query {
        Some(query) => format!("{path}?{query}"),
        None => path.clone(),
    };

    let routes = state.routes();
    let found = routes.find(method.as_str(), &path, query.as_deref(), wanted.as_deref());
    if !state.delay.is_zero() {
        tokio::time::sleep(state.delay).await;
    }
    let (response, request_path, example) = match found {
        Answer::Example(route) => {
            let response = match getman_core::examples::body(
                &state.workspace,
                &route.request,
                &route.example,
            ) {
                Ok(body) => reply(&route.example, body, &method),
                Err(e) => json_reply(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &json!({
                        "error": format!("The body of the example {:?} cannot be read: {e}", route.example.name),
                    }),
                ),
            };
            (
                response,
                Some(route.request.clone()),
                Some(route.example.name.clone()),
            )
        }
        Answer::NoSuchExample { name, available } => (
            json_reply(
                StatusCode::NOT_FOUND,
                &json!({
                    "error": format!("No example named {name:?} answers {method} {path}. Name one of these in the {EXAMPLE_HEADER} header, or leave the header out."),
                    "examples": available,
                }),
            ),
            None,
            None,
        ),
        Answer::NotFound => (
            json_reply(
                StatusCode::NOT_FOUND,
                &json!({
                    "error": format!("No example answers {method} {path}."),
                    "mocked": routes.summary(),
                }),
            ),
            None,
            None,
        ),
    };

    let hit = Hit {
        at: getman_core::time::now_millis(),
        method: method.to_string(),
        path: shown_path,
        status: response.status().as_u16(),
        request: request_path,
        example,
    };
    (state.log)(&hit);
    let mut hits = state.hits.lock().expect("mock hits lock");
    hits.push_front(hit);
    hits.truncate(HITS_KEPT);
    response
}

/// Headers that describe how the original response travelled, not what
/// it was, and would be wrong for the body as stored.
const SKIPPED_HEADERS: &[&str] = &[
    "connection",
    "content-encoding",
    "content-length",
    "keep-alive",
    "transfer-encoding",
    "upgrade",
];

/// The example's status, headers and body.
fn reply(example: &getman_core::model::Example, body: Vec<u8>, method: &Method) -> Response {
    let status = StatusCode::from_u16(example.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let mut builder = Response::builder().status(status);
    let mut has_type = false;
    for header in example.headers.iter().filter(|h| h.enabled) {
        let name = header.name.trim().to_ascii_lowercase();
        if SKIPPED_HEADERS.contains(&name.as_str()) {
            continue;
        }
        let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(header.value.trim()),
        ) else {
            continue;
        };
        has_type |= name == header::CONTENT_TYPE;
        builder = builder.header(name, value);
    }
    if !has_type && !body.is_empty() {
        let guess = if serde_json::from_slice::<serde_json::Value>(&body).is_ok() {
            "application/json"
        } else if std::str::from_utf8(&body).is_ok() {
            "text/plain; charset=utf-8"
        } else {
            "application/octet-stream"
        };
        builder = builder.header(header::CONTENT_TYPE, guess);
    }
    if let Ok(name) = HeaderValue::from_str(&example.name) {
        builder = builder.header(EXAMPLE_HEADER, name);
    }
    let body = if *method == Method::HEAD {
        Body::empty()
    } else {
        Body::from(body)
    };
    builder.body(body).unwrap_or_default()
}

fn json_reply(status: StatusCode, value: &serde_json::Value) -> Response {
    let mut text = serde_json::to_string_pretty(value).unwrap_or_default();
    text.push('\n');
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(text))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use getman_core::http::{
        Header, HttpClient, Outgoing, OutgoingBody, Response as HttpResponse, Transport,
    };
    use getman_core::model::{Example, KeyValue, Request};

    async fn served() -> (tempfile::TempDir, Mock, Arc<Mutex<Vec<String>>>) {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::init(dir.path(), "Shop").unwrap();
        let users = ws.create_folder("", "Users").unwrap();
        ws.create_request(
            &users,
            "Get user",
            &Request {
                url: "{{baseUrl}}/users/:id".into(),
                examples: vec![
                    Example {
                        name: "Missing".into(),
                        status: 404,
                        body: r#"{"error": "no such user"}"#.into(),
                        ..Example::default()
                    },
                    Example {
                        name: "Found".into(),
                        status: 200,
                        headers: vec![
                            KeyValue::new("Content-Type", "application/json"),
                            KeyValue::new("Content-Encoding", "gzip"),
                            KeyValue::new("X-Trace", "abc"),
                        ],
                        body: r#"{"id": 7}"#.into(),
                        ..Example::default()
                    },
                ],
                ..Request::default()
            },
        )
        .unwrap();
        let listener = bind(Some(0)).await.unwrap();
        let logged = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&logged);
        let mock = start(
            ws,
            "",
            listener,
            Duration::ZERO,
            Box::new(move |hit: &Hit| {
                sink.lock()
                    .unwrap()
                    .push(format!("{} {} {}", hit.method, hit.path, hit.status));
            }),
        )
        .unwrap();
        (dir, mock, logged)
    }

    async fn get(mock: &Mock, path: &str, example: Option<&str>) -> HttpResponse {
        get_with(mock, path, example, None).await
    }

    async fn get_with(
        mock: &Mock,
        path: &str,
        example: Option<&str>,
        host: Option<&str>,
    ) -> HttpResponse {
        let mut headers = Vec::new();
        if let Some(name) = example {
            headers.push(Header::new(EXAMPLE_HEADER, name));
        }
        if let Some(host) = host {
            headers.push(Header::new("Host", host));
        }
        let outgoing = Outgoing {
            method: "GET".into(),
            url: format!("{}{path}", mock.url()),
            headers,
            body: OutgoingBody::None,
        };
        HttpClient::new()
            .send(&outgoing, Transport::default())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn examples_answer_their_routes() {
        let (_dir, mock, logged) = served().await;
        let found = get(&mock, "/users/7", None).await;
        assert_eq!(found.status, 200);
        assert_eq!(found.header("x-trace"), Some("abc"));
        assert_eq!(found.header(EXAMPLE_HEADER), Some("Found"));
        assert_eq!(found.header("content-encoding"), None);
        assert_eq!(found.text(), r#"{"id": 7}"#);

        let missing = get(&mock, "/users/7", Some("Missing")).await;
        assert_eq!(missing.status, 404);
        assert_eq!(missing.header("content-type"), Some("application/json"));

        let unknown = get(&mock, "/nothing", None).await;
        assert_eq!(unknown.status, 404);
        let listing = unknown.json().unwrap();
        assert_eq!(listing["mocked"][0]["path"], "/users/:id");
        assert_eq!(listing["mocked"][0]["examples"][1]["name"], "Found");

        let wrong = get(&mock, "/users/7", Some("Gone")).await;
        assert_eq!(wrong.status, 404);
        assert_eq!(
            wrong.json().unwrap()["examples"],
            json!(["Missing", "Found"])
        );

        assert_eq!(
            *logged.lock().unwrap(),
            [
                "GET /users/7 200",
                "GET /users/7 404",
                "GET /nothing 404",
                "GET /users/7 404"
            ]
        );
        assert_eq!(mock.hits()[0].path, "/users/7");
        assert_eq!(mock.hits().len(), 4);
        mock.stop();
    }

    #[tokio::test]
    async fn other_hosts_are_refused() {
        let (_dir, mock, _) = served().await;
        let response = get_with(&mock, "/users/1", None, Some("evil.example")).await;
        assert_eq!(response.status, 403);
    }
}
