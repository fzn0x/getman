//! The HTML report: one self-contained page with the totals and every
//! request's checks, for people. It loads nothing from anywhere, so it can
//! be attached to a CI run, mailed, or printed to PDF.

use std::fmt::Write as _;

use getman_core::execute::Execution;
use getman_core::time::iso8601_millis;

use super::{Recorded, failures, outcome, tally};
use crate::style;

pub(crate) fn render(recorded: &Recorded) -> String {
    let about = &recorded.about;
    let summary = recorded.summary.clone().unwrap_or_default();
    let passed = summary.succeeded();
    let mut page = String::with_capacity(16 * 1024);

    let _ = write!(
        page,
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <meta name=\"generator\" content=\"getman {version}\">\n\
         <title>{title} · Getman run report</title>\n<style>{STYLE}</style>\n</head>\n<body>\n<main>\n",
        version = env!("CARGO_PKG_VERSION"),
        title = escape(&about.workspace),
    );

    // Heading.
    let mut facts = vec![escape(&about.what())];
    facts.push(match &about.environment {
        Some(name) => format!("{} environment", escape(name)),
        None => "no environment".to_owned(),
    });
    if about.started > 0 {
        let at = iso8601_millis(about.started);
        facts.push(format!(
            "<time datetime=\"{at}\">{}</time>",
            escape(&format!(
                "{} UTC",
                at.get(..16).unwrap_or_default().replace('T', " ")
            ))
        ));
    }
    if recorded.iterations > 1 {
        facts.push(format!("{} iterations", recorded.iterations));
    }
    let _ = write!(
        page,
        "<header>\n<p class=\"eyebrow\">Getman run report</p>\n<h1>{}</h1>\n<p class=\"facts\">{}</p>\n</header>\n",
        escape(&about.workspace),
        facts.join(" · ")
    );

    // Verdict and totals.
    let verdict = if passed { "All passed" } else { "Failed" };
    let _ = write!(
        page,
        "<section class=\"verdict {}\" aria-label=\"Result\"><strong>{verdict}.</strong>",
        if passed { "pass" } else { "fail" }
    );
    if let Some(reason) = &summary.stopped {
        let _ = write!(page, " Stopped: {}.", escape(reason));
    }
    page.push_str("</section>\n<dl class=\"totals\">\n");
    let total = |label: &str, main: String, sub: String, bad: bool| {
        format!(
            "<div{}><dt>{label}</dt><dd class=\"big\">{main}</dd><dd>{sub}</dd></div>\n",
            if bad { " class=\"bad\"" } else { "" }
        )
    };
    page.push_str(&total(
        "Requests",
        format!("{} run", summary.requests),
        format!(
            "{} failed · {} skipped",
            summary.failed_requests, summary.skipped_requests
        ),
        summary.failed_requests > 0,
    ));
    page.push_str(&total(
        "Tests",
        format!("{} passed", summary.tests_passed),
        format!("{} failed", summary.tests_failed),
        summary.tests_failed > 0,
    ));
    page.push_str(&total(
        "Checks",
        format!("{} passed", summary.checks_passed),
        format!("{} failed", summary.checks_failed),
        summary.checks_failed > 0,
    ));
    page.push_str(&total(
        "Time",
        style::duration(summary.duration_ms),
        format!(
            "{} average response",
            style::duration(summary.average_response_ms)
        ),
        false,
    ));
    page.push_str("</dl>\n");

    if !summary.warnings.is_empty() {
        page.push_str("<ul class=\"warnings\">\n");
        for warning in &summary.warnings {
            let _ = writeln!(page, "<li>{}</li>", escape(warning));
        }
        page.push_str("</ul>\n");
    }

    // Requests.
    let slowest = recorded
        .requests
        .iter()
        .filter_map(|(_, e)| e.response.as_ref().map(|r| r.time_ms))
        .fold(0.0_f64, f64::max);
    let _ = write!(
        page,
        "<h2>Requests <span class=\"count\">{}</span></h2>\n<ol class=\"requests\">\n",
        recorded.requests.len()
    );
    for (iteration, execution) in &recorded.requests {
        request(&mut page, recorded, *iteration, execution, slowest);
    }
    page.push_str("</ol>\n");
    if recorded.requests.is_empty() {
        page.push_str("<p class=\"none\">No request ran.</p>\n");
    }

    let _ = write!(
        page,
        "<footer>Made by getman {} from the files in the workspace. Failed requests are open; select a request to see its checks.</footer>\n</main>\n</body>\n</html>\n",
        env!("CARGO_PKG_VERSION")
    );
    page
}

