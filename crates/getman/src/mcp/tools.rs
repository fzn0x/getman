//! The tools `getman mcp` offers. Everything they return is masked first
//! (see [`crate::mask`]).

use std::sync::atomic::AtomicBool;

use getman_core::Workspace;
use getman_core::execute::{Context, Executor, Overlay, persist_changes};
use getman_core::http::OutgoingBody;
use getman_core::import::curl;
use getman_core::model::{Request, Vars};
use getman_core::runner::{self, RunEvent, RunOptions};
use getman_core::workspace::Node;
use serde_json::{Map, Value, json};

use crate::mask::{MASK, Masker, secret_values};
use crate::report::{failures, outcome};
use crate::workspaces;

/// Body characters returned unless the caller asks for another limit.
const DEFAULT_BODY_CHARS: usize = 20_000;
const MAX_BODY_CHARS: usize = 200_000;

const NAMES: &[&str] = &[
    "list_requests",
    "read_request",
    "send_request",
    "run_folder",
    "list_environments",
    "save_request",
    "import_curl",
];

pub(super) fn exists(name: &str) -> bool {
    NAMES.contains(&name)
}

/// The tool definitions, in a fixed order.
pub(super) fn list() -> Vec<Value> {
    let environment = json!({
        "type": "string",
        "description": "Environment to use, by id (its file name) or display name. See list_environments."
    });
    let variables = json!({
        "type": "object",
        "description": "Variables that override every other value, as name: value.",
        "additionalProperties": { "type": ["string", "number", "boolean"] }
    });
    vec![
        json!({
            "name": "list_requests",
            "title": "List requests",
            "description": "The workspace's folders and requests as a tree, in the order they run. Each request has its path, name and HTTP method.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "folder": { "type": "string", "description": "Only this folder, such as users. The whole workspace when left out." }
                },
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "openWorldHint": false }
        }),
        json!({
            "name": "read_request",
            "title": "Read a request",
            "description": "A request as stored: method, URL, headers, body, auth, scripts, checks, captures and settings, plus the names of its saved examples. Variables are left as {{name}}.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path of the request file, such as users/get-user.yaml." }
                },
                "required": ["path"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "openWorldHint": false }
        }),
        json!({
            "name": "send_request",
            "title": "Send a request",
            "description": "Sends a saved request with its scripts, checks and captures, and returns the request as it went out (method, URL, headers and body, after variables, auth and pre-request scripts), the status, headers, body (cut to a limit), timing and check and test results.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path of the request file, such as users/get-user.yaml." },
                    "environment": environment,
                    "variables": variables,
                    "keep_variables": { "type": "boolean", "description": "Save variables that the request's scripts and captures set, such as a token, to the git-ignored local files so later requests see them. Default false." },
                    "max_body_chars": { "type": "integer", "minimum": 0, "maximum": MAX_BODY_CHARS, "description": "Most body characters to return, for the response and the request each. Default 20000." }
                },
                "required": ["path"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": true }
        }),
        json!({
            "name": "run_folder",
            "title": "Run a folder",
            "description": "Runs every request in a folder, or the whole workspace, in order, and returns the totals and each request's status, time and failures.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Folder or request to run. The whole workspace when left out." },
                    "environment": environment,
                    "variables": variables,
                    "iterations": { "type": "integer", "minimum": 1, "maximum": 100, "description": "How many times to run. Default 1." },
                    "bail": { "type": "boolean", "description": "Stop at the first failure. Default false." }
                },
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": true }
        }),
        json!({
            "name": "list_environments",
            "title": "List environments",
            "description": "The workspace's environments with their variables. Secret values and values from git-ignored local files are shown as [secret].",
            "inputSchema": { "type": "object", "additionalProperties": false },
            "annotations": { "readOnlyHint": true, "openWorldHint": false }
        }),
        json!({
            "name": "save_request",
            "title": "Create or update a request",
            "description": "Writes a request file. Give path to replace an existing request, or folder and name to create one. The request uses the shape read_request returns: name, method, url, headers [{name, value}], body {type: json|xml|text|form|multipart|binary|graphql, ...}, auth, scripts, checks, captures, settings. Saved examples are kept unless examples is given.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "An existing request to replace, such as users/get-user.yaml." },
                    "folder": { "type": "string", "description": "Folder to create the request in. The workspace root when left out." },
                    "name": { "type": "string", "description": "Name of a new request; its file name is made from it." },
                    "request": { "type": "object", "description": "The request, for example {\"method\": \"GET\", \"url\": \"{{baseUrl}}/users/:id\", \"checks\": [\"status == 200\"]}." }
                },
                "required": ["request"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": true, "openWorldHint": false }
        }),
        json!({
            "name": "import_curl",
            "title": "Import a cURL command",
            "description": "Turns a cURL command into a new request file and returns its path and contents.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "The command, starting with curl." },
                    "folder": { "type": "string", "description": "Folder to create the request in. The workspace root when left out." },
                    "name": { "type": "string", "description": "Name of the request. Made from the URL when left out." }
                },
                "required": ["command"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": false }
        }),
    ]
}

