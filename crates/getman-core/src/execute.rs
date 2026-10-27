//! Executing one request from start to finish: pre-request scripts,
//! variables, sending with authentication, post-response scripts, checks
//! and captures.

use std::sync::Arc;
use std::time::Duration;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::auth::Authenticator;
use crate::checks::{TestResult, as_text, run_captures, run_checks};
use crate::error::Result;
use crate::http::{CookieInfo, HttpClient, Outgoing, Response};
use crate::model::{Body, Folder, Request, Vars};
use crate::prepare::{Effective, Prepared, prepare};
use crate::script::{
    self, ChangeScope, Event, LogEntry, NextRequest, ScriptBody, ScriptError, ScriptHost,
    ScriptInfo, ScriptInput, ScriptOutput, ScriptRequest, ScriptResponse, ScriptScopes, VarChange,
};
use crate::vars::{Scope, VarStack};
use crate::workspace::Workspace;

/// Variable values set while running, over the values in the files. A
/// `None` hides a variable the files define.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Overlay {
    /// Global variables.
    pub globals: IndexMap<String, Option<String>>,
    /// Workspace variables.
    pub collection: IndexMap<String, Option<String>>,
    /// Environment variables.
    pub environment: IndexMap<String, Option<String>>,
    /// Local variables, which last for the run.
    pub local: IndexMap<String, Option<String>>,
}

impl Overlay {
    /// Records a change made by a script or a capture.
    pub fn apply(&mut self, change: &VarChange) {
        let map = match change.scope {
            ChangeScope::Global => &mut self.globals,
            ChangeScope::Collection => &mut self.collection,
            ChangeScope::Environment => &mut self.environment,
            ChangeScope::Local => &mut self.local,
        };
        map.insert(change.name.clone(), change.value.clone());
    }
}

fn overlaid(base: &Vars, overlay: &IndexMap<String, Option<String>>) -> Vars {
    let mut vars = base.clone();
    for (name, value) in overlay {
        match value {
            Some(value) => {
                vars.insert(name.clone(), value.clone());
            }
            None => {
                vars.shift_remove(name);
            }
        }
    }
    vars
}

/// The state a request runs in: the environment, values set so far, and
/// the iteration of a run.
#[derive(Debug, Clone)]
pub struct Context {
    /// Id of the selected environment.
    pub environment: Option<String>,
    /// Values set by scripts and captures so far.
    pub overlay: Overlay,
    /// The current row of a data file.
    pub data: serde_json::Map<String, Value>,
    /// Iteration of the run, from 0.
    pub iteration: usize,
    /// Number of iterations in the run.
    pub iteration_count: usize,
    /// How long one script may run.
    pub script_timeout: Duration,
}

impl Default for Context {
    fn default() -> Self {
        Self {
            environment: None,
            overlay: Overlay::default(),
            data: serde_json::Map::new(),
            iteration: 0,
            iteration_count: 1,
            script_timeout: script::DEFAULT_TIMEOUT,
        }
    }
}

/// Everything that happened when a request ran.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Execution {
    /// Path of the request in the workspace, if it is saved.
    pub path: Option<String>,
    /// Display name.
    pub name: String,
    /// The request as it went out.
    pub request: Option<Outgoing>,
    /// The response, if one arrived.
    pub response: Option<Response>,
    /// Why the request could not be sent or answered.
    pub error: Option<String>,
    /// Results of `gm.test` in scripts.
    pub tests: Vec<TestResult>,
    /// Results of the request's checks.
    pub checks: Vec<TestResult>,
    /// Console output of the scripts.
    pub logs: Vec<LogEntry>,
    /// Scripts that failed.
    pub script_errors: Vec<ScriptError>,
    /// Variables set or removed by scripts and captures, in order.
    pub changes: Vec<VarChange>,
    /// Variables referenced but not defined.
    pub missing_variables: Vec<String>,
    /// Problems that did not stop the request, such as failed captures.
    pub warnings: Vec<String>,
    /// Whether a pre-request script skipped the request.
    pub skipped: bool,
    /// Where a run goes next, if a script said.
    pub next_request: Option<NextRequest>,
}

