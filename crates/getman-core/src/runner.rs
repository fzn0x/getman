//! Running many requests: a folder or the whole workspace, in order, for
//! one or more iterations.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::data::Row;
use crate::error::{Error, Result};
use crate::execute::{Context, Execution, Executor, Overlay};
use crate::model::{Settings, Vars};
use crate::script::NextRequest;
use crate::workspace::Node;

/// What to run and how.
#[derive(Debug, Clone)]
pub struct RunOptions {
    /// A folder or request path; empty for the whole workspace.
    pub target: String,
    /// Folders and requests to run one after another, in this order,
    /// instead of `target`. Left empty, `target` runs.
    pub targets: Vec<String>,
    /// Id of the environment to use.
    pub environment: Option<String>,
    /// Number of iterations. Defaults to the number of data rows, or 1.
    pub iterations: Option<usize>,
    /// Data rows, one per iteration; the last row repeats if there are
    /// more iterations than rows.
    pub data: Vec<Row>,
    /// Pause between requests.
    pub delay: Duration,
    /// Stop at the first failure.
    pub bail: bool,
    /// Variables that override every scope at the start.
    pub vars: Vars,
    /// How long one script may run.
    pub script_timeout: Duration,
    /// Transport settings that override every file's.
    pub settings: Settings,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            target: String::new(),
            targets: Vec::new(),
            environment: None,
            iterations: None,
            data: Vec::new(),
            delay: Duration::ZERO,
            bail: false,
            vars: Vars::new(),
            script_timeout: crate::script::DEFAULT_TIMEOUT,
            settings: Settings::default(),
        }
    }
}

/// Progress of a run, reported as it happens.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum RunEvent {
    /// The run started.
    Start {
        /// Requests in one iteration.
        requests: Vec<RunItem>,
        /// Number of iterations.
        iterations: usize,
    },
    /// An iteration started.
    Iteration {
        /// The iteration, from 0.
        iteration: usize,
    },
    /// A request finished.
    Request {
        /// The iteration, from 0.
        iteration: usize,
        /// Position of the request in the run's list.
        index: usize,
        /// What happened.
        execution: Box<Execution>,
    },
    /// The run finished.
    Finish {
        /// Totals.
        summary: RunSummary,
    },
}

/// A request in a run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunItem {
    /// Path in the workspace.
    pub path: String,
    /// Display name.
    pub name: String,
    /// HTTP method.
    pub method: String,
}

/// Totals of a run.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RunSummary {
    /// Iterations started.
    pub iterations: usize,
    /// Requests executed, skipped ones excluded.
    pub requests: usize,
    /// Requests with an error, a failed script, test or check.
    pub failed_requests: usize,
    /// Requests skipped by their scripts.
    pub skipped_requests: usize,
    /// Passed `gm.test` results.
    pub tests_passed: usize,
    /// Failed `gm.test` results.
    pub tests_failed: usize,
    /// Passed checks.
    pub checks_passed: usize,
    /// Failed checks.
    pub checks_failed: usize,
    /// Wall time of the run in milliseconds.
    pub duration_ms: f64,
    /// Mean response time in milliseconds.
    pub average_response_ms: f64,
    /// Why the run ended early, if it did.
    pub stopped: Option<String>,
    /// Problems that did not stop the run.
    pub warnings: Vec<String>,
}

impl RunSummary {
    /// Whether nothing failed.
    pub fn passed(&self) -> bool {
        self.failed_requests == 0 && self.tests_failed == 0 && self.checks_failed == 0
    }
}

/// The outcome of a run.
#[derive(Debug, Clone)]
pub struct RunReport {
    /// Totals.
    pub summary: RunSummary,
    /// Variable values the run set, for saving if wanted.
    pub overlay: Overlay,
}

