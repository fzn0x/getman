//! `getman run`: run a folder or the whole workspace.

use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use getman_core::execute::{Executor, persist_changes};
use getman_core::runner::{self, RunEvent, RunOptions};
use getman_core::script::{ChangeScope, VarChange};

use super::Outcome;
use crate::cli::{ReporterKind, RunArgs};
use crate::report::{self, Pretty, Recorded};
use crate::workspaces;

pub(crate) async fn run(args: RunArgs) -> Outcome {
    let workspace = workspaces::find(args.workspace.workspace.as_deref())?;
    let target = match &args.target {
        Some(target) => workspaces::item_path(&workspace, target)?,
        None => String::new(),
    };
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
        target,
        environment: environment.clone(),
        iterations: args.iterations,
        data,
        delay: Duration::from_millis(args.delay),
        bail: args.bail,
        vars: args.vars.vars.iter().cloned().collect(),
        script_timeout: super::script_timeout(&args.vars),
        settings: args.net.settings(),
    };
    let executor = Executor::new(workspace.clone());
    let environment_name = environment.as_ref().map(|id| {
        workspace
            .read_environment(id)
            .map_or_else(|_| id.clone(), |files| files.shared.name)
    });

    let mut pretty = Pretty::new(args.verbose, workspace.name(), environment_name);
    let mut recorded = Recorded::default();
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let cancel = Arc::clone(&cancel);
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        });
    }

    let report = runner::run(&executor, &options, &cancel, |event| {
        if show_cli {
            pretty.event(&event);
        }
        recorded.record(event);
    })
    .await
    .map_err(|e| e.to_string())?;

    for reporter in &reporters {
        let output = match reporter.kind {
            ReporterKind::Cli => continue,
            ReporterKind::Json => report::json::render(&recorded),
            ReporterKind::Junit => report::junit::render(&recorded, &workspace.name()),
        };
        match &reporter.path {
            Some(path) => {
                if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("{}: {e}", parent.display()))?;
                }
                std::fs::write(path, output).map_err(|e| format!("{}: {e}", path.display()))?;
                crate::style::note(&format!(
                    "Wrote the {} report to {}",
                    match reporter.kind {
                        ReporterKind::Json => "JSON",
                        _ => "JUnit",
                    },
                    path.display()
                ));
            }
            None => println!("{output}"),
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

    Ok(
        if report.summary.passed() && report.summary.stopped.is_none() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        },
    )
}

impl Recorded {
    pub(crate) fn record(&mut self, event: RunEvent) {
        match event {
            RunEvent::Start { iterations, .. } => self.iterations = iterations,
            RunEvent::Request {
                iteration,
                execution,
                ..
            } => self.requests.push((iteration, *execution)),
            RunEvent::Finish { summary } => self.summary = Some(summary),
            RunEvent::Iteration { .. } => {}
        }
    }
}