fn request(
    page: &mut String,
    recorded: &Recorded,
    iteration: usize,
    execution: &Execution,
    slowest: f64,
) {
    let result = outcome(execution);
    let method = recorded.method(execution);
    let mark = match result {
        "failed" => "✗",
        "skipped" => "–",
        _ => "✓",
    };
    let status = match &execution.response {
        Some(response) => format!(
            "<span class=\"status s{}\">{} {}</span>",
            response.status / 100,
            response.status,
            escape(&response.status_text)
        ),
        None if execution.skipped => "<span class=\"status\">skipped</span>".to_owned(),
        None => "<span class=\"status s5\">no response</span>".to_owned(),
    };
    let time = execution
        .response
        .as_ref()
        .map(|r| style::duration(r.time_ms))
        .unwrap_or_default();
    let _ = write!(
        page,
        "<li class=\"request {result}\"><details{}><summary><span class=\"mark\" aria-label=\"{result}\">{mark}</span><span class=\"method\">{}</span><span class=\"name\">{}{}</span>{status}<span class=\"time\">{time}</span></summary>\n<div class=\"detail\">\n",
        if result == "failed" { " open" } else { "" },
        escape(&method),
        escape(&execution.name),
        if recorded.iterations > 1 {
            format!(" <span class=\"iteration\">#{}</span>", iteration + 1)
        } else {
            String::new()
        }
    );

    if let Some(sent) = &execution.request {
        let _ = writeln!(
            page,
            "<p class=\"url\">{} {}</p>",
            escape(&sent.method),
            escape(&sent.url)
        );
    } else if let Some(path) = &execution.path {
        let _ = writeln!(page, "<p class=\"url\">{}</p>", escape(path));
    }
    if let Some(response) = &execution.response {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let width = if slowest > 0.0 {
            ((response.time_ms / slowest) * 100.0)
                .clamp(1.0, 100.0)
                .round() as u32
        } else {
            0
        };
        if let Some(call) = &execution.realtime {
            let _ = writeln!(
                page,
                "<p class=\"timing\"><span class=\"bar\"><span style=\"width:{width}%\"></span></span>{} to connect · open for {} · {} received</p>",
                style::duration(response.time_ms),
                style::duration(call.duration_ms),
                style::size(response.size.body)
            );
        } else {
            let _ = writeln!(
                page,
                "<p class=\"timing\"><span class=\"bar\"><span style=\"width:{width}%\"></span></span>{} total · {} to the first byte · {} body</p>",
                style::duration(response.time_ms),
                style::duration(response.headers_ms),
                style::size(response.size.body)
            );
        }
    }
    if let Some(call) = &execution.realtime
        && !call.messages.is_empty()
    {
        let _ = writeln!(
            page,
            "<h3>Messages <span class=\"count\">{}</span></h3>\n<ul class=\"results\">",
            call.messages.len()
        );
        for message in &call.messages {
            let (arrow, class) = match message.direction {
                getman_core::realtime::Direction::Sent => ("→", "skip"),
                getman_core::realtime::Direction::Received => ("←", "pass"),
            };
            let shown: String = message.data.chars().take(300).collect();
            let _ = writeln!(
                page,
                "<li class=\"{class}\"><span class=\"mark\">{arrow}</span><span class=\"check\">{}</span><span class=\"why\">{} ms</span></li>",
                escape(&shown),
                message.time.round()
            );
        }
        page.push_str("</ul>\n");
    }

    let reasons = failures(execution);
    if let Some(error) = &execution.error {
        let _ = writeln!(page, "<p class=\"error\">{}</p>", escape(error));
    }
    for error in &execution.script_errors {
        let _ = writeln!(
            page,
            "<p class=\"error\">Script in {}: {}</p>",
            escape(&error.origin),
            escape(&error.message)
        );
    }
    for (title, results) in [
        ("Checks", &execution.checks),
        ("Script tests", &execution.tests),
    ] {
        if results.is_empty() {
            continue;
        }
        let (passed, failed) = tally(results);
        let _ = writeln!(
            page,
            "<h3>{title} <span class=\"count\">{passed} passed{}</span></h3>\n<ul class=\"results\">",
            if failed > 0 {
                format!(" · {failed} failed")
            } else {
                String::new()
            }
        );
        for result in results {
            let (class, mark) = if result.skipped {
                ("skip", "–")
            } else if result.passed {
                ("pass", "✓")
            } else {
                ("fail", "✗")
            };
            let _ = write!(
                page,
                "<li class=\"{class}\"><span class=\"mark\">{mark}</span><span class=\"check\">{}</span>",
                escape(&result.name)
            );
            if let Some(message) = &result.message {
                let _ = write!(page, "<span class=\"why\">{}</span>", escape(message));
            }
            page.push_str("</li>\n");
        }
        page.push_str("</ul>\n");
    }
    if execution.tests.is_empty()
        && execution.checks.is_empty()
        && reasons.is_empty()
        && !execution.skipped
    {
        page.push_str("<p class=\"none\">No checks or tests.</p>\n");
    }
    for name in &execution.missing_variables {
        let _ = writeln!(
            page,
            "<p class=\"warning\">{}</p>",
            escape(&getman_core::vars::missing_reason(name))
        );
    }
    for warning in &execution.warnings {
        let _ = writeln!(page, "<p class=\"warning\">{}</p>", escape(warning));
    }
    page.push_str("</div>\n</details></li>\n");
}