/// The requests a target covers, in order.
pub fn requests_in(tree: &[Node], target: &str) -> Result<Vec<RunItem>> {
    fn collect(nodes: &[Node], out: &mut Vec<RunItem>) {
        for node in nodes {
            match node {
                Node::Folder { children, .. } => collect(children, out),
                Node::Request {
                    path,
                    name,
                    method,
                    error,
                } => {
                    if error.is_none() {
                        out.push(RunItem {
                            path: path.clone(),
                            name: name.clone(),
                            method: method.clone(),
                        });
                    }
                }
            }
        }
    }
    fn find<'a>(nodes: &'a [Node], target: &str) -> Option<&'a Node> {
        nodes.iter().find_map(|node| {
            if node.path() == target {
                return Some(node);
            }
            match node {
                Node::Folder { children, .. } => find(children, target),
                Node::Request { .. } => None,
            }
        })
    }

    let target = target.trim_matches('/');
    let mut items = Vec::new();
    if target.is_empty() {
        collect(tree, &mut items);
    } else {
        let node = find(tree, target).ok_or_else(|| Error::NotFound(target.to_owned()))?;
        collect(std::slice::from_ref(node), &mut items);
    }
    Ok(items)
}

/// The requests several targets cover, one target after another. A
/// request named twice runs twice.
pub fn requests_in_all(tree: &[Node], targets: &[String]) -> Result<Vec<RunItem>> {
    let mut items = Vec::new();
    for target in targets {
        items.extend(requests_in(tree, target)?);
    }
    Ok(items)
}

/// The requests a run covers: its `targets` when it has some, else its
/// `target`.
pub fn planned(tree: &[Node], options: &RunOptions) -> Result<Vec<RunItem>> {
    if options.targets.is_empty() {
        requests_in(tree, &options.target)
    } else {
        requests_in_all(tree, &options.targets)
    }
}