/// Runs the tools against one workspace. The HTTP client, its cookies and
/// cached OAuth 2.0 tokens last as long as the server.
pub(super) struct Tools {
    executor: Executor,
}

type Arguments = Map<String, Value>;
type Outcome = Result<Value, String>;

impl Tools {
    pub(super) fn new(workspace: Workspace) -> Self {
        Self {
            executor: Executor::new(workspace),
        }
    }

    fn workspace(&self) -> &Workspace {
        &self.executor.workspace
    }

    /// Calls a tool and returns its result, with secret values masked.
    pub(super) async fn call(&mut self, name: &str, arguments: &Arguments) -> Value {
        // Read before the tool runs too: a script may replace a secret, such
        // as a token, after the old value went out.
        let before = secret_values(self.workspace());
        let outcome = match name {
            "list_requests" => self.list_requests(arguments),
            "read_request" => self.read_request(arguments),
            "send_request" => self.send_request(arguments).await,
            "run_folder" => self.run_folder(arguments).await,
            "list_environments" => self.list_environments(),
            "save_request" => self.save_request(arguments),
            "import_curl" => self.import_curl(arguments),
            other => Err(format!("there is no tool {other}")),
        };
        let masker = Masker::new(before.into_iter().chain(secret_values(self.workspace())));
        match outcome {
            Ok(mut value) => {
                masker.mask_value(&mut value);
                let text = serde_json::to_string_pretty(&value).unwrap_or_default();
                json!({
                    "content": [{ "type": "text", "text": text }],
                    "structuredContent": value,
                    "isError": false,
                })
            }
            Err(message) => json!({
                "content": [{ "type": "text", "text": masker.mask(&message) }],
                "isError": true,
            }),
        }
    }

    fn list_requests(&self, arguments: &Arguments) -> Outcome {
        let folder = path_arg(arguments, "folder")?.unwrap_or_default();
        let tree = self.workspace().tree().map_err(|e| e.to_string())?;
        let nodes = if folder.is_empty() {
            tree
        } else {
            match find(&tree, &folder) {
                Some(Node::Folder { children, .. }) => children.clone(),
                Some(Node::Request { .. }) => {
                    return Err(format!("{folder} is a request, not a folder"));
                }
                None => return Err(format!("there is no folder {folder}")),
            }
        };
        Ok(json!({
            "workspace": self.workspace().name(),
            "folder": folder,
            "tree": nodes,
        }))
    }

    fn read_request(&self, arguments: &Arguments) -> Outcome {
        let path = required_path(arguments, "path")?;
        let mut request = self
            .workspace()
            .read_request(&path)
            .map_err(|e| e.to_string())?;
        let examples: Vec<Value> = request
            .examples
            .drain(..)
            .map(|e| json!({ "name": e.name, "status": e.status }))
            .collect();
        Ok(json!({ "path": path, "request": request, "examples": examples }))
    }

