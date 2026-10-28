//! The JSON API for GraphQL requests: reading a server's schema for the
//! editor's completion, linting and docs.
//!
//! `/graphql/schema` sends the introspection query with the request's own
//! URL, headers, authentication and settings, and keeps the answer in
//! memory by URL until the process ends or the UI asks for a refresh.

use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use getman_core::execute::Context;
use getman_core::model::{Body, Request, Scripts};
use serde::Deserialize;
use serde_json::{Value, json};

use super::AppState;
use super::api::{ApiError, ApiResult};

type Shared = State<Arc<AppState>>;

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/graphql/schema", post(schema))
}

/// The standard introspection query, without the newer fields that older
/// servers reject.
const INTROSPECTION: &str = "query IntrospectionQuery { __schema { queryType { name } mutationType { name } subscriptionType { name } types { ...FullType } directives { name description locations args { ...InputValue } } } } fragment FullType on __Type { kind name description fields(includeDeprecated: true) { name description args { ...InputValue } type { ...TypeRef } isDeprecated deprecationReason } inputFields { ...InputValue } interfaces { ...TypeRef } enumValues(includeDeprecated: true) { name description isDeprecated deprecationReason } possibleTypes { ...TypeRef } } fragment InputValue on __InputValue { name description type { ...TypeRef } defaultValue } fragment TypeRef on __Type { kind name ofType { kind name ofType { kind name ofType { kind name ofType { kind name ofType { kind name ofType { kind name ofType { kind name } } } } } } } }";

/// How many schemas are kept; the one used longest ago goes first.
const KEPT: usize = 16;

/// Schemas by the URL they were read from. Kept for the process, not per
/// workspace: a URL names one server whichever workspace asks.
static SCHEMAS: Mutex<Vec<(String, Cached)>> = Mutex::new(Vec::new());

#[derive(Clone)]
struct Cached {
    /// The `data` of the introspection answer.
    data: Value,
    fetched_at: u64,
    status: u16,
}

fn cached(url: &str) -> Option<Cached> {
    let mut schemas = SCHEMAS.lock().expect("schemas lock");
    let at = schemas.iter().position(|(key, _)| key == url)?;
    // Most recently used last.
    let entry = schemas.remove(at);
    schemas.push(entry.clone());
    Some(entry.1)
}

fn keep(url: &str, schema: Cached) {
    let mut schemas = SCHEMAS.lock().expect("schemas lock");
    schemas.retain(|(key, _)| key != url);
    schemas.push((url.to_owned(), schema));
    if schemas.len() > KEPT {
        schemas.remove(0);
    }
}

#[derive(Deserialize)]
struct SchemaArg {
    #[serde(default)]
    path: Option<String>,
    request: Request,
    #[serde(default)]
    environment: Option<String>,
    /// `cached` answers from memory without sending anything, `fetch`
    /// sends when nothing is kept and `refresh` always sends.
    #[serde(default)]
    mode: Mode,
}

#[derive(Deserialize, Default, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Mode {
    Cached,
    #[default]
    Fetch,
    Refresh,
}

fn bad(message: impl Into<String>) -> ApiError {
    ApiError(axum::http::StatusCode::BAD_REQUEST, message.into())
}

/// Reads the schema of the GraphQL server a request talks to. The answer
/// is `{"schema": null}` when `mode` is `cached` and nothing is kept.
async fn schema(State(state): Shared, Json(arg): Json<SchemaArg>) -> ApiResult {
    let mut request = arg.request;
    if !request.kind.is_http() {
        return Err(bad("Only HTTP requests can be GraphQL requests."));
    }
    // The request's own URL, headers, authentication and settings, but
    // not its body, scripts or checks, which belong to the operation.
    request.method = "POST".into();
    request.body = Body::Graphql {
        query: INTROSPECTION.into(),
        variables: String::new(),
        operation_name: "IntrospectionQuery".into(),
    };
    request.scripts = Scripts::default();
    request.checks.clear();
    request.captures.clear();
    let mut context = Context {
        environment: arg.environment.filter(|e| !e.is_empty()),
        ..Context::default()
    };
    let path = arg.path.as_deref().filter(|p| !p.is_empty());
    let prepared = state.executor.prepare(path, &request, &context)?;
    let url = prepared.outgoing.url;
    if arg.mode != Mode::Refresh
        && let Some(found) = cached(&url)
    {
        return Ok(Json(answer(&url, &found, true, 0.0)));
    }
    if arg.mode == Mode::Cached {
        return Ok(Json(json!({ "schema": null, "url": url })));
    }
    let execution = state.executor.execute(path, &request, &mut context).await;
    if let Some(error) = execution.error {
        return Err(bad(error));
    }
    let Some(response) = execution.response else {
        return Err(bad("The server did not answer."));
    };
    let status = response.status;
    let Ok(body) = serde_json::from_slice::<Value>(&response.body.0) else {
        return Err(bad(format!(
            "The server answered {status} {}, which is not a GraphQL answer. Check the URL.",
            response.status_text
        )));
    };
    if !body["data"]["__schema"].is_object() {
        let reasons: Vec<&str> = body["errors"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|e| e["message"].as_str())
            .collect();
        return Err(bad(if reasons.is_empty() {
            format!("The server answered {status} without a schema.")
        } else {
            format!(
                "The server answered {status} without a schema: {}",
                reasons.join("; ")
            )
        }));
    }
    let fetched = Cached {
        data: body["data"].clone(),
        fetched_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
        status,
    };
    keep(&url, fetched.clone());
    Ok(Json(answer(&url, &fetched, false, response.time_ms)))
}

