//! A local HTTP server the tests send requests to.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::time::Duration;

use axum::Router;
use axum::extract::{Path, Query, Request};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get};
use serde_json::{Value, json};

/// A running test server.
pub(crate) struct TestServer {
    addr: SocketAddr,
}

impl TestServer {
    /// The URL of a path on the server.
    pub(crate) fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr)
    }
}

/// Starts a server on a free port of the loopback interface.
pub(crate) async fn spawn() -> TestServer {
    let app = Router::new()
        .route("/echo", any(echo))
        .route("/echo/{*rest}", any(echo))
        .route("/redirect", get(redirect))
        .route("/cookies/set", get(set_cookies))
        .route("/bytes", get(bytes))
        .route("/slow", get(slow))
        .route("/status/{code}", any(status));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    TestServer { addr }
}

/// Replies with what it received, as JSON.
async fn echo(request: Request) -> axum::Json<Value> {
    let (parts, body) = request.into_parts();
    let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
    let mut headers = BTreeMap::new();
    for (name, value) in &parts.headers {
        headers
            .entry(name.to_string())
            .and_modify(|v: &mut String| {
                v.push_str(", ");
                v.push_str(value.to_str().unwrap_or_default());
            })
            .or_insert_with(|| value.to_str().unwrap_or_default().to_owned());
    }
    axum::Json(json!({
        "method": parts.method.as_str(),
        "path": parts.uri.path(),
        "query": parts.uri.query().unwrap_or_default(),
        "headers": headers,
        "body": String::from_utf8_lossy(&bytes),
    }))
}

async fn redirect() -> impl IntoResponse {
    (StatusCode::FOUND, [(header::LOCATION, "/echo")])
}

async fn set_cookies(Query(cookies): Query<BTreeMap<String, String>>) -> Response {
    let mut response = Response::builder().status(StatusCode::OK);
    for (name, value) in cookies {
        response = response.header(header::SET_COOKIE, format!("{name}={value}; Path=/"));
    }
    response.body(axum::body::Body::from("set")).unwrap()
}

async fn bytes() -> Vec<u8> {
    vec![0, 159, 146, 150]
}

async fn slow() -> &'static str {
    tokio::time::sleep(Duration::from_secs(5)).await;
    "late"
}

async fn status(Path(code): Path<u16>) -> StatusCode {
    StatusCode::from_u16(code).unwrap_or(StatusCode::BAD_REQUEST)
}