    async fn send_request(&self, arguments: &Arguments) -> Outcome {
        let path = required_path(arguments, "path")?;
        let environment = self.environment(arguments)?;
        let limit = match arguments.get("max_body_chars") {
            None | Some(Value::Null) => DEFAULT_BODY_CHARS,
            Some(value) => value
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .ok_or("max_body_chars must be a whole number")?
                .min(MAX_BODY_CHARS),
        };
        let keep = flag(arguments, "keep_variables")?;
        let mut overlay = Overlay::default();
        for (name, value) in variables(arguments)? {
            overlay.local.insert(name, Some(value));
        }
        let mut context = Context {
            environment: environment.clone(),
            overlay,
            ..Context::default()
        };
        let execution = self.executor.execute_path(&path, &mut context).await;
        if keep {
            persist_changes(self.workspace(), environment.as_deref(), &execution.changes)
                .map_err(|e| format!("the variables could not be saved: {e}"))?;
        }

        let mut result = json!({
            "path": path,
            "name": execution.name,
            "passed": !execution.failed(),
        });
        if let Some(sent) = &execution.request {
            let mut request = json!({
                "method": sent.method,
                "url": sent.url,
                "headers": sent.headers,
            });
            match &sent.body {
                OutgoingBody::None => {}
                OutgoingBody::Bytes { data } => put_body(&mut request, &data.0, limit),
                OutgoingBody::File { path } => request["body_file"] = json!(path),
                OutgoingBody::Multipart { parts } => request["parts"] = json!(parts),
            }
            result["request"] = request;
        }
        if let Some(error) = &execution.error {
            result["error"] = json!(error);
        }
        if let Some(response) = &execution.response {
            result["status"] = json!(response.status);
            result["status_text"] = json!(response.status_text);
            result["time_ms"] = json!(response.time_ms);
            result["size_bytes"] = json!(response.size.body);
            result["headers"] = json!(response.headers);
            put_body(&mut result, &response.body.0, limit);
        }
        result["checks"] = json!(execution.checks);
        result["tests"] = json!(execution.tests);
        if !execution.script_errors.is_empty() {
            result["script_errors"] = json!(execution.script_errors);
        }
        if !execution.missing_variables.is_empty() {
            result["missing_variables"] = json!(execution.missing_variables);
        }
        let mut warnings = execution.warnings.clone();
        if execution.response.as_ref().is_some_and(|r| r.truncated) {
            warnings.push("The response was larger than Getman reads and was cut short.".into());
        }
        if !warnings.is_empty() {
            result["warnings"] = json!(warnings);
        }
        if !execution.logs.is_empty() {
            result["logs"] = json!(execution.logs);
        }
        if execution.skipped {
            result["skipped"] = json!(true);
        }
        Ok(result)
    }

    async fn run_folder(&self, arguments: &Arguments) -> Outcome {
        let target = path_arg(arguments, "path")?.unwrap_or_default();
        let environment = self.environment(arguments)?;
        let iterations = match arguments.get("iterations") {
            None | Some(Value::Null) => None,
            Some(value) => Some(
                value
                    .as_u64()
                    .filter(|n| (1..=100).contains(n))
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or("iterations must be a whole number from 1 to 100")?,
            ),
        };
        let options = RunOptions {
            target,
            environment,
            iterations,
            bail: flag(arguments, "bail")?,
            vars: variables(arguments)?,
            ..RunOptions::default()
        };
        let mut requests = Vec::new();
        let report = runner::run(&self.executor, &options, &AtomicBool::new(false), |event| {
            if let RunEvent::Request {
                iteration,
                execution,
                ..
            } = event
            {
                let response = execution.response.as_ref();
                requests.push(json!({
                    "iteration": iteration + 1,
                    "name": execution.name,
                    "path": execution.path,
                    "status": response.map(|r| r.status),
                    "time_ms": response.map(|r| r.time_ms),
                    "result": outcome(&execution),
                    "failures": failures(&execution),
                }));
            }
        })
        .await
        .map_err(|e| e.to_string())?;
        let summary = &report.summary;
        Ok(json!({
            "passed": summary.passed() && summary.stopped.is_none(),
            "summary": summary,
            "requests": requests,
        }))
    }

    fn list_environments(&self) -> Outcome {
        let workspace = self.workspace();
        let mut list = Vec::new();
        for summary in workspace.environments().map_err(|e| e.to_string())? {
            let Ok(files) = workspace.read_environment(&summary.id) else {
                continue;
            };
            let mut variables = Map::new();
            for (name, value) in &files.shared.variables {
                let secret = files.shared.secrets.contains(name) || files.local.contains_key(name);
                variables.insert(
                    name.clone(),
                    json!(if secret { MASK } else { value.as_str() }),
                );
            }
            for name in files.local.keys() {
                variables.entry(name.clone()).or_insert(json!(MASK));
            }
            list.push(json!({
                "id": summary.id,
                "name": summary.name,
                "variables": variables,
            }));
        }
        Ok(json!({ "environments": list }))
    }

