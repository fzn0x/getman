//! `getman mcp`: a Model Context Protocol server over standard input and
//! output, so AI agents can read, send and run a workspace's requests.
//!
//! Messages are JSON-RPC 2.0, one per line. The server speaks both eras of
//! the protocol: the per-request `_meta` of revision 2026-07-28, with
//! `server/discover`, and the `initialize` handshake of the revisions up
//! to 2025-11-25. Nothing but protocol messages goes to standard output;
//! notes go to standard error.

mod mask;
mod tools;

use getman_core::Workspace;
use serde_json::{Map, Value, json};
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};

use tools::Tools;

/// The newest revision, whose requests carry their version in `_meta`.
const MODERN: &str = "2026-07-28";

/// Every revision served, newest first. All but the first use the
/// `initialize` handshake.
const SUPPORTED: &[&str] = &[
    MODERN,
    "2025-11-25",
    "2025-06-18",
    "2025-03-26",
    "2024-11-05",
];

/// The newest revision with the handshake, for clients that ask for a
/// version this server does not know.
const NEWEST_LEGACY: &str = "2025-11-25";

const VERSION_KEY: &str = "io.modelcontextprotocol/protocolVersion";
const CAPABILITIES_KEY: &str = "io.modelcontextprotocol/clientCapabilities";
const SERVER_INFO_KEY: &str = "io.modelcontextprotocol/serverInfo";

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const UNSUPPORTED_VERSION: i64 = -32022;

const INSTRUCTIONS: &str = "Getman workspace: API requests stored as YAML files. Use list_requests to see them, read_request to look at one, send_request to send one and see its response and checks, run_folder to run many and get a summary, list_environments to pick an environment, save_request to create or change a request, and import_curl to turn a cURL command into a request. Paths are relative to the workspace root, such as users/get-user.yaml. Values from git-ignored *.local.yaml files, variables marked secret and GETMAN_* variables are replaced by [secret] in every result, in their encoded forms too.";

