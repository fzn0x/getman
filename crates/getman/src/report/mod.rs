//! Reporting runs: in the terminal as they happen, and as JSON, JUnit
//! XML, HTML, CSV or Excel files for CI and for people.

pub(crate) mod csv;
pub(crate) mod html;
pub(crate) mod json;
pub(crate) mod junit;
pub(crate) mod xlsx;

use getman_core::checks::TestResult;
use getman_core::execute::Execution;
use getman_core::runner::{RunEvent, RunItem, RunSummary};
use getman_core::vars::missing_reason;
use serde::{Deserialize, Serialize};

use crate::style::{self, BOLD, CYAN, DIM, GREEN, RED, YELLOW, paint};

/// Everything a run reported, for the file reporters.
#[derive(Debug, Default)]
pub(crate) struct Recorded {
    pub(crate) iterations: usize,
    pub(crate) requests: Vec<(usize, Execution)>,
    pub(crate) summary: Option<RunSummary>,
    /// The requests planned for one iteration.
    pub(crate) items: Vec<RunItem>,
    /// What ran and where, for the reports' headings.
    pub(crate) about: About,
}

impl Recorded {
    pub(crate) fn record(&mut self, event: RunEvent) {
        match event {
            RunEvent::Start {
                iterations,
                requests,
            } => {
                self.iterations = iterations;
                self.items = requests;
            }
            RunEvent::Request {
                iteration,
                execution,
                ..
            } => self.requests.push((iteration, *execution)),
            RunEvent::Finish { summary } => self.summary = Some(summary),
            RunEvent::Iteration { .. } => {}
        }
    }

    /// The method of a request, also when it was never sent.
    pub(crate) fn method(&self, execution: &Execution) -> String {
        if let Some(request) = &execution.request {
            return request.method.clone();
        }
        self.items
            .iter()
            .find(|item| Some(&item.path) == execution.path.as_ref())
            .map(|item| item.method.clone())
            .unwrap_or_default()
    }
}

/// What a run covered, where and when.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct About {
    /// The workspace's display name.
    pub(crate) workspace: String,
    /// The folders and requests that ran; empty for the whole workspace.
    pub(crate) targets: Vec<String>,
    /// The environment's display name.
    pub(crate) environment: Option<String>,
    /// When the run started, in milliseconds since the Unix epoch.
    pub(crate) started: i64,
}

impl About {
    /// What ran, in words.
    pub(crate) fn what(&self) -> String {
        const SHOWN: usize = 3;
        if self.targets.iter().all(String::is_empty) {
            "every request in the workspace".to_owned()
        } else if self.targets.len() > SHOWN + 1 {
            format!(
                "{} and {} more",
                self.targets[..SHOWN].join(", "),
                self.targets.len() - SHOWN
            )
        } else {
            self.targets.join(", ")
        }
    }
}

/// A report written to a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Format {
    Json,
    Junit,
    Html,
    Csv,
    Xlsx,
}

impl Format {
    pub(crate) const ALL: [Self; 5] = [Self::Html, Self::Json, Self::Junit, Self::Csv, Self::Xlsx];

    /// The name used on the command line and in the API.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Junit => "junit",
            Self::Html => "html",
            Self::Csv => "csv",
            Self::Xlsx => "xlsx",
        }
    }

    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|format| format.name().eq_ignore_ascii_case(name))
    }

    /// The name people know the format by.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Json => "JSON",
            Self::Junit => "JUnit",
            Self::Html => "HTML",
            Self::Csv => "CSV",
            Self::Xlsx => "Excel",
        }
    }

    pub(crate) fn extension(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Junit => "xml",
            Self::Html => "html",
            Self::Csv => "csv",
            Self::Xlsx => "xlsx",
        }
    }

    pub(crate) fn content_type(self) -> &'static str {
        match self {
            Self::Json => "application/json",
            Self::Junit => "application/xml",
            Self::Html => "text/html; charset=utf-8",
            Self::Csv => "text/csv; charset=utf-8",
            Self::Xlsx => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        }
    }

    /// The report's bytes.
    pub(crate) fn render(self, recorded: &Recorded) -> Result<Vec<u8>, String> {
        Ok(match self {
            Self::Json => json::render(recorded).into_bytes(),
            Self::Junit => junit::render(recorded).into_bytes(),
            Self::Html => html::render(recorded).into_bytes(),
            Self::Csv => csv::render(recorded).into_bytes(),
            Self::Xlsx => xlsx::render(recorded)?,
        })
    }
}

