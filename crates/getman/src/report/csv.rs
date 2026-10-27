//! The CSV report: one row per request per iteration, for spreadsheets
//! and scripts.

use super::{Recorded, Row, rows};

/// The columns, in order.
pub(crate) const COLUMNS: &[&str] = &[
    "iteration",
    "name",
    "path",
    "method",
    "url",
    "status",
    "status_text",
    "time_ms",
    "first_byte_ms",
    "size_bytes",
    "result",
    "tests_passed",
    "tests_failed",
    "checks_passed",
    "checks_failed",
    "failures",
];

pub(crate) fn render(recorded: &Recorded) -> String {
    let mut out = String::new();
    out.push_str(&COLUMNS.join(","));
    out.push_str("\r\n");
    for row in rows(recorded) {
        let cells = cells(&row);
        let line: Vec<String> = cells.iter().map(|cell| field(cell)).collect();
        out.push_str(&line.join(","));
        out.push_str("\r\n");
    }
    out
}

fn cells(row: &Row) -> Vec<String> {
    let number = |value: Option<f64>| value.map(|v| format!("{v:.2}")).unwrap_or_default();
    vec![
        row.iteration.to_string(),
        text(&row.name),
        text(&row.path),
        text(&row.method),
        text(&row.url),
        row.status.map(|s| s.to_string()).unwrap_or_default(),
        text(&row.status_text),
        number(row.time_ms),
        number(row.first_byte_ms),
        row.size.map(|s| s.to_string()).unwrap_or_default(),
        row.result.to_owned(),
        row.tests.0.to_string(),
        row.tests.1.to_string(),
        row.checks.0.to_string(),
        row.checks.1.to_string(),
        text(&row.failures.join(" | ")),
    ]
}

/// Text from a request or response, made safe for spreadsheets: a cell
/// starting with `=`, `+`, `-`, `@`, a tab or a carriage return would run
/// as a formula, so it gets a leading apostrophe.
fn text(value: &str) -> String {
    if value.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{value}")
    } else {
        value.to_owned()
    }
}

/// Quotes a field when it holds a comma, a quote or a line break.
fn field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_row_per_request_with_quoting() {
        let csv = render(&super::super::sample());
        let lines: Vec<&str> = csv.split("\r\n").collect();
        assert_eq!(lines[0], COLUMNS.join(","));
        assert_eq!(lines.len(), 5, "{csv}");
        assert!(
            lines[1].starts_with(
                "1,\"List users, all\",users/list.yaml,GET,http://127.0.0.1:9/users,200,OK,12.50,"
            ),
            "{csv}"
        );
        assert!(lines[1].contains(",failed,0,0,1,1,"), "{csv}");
        assert!(
            lines[1].ends_with(",json.length > 0: json.length is 0; expected > 0"),
            "{csv}"
        );
        assert!(lines[2].starts_with("1,'=cmd,"), "{csv}");
        assert!(lines[2].contains(",failed,"), "{csv}");
        assert!(lines[3].starts_with("2,"), "{csv}");
        assert!(lines[3].contains(",passed,"), "{csv}");
        assert_eq!(lines[4], "");
    }
}