/// Serves the workspace on standard input and output until input ends.
pub(crate) async fn serve(workspace: Workspace) -> Result<(), String> {
    crate::style::note(&format!(
        "getman mcp: serving {} over standard input and output",
        workspace.root().display()
    ));
    let mut server = Server::new(workspace);
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();
    while let Some(line) = lines.next_line().await.map_err(|e| e.to_string())? {
        if let Some(reply) = server.handle_line(&line).await {
            stdout
                .write_all(reply.as_bytes())
                .await
                .map_err(|e| e.to_string())?;
            stdout.write_all(b"\n").await.map_err(|e| e.to_string())?;
            stdout.flush().await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// The protocol state of one connection.
pub(crate) struct Server {
    tools: Tools,
}

impl Server {
    pub(crate) fn new(workspace: Workspace) -> Self {
        Self {
            tools: Tools::new(workspace),
        }
    }

    /// Handles one line of input and returns the line to answer with.
    pub(crate) async fn handle_line(&mut self, line: &str) -> Option<String> {
        let line = line.trim();
        if line.is_empty() {
            return None;
        }
        let reply = match serde_json::from_str::<Value>(line) {
            Ok(message) => self.handle(message).await?,
            Err(e) => error(
                &Value::Null,
                PARSE_ERROR,
                &format!("Parse error: {e}"),
                None,
            ),
        };
        Some(reply.to_string())
    }

    /// Handles one message, or a batch of them. Notifications and
    /// responses get no answer.
    pub(crate) async fn handle(&mut self, message: Value) -> Option<Value> {
        if let Value::Array(batch) = message {
            if batch.is_empty() {
                return Some(error(
                    &Value::Null,
                    INVALID_REQUEST,
                    "Invalid Request: empty batch",
                    None,
                ));
            }
            let mut replies = Vec::new();
            for message in batch {
                if let Some(reply) = Box::pin(self.handle(message)).await {
                    replies.push(reply);
                }
            }
            return (!replies.is_empty()).then_some(Value::Array(replies));
        }
        let Value::Object(message) = message else {
            return Some(error(
                &Value::Null,
                INVALID_REQUEST,
                "Invalid Request: not an object",
                None,
            ));
        };
        let id = message.get("id").cloned();
        let valid_id = matches!(id, Some(Value::String(_) | Value::Number(_)));
        if message.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return Some(error(
                &id.unwrap_or(Value::Null),
                INVALID_REQUEST,
                "Invalid Request: jsonrpc must be \"2.0\"",
                None,
            ));
        }
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            // A response to a request this server never sends.
            return None;
        };
        let Some(id) = id else {
            // Notifications, such as notifications/initialized, need nothing.
            return None;
        };
        if !valid_id {
            return Some(error(
                &Value::Null,
                INVALID_REQUEST,
                "Invalid Request: the id must be a string or a number",
                None,
            ));
        }
        let params = message.get("params").cloned().unwrap_or(json!({}));
        Some(match self.request(method, &params).await {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err((code, text, data)) => error(&id, code, &text, data),
        })
    }

    async fn request(&mut self, method: &str, params: &Value) -> Result<Value, Failure> {
        let meta = params.get("_meta").and_then(Value::as_object);
        let version = meta.and_then(|m| m.get(VERSION_KEY));
        let modern = version.is_some();
        if let Some(version) = version {
            let version = version.as_str().unwrap_or_default();
            if !SUPPORTED.contains(&version) {
                return Err((
                    UNSUPPORTED_VERSION,
                    "Unsupported protocol version".into(),
                    Some(json!({ "supported": SUPPORTED, "requested": version })),
                ));
            }
            if !meta.is_some_and(|m| m.contains_key(CAPABILITIES_KEY)) {
                return Err(invalid(format!(
                    "Invalid params: _meta needs {CAPABILITIES_KEY}"
                )));
            }
        }

        let mut result = match method {
            "initialize" => initialize(params),
            "server/discover" => {
                if !modern {
                    return Err(invalid(format!(
                        "Invalid params: server/discover needs _meta with {VERSION_KEY} and {CAPABILITIES_KEY}"
                    )));
                }
                json!({
                    "supportedVersions": SUPPORTED,
                    "capabilities": capabilities(),
                    "instructions": INSTRUCTIONS,
                })
            }
            "ping" => json!({}),
            "tools/list" => json!({ "tools": tools::list() }),
            "tools/call" => {
                let name = params
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid("Invalid params: tools/call needs a name".into()))?;
                let arguments = match params.get("arguments") {
                    None | Some(Value::Null) => Map::new(),
                    Some(Value::Object(arguments)) => arguments.clone(),
                    Some(_) => {
                        return Err(invalid(
                            "Invalid params: arguments must be an object".into(),
                        ));
                    }
                };
                if !tools::exists(name) {
                    return Err(invalid(format!("Unknown tool: {name}")));
                }
                self.tools.call(name, &arguments).await
            }
            other => {
                return Err((METHOD_NOT_FOUND, format!("Method not found: {other}"), None));
            }
        };
        if modern && let Value::Object(result) = &mut result {
            result.insert("resultType".into(), json!("complete"));
            result.insert("_meta".into(), json!({ SERVER_INFO_KEY: server_info() }));
        }
        Ok(result)
    }
}

/// A JSON-RPC error: code, message and optional data.
type Failure = (i64, String, Option<Value>);

fn invalid(message: String) -> Failure {
    (INVALID_PARAMS, message, None)
}

fn error(id: &Value, code: i64, message: &str, data: Option<Value>) -> Value {
    let mut error = json!({ "code": code, "message": message });
    if let Some(data) = data {
        error["data"] = data;
    }
    json!({ "jsonrpc": "2.0", "id": id, "error": error })
}

fn server_info() -> Value {
    json!({
        "name": "getman",
        "title": "Getman",
        "version": env!("CARGO_PKG_VERSION"),
    })
}

fn capabilities() -> Value {
    json!({ "tools": { "listChanged": false } })
}

/// The handshake of revisions up to 2025-11-25: answers with the version
/// the client asked for when it is served, else the newest one with a
/// handshake.
fn initialize(params: &Value) -> Value {
    let asked = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let version = if asked != MODERN && SUPPORTED.contains(&asked) {
        asked
    } else {
        NEWEST_LEGACY
    };
    json!({
        "protocolVersion": version,
        "capabilities": capabilities(),
        "serverInfo": server_info(),
        "instructions": INSTRUCTIONS,
    })
}

#[cfg(test)]
mod tests;