impl Execution {
    /// Whether anything failed: an error, a script, a test or a check.
    pub fn failed(&self) -> bool {
        self.error.is_some()
            || !self.script_errors.is_empty()
            || self.tests.iter().chain(&self.checks).any(|t| !t.passed)
    }
}

/// Runs requests of a workspace.
#[derive(Debug, Clone)]
pub struct Executor {
    /// The workspace the requests belong to.
    pub workspace: Workspace,
    /// The client requests are sent with.
    pub http: Arc<HttpClient>,
    /// Authentication state, such as cached OAuth 2.0 tokens.
    pub authenticator: Arc<Authenticator>,
}

impl Executor {
    /// An executor with a fresh client and no cached tokens.
    pub fn new(workspace: Workspace) -> Self {
        Self {
            workspace,
            http: Arc::new(HttpClient::new()),
            authenticator: Arc::new(Authenticator::new()),
        }
    }

    /// Builds a request as it would be sent, without running its scripts or
    /// sending it, for showing it or writing it as code. Credentials that
    /// need a round trip (Digest, OAuth 2.0) are not included.
    pub fn prepare(
        &self,
        path: Option<&str>,
        request: &Request,
        context: &Context,
    ) -> Result<Prepared> {
        let chain = match path {
            Some(path) => self.workspace.folder_chain(path)?,
            None => vec![self.workspace.read_folder("")?],
        };
        let files = self.file_vars(context.environment.as_deref())?;
        let effective = Effective::new(&chain, request);
        let vars = var_stack(&files, &chain, request, context);
        prepare(
            request,
            &effective.headers,
            &effective.auth,
            &vars,
            self.workspace.root(),
        )
    }

    /// Runs a saved request.
    pub async fn execute_path(&self, path: &str, context: &mut Context) -> Execution {
        match self.workspace.read_request(path) {
            Ok(request) => self.execute(Some(path), &request, context).await,
            Err(e) => Execution {
                path: Some(path.to_owned()),
                name: path.to_owned(),
                error: Some(e.to_string()),
                ..Execution::default()
            },
        }
    }

    /// Runs a request. `path` places it in the workspace, for what it
    /// inherits; an unsaved request inherits from the workspace only.
    /// Variable changes are applied to `context` and listed in the result.
    pub async fn execute(
        &self,
        path: Option<&str>,
        request: &Request,
        context: &mut Context,
    ) -> Execution {
        let mut execution = Execution {
            path: path.map(str::to_owned),
            name: request.name.clone(),
            ..Execution::default()
        };
        if let Err(e) = self
            .execute_inner(path, request, context, &mut execution)
            .await
        {
            execution.error = Some(e.to_string());
        }
        execution
    }

