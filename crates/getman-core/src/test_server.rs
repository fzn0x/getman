//! A local HTTP server the tests send requests to.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::time::Duration;

use axum::Router;
use axum::extract::{Path, Query, Request};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get, post};
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
        .route("/status/{code}", any(status))
        .route("/digest", get(digest))
        .route("/token", post(token));
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

/// Requires Digest authentication as `ada` with password `lovelace`.
async fn digest(headers: HeaderMap) -> Response {
    const REALM: &str = "getman-test";
    const NONCE: &str = "dcd98b7102dd2f0e8b11d0f600bfb0c093";
    let challenge = || {
        (
            StatusCode::UNAUTHORIZED,
            [(
                header::WWW_AUTHENTICATE,
                format!(r#"Digest realm="{REALM}", qop="auth", nonce="{NONCE}", opaque="5ccc069c403ebaf9f0171e9517f40e41""#),
            )],
        )
            .into_response()
    };
    let Some(authorization) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Digest "))
    else {
        return challenge();
    };
    let field = |name: &str| -> String {
        authorization
            .split(", ")
            .find_map(|pair| pair.strip_prefix(&format!("{name}=")))
            .unwrap_or_default()
            .trim_matches('"')
            .to_owned()
    };
    let md5 = |text: String| {
        use md5::Digest as _;
        hex::encode(md5::Md5::digest(text))
    };
    let ha1 = md5(format!("ada:{REALM}:lovelace"));
    let ha2 = md5(format!("GET:{}", field("uri")));
    let expected = md5(format!(
        "{ha1}:{NONCE}:{}:{}:auth:{ha2}",
        field("nc"),
        field("cnonce")
    ));
    if field("response") == expected && field("username") == "ada" {
        (StatusCode::OK, "welcome").into_response()
    } else {
        challenge()
    }
}

/// An OAuth 2.0 token endpoint accepting client `app` with secret `s3cret`.
async fn token(headers: HeaderMap, body: String) -> Response {
    let form: BTreeMap<String, String> = url::form_urlencoded::parse(body.as_bytes())
        .into_owned()
        .collect();
    let basic = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == "Basic YXBwOnMzY3JldA==");
    let in_body = form.get("client_id").map(String::as_str) == Some("app")
        && form.get("client_secret").map(String::as_str) == Some("s3cret");
    if !(basic || in_body) {
        return (
            StatusCode::UNAUTHORIZED,
            axum::Json(json!({"error": "invalid_client"})),
        )
            .into_response();
    }
    let grant = form.get("grant_type").cloned().unwrap_or_default();
    let scope = form.get("scope").cloned().unwrap_or_default();
    axum::Json(json!({
        "access_token": format!("token-for-{grant}-{scope}"),
        "token_type": "Bearer",
        "expires_in": 3600,
    }))
    .into_response()
}