/// How a request ended: `passed`, `failed` or `skipped`.
pub(crate) fn outcome(execution: &Execution) -> &'static str {
    if execution.failed() {
        "failed"
    } else if execution.skipped {
        "skipped"
    } else {
        "passed"
    }
}

/// Why a request failed, one line per reason.
pub(crate) fn failures(execution: &Execution) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(error) = &execution.error {
        out.push(error.clone());
    }
    for error in &execution.script_errors {
        out.push(format!("script in {}: {}", error.origin, error.message));
    }
    for result in execution.tests.iter().chain(&execution.checks) {
        if !result.passed {
            out.push(match &result.message {
                Some(message) => format!("{}: {message}", result.name),
                None => result.name.clone(),
            });
        }
    }
    out
}

/// Passed and failed counts of a list of tests or checks; skipped tests
/// count as neither.
pub(crate) fn tally(results: &[TestResult]) -> (usize, usize) {
    let failed = results.iter().filter(|r| !r.passed).count();
    let passed = results.iter().filter(|r| r.passed && !r.skipped).count();
    (passed, failed)
}

/// One request of one iteration, flattened for tables.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Row {
    /// The iteration, from 1.
    pub(crate) iteration: usize,
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) method: String,
    pub(crate) url: String,
    pub(crate) status: Option<u16>,
    pub(crate) status_text: String,
    pub(crate) time_ms: Option<f64>,
    pub(crate) first_byte_ms: Option<f64>,
    pub(crate) size: Option<u64>,
    /// `passed`, `failed` or `skipped`.
    pub(crate) result: &'static str,
    /// Passed and failed script tests.
    pub(crate) tests: (usize, usize),
    /// Passed and failed checks.
    pub(crate) checks: (usize, usize),
    pub(crate) failures: Vec<String>,
}

/// The rows of a run, in the order the requests ran.
pub(crate) fn rows(recorded: &Recorded) -> Vec<Row> {
    recorded
        .requests
        .iter()
        .map(|(iteration, execution)| {
            let response = execution.response.as_ref();
            Row {
                iteration: iteration + 1,
                name: execution.name.clone(),
                path: execution.path.clone().unwrap_or_default(),
                method: recorded.method(execution),
                url: execution
                    .request
                    .as_ref()
                    .map(|r| r.url.clone())
                    .unwrap_or_default(),
                status: response.map(|r| r.status),
                status_text: response.map(|r| r.status_text.clone()).unwrap_or_default(),
                time_ms: response.map(|r| r.time_ms),
                first_byte_ms: response.map(|r| r.headers_ms),
                size: response.map(|r| r.size.body),
                result: outcome(execution),
                tests: tally(&execution.tests),
                checks: tally(&execution.checks),
                failures: failures(execution),
            }
        })
        .collect()
}