    fn save_request(&self, arguments: &Arguments) -> Outcome {
        let given = arguments
            .get("request")
            .and_then(Value::as_object)
            .ok_or("request must be an object, such as {\"method\": \"GET\", \"url\": \"https://example.com\"}")?;
        let mut request: Request = serde_json::from_value(Value::Object(given.clone()))
            .map_err(|e| format!("the request is not valid: {e}"))?;
        let workspace = self.workspace();
        if let Some(path) = path_arg(arguments, "path")? {
            let saved = workspace.read_request(&path).map_err(|e| e.to_string())?;
            if !given.contains_key("examples") {
                request.examples = saved.examples;
            }
            if !given.contains_key("name") {
                request.name = saved.name;
            }
            workspace
                .write_request(&path, &request)
                .map_err(|e| e.to_string())?;
            return Ok(json!({ "path": path, "created": false }));
        }
        let name = string_arg(arguments, "name")?
            .or_else(|| (!request.name.trim().is_empty()).then(|| request.name.clone()))
            .ok_or("give a name for the new request, or a path to replace one")?;
        let folder = path_arg(arguments, "folder")?.unwrap_or_default();
        let path = workspace
            .create_request(&folder, name.trim(), &request)
            .map_err(|e| e.to_string())?;
        Ok(json!({ "path": path, "created": true }))
    }

    fn import_curl(&self, arguments: &Arguments) -> Outcome {
        let command = string_arg(arguments, "command")?.ok_or("give the cURL command")?;
        let mut request = curl::parse(&command).map_err(|e| e.to_string())?;
        if let Some(name) = string_arg(arguments, "name")? {
            request.name = name.trim().to_owned();
        }
        let folder = path_arg(arguments, "folder")?.unwrap_or_default();
        let path = self
            .workspace()
            .create_request(&folder, &request.name, &request)
            .map_err(|e| e.to_string())?;
        let request = self
            .workspace()
            .read_request(&path)
            .map_err(|e| e.to_string())?;
        Ok(json!({ "path": path, "request": request }))
    }

    fn environment(&self, arguments: &Arguments) -> Result<Option<String>, String> {
        let given = string_arg(arguments, "environment")?;
        workspaces::environment(self.workspace(), given.as_deref())
    }
}

/// Sets `body` in `into` to the text of `bytes` cut to `limit` characters,
/// with `body_truncated` and `body_chars` when it was cut.
fn put_body(into: &mut Value, bytes: &[u8], limit: usize) {
    match std::str::from_utf8(bytes) {
        Ok(text) => {
            let total = text.chars().count();
            into["body"] = json!(text.chars().take(limit).collect::<String>());
            if total > limit {
                into["body_truncated"] = json!(true);
                into["body_chars"] = json!(total);
            }
        }
        Err(_) => {
            into["body"] = json!(format!(
                "A binary body of {} bytes, not shown.",
                bytes.len()
            ));
        }
    }
}

fn find<'a>(nodes: &'a [Node], path: &str) -> Option<&'a Node> {
    nodes.iter().find_map(|node| {
        if node.path() == path {
            return Some(node);
        }
        match node {
            Node::Folder { children, .. } => find(children, path),
            Node::Request { .. } => None,
        }
    })
}

fn string_arg(arguments: &Arguments, key: &str) -> Result<Option<String>, String> {
    match arguments.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(_) => Err(format!("{key} must be a string")),
    }
}

/// A workspace path argument, with `\` turned into `/`.
fn path_arg(arguments: &Arguments, key: &str) -> Result<Option<String>, String> {
    Ok(string_arg(arguments, key)?
        .map(|path| path.replace('\\', "/").trim_matches('/').to_owned())
        .filter(|path| !path.is_empty()))
}

fn required_path(arguments: &Arguments, key: &str) -> Result<String, String> {
    path_arg(arguments, key)?.ok_or_else(|| format!("give {key}, such as users/get-user.yaml"))
}

fn flag(arguments: &Arguments, key: &str) -> Result<bool, String> {
    match arguments.get(key) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(value)) => Ok(*value),
        Some(_) => Err(format!("{key} must be true or false")),
    }
}

fn variables(arguments: &Arguments) -> Result<Vars, String> {
    match arguments.get("variables") {
        None | Some(Value::Null) => Ok(Vars::new()),
        Some(Value::Object(map)) => map
            .iter()
            .map(|(name, value)| {
                let text = match value {
                    Value::String(text) => text.clone(),
                    Value::Number(_) | Value::Bool(_) => value.to_string(),
                    _ => return Err(format!("the variable {name} must be a string")),
                };
                Ok((name.clone(), text))
            })
            .collect(),
        Some(_) => Err("variables must be an object of name: value".into()),
    }
}