    async fn execute_inner(
        &self,
        path: Option<&str>,
        request: &Request,
        context: &mut Context,
        execution: &mut Execution,
    ) -> Result<()> {
        let chain = match path {
            Some(path) => self.workspace.folder_chain(path)?,
            None => vec![self.workspace.read_folder("")?],
        };
        let files = self.file_vars(context.environment.as_deref())?;
        let mut request = request.clone();
        let effective = Effective::new(&chain, &request);
        let mut headers = effective.headers.clone();

        let info = ScriptInfo {
            request_name: request.name.clone(),
            request_path: path.unwrap_or_default().to_owned(),
            iteration: context.iteration,
            iteration_count: context.iteration_count,
        };
        let host = ScriptHost {
            http: Some((Arc::clone(&self.http), tokio::runtime::Handle::current())),
            transport: effective.transport,
            timeout: context.script_timeout,
        };

        // Pre-request scripts.
        if !effective.pre_request.is_empty() {
            let input = ScriptInput {
                event: Event::PreRequest,
                info: info.clone(),
                scopes: script_scopes(&files, &chain, &request, context),
                request: script_request(&request, &headers),
                response: None,
                cookies: self.cookies_for(&request.url, &files, &chain, context),
            };
            let output = run_scripts(host.clone(), effective.pre_request.clone(), input).await;
            if let Some(edited) = &output.request {
                request.method.clone_from(&edited.method);
                request.url.clone_from(&edited.url);
                headers.clone_from(&edited.headers);
                if let Some(text) = &edited.body {
                    apply_body_text(&mut request.body, text);
                }
            }
            let failed = absorb(output, context, execution);
            if failed {
                execution.error =
                    Some("a pre-request script failed, so the request was not sent".into());
                return Ok(());
            }
            if execution.skipped {
                return Ok(());
            }
        }

        // Send.
        let vars = var_stack(&files, &chain, &request, context);
        let prepared = prepare(
            &request,
            &headers,
            &effective.auth,
            &vars,
            self.workspace.root(),
        )?;
        execution.missing_variables = prepared.missing;
        let mut outgoing = prepared.outgoing;
        let sent = self
            .authenticator
            .send(
                &self.http,
                &effective.auth,
                &vars,
                &mut outgoing,
                effective.transport,
            )
            .await;
        execution.request = Some(outgoing);
        let response = match sent {
            Ok(response) => response,
            Err(e) => {
                execution.error = Some(e.to_string());
                return Ok(());
            }
        };

        // Post-response scripts, then checks and captures.
        if !effective.post_response.is_empty() {
            let input = ScriptInput {
                event: Event::PostResponse,
                info,
                scopes: script_scopes(&files, &chain, &request, context),
                request: script_request(&request, &headers),
                response: Some(ScriptResponse::from(&response)),
                cookies: self.cookies_for(&request.url, &files, &chain, context),
            };
            let output = run_scripts(host, effective.post_response.clone(), input).await;
            absorb(output, context, execution);
        }
        execution.checks = run_checks(&request.checks, &response);
        let captured = run_captures(&request.captures, &response);
        let scope = if context.environment.is_some() {
            ChangeScope::Environment
        } else {
            ChangeScope::Collection
        };
        for (name, value) in captured.values {
            let change = VarChange {
                scope,
                name,
                value: Some(value),
            };
            context.overlay.apply(&change);
            execution.changes.push(change);
        }
        execution.warnings.extend(captured.failures);
        execution.response = Some(response);
        Ok(())
    }

    fn file_vars(&self, environment: Option<&str>) -> Result<FileVars> {
        let local = self.workspace.read_local_state()?;
        let meta = self.workspace.read_folder("")?;
        let mut collection = meta.variables;
        for (name, value) in local.variables {
            collection.insert(name, value);
        }
        let (environment_vars, environment_name) = match environment {
            Some(id) => {
                let files = self.workspace.read_environment(id)?;
                (files.effective(), Some(files.shared.name))
            }
            None => (Vars::new(), None),
        };
        Ok(FileVars {
            globals: local.globals,
            collection,
            environment: environment_vars,
            environment_name,
        })
    }

    fn cookies_for(
        &self,
        raw_url: &str,
        files: &FileVars,
        chain: &[Folder],
        context: &Context,
    ) -> Vec<CookieInfo> {
        let vars = var_stack(files, chain, &Request::default(), context);
        let url = vars.render(raw_url);
        let host = url::Url::parse(&url)
            .or_else(|_| url::Url::parse(&format!("http://{url}")))
            .ok()
            .and_then(|u| u.host_str().map(str::to_owned));
        let Some(host) = host else {
            return Vec::new();
        };
        self.http
            .cookies()
            .into_iter()
            .filter(|c| {
                host == c.domain
                    || host.ends_with(&format!(".{}", c.domain.trim_start_matches('.')))
            })
            .collect()
    }
}

/// Takes in what scripts did. Returns whether a script failed.
fn absorb(output: ScriptOutput, context: &mut Context, execution: &mut Execution) -> bool {
    for change in &output.changes {
        context.overlay.apply(change);
    }
    execution.changes.extend(output.changes);
    execution.tests.extend(output.tests);
    execution.logs.extend(output.logs);
    execution.skipped |= output.skip_request;
    if output.next_request.is_some() {
        execution.next_request = output.next_request;
    }
    let failed = output.error.is_some();
    execution.script_errors.extend(output.error);
    failed
}

/// Values from the workspace's files, before anything set while running.
#[derive(Debug, Clone, Default)]
struct FileVars {
    globals: Vars,
    collection: Vars,
    environment: Vars,
    environment_name: Option<String>,
}

fn data_vars(data: &serde_json::Map<String, Value>) -> Vars {
    data.iter().map(|(k, v)| (k.clone(), as_text(v))).collect()
}

