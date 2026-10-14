//! Reporting runs: in the terminal as they happen, and as JSON or JUnit
//! XML files for CI.

pub(crate) mod json;
pub(crate) mod junit;

use getman_core::execute::Execution;
use getman_core::runner::{RunEvent, RunSummary};

use crate::style::{self, BOLD, CYAN, DIM, GREEN, RED, YELLOW, paint};

/// Everything a run reported, for the file reporters.
#[derive(Debug, Default)]
pub(crate) struct Recorded {
    pub(crate) iterations: usize,
    pub(crate) requests: Vec<(usize, Execution)>,
    pub(crate) summary: Option<RunSummary>,
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
            anstream::println!(
                "      {}",
                paint(YELLOW, &format!("{{{{{name}}}}} is not defined"))
            );
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
