//! The JSON report: totals and every request's outcome, without bodies.

use serde_json::{Value, json};

use super::Recorded;

pub(crate) fn render(recorded: &Recorded) -> String {
    let requests: Vec<Value> = recorded
        .requests
        .iter()
        .map(|(iteration, execution)| {
            json!({
                "iteration": iteration,
                "name": execution.name,
                "path": execution.path,
                "method": execution.request.as_ref().map(|r| r.method.clone()),
                "url": execution.request.as_ref().map(|r| r.url.clone()),
                "status": execution.response.as_ref().map(|r| r.status),
                "time_ms": execution.response.as_ref().map(|r| r.time_ms),
                "timings": execution.response.as_ref().and_then(|r| r.timings),
                "http_version": execution.response.as_ref().map(|r| r.http_version.clone()),
                "size": execution.response.as_ref().map(|r| r.size.body),
                "passed": !execution.failed(),
                "skipped": execution.skipped,
                "error": execution.error,
                "script_errors": execution.script_errors,
                "tests": execution.tests,
                "checks": execution.checks,
                "missing_variables": execution.missing_variables,
                "warnings": execution.warnings,
                "logs": execution.logs,
            })
        })
        .collect();
    let report = json!({
        "getman": env!("CARGO_PKG_VERSION"),
        "iterations": recorded.iterations,
        "summary": recorded.summary,
        "requests": requests,
    });
    serde_json::to_string_pretty(&report).unwrap_or_default()
}
