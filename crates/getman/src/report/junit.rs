//! The JUnit XML report, which CI services show as test results: one
//! test suite per request, one test case per test or check.

use std::fmt::Write as _;

use getman_core::time::iso8601;

use super::Recorded;

pub(crate) fn render(recorded: &Recorded) -> String {
    let workspace = &recorded.about.workspace;
    let mut suites = String::new();
    let (mut total, mut failures, mut errors) = (0_usize, 0_usize, 0_usize);
    let mut seconds_total = 0.0;
    let timestamp = iso8601(std::time::SystemTime::now());

    for (iteration, execution) in &recorded.requests {
        let name = if recorded.iterations > 1 {
            format!("{} (iteration {})", execution.name, iteration + 1)
        } else {
            execution.name.clone()
        };
        let classname = execution
            .path
            .clone()
            .unwrap_or_else(|| execution.name.clone());
        let seconds = execution
            .response
            .as_ref()
            .map_or(0.0, |r| r.time_ms / 1000.0);
        seconds_total += seconds;

        let mut cases = String::new();
        let (mut suite_tests, mut suite_failures, mut suite_errors) = (0, 0, 0);
        let mut skipped = 0;
        if let Some(error) = &execution.error {
            suite_tests += 1;
            suite_errors += 1;
            let _ = writeln!(
                cases,
                "    <testcase name=\"request\" classname=\"{}\" time=\"{seconds:.3}\">\n      <error message=\"{}\" type=\"RequestError\"/>\n    </testcase>",
                escape(&classname),
                escape(error)
            );
        }
        for script_error in &execution.script_errors {
            suite_tests += 1;
            suite_errors += 1;
            let _ = writeln!(
                cases,
                "    <testcase name=\"script in {}\" classname=\"{}\" time=\"0\">\n      <error message=\"{}\" type=\"ScriptError\"/>\n    </testcase>",
                escape(&script_error.origin),
                escape(&classname),
                escape(&script_error.message)
            );
        }
        for result in execution.tests.iter().chain(&execution.checks) {
            suite_tests += 1;
            let _ = write!(
                cases,
                "    <testcase name=\"{}\" classname=\"{}\" time=\"0\"",
                escape(&result.name),
                escape(&classname)
            );
            if result.skipped {
                skipped += 1;
                cases.push_str(">\n      <skipped/>\n    </testcase>\n");
            } else if result.passed {
                cases.push_str("/>\n");
            } else {
                suite_failures += 1;
                let message = result.message.as_deref().unwrap_or("failed");
                let _ = writeln!(
                    cases,
                    ">\n      <failure message=\"{}\" type=\"AssertionError\">{}</failure>\n    </testcase>",
                    escape(message),
                    escape(message)
                );
            }
        }
        if suite_tests == 0 {
            // A request without tests still reports that it was answered.
            suite_tests = 1;
            if execution.skipped {
                skipped += 1;
                let _ = writeln!(
                    cases,
                    "    <testcase name=\"request\" classname=\"{}\" time=\"0\">\n      <skipped/>\n    </testcase>",
                    escape(&classname)
                );
            } else {
                let _ = writeln!(
                    cases,
                    "    <testcase name=\"request\" classname=\"{}\" time=\"{seconds:.3}\"/>",
                    escape(&classname)
                );
            }
        }
        total += suite_tests;
        failures += suite_failures;
        errors += suite_errors;
        let _ = write!(
            suites,
            "  <testsuite name=\"{}\" tests=\"{suite_tests}\" failures=\"{suite_failures}\" errors=\"{suite_errors}\" skipped=\"{skipped}\" time=\"{seconds:.3}\" timestamp=\"{timestamp}\">\n{cases}  </testsuite>\n",
            escape(&name)
        );
    }

    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuites name=\"{}\" tests=\"{total}\" failures=\"{failures}\" errors=\"{errors}\" time=\"{seconds_total:.3}\">\n{suites}</testsuites>\n",
        escape(workspace)
    )
}

/// Escapes text for XML attributes and content, dropping characters XML
/// 1.0 cannot hold.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\n' => out.push_str("&#10;"),
            '\t' | '\r' => out.push(' '),
            c if (c as u32) < 0x20 => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use getman_core::checks::TestResult;
    use getman_core::execute::Execution;

    use super::*;

    #[test]
    fn failures_and_errors_are_counted() {
        let recorded = Recorded {
            iterations: 1,
            requests: vec![
                (
                    0,
                    Execution {
                        name: "List <users>".into(),
                        path: Some("users/list.yaml".into()),
                        checks: vec![
                            TestResult {
                                name: "status == 200".into(),
                                passed: true,
                                message: None,
                                skipped: false,
                            },
                            TestResult {
                                name: "json.length > 0".into(),
                                passed: false,
                                message: Some("json.length is 0; expected > 0".into()),
                                skipped: false,
                            },
                        ],
                        ..Execution::default()
                    },
                ),
                (
                    0,
                    Execution {
                        name: "Down".into(),
                        error: Some("Could not connect: refused \u{1}".into()),
                        ..Execution::default()
                    },
                ),
            ],
            about: super::super::About {
                workspace: "API & co".into(),
                ..super::super::About::default()
            },
            ..Recorded::default()
        };
        let xml = render(&recorded);
        assert!(
            xml.contains(r#"<testsuites name="API &amp; co" tests="3" failures="1" errors="1""#),
            "{xml}"
        );
        assert!(
            xml.contains(r#"<testsuite name="List &lt;users&gt;" tests="2" failures="1""#),
            "{xml}"
        );
        assert!(
            xml.contains(r#"<failure message="json.length is 0; expected &gt; 0""#),
            "{xml}"
        );
        assert!(
            xml.contains(r#"<error message="Could not connect: refused " type="RequestError"/>"#),
            "{xml}"
        );
    }
}
