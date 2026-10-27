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
    let passed = summary.passed() && summary.stopped.is_none();
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
            escape(&at.replace('T', " ").replace(".000Z", " UTC"))
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
        let _ = writeln!(
            page,
            "<p class=\"timing\"><span class=\"bar\"><span style=\"width:{width}%\"></span></span>{} total · {} to the first byte · {} body</p>",
            style::duration(response.time_ms),
            style::duration(response.headers_ms),
            style::size(response.size.body)
        );
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

const STYLE: &str = r"
:root{color-scheme:light dark;--paper:#fbfaf7;--ink:#0a0a0a;--ink-2:#4f4d48;--ink-3:#6b6862;--rule:#c9c5bb;--faint:#e8e5de;--ok:#0e7a3d;--bad:#c4001a;--warn:#a35a00;--redirect:#1e4fd0;--wash:#fbe3e5}
@media (prefers-color-scheme:dark){:root{--paper:#141412;--ink:#f2f0eb;--ink-2:#b4b1a8;--ink-3:#918e86;--rule:#4d4b46;--faint:#23221f;--ok:#45c47a;--bad:#ff5a68;--warn:#f0a33a;--redirect:#7aa2ff;--wash:#3a1418}}
*{box-sizing:border-box}
body{margin:0;background:var(--paper);color:var(--ink);font:14px/1.5 system-ui,-apple-system,'Segoe UI',Roboto,'Helvetica Neue',Arial,'Noto Sans',sans-serif}
main{max-width:1040px;margin:0 auto;padding:32px 24px 48px}
header{border-bottom:2px solid var(--ink);padding-bottom:16px}
.eyebrow{margin:0;color:var(--ink-3);font:12px/1 ui-monospace,'Cascadia Mono',Menlo,Consolas,monospace;text-transform:uppercase;letter-spacing:.06em}
h1{margin:8px 0 6px;font-size:28px;line-height:1.15}
.facts{margin:0;color:var(--ink-2)}
.verdict{margin:16px 0 0;padding:10px 12px;border-left:4px solid var(--ok)}
.verdict.fail{border-color:var(--bad);background:var(--wash)}
.verdict strong{color:var(--ok)}
.verdict.fail strong{color:var(--bad)}
.totals{display:grid;grid-template-columns:repeat(auto-fit,minmax(180px,1fr));gap:0;margin:16px 0 0;border:1px solid var(--rule)}
.totals div{padding:10px 12px;border-right:1px solid var(--rule)}
.totals div:last-child{border-right:0}
.totals dt{color:var(--ink-3);font:11px/1 ui-monospace,'Cascadia Mono',Menlo,Consolas,monospace;text-transform:uppercase;letter-spacing:.06em}
.totals dd{margin:0;color:var(--ink-2)}
.totals .big{margin-top:6px;color:var(--ink);font-size:20px;font-weight:650;font-variant-numeric:tabular-nums}
.totals .bad dd:last-child{color:var(--bad);font-weight:600}
.warnings{margin:16px 0 0;padding:8px 12px 8px 28px;border:1px solid var(--warn);color:var(--warn)}
h2{margin:28px 0 8px;font-size:18px}
h3{margin:12px 0 4px;font-size:13px;color:var(--ink-2)}
.count{color:var(--ink-3);font-weight:400}
.requests{list-style:none;margin:0;padding:0;border-top:1px solid var(--ink)}
.request{border-bottom:1px solid var(--rule);break-inside:avoid}
summary{display:grid;grid-template-columns:20px 64px minmax(0,1fr) auto 72px;gap:12px;align-items:baseline;padding:7px 4px;cursor:pointer;list-style:none}
summary::-webkit-details-marker{display:none}
summary:hover{background:var(--faint)}
summary:focus-visible{outline:2px dotted var(--ink);outline-offset:-2px}
.mark{font-weight:700;color:var(--ok)}
.failed>details>summary .mark,.results .fail .mark{color:var(--bad)}
.skipped>details>summary .mark,.results .skip .mark{color:var(--ink-3)}
.method{font:600 12px/1.5 ui-monospace,'Cascadia Mono',Menlo,Consolas,monospace}
.name{overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-weight:600}
.failed .name{color:var(--bad)}
.iteration{color:var(--ink-3);font-weight:400}
.status{font-variant-numeric:tabular-nums;color:var(--ink-2)}
.s2{color:var(--ok)}.s3{color:var(--redirect)}.s4,.s5{color:var(--bad)}
.time{text-align:right;font-variant-numeric:tabular-nums;color:var(--ink-2)}
.detail{padding:4px 4px 12px 36px}
.detail p{margin:4px 0}
.url{font:12px/1.5 ui-monospace,'Cascadia Mono',Menlo,Consolas,monospace;color:var(--ink-2);word-break:break-all}
.timing{display:flex;align-items:center;gap:8px;color:var(--ink-2);font-variant-numeric:tabular-nums}
.bar{display:inline-block;width:120px;height:6px;background:var(--faint)}
.bar span{display:block;height:100%;background:var(--ink-3)}
.error{color:var(--bad);font-weight:600}
.warning{color:var(--warn)}
.none{color:var(--ink-3)}
.results{list-style:none;margin:0;padding:0}
.results li{display:grid;grid-template-columns:20px minmax(0,1fr);gap:0 8px;padding:2px 0}
.check{font:12px/1.6 ui-monospace,'Cascadia Mono',Menlo,Consolas,monospace;word-break:break-word}
.why{grid-column:2;color:var(--bad);font-size:13px}
footer{margin-top:32px;color:var(--ink-3);font-size:12px}
@media (max-width:640px){summary{grid-template-columns:20px 56px minmax(0,1fr) 64px}.status{display:none}}
@media print{:root{--paper:#fff;--ink:#000;--faint:#eee}body{font-size:10pt}main{max-width:none;padding:0}.verdict.fail{background:none}summary{padding:4px 0}.request{break-inside:avoid}footer{display:none}}
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
}