/// Runs the requests a target covers. Events are reported as they happen;
/// setting `cancel` stops the run after the current request.
pub async fn run(
    executor: &Executor,
    options: &RunOptions,
    cancel: &AtomicBool,
    mut on_event: impl FnMut(RunEvent),
) -> Result<RunReport> {
    let started = Instant::now();
    let items = planned(&executor.workspace.tree()?, options)?;
    let iterations = options
        .iterations
        .unwrap_or(options.data.len().max(1))
        .max(1);
    on_event(RunEvent::Start {
        requests: items.clone(),
        iterations,
    });

    let mut summary = RunSummary::default();
    let mut overlay = Overlay::default();
    for (name, value) in &options.vars {
        overlay.local.insert(name.clone(), Some(value.clone()));
    }
    let mut response_time_total = 0.0;
    let mut responses = 0_usize;
    let step_limit = items.len() * 100 + 1000;

    'iterations: for iteration in 0..iterations {
        summary.iterations += 1;
        on_event(RunEvent::Iteration { iteration });
        let mut context = Context {
            environment: options.environment.clone(),
            overlay: std::mem::take(&mut overlay),
            data: options
                .data
                .get(iteration)
                .or(options.data.last())
                .cloned()
                .unwrap_or_default(),
            iteration,
            iteration_count: iterations,
            script_timeout: options.script_timeout,
            settings: options.settings.clone(),
            cancel: None,
        };
        let mut index = 0;
        let mut steps = 0;
        while index < items.len() {
            if cancel.load(Ordering::Relaxed) {
                summary.stopped = Some("cancelled".into());
                overlay = context.overlay;
                break 'iterations;
            }
            steps += 1;
            if steps > step_limit {
                summary.warnings.push(format!(
                    "iteration {} ran {step_limit} requests; setNextRequest probably loops, so the iteration was stopped",
                    iteration + 1
                ));
                break;
            }
            if steps > 1 && !options.delay.is_zero() {
                tokio::time::sleep(options.delay).await;
            }

            let item = &items[index];
            let mut execution = executor.execute_path(&item.path, &mut context).await;
            if execution.name.is_empty() {
                execution.name.clone_from(&item.name);
            }
            if execution.skipped {
                summary.skipped_requests += 1;
            } else {
                summary.requests += 1;
            }
            if execution.failed() {
                summary.failed_requests += 1;
            }
            for test in &execution.tests {
                if test.passed {
                    summary.tests_passed += usize::from(!test.skipped);
                } else {
                    summary.tests_failed += 1;
                }
            }
            for check in &execution.checks {
                if check.passed {
                    summary.checks_passed += 1;
                } else {
                    summary.checks_failed += 1;
                }
            }
            if let Some(response) = &execution.response {
                response_time_total += response.time_ms;
                responses += 1;
            }
            let failed = execution.failed();
            let next = execution.next_request.clone();
            on_event(RunEvent::Request {
                iteration,
                index,
                execution: Box::new(execution),
            });

            if failed && options.bail {
                summary.stopped = Some(format!(
                    "{} failed and the run stops at the first failure",
                    item.name
                ));
                overlay = context.overlay;
                break 'iterations;
            }
            index = match next {
                None => index + 1,
                Some(NextRequest::Stop) => break,
                Some(NextRequest::Jump(name)) => {
                    if let Some(found) = items.iter().position(|i| i.name == name || i.path == name)
                    {
                        found
                    } else {
                        summary.warnings.push(format!(
                            "setNextRequest({name:?}) names no request in this run; the iteration was stopped"
                        ));
                        break;
                    }
                }
            };
        }
        overlay = context.overlay;
    }

    summary.duration_ms = (started.elapsed().as_secs_f64() * 1000.0 * 100.0).round() / 100.0;
    if responses > 0 {
        #[allow(clippy::cast_precision_loss)]
        let mean = response_time_total / responses as f64;
        summary.average_response_ms = (mean * 100.0).round() / 100.0;
    }
    on_event(RunEvent::Finish {
        summary: summary.clone(),
    });
    Ok(RunReport { summary, overlay })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Request, Scripts};
    use crate::test_server;
    use crate::workspace::Workspace;

    fn setup(base: &str) -> (tempfile::TempDir, Executor) {
        let dir = tempfile::tempdir().unwrap();
        let workspace = Workspace::init(dir.path(), "Run").unwrap();
        let mut meta = workspace.read_folder("").unwrap();
        meta.variables.insert("base".into(), base.to_owned());
        workspace.write_folder("", &meta).unwrap();
        (dir, Executor::new(workspace))
    }

    fn request(url: &str, checks: &[&str], post: &str) -> Request {
        Request {
            url: url.into(),
            checks: checks.iter().map(|c| (*c).to_owned()).collect(),
            scripts: Scripts {
                pre_request: String::new(),
                post_response: post.into(),
            },
            ..Request::default()
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn runs_every_request_for_every_data_row() {
        let server = test_server::spawn().await;
        let (_dir, executor) = setup(&server.url(""));
        let ws = &executor.workspace;
        let folder = ws.create_folder("", "Flow").unwrap();
        ws.create_request(
            &folder,
            "A first",
            &request("{{base}}/echo/{{name}}", &["status == 200"], ""),
        )
        .unwrap();
        ws.create_request(
            &folder,
            "B second",
            &request(
                "{{base}}/status/404",
                &["status == 200"],
                "gm.test('row', () => gm.expect(gm.iterationData.get('name')).to.be.a('string'));",
            ),
        )
        .unwrap();
        ws.create_request("", "Outside", &request("{{base}}/echo", &[], ""))
            .unwrap();

        let mut events = Vec::new();
        let options = RunOptions {
            target: folder,
            data: crate::data::parse_csv("name\nada\ngrace\n").unwrap(),
            ..RunOptions::default()
        };
        let report = run(&executor, &options, &AtomicBool::new(false), |e| {
            events.push(e);
        })
        .await
        .unwrap();
        let summary = report.summary;
        assert_eq!(summary.iterations, 2);
        assert_eq!(summary.requests, 4);
        assert_eq!(summary.failed_requests, 2);
        assert_eq!((summary.checks_passed, summary.checks_failed), (2, 2));
        assert_eq!(summary.tests_passed, 2);
        assert!(!summary.passed());
        let paths: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                RunEvent::Request { execution, .. } => {
                    execution.response.as_ref().map(|r| r.url.clone())
                }
                _ => None,
            })
            .collect();
        assert!(paths[0].ends_with("/echo/ada"), "{paths:?}");
        assert!(paths[2].ends_with("/echo/grace"), "{paths:?}");
        assert!(matches!(events.last(), Some(RunEvent::Finish { .. })));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn bail_stops_at_the_first_failure() {
        let server = test_server::spawn().await;
        let (_dir, executor) = setup(&server.url(""));
        let ws = &executor.workspace;
        ws.create_request(
            "",
            "1 fails",
            &request("{{base}}/status/500", &["status == 200"], ""),
        )
        .unwrap();
        ws.create_request("", "2 never runs", &request("{{base}}/echo", &[], ""))
            .unwrap();
        let options = RunOptions {
            bail: true,
            ..RunOptions::default()
        };
        let report = run(&executor, &options, &AtomicBool::new(false), |_| {})
            .await
            .unwrap();
        assert_eq!(report.summary.requests, 1);
        assert!(
            report
                .summary
                .stopped
                .as_deref()
                .unwrap()
                .contains("1 fails")
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn set_next_request_jumps_and_stops() {
        let server = test_server::spawn().await;
        let (_dir, executor) = setup(&server.url(""));
        let ws = &executor.workspace;
        let count = r"
            const n = Number(gm.collectionVariables.get('n') || 0) + 1;
            gm.collectionVariables.set('n', n);
            gm.execution.setNextRequest(n < 3 ? 'Again' : null);
        ";
        ws.create_request("", "Again", &request("{{base}}/echo", &[], count))
            .unwrap();
        ws.create_request("", "Zz not reached", &request("{{base}}/echo", &[], ""))
            .unwrap();
        let mut collected = Vec::new();
        let report = run(
            &executor,
            &RunOptions::default(),
            &AtomicBool::new(false),
            |e| {
                if let RunEvent::Request { execution, .. } = e {
                    collected.push(execution.name.clone());
                }
            },
        )
        .await
        .unwrap();
        assert_eq!(collected, ["Again", "Again", "Again"]);
        assert_eq!(report.summary.warnings, Vec::<String>::new());
        assert_eq!(
            report.overlay.collection.get("n"),
            Some(&Some("3".to_owned()))
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn targets_run_in_the_order_given() {
        let server = test_server::spawn().await;
        let (_dir, executor) = setup(&server.url(""));
        let ws = &executor.workspace;
        let folder = ws.create_folder("", "Flow").unwrap();
        ws.create_request(&folder, "A", &request("{{base}}/echo/a", &[], ""))
            .unwrap();
        ws.create_request(&folder, "B", &request("{{base}}/echo/b", &[], ""))
            .unwrap();
        let last = ws
            .create_request("", "Last", &request("{{base}}/echo/last", &[], ""))
            .unwrap();
        let options = RunOptions {
            targets: vec![last, "flow/b.yaml".into(), folder],
            ..RunOptions::default()
        };
        let mut names = Vec::new();
        run(&executor, &options, &AtomicBool::new(false), |e| {
            if let RunEvent::Request { execution, .. } = e {
                names.push(execution.name.clone());
            }
        })
        .await
        .unwrap();
        assert_eq!(names, ["Last", "B", "A", "B"]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn cancelling_stops_before_the_next_request() {
        let server = test_server::spawn().await;
        let (_dir, executor) = setup(&server.url(""));
        executor
            .workspace
            .create_request("", "One", &request("{{base}}/echo", &[], ""))
            .unwrap();
        let cancel = AtomicBool::new(true);
        let report = run(&executor, &RunOptions::default(), &cancel, |_| {})
            .await
            .unwrap();
        assert_eq!(report.summary.requests, 0);
        assert_eq!(report.summary.stopped.as_deref(), Some("cancelled"));
    }

    #[test]
    fn targets_select_requests() {
        let tree = vec![
            Node::Folder {
                path: "a".into(),
                name: "A".into(),
                children: vec![Node::Request {
                    path: "a/x.yaml".into(),
                    name: "X".into(),
                    method: "GET".into(),
                    error: None,
                }],
            },
            Node::Request {
                path: "y.yaml".into(),
                name: "Y".into(),
                method: "POST".into(),
                error: None,
            },
        ];
        let names = |target: &str| -> Vec<String> {
            requests_in(&tree, target)
                .unwrap()
                .into_iter()
                .map(|i| i.name)
                .collect()
        };
        assert_eq!(names(""), ["X", "Y"]);
        assert_eq!(names("a"), ["X"]);
        assert_eq!(names("y.yaml"), ["Y"]);
        assert!(requests_in(&tree, "nope").is_err());

        let all = |targets: &[&str]| -> Vec<String> {
            let targets: Vec<String> = targets.iter().map(|t| (*t).to_owned()).collect();
            requests_in_all(&tree, &targets)
                .unwrap()
                .into_iter()
                .map(|i| i.name)
                .collect()
        };
        assert_eq!(all(&["y.yaml", "a"]), ["Y", "X"]);
        assert_eq!(all(&["a/x.yaml", "a/x.yaml"]), ["X", "X"]);
        assert!(requests_in_all(&tree, &["a".into(), "nope".into()]).is_err());
    }
}
