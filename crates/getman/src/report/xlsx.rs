//! The Excel report: a Summary sheet with the totals and a Requests sheet
//! with one row per request per iteration.

use getman_core::time::iso8601_millis;
use rust_xlsxwriter::{Format, Workbook, Worksheet, XlsxError};

use super::{Recorded, rows};

/// Excel refuses longer text in a cell.
const MAX_CELL: usize = 32_000;

pub(crate) fn render(recorded: &Recorded) -> Result<Vec<u8>, String> {
    build(recorded).map_err(|e| format!("could not write the Excel report: {e}"))
}

fn build(recorded: &Recorded) -> Result<Vec<u8>, XlsxError> {
    let mut workbook = Workbook::new();
    let bold = Format::new().set_bold();
    let millis = Format::new().set_num_format("0.0");

    let about = &recorded.about;
    let summary = recorded.summary.clone().unwrap_or_default();
    let passed = summary.passed() && summary.stopped.is_none();

    let sheet = workbook.add_worksheet();
    sheet.set_name("Summary")?;
    sheet.set_column_width(0, 22)?;
    sheet.set_column_width(1, 60)?;
    let mut row = 0;
    let mut text = |sheet: &mut Worksheet, label: &str, value: &str| -> Result<(), XlsxError> {
        sheet.write_string_with_format(row, 0, label, &bold)?;
        sheet.write_string(row, 1, cell(value))?;
        row += 1;
        Ok(())
    };
    text(sheet, "Workspace", &about.workspace)?;
    text(sheet, "Ran", &about.what())?;
    text(
        sheet,
        "Environment",
        about.environment.as_deref().unwrap_or("none"),
    )?;
    if about.started > 0 {
        text(sheet, "Started (UTC)", &iso8601_millis(about.started))?;
    }
    text(sheet, "Result", if passed { "passed" } else { "failed" })?;
    if let Some(reason) = &summary.stopped {
        text(sheet, "Stopped", reason)?;
    }
    for warning in &summary.warnings {
        text(sheet, "Warning", warning)?;
    }
    #[allow(clippy::cast_precision_loss)]
    let counts = [
        ("Iterations", summary.iterations as f64),
        ("Requests run", summary.requests as f64),
        ("Requests failed", summary.failed_requests as f64),
        ("Requests skipped", summary.skipped_requests as f64),
        ("Tests passed", summary.tests_passed as f64),
        ("Tests failed", summary.tests_failed as f64),
        ("Checks passed", summary.checks_passed as f64),
        ("Checks failed", summary.checks_failed as f64),
    ];
    for (label, value) in counts {
        sheet.write_string_with_format(row, 0, label, &bold)?;
        sheet.write_number(row, 1, value)?;
        row += 1;
    }
    for (label, value) in [
        ("Total time (ms)", summary.duration_ms),
        ("Average response (ms)", summary.average_response_ms),
    ] {
        sheet.write_string_with_format(row, 0, label, &bold)?;
        sheet.write_number_with_format(row, 1, value, &millis)?;
        row += 1;
    }

    let sheet = workbook.add_worksheet();
    sheet.set_name("Requests")?;
    let columns: [(&str, f64); 16] = [
        ("Iteration", 10.0),
        ("Name", 32.0),
        ("Path", 32.0),
        ("Method", 9.0),
        ("URL", 48.0),
        ("Status", 8.0),
        ("Status text", 16.0),
        ("Time (ms)", 11.0),
        ("First byte (ms)", 14.0),
        ("Size (bytes)", 12.0),
        ("Result", 9.0),
        ("Tests passed", 12.0),
        ("Tests failed", 12.0),
        ("Checks passed", 13.0),
        ("Checks failed", 13.0),
        ("Failures", 80.0),
    ];
    for (col, (title, width)) in (0_u16..).zip(columns) {
        sheet.write_string_with_format(0, col, title, &bold)?;
        sheet.set_column_width(col, width)?;
    }
    let mut last = 0;
    for (at, item) in (1_u32..).zip(rows(recorded)) {
        last = at;
        #[allow(clippy::cast_precision_loss)]
        let number = |n: usize| n as f64;
        sheet.write_number(at, 0, number(item.iteration))?;
        sheet.write_string(at, 1, cell(&item.name))?;
        sheet.write_string(at, 2, cell(&item.path))?;
        sheet.write_string(at, 3, cell(&item.method))?;
        sheet.write_string(at, 4, cell(&item.url))?;
        if let Some(status) = item.status {
            sheet.write_number(at, 5, status)?;
        }
        sheet.write_string(at, 6, cell(&item.status_text))?;
        if let Some(time) = item.time_ms {
            sheet.write_number_with_format(at, 7, time, &millis)?;
        }
        if let Some(time) = item.first_byte_ms {
            sheet.write_number_with_format(at, 8, time, &millis)?;
        }
        if let Some(size) = item.size {
            #[allow(clippy::cast_precision_loss)]
            sheet.write_number(at, 9, size as f64)?;
        }
        sheet.write_string(at, 10, item.result)?;
        sheet.write_number(at, 11, number(item.tests.0))?;
        sheet.write_number(at, 12, number(item.tests.1))?;
        sheet.write_number(at, 13, number(item.checks.0))?;
        sheet.write_number(at, 14, number(item.checks.1))?;
        sheet.write_string(at, 15, cell(&item.failures.join("\n")))?;
    }
    sheet.set_freeze_panes(1, 0)?;
    if last > 0 {
        sheet.autofilter(0, 0, last, 15)?;
    }

    workbook.save_to_buffer()
}

/// Text cut to what a cell can hold.
fn cell(text: &str) -> String {
    if text.len() <= MAX_CELL {
        return text.to_owned();
    }
    let mut end = MAX_CELL;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_workbook_is_a_zip_with_two_sheets() {
        let bytes = render(&super::super::sample()).unwrap();
        assert!(bytes.starts_with(b"PK\x03\x04"));
        let names = |needle: &[u8]| bytes.windows(needle.len()).any(|w| w == needle);
        assert!(names(b"xl/worksheets/sheet1.xml"));
        assert!(names(b"xl/worksheets/sheet2.xml"));
        assert_eq!(cell(&"é".repeat(20_000)).chars().count(), 16_001);
    }
}