fn var_stack(files: &FileVars, chain: &[Folder], request: &Request, context: &Context) -> VarStack {
    let overlay = &context.overlay;
    let mut stack = VarStack::new();
    stack.push(Scope::Global, overlaid(&files.globals, &overlay.globals));
    stack.push(
        Scope::Workspace,
        overlaid(&files.collection, &overlay.collection),
    );
    stack.push(
        Scope::Environment,
        overlaid(&files.environment, &overlay.environment),
    );
    for folder in chain.iter().skip(1) {
        stack.push(Scope::Folder, folder.variables.clone());
    }
    stack.push(Scope::Request, request.variables.clone());
    stack.push(Scope::Data, data_vars(&context.data));
    stack.push(Scope::Local, overlaid(&Vars::new(), &overlay.local));
    stack
}

fn script_scopes(
    files: &FileVars,
    chain: &[Folder],
    request: &Request,
    context: &Context,
) -> ScriptScopes {
    let overlay = &context.overlay;
    let mut folder = Vars::new();
    for level in chain.iter().skip(1) {
        for (name, value) in &level.variables {
            folder.insert(name.clone(), value.clone());
        }
    }
    ScriptScopes {
        globals: overlaid(&files.globals, &overlay.globals),
        collection: overlaid(&files.collection, &overlay.collection),
        environment: overlaid(&files.environment, &overlay.environment),
        environment_name: files.environment_name.clone(),
        folder,
        request: request.variables.clone(),
        data: context.data.clone(),
        local: overlaid(&Vars::new(), &overlay.local),
    }
}

fn script_request(request: &Request, headers: &[crate::model::KeyValue]) -> ScriptRequest {
    let body = match &request.body {
        Body::None => None,
        Body::Json { content } | Body::Xml { content } | Body::Text { content, .. } => {
            Some(ScriptBody {
                mode: "raw".into(),
                raw: Some(content.clone()),
            })
        }
        Body::Graphql { query, .. } => Some(ScriptBody {
            mode: "graphql".into(),
            raw: Some(query.clone()),
        }),
        Body::Form { .. } => Some(ScriptBody {
            mode: "urlencoded".into(),
            raw: None,
        }),
        Body::Multipart { .. } => Some(ScriptBody {
            mode: "formdata".into(),
            raw: None,
        }),
        Body::Binary { .. } => Some(ScriptBody {
            mode: "file".into(),
            raw: None,
        }),
    };
    ScriptRequest {
        method: request.method.clone(),
        url: request.url.clone(),
        headers: headers.to_vec(),
        body,
    }
}

/// Puts text a script wrote into the body, if the body holds text.
fn apply_body_text(body: &mut Body, text: &str) {
    match body {
        Body::Json { content } | Body::Xml { content } | Body::Text { content, .. } => {
            if content != text {
                text.clone_into(content);
            }
        }
        Body::Graphql { query, .. } => text.clone_into(query),
        Body::None if !text.is_empty() => {
            *body = if serde_json::from_str::<Value>(text).is_ok() {
                Body::Json {
                    content: text.to_owned(),
                }
            } else {
                Body::Text {
                    content: text.to_owned(),
                    content_type: None,
                }
            };
        }
        _ => {}
    }
}

async fn run_scripts(
    host: ScriptHost,
    scripts: Vec<crate::prepare::Script>,
    input: ScriptInput,
) -> ScriptOutput {
    tokio::task::spawn_blocking(move || script::run(&host, &scripts, &input))
        .await
        .unwrap_or_else(|e| ScriptOutput {
            error: Some(ScriptError {
                origin: "getman".into(),
                message: format!("the script engine stopped unexpectedly: {e}"),
            }),
            ..ScriptOutput::default()
        })
}