/// A small run with a pass, a failed check, a request that could not be
/// sent and two iterations, for the reporters' tests.
#[cfg(test)]
pub(crate) fn sample() -> Recorded {
    use getman_core::http::{Outgoing, OutgoingBody, Payload, Response, Size};

    let sent = |url: &str| Outgoing {
        method: "GET".into(),
        url: url.into(),
        headers: Vec::new(),
        body: OutgoingBody::None,
    };
    let answer = Response {
        status: 200,
        status_text: "OK".into(),
        http_version: "HTTP/1.1".into(),
        url: "http://127.0.0.1:9/users".into(),
        headers: Vec::new(),
        body: Payload(b"[]".to_vec()),
        truncated: false,
        time_ms: 12.5,
        headers_ms: 8.0,
        size: Size {
            headers: 60,
            body: 2,
        },
        remote_addr: None,
    };
    let pass = |name: &str| TestResult {
        name: name.into(),
        passed: true,
        message: None,
        skipped: false,
    };
    let list = Execution {
        name: "List users, all".into(),
        path: Some("users/list.yaml".into()),
        request: Some(sent("http://127.0.0.1:9/users")),
        response: Some(answer),
        checks: vec![
            pass("status == 200"),
            TestResult {
                name: "json.length > 0".into(),
                passed: false,
                message: Some("json.length is 0; expected > 0".into()),
                skipped: false,
            },
        ],
        ..Execution::default()
    };
    let mut fixed = list.clone();
    fixed.checks.truncate(1);
    fixed.tests = vec![pass("has users")];
    let down = Execution {
        name: "=cmd".into(),
        path: Some("down.yaml".into()),
        request: Some(sent("http://127.0.0.1:9/down")),
        error: Some("Could not connect: connection refused".into()),
        ..Execution::default()
    };
    Recorded {
        iterations: 2,
        requests: vec![(0, list), (0, down), (1, fixed)],
        summary: Some(RunSummary {
            iterations: 2,
            requests: 3,
            failed_requests: 2,
            tests_passed: 1,
            checks_passed: 2,
            checks_failed: 1,
            duration_ms: 40.0,
            average_response_ms: 12.5,
            ..RunSummary::default()
        }),
        items: vec![
            RunItem {
                path: "users/list.yaml".into(),
                name: "List users, all".into(),
                method: "GET".into(),
            },
            RunItem {
                path: "down.yaml".into(),
                name: "=cmd".into(),
                method: "GET".into(),
            },
        ],
        about: About {
            workspace: "Shop & co".into(),
            targets: vec!["users".into(), "down.yaml".into()],
            environment: Some("staging".into()),
            started: 1_791_954_429_120,
        },
    }
}

/// Prints a run to the terminal as it happens.
pub(crate) struct Pretty {
    verbose: bool,
    workspace: String,
    environment: Option<String>,
    iterations: usize,
    name_width: usize,
    /// Methods of the planned requests, for requests that were not sent.
    methods: Vec<String>,
}

impl Pretty {
    pub(crate) fn new(verbose: bool, workspace: String, environment: Option<String>) -> Self {
        Self {
            verbose,
            workspace,
            environment,
            iterations: 1,
            name_width: 0,
            methods: Vec::new(),
        }
    }

    pub(crate) fn event(&mut self, event: &RunEvent) {
        match event {
            RunEvent::Start {
                requests,
                iterations,
            } => {
                self.iterations = *iterations;
                self.methods = requests.iter().map(|r| r.method.clone()).collect();
                self.name_width = requests
                    .iter()
                    .map(|r| r.name.chars().count())
                    .max()
                    .unwrap_or(0)
                    .min(40);
                let env = self
                    .environment
                    .as_ref()
                    .map(|e| format!(" with the {} environment", paint(BOLD, e)))
                    .unwrap_or_default();
                anstream::println!(
                    "{} {}{env}: {} {}{}",
                    paint(CYAN.bold(), "getman"),
                    paint(BOLD, &self.workspace),
                    requests.len(),
                    if requests.len() == 1 {
                        "request"
                    } else {
                        "requests"
                    },
                    if *iterations > 1 {
                        format!(" × {iterations} iterations")
                    } else {
                        String::new()
                    }
                );
            }
            RunEvent::Iteration { iteration } => {
                if self.iterations > 1 {
                    anstream::println!(
                        "\n{}",
                        paint(
                            BOLD,
                            &format!("Iteration {}/{}", iteration + 1, self.iterations)
                        )
                    );
                } else {
                    println!();
                }
            }
            RunEvent::Request {
                index, execution, ..
            } => self.request(*index, execution),
            RunEvent::Finish { summary } => print_summary(summary),
        }
    }

