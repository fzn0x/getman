//! Colors and messages for the terminal. Colors are dropped when the
//! output is not a terminal or `NO_COLOR` is set.

use anstyle::{AnsiColor, Style};

pub(crate) const BOLD: Style = Style::new().bold();
pub(crate) const DIM: Style = Style::new().dimmed();
pub(crate) const GREEN: Style = AnsiColor::Green.on_default();
pub(crate) const RED: Style = AnsiColor::Red.on_default();
pub(crate) const YELLOW: Style = AnsiColor::Yellow.on_default();
pub(crate) const CYAN: Style = AnsiColor::Cyan.on_default();

/// Text in a style.
pub(crate) fn paint(style: Style, text: &str) -> String {
    format!("{style}{text}{style:#}")
}

/// Prints an error to standard error.
pub(crate) fn error(message: &str) {
    anstream::eprintln!("{} {message}", paint(RED.bold(), "error:"));
}

/// Prints a warning to standard error.
pub(crate) fn warn(message: &str) {
    anstream::eprintln!("{} {message}", paint(YELLOW.bold(), "warning:"));
}

/// Prints a note to standard error.
pub(crate) fn note(message: &str) {
    anstream::eprintln!("{}", paint(DIM, message));
}

/// The style for a status code.
pub(crate) fn status(code: u16) -> Style {
    match code {
        200..=299 => GREEN,
        300..=399 => CYAN,
        _ => RED,
    }
}

/// A byte count for people: `512 B`, `2.1 kB`, `3.4 MB`.
pub(crate) fn size(bytes: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let value = bytes as f64;
    if bytes < 1000 {
        format!("{bytes} B")
    } else if bytes < 1_000_000 {
        format!("{:.1} kB", value / 1000.0)
    } else {
        format!("{:.1} MB", value / 1_000_000.0)
    }
}

/// A duration for people: `87 ms`, `1.42 s`.
pub(crate) fn duration(ms: f64) -> String {
    if ms < 1000.0 {
        format!("{ms:.0} ms")
    } else {
        format!("{:.2} s", ms / 1000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_and_durations_read_well() {
        assert_eq!(size(512), "512 B");
        assert_eq!(size(2_100), "2.1 kB");
        assert_eq!(size(3_400_000), "3.4 MB");
        assert_eq!(duration(87.4), "87 ms");
        assert_eq!(duration(1420.0), "1.42 s");
    }
}