fn answer(url: &str, schema: &Cached, from_memory: bool, time_ms: f64) -> Value {
    json!({
        "schema": schema.data,
        "url": url,
        "cached": from_memory,
        "fetched_at": schema.fetched_at,
        "status": schema.status,
        "time_ms": time_ms,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use axum::body::Body as HttpBody;
    use axum::http::{HeaderMap, Request as HttpRequest, StatusCode};
    use getman_core::Workspace;
    use getman_core::execute::Executor;
    use tower::ServiceExt as _;

    use super::super::{grpc, history, realtime, router};
    use super::*;

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

    /// A GraphQL server that answers introspection when the token is
    /// right and counts how often it was asked.
    async fn server() -> (String, Arc<Mutex<Vec<Value>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&seen);
        let app = Router::new().route(
            "/graphql",
            post(move |headers: HeaderMap, Json(body): Json<Value>| {
                let log = Arc::clone(&log);
                async move {
                    log.lock().unwrap().push(body.clone());
                    if headers.get("authorization").and_then(|v| v.to_str().ok())
                        != Some("Bearer s3cret")
                    {
                        return Json(json!({ "errors": [{ "message": "Not signed in." }] }));
                    }
                    if body["operationName"] != "IntrospectionQuery" {
                        return Json(json!({ "errors": [{ "message": "Wrong operation." }] }));
                    }
                    Json(json!({ "data": { "__schema": {
                        "queryType": { "name": "Query" },
                        "types": [{ "kind": "OBJECT", "name": "Query", "fields": [] }],
                    } } }))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/graphql", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await });
        (url, seen)
    }

    async fn call(router: &Router, body: &Value) -> (StatusCode, Value) {
        let request = HttpRequest::post("/api/graphql/schema")
            .header("host", "127.0.0.1:7878")
            .header("x-getman-token", "secret-token")
            .header("content-type", "application/json")
            .body(HttpBody::from(body.to_string()))
            .unwrap();
        let response = router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or_default())
    }

    fn arg(url: &str, token: &str, mode: &str) -> Value {
        json!({
            "request": {
                "method": "GET",
                "url": url,
                "auth": { "type": "bearer", "token": token },
                "body": { "type": "graphql", "query": "{ me }" },
                "scripts": { "pre_request": "throw new Error('no')" },
            },
            "mode": mode,
        })
    }

    #[tokio::test]
    async fn the_schema_is_read_with_the_requests_auth_and_kept_by_url() {
        let (_dir, router) = app();
        let (url, seen) = server().await;

        // Nothing is sent just to look.
        let (status, body) = call(&router, &arg(&url, "s3cret", "cached")).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(body["schema"].is_null());
        assert!(seen.lock().unwrap().is_empty());

        let (status, body) = call(&router, &arg(&url, "s3cret", "fetch")).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["schema"]["__schema"]["queryType"]["name"], "Query");
        assert_eq!(body["cached"], false);
        {
            let seen = seen.lock().unwrap();
            assert_eq!(seen.len(), 1);
            assert_eq!(seen[0]["operationName"], "IntrospectionQuery");
            assert!(
                seen[0]["query"]
                    .as_str()
                    .unwrap()
                    .contains("query IntrospectionQuery")
            );
        }

        // Asking again is answered from memory, with no new request.
        for mode in ["cached", "fetch"] {
            let (_, body) = call(&router, &arg(&url, "wrong", mode)).await;
            assert_eq!(body["cached"], true, "{mode}: {body}");
            assert!(body["schema"]["__schema"].is_object());
        }
        assert_eq!(seen.lock().unwrap().len(), 1);

        // Refresh asks the server again, and shows what it says.
        let (status, body) = call(&router, &arg(&url, "wrong", "refresh")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(
            body["error"].as_str().unwrap().contains("Not signed in."),
            "{body}"
        );
        assert_eq!(seen.lock().unwrap().len(), 2);
        let (_, body) = call(&router, &arg(&url, "s3cret", "refresh")).await;
        assert_eq!(body["cached"], false);
    }

    #[tokio::test]
    async fn problems_are_explained() {
        let (_dir, router) = app();
        let (status, body) = call(
            &router,
            &json!({ "request": { "kind": "websocket", "url": "ws://x.test" } }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["error"].as_str().unwrap().contains("HTTP"), "{body}");
        let (status, body) = call(
            &router,
            &arg("http://127.0.0.1:1/graphql", "s3cret", "fetch"),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["error"].is_string(), "{body}");
    }
}