    fn request(&self, index: usize, execution: &Execution) {
        let method = execution
            .request
            .as_ref()
            .map(|r| r.method.as_str())
            .or_else(|| self.methods.get(index).map(String::as_str))
            .unwrap_or_default()
            .to_owned();
        let mark = if execution.skipped {
            paint(YELLOW, "–")
        } else if execution.failed() {
            paint(RED, "✗")
        } else {
            paint(GREEN, "✓")
        };
        let outcome = if execution.skipped {
            paint(YELLOW, "skipped")
        } else if let Some(response) = &execution.response {
            format!(
                "{} {}",
                paint(
                    style::status(response.status),
                    &format!("{} {}", response.status, response.status_text)
                ),
                paint(
                    DIM,
                    &format!(
                        "{} · {}",
                        style::duration(response.time_ms),
                        style::size(response.size.body)
                    )
                )
            )
        } else {
            String::new()
        };
        anstream::println!(
            "  {mark} {} {:<width$}  {outcome}",
            paint(DIM, &format!("{method:<7}")),
            execution.name,
            width = self.name_width
        );
        if let Some(error) = &execution.error {
            anstream::println!("      {}", paint(RED, error));
        }
        for error in &execution.script_errors {
            anstream::println!(
                "      {} {}",
                paint(RED, &format!("script in {}:", error.origin)),
                error.message
            );
        }
        for result in execution.tests.iter().chain(&execution.checks) {
            if result.skipped {
                anstream::println!("      {} {}", paint(YELLOW, "–"), paint(DIM, &result.name));
            } else if result.passed {
                anstream::println!("      {} {}", paint(GREEN, "✓"), paint(DIM, &result.name));
            } else {
                anstream::println!("      {} {}", paint(RED, "✗"), result.name);
                if let Some(message) = &result.message {
                    anstream::println!("        {}", paint(RED, message));
                }
            }
        }
        for name in &execution.missing_variables {
            anstream::println!("      {}", paint(YELLOW, &missing_reason(name)));
        }
        for warning in &execution.warnings {
            anstream::println!("      {}", paint(YELLOW, warning));
        }
        if self.verbose {
            for log in &execution.logs {
                anstream::println!(
                    "      {}",
                    paint(DIM, &format!("[{}] {}", log.level, log.message))
                );
            }
            if let Some(response) = &execution.response {
                anstream::println!(
                    "      {}",
                    paint(
                        DIM,
                        &format!(
                            "{} {}",
                            execution.request.as_ref().map_or("", |r| r.method.as_str()),
                            response.url
                        )
                    )
                );
                for header in &response.headers {
                    anstream::println!(
                        "      {}",
                        paint(DIM, &format!("{}: {}", header.name, header.value))
                    );
                }
                let text = response.text();
                let shown: String = text.chars().take(2000).collect();
                for line in shown.lines() {
                    anstream::println!("      {}", paint(DIM, line));
                }
                if text.chars().count() > 2000 {
                    anstream::println!("      {}", paint(DIM, "…"));
                }
            }
        }
    }
}

fn print_summary(summary: &RunSummary) {
    println!();
    let failed = |count: usize, what: &str| {
        if count == 0 {
            paint(DIM, &format!("{count} {what}"))
        } else {
            paint(RED.bold(), &format!("{count} {what}"))
        }
    };
    anstream::println!(
        "  {}  {} run · {} · {}",
        paint(BOLD, "Requests"),
        summary.requests,
        failed(summary.failed_requests, "failed"),
        paint(DIM, &format!("{} skipped", summary.skipped_requests))
    );
    anstream::println!(
        "  {}     {} passed · {}",
        paint(BOLD, "Tests"),
        summary.tests_passed,
        failed(summary.tests_failed, "failed")
    );
    anstream::println!(
        "  {}    {} passed · {}",
        paint(BOLD, "Checks"),
        summary.checks_passed,
        failed(summary.checks_failed, "failed")
    );
    anstream::println!(
        "  {}      {} total · {} average response",
        paint(BOLD, "Time"),
        style::duration(summary.duration_ms),
        style::duration(summary.average_response_ms)
    );
    for warning in &summary.warnings {
        style::warn(warning);
    }
    if let Some(reason) = &summary.stopped {
        anstream::println!("\n  {}", paint(YELLOW, &format!("Stopped: {reason}")));
    }
    let verdict = if summary.passed() && summary.stopped.is_none() {
        paint(GREEN.bold(), "All passed.")
    } else {
        paint(RED.bold(), "Failed.")
    };
    anstream::println!("\n  {verdict}");
}