/// Saves variable changes to the workspace's git-ignored local files, so
/// values such as tokens survive a restart: environment changes go to
/// `<environment>.local.yaml`, workspace and global ones to
/// `getman.local.yaml`. Local variables are not saved.
pub fn persist_changes(
    workspace: &Workspace,
    environment: Option<&str>,
    changes: &[VarChange],
) -> Result<()> {
    if changes.is_empty() {
        return Ok(());
    }
    let mut state = workspace.read_local_state()?;
    let mut state_changed = false;
    let mut environment_files = match environment {
        Some(id) => Some(workspace.read_environment(id)?),
        None => None,
    };
    for change in changes {
        let target = match change.scope {
            ChangeScope::Global => {
                state_changed = true;
                &mut state.globals
            }
            ChangeScope::Collection => {
                state_changed = true;
                &mut state.variables
            }
            ChangeScope::Environment => match environment_files.as_mut() {
                Some(files) => &mut files.local,
                None => continue,
            },
            ChangeScope::Local => continue,
        };
        match &change.value {
            Some(value) => {
                target.insert(change.name.clone(), value.clone());
            }
            None => {
                target.shift_remove(&change.name);
            }
        }
    }
    if state_changed {
        workspace.write_local_state(&state)?;
    }
    if let (Some(id), Some(files)) = (environment, environment_files) {
        workspace.write_environment(id, &files)?;
    }
    Ok(())
}

