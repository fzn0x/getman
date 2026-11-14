//! `getman run`: run folders, requests or the whole workspace.

use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use getman_core::execute::{Executor, persist_changes};
use getman_core::runner::{self, RunOptions};
use getman_core::script::{ChangeScope, VarChange};

use super::Outcome;
use crate::cli::{ReporterKind, RunArgs};
use crate::mask::Secrets;
use crate::report::{self, Pretty, Recorded};
use crate::workspaces;

pub(crate) async fn run(args: RunArgs) -> Outcome {
    let workspace = workspaces::find(args.workspace.workspace.as_deref())?;
    let targets = args
        .targets
        .iter()
        .map(|target| workspaces::item_path(&workspace, target))
        .collect::<Result<Vec<_>, _>>()?;
    let environment = workspaces::environment(&workspace, args.vars.env.as_deref())?;
    let data = match &args.data {
        Some(path) => getman_core::data::read(path).map_err(|e| e.to_string())?,
        None => Vec::new(),
    };

    let mut reporters = args.reporters.clone();
    if reporters.is_empty() {
        reporters.push(crate::cli::Reporter {
            kind: ReporterKind::Cli,
            path: None,
        });
    }
    let show_cli = reporters.iter().any(|r| r.kind == ReporterKind::Cli);

    let options = RunOptions {
        targets: targets.clone(),
        environment: environment.clone(),
        iterations: args.iterations,
        data,
        delay: Duration::from_millis(args.delay),
        bail: args.bail,
        vars: args.vars.vars.iter().cloned().collect(),
        script_timeout: super::script_timeout(&args.vars),
        settings: args.net.settings(),
        no_scripts: args.no_scripts,
        ..RunOptions::default()
    };
    let executor = Executor::new(workspace.clone());
    // What the run prints, reports and keeps hides the environment's
    // secrets, also the values given to them for this run.
    let mut secrets = Secrets::of(&workspace, environment.as_deref());
    secrets.learn(
        options
            .vars
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str())),
    );
    secrets.learn_rows(&options.data);
    let environment_name = environment.as_ref().map(|id| {
        workspace
            .read_environment(id)
            .map_or_else(|_| id.clone(), |files| files.shared.name)
    });

    let mut pretty = Pretty::new(args.verbose, workspace.name(), environment_name.clone());
    let mut recorded = Recorded {
        about: report::About {
            workspace: workspace.name(),
            targets,
            environment: environment_name,
            started: getman_core::time::now_millis(),
        },
        ..Recorded::default()
    };
    let mut stored = args.save.then(|| {
        crate::runs::StoredRun::new(getman_core::dynamic::uuid_v4(), recorded.about.clone())
    });
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let cancel = Arc::clone(&cancel);
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        });
    }

    let report = runner::run(&executor, &options, &cancel, |mut event| {
        secrets.mask_event(&mut event);
        if show_cli {
            pretty.event(&event);
        }
        if let Some(stored) = &mut stored {
            stored.record(&event);
        }
        recorded.record(event);
    })
    .await
    .map_err(|e| e.to_string())?;

    for reporter in &reporters {
        let Some(format) = reporter.kind.format() else {
            continue;
        };
        let output = format.render(&recorded)?;
        match &reporter.path {
            Some(path) => {
                if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("{}: {e}", parent.display()))?;
                }
                std::fs::write(path, output).map_err(|e| format!("{}: {e}", path.display()))?;
                crate::style::note(&format!(
                    "Wrote the {} report to {}",
                    format.label(),
                    path.display()
                ));
            }
            None => {
                use std::io::Write as _;
                let mut out = std::io::stdout().lock();
                out.write_all(&output)
                    .and_then(|()| {
                        out.write_all(
                            b"
",
                        )
                    })
                    .map_err(|e| e.to_string())?;
            }
        }
    }

    if let Some(stored) = &stored {
        match crate::runs::save(&workspace, stored) {
            Ok(()) => {
                crate::style::note("Kept the run in .getman/runs; the Runner in the UI lists it");
            }
            Err(e) => crate::style::warn(&format!("the run could not be kept: {e}")),
        }
    }

    if args.persist {
        let overlay = &report.overlay;
        let mut changes = Vec::new();
        for (scope, map) in [
            (ChangeScope::Global, &overlay.globals),
            (ChangeScope::Collection, &overlay.collection),
            (ChangeScope::Environment, &overlay.environment),
        ] {
            for (name, value) in map {
                changes.push(VarChange {
                    scope,
                    name: name.clone(),
                    value: value.clone(),
                });
            }
        }
        persist_changes(&workspace, environment.as_deref(), &changes).map_err(|e| e.to_string())?;
    }

    Ok(if report.summary.succeeded() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use clap::Parser as _;
    use getman_core::model::{Request, Scripts};
    use getman_core::workspace::Workspace;

    use super::*;
    use crate::cli::{Cli, Command};

    /// A workspace whose first request runs `post` after a check, and a
    /// second request the run should never reach.
    async fn workspace(check: &str, post: &str) -> tempfile::TempDir {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, Router::new().fallback(|| async { "ok" })).await
        });
        let dir = tempfile::tempdir().unwrap();
        let workspace = Workspace::init(dir.path(), "Stop").unwrap();
        for (name, post) in [("Login", post), ("Never", "")] {
            let request = Request {
                url: format!("http://{address}/{name}"),
                checks: vec![check.to_owned()],
                scripts: Scripts {
                    pre_request: String::new(),
                    post_response: post.to_owned(),
                },
                ..Request::default()
            };
            workspace.create_request("", name, &request).unwrap();
        }
        dir
    }

    /// Runs `getman run` on the workspace, and returns its exit status and
    /// the JSON report.
    async fn run_workspace(dir: &tempfile::TempDir) -> (ExitCode, serde_json::Value) {
        let report = dir.path().join("report.json");
        let cli = Cli::try_parse_from([
            "getman".to_owned(),
            "run".to_owned(),
            "-w".to_owned(),
            dir.path().display().to_string(),
            "-r".to_owned(),
            format!("json={}", report.display()),
        ])
        .unwrap();
        let Some(Command::Run(args)) = cli.command else {
            panic!("not a run")
        };
        let status = run(args).await.unwrap();
        let report = std::fs::read_to_string(report).unwrap();
        (status, serde_json::from_str(&report).unwrap())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_run_a_script_stops_on_purpose_passes_unless_something_failed() {
        let stop = "gm.test('signed in', () => {}); gm.execution.stopExecution();";
        let dir = workspace("status == 200", stop).await;
        let (status, report) = run_workspace(&dir).await;
        assert_eq!(report["summary"]["stopped"], "Login stopped the run");
        assert_eq!(report["summary"]["requests"], 1);
        assert_eq!(status, ExitCode::SUCCESS, "{report:#}");

        let dir = workspace("status == 201", stop).await;
        let (status, report) = run_workspace(&dir).await;
        assert_eq!(report["summary"]["stopped"], "Login stopped the run");
        assert_eq!(status, ExitCode::FAILURE, "{report:#}");
    }
}