/// Escapes text for HTML content and attribute values.
pub(crate) fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

// The page stands alone, so it carries Getman's Material Design 3 color
// roles as values (web/src/styles/tokens.css) and uses system fonts.
const STYLE: &str = r"
:root{color-scheme:light dark;--surface:#fbf8fe;--low:#f5f2f9;--high:#eae7ed;--highest:#e4e1e7;--ink:#1b1b20;--ink-2:#454650;--outline:#767681;--rule:#c6c5d1;--primary:#4355b9;--ok:#1a6c31;--ok-box:#a3f5aa;--on-ok-box:#002108;--bad:#ba1a1a;--bad-box:#ffdad6;--on-bad-box:#93000a;--warn:#865300;--warn-box:#ffddb8;--on-warn-box:#2b1700;--info:#006496;--hover:color-mix(in srgb,var(--ink) 8%,transparent)}
@media (prefers-color-scheme:dark){:root{--surface:#131317;--low:#1b1b20;--high:#292a2e;--highest:#343439;--ink:#e4e1e7;--ink-2:#c6c5d1;--outline:#90909b;--rule:#454650;--primary:#bac3ff;--ok:#88d990;--ok-box:#00531f;--on-ok-box:#a3f5aa;--bad:#ffb4ab;--bad-box:#93000a;--on-bad-box:#ffdad6;--warn:#ffb960;--warn-box:#653e00;--on-warn-box:#ffddb8;--info:#91cdff}}
*{box-sizing:border-box}
body{margin:0;background:var(--surface);color:var(--ink);font:14px/1.5 'Google Sans Flex','Google Sans',system-ui,-apple-system,'Segoe UI',Roboto,'Helvetica Neue',Arial,sans-serif;-webkit-font-smoothing:antialiased}
code,.method,.url,.check{font-family:'Google Sans Code',ui-monospace,'Cascadia Mono','SF Mono',Menlo,Consolas,monospace}
main{max-width:1040px;margin:0 auto;padding:40px 24px 56px}
header{padding-bottom:20px;border-bottom:1px solid var(--rule)}
.eyebrow{display:inline-block;margin:0;padding:4px 10px;border:1px solid var(--rule);border-radius:8px;color:var(--ink-2);font-size:12px;font-weight:500}
h1{margin:12px 0 4px;font-size:32px;font-weight:500;line-height:1.15;letter-spacing:-.01em}
.facts{margin:0;color:var(--ink-2)}
.verdict{margin:20px 0 0;padding:14px 18px;border-radius:16px;background:var(--ok-box);color:var(--on-ok-box)}
.verdict.fail{background:var(--bad-box);color:var(--on-bad-box)}
.verdict strong{font-weight:600}
.totals{display:grid;grid-template-columns:repeat(auto-fit,minmax(180px,1fr));gap:12px;margin:12px 0 0}
.totals div{padding:16px 18px;border-radius:16px;background:var(--low)}
.totals dt{color:var(--ink-2);font-size:12px;font-weight:500}
.totals dd{margin:0;color:var(--ink-2)}
.totals .big{margin-top:4px;color:var(--ink);font-size:24px;font-weight:500;line-height:1.2;font-variant-numeric:tabular-nums}
.totals .bad dd:last-child{color:var(--bad);font-weight:600}
.warnings{margin:12px 0 0;padding:12px 18px 12px 34px;border-radius:12px;background:var(--warn-box);color:var(--on-warn-box)}
h2{margin:32px 0 10px;font-size:20px;font-weight:500}
h3{margin:12px 0 4px;font-size:13px;font-weight:500;color:var(--ink-2)}
.count{color:var(--ink-2);font-weight:400}
.requests{list-style:none;margin:0;padding:0;border:1px solid var(--rule);border-radius:12px;overflow:hidden}
.request{border-bottom:1px solid var(--rule);break-inside:avoid}
.request:last-child{border-bottom:0}
summary{display:grid;grid-template-columns:22px 64px minmax(0,1fr) auto 72px;gap:12px;align-items:center;padding:8px 14px;cursor:pointer;list-style:none}
summary::-webkit-details-marker{display:none}
summary:hover{background:var(--hover)}
summary:focus-visible{outline:2px solid var(--primary);outline-offset:-2px}
details[open]>summary{background:var(--low)}
.mark{display:inline-grid;place-items:center;width:20px;height:20px;border-radius:50%;background:var(--ok-box);color:var(--on-ok-box);font-size:12px;font-weight:700;line-height:1}
.failed>details>summary .mark,.results .fail .mark{background:var(--bad-box);color:var(--on-bad-box)}
.skipped>details>summary .mark,.results .skip .mark{background:var(--highest);color:var(--ink-2)}
.method{font-size:12px;font-weight:700;letter-spacing:.02em}
.name{overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-weight:500}
.failed .name{color:var(--bad)}
.iteration{color:var(--ink-2);font-weight:400}
.status{font-variant-numeric:tabular-nums;color:var(--ink-2)}
.s2{color:var(--ok)}.s3{color:var(--info)}.s4,.s5{color:var(--bad)}
.time{text-align:right;font-variant-numeric:tabular-nums;color:var(--ink-2)}
.detail{padding:8px 14px 14px 48px;background:var(--low)}
.detail p{margin:4px 0}
.url{font-size:12px;color:var(--ink-2);word-break:break-all}
.timing{display:flex;align-items:center;gap:10px;color:var(--ink-2);font-variant-numeric:tabular-nums}
.bar{display:inline-block;width:120px;height:6px;border-radius:3px;background:var(--highest);overflow:hidden}
.bar span{display:block;height:100%;border-radius:3px;background:var(--primary)}
.error{margin:6px 0!important;padding:8px 12px;border-radius:8px;background:var(--bad-box);color:var(--on-bad-box);font-weight:500}
.warning{margin:6px 0!important;padding:8px 12px;border-radius:8px;background:var(--warn-box);color:var(--on-warn-box)}
.none{color:var(--ink-2)}
.results{list-style:none;margin:0;padding:0}
.results li{display:grid;grid-template-columns:20px minmax(0,1fr);gap:2px 10px;align-items:center;padding:3px 0}
.results .mark{width:18px;height:18px;font-size:11px}
.check{font-size:12px;word-break:break-word}
.why{grid-column:2;color:var(--bad);font-size:13px}
footer{margin-top:32px;color:var(--ink-2);font-size:12px}
@media (max-width:640px){summary{grid-template-columns:22px 56px minmax(0,1fr) 64px}.status{display:none}.detail{padding-left:14px}}
@media print{:root{color-scheme:light;--surface:#fff;--low:#fff;--ink:#000;--hover:transparent}body{font-size:10pt}main{max-width:none;padding:0}.verdict,.totals div,.warnings,.error,.warning{border:1px solid #888;background:none;color:inherit}.requests{border-radius:0}summary{padding:4px 0}.request{break-inside:avoid}footer{display:none}}
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_stands_alone_and_opens_failures() {
        let recorded = super::super::sample();
        let html = render(&recorded);
        assert!(html.starts_with("<!doctype html>"), "{html}");
        for external in ["<script", "<link", "src=", "@import", "url("] {
            assert!(!html.contains(external), "{external} in {html}");
        }
        assert!(html.contains("<h1>Shop &amp; co</h1>"), "{html}");
        // The failing request is open, the passing one closed.
        assert!(
            html.contains("class=\"request failed\"><details open>"),
            "{html}"
        );
        assert!(
            html.contains("class=\"request passed\"><details>"),
            "{html}"
        );
        assert!(
            html.contains("<span class=\"why\">json.length is 0; expected &gt; 0</span>"),
            "{html}"
        );
        assert!(html.contains("Could not connect"), "{html}");
        assert!(html.contains("<strong>Failed.</strong>"), "{html}");
        assert!(html.contains("staging environment"), "{html}");
    }

    #[test]
    fn a_run_a_script_stopped_passes_and_says_it_stopped() {
        let mut recorded = super::super::sample();
        recorded.summary = Some(getman_core::runner::RunSummary {
            requests: 1,
            stopped: Some("Login stopped the run".into()),
            stopped_by_script: true,
            ..getman_core::runner::RunSummary::default()
        });
        let html = render(&recorded);
        assert!(
            html.contains("<strong>All passed.</strong> Stopped: Login stopped the run."),
            "{html}"
        );

        // Cancelled, the run did not pass.
        recorded.summary = Some(getman_core::runner::RunSummary {
            stopped: Some("cancelled".into()),
            ..getman_core::runner::RunSummary::default()
        });
        let html = render(&recorded);
        assert!(
            html.contains("<strong>Failed.</strong> Stopped: cancelled."),
            "{html}"
        );
    }

    #[test]
    fn a_connection_shows_its_messages() {
        use getman_core::realtime::{Direction, Entry, RealtimeCall};

        let mut recorded = super::super::sample();
        let entry = |direction, data: &str, time| Entry {
            direction,
            data: data.into(),
            time,
            size: data.len() as u64,
            binary: false,
            name: None,
            event: None,
            id: None,
            ack: None,
        };
        let mut execution = recorded.requests[0].1.clone();
        execution.checks.clear();
        execution.realtime = Some(RealtimeCall {
            opened: true,
            duration_ms: 1200.0,
            messages: vec![
                entry(Direction::Sent, "join <room>", 1.0),
                entry(Direction::Received, "welcome", 14.0),
            ],
            ..RealtimeCall::default()
        });
        recorded.requests = vec![(0, execution)];
        let html = render(&recorded);
        assert!(
            html.contains("<h3>Messages <span class=\"count\">2</span></h3>"),
            "{html}"
        );
        assert!(html.contains("join &lt;room&gt;"), "{html}");
        assert!(html.contains("to connect · open for"), "{html}");
    }
}