/// The variables a request would see, with the scope each value comes from,
/// for showing in an editor.
pub fn resolved_variables(
    workspace: &Workspace,
    path: Option<&str>,
    request: &Request,
    environment: Option<&str>,
) -> Result<Vec<crate::vars::ResolvedVar>> {
    let executor_files = {
        let local = workspace.read_local_state()?;
        let meta = workspace.read_folder("")?;
        let mut collection = meta.variables;
        for (name, value) in local.variables {
            collection.insert(name, value);
        }
        let environment_vars = match environment {
            Some(id) => workspace.read_environment(id)?.effective(),
            None => Vars::new(),
        };
        FileVars {
            globals: local.globals,
            collection,
            environment: environment_vars,
            environment_name: None,
        }
    };
    let chain = match path {
        Some(path) => workspace.folder_chain(path)?,
        None => vec![workspace.read_folder("")?],
    };
    let context = Context {
        environment: environment.map(str::to_owned),
        ..Context::default()
    };
    Ok(var_stack(&executor_files, &chain, request, &context).resolved())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{KeyValue, Scripts};
    use crate::test_server;
    use crate::workspace::EnvironmentFiles;

    fn setup() -> (tempfile::TempDir, Executor) {
        let dir = tempfile::tempdir().unwrap();
        let workspace = Workspace::init(dir.path(), "Test").unwrap();
        (dir, Executor::new(workspace))
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_request_runs_scripts_checks_and_captures() {
        let server = test_server::spawn().await;
        let (_dir, executor) = setup();
        let ws = &executor.workspace;
        let mut meta = ws.read_folder("").unwrap();
        meta.variables.insert("base".into(), server.url(""));
        meta.headers.push(KeyValue::new("X-Workspace", "yes"));
        meta.scripts.pre_request = "gm.variables.set('stamp', 'from-workspace');".into();
        ws.write_folder("", &meta).unwrap();
        let env = ws.create_environment("Dev").unwrap();
        ws.write_environment(
            &env,
            &EnvironmentFiles {
                shared: crate::model::Environment {
                    variables: [("who".to_owned(), "ada".to_owned())].into_iter().collect(),
                    ..crate::model::Environment::default()
                },
                local: Vars::new(),
            },
        )
        .unwrap();
        let folder = ws.create_folder("", "Users").unwrap();
        let request = Request {
            method: "POST".into(),
            url: "{{base}}/echo/{{who}}".into(),
            headers: vec![KeyValue::new("X-Stamp", "{{stamp}}")],
            body: Body::Json {
                content: r#"{"name": "{{who}}"}"#.into(),
            },
            scripts: Scripts {
                pre_request: "gm.request.headers.add({key: 'X-Added', value: 'by script'});".into(),
                post_response: r#"
                    gm.test("echoes", () => gm.expect(gm.response.json().path).to.equal("/echo/ada"));
                    gm.environment.set("lastMethod", gm.response.json().method);
                "#
                .into(),
            },
            checks: vec!["status == 200".into(), "json.body contains ada".into()],
            captures: [("echoedPath".to_owned(), "json.path".to_owned())].into_iter().collect(),
            ..Request::default()
        };
        let path = ws.create_request(&folder, "Echo", &request).unwrap();
        let mut context = Context {
            environment: Some(env.clone()),
            ..Context::default()
        };
        let execution = executor.execute_path(&path, &mut context).await;
        assert!(!execution.failed(), "{execution:#?}");
        let echo = execution.response.as_ref().unwrap().json().unwrap();
        assert_eq!(echo["headers"]["x-workspace"], "yes");
        assert_eq!(echo["headers"]["x-stamp"], "from-workspace");
        assert_eq!(echo["headers"]["x-added"], "by script");
        assert_eq!(echo["body"], r#"{"name": "ada"}"#);
        assert_eq!(execution.tests.len(), 1);
        assert_eq!(execution.checks.len(), 2);
        assert_eq!(
            context.overlay.environment.get("echoedPath"),
            Some(&Some("/echo/ada".to_owned()))
        );
        assert_eq!(
            context.overlay.environment.get("lastMethod"),
            Some(&Some("POST".to_owned()))
        );

        persist_changes(ws, Some(&env), &execution.changes).unwrap();
        let files = ws.read_environment(&env).unwrap();
        assert_eq!(files.local["lastMethod"], "POST");
        assert!(!files.shared.variables.contains_key("lastMethod"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_failing_pre_request_script_stops_the_request() {
        let (_dir, executor) = setup();
        let request = Request {
            url: "http://127.0.0.1:9/".into(),
            scripts: Scripts {
                pre_request: "throw new Error('no token');".into(),
                post_response: String::new(),
            },
            ..Request::default()
        };
        let execution = executor
            .execute(None, &request, &mut Context::default())
            .await;
        assert!(execution.request.is_none());
        assert_eq!(
            execution.script_errors[0].message,
            "Error: no token (line 1)"
        );
        assert!(execution.failed());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn transport_errors_and_missing_variables_are_reported() {
        let (_dir, executor) = setup();
        let request = Request {
            url: "http://127.0.0.1:9/{{nope}}".into(),
            ..Request::default()
        };
        let execution = executor
            .execute(None, &request, &mut Context::default())
            .await;
        assert!(
            execution
                .error
                .as_deref()
                .unwrap()
                .starts_with("Could not connect")
        );
        assert_eq!(execution.missing_variables, ["nope"]);
        assert!(execution.request.is_some());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn skipped_requests_are_not_sent() {
        let (_dir, executor) = setup();
        let request = Request {
            url: "http://127.0.0.1:9/".into(),
            scripts: Scripts {
                pre_request: "gm.execution.skipRequest();".into(),
                post_response: String::new(),
            },
            ..Request::default()
        };
        let execution = executor
            .execute(None, &request, &mut Context::default())
            .await;
        assert!(execution.skipped);
        assert!(execution.request.is_none());
        assert!(!execution.failed());
    }

    #[test]
    fn prepare_builds_the_request_without_sending_it() {
        let (_dir, executor) = setup();
        let request = Request {
            url: "https://example.com/{{a}}".into(),
            scripts: Scripts {
                pre_request: "throw new Error('not run');".into(),
                post_response: String::new(),
            },
            variables: [("a".to_owned(), "b".to_owned())].into_iter().collect(),
            ..Request::default()
        };
        let prepared = executor
            .prepare(None, &request, &Context::default())
            .unwrap();
        assert_eq!(prepared.outgoing.url, "https://example.com/b");
    }

    #[test]
    fn resolved_variables_show_where_values_come_from() {
        let (_dir, executor) = setup();
        let ws = &executor.workspace;
        let mut meta = ws.read_folder("").unwrap();
        meta.variables.insert("a".into(), "workspace".into());
        meta.variables.insert("b".into(), "workspace".into());
        ws.write_folder("", &meta).unwrap();
        let request = Request {
            variables: [("b".to_owned(), "request".to_owned())]
                .into_iter()
                .collect(),
            ..Request::default()
        };
        let resolved = resolved_variables(ws, None, &request, None).unwrap();
        let pairs: Vec<_> = resolved
            .iter()
            .map(|r| (r.name.as_str(), r.value.as_str(), r.scope))
            .collect();
        assert_eq!(
            pairs,
            [
                ("a", "workspace", Scope::Workspace),
                ("b", "request", Scope::Request)
            ]
        );
    }
}
