//! Turning responses into coloured terminal lines.

use getman_core::execute::Execution;
use getman_core::model::RequestKind;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// What the tree and request line show instead of the method for requests
/// that are not HTTP.
pub(crate) fn kind_mark(kind: RequestKind) -> Option<&'static str> {
    match kind {
        RequestKind::Http => None,
        RequestKind::Websocket => Some("WS"),
        RequestKind::Sse => Some("SSE"),
        RequestKind::Socketio => Some("SIO"),
        RequestKind::Grpc => Some("GRPC"),
    }
}

/// The colour of an HTTP method.
pub(crate) fn method_color(method: &str) -> Color {
    match method {
        "GET" => Color::Green,
        "POST" => Color::Yellow,
        "PUT" => Color::Blue,
        "PATCH" => Color::Magenta,
        "DELETE" => Color::Red,
        _ => Color::Gray,
    }
}

/// The colour of a status code.
pub(crate) fn status_color(status: u16) -> Color {
    match status {
        200..=299 => Color::Green,
        300..=399 => Color::Cyan,
        _ => Color::Red,
    }
}

/// The response body as lines, JSON pretty-printed and coloured.
pub(crate) fn response_lines(execution: &Execution) -> Vec<Line<'static>> {
    let Some(response) = &execution.response else {
        return Vec::new();
    };
    let text = response.text();
    match response.json() {
        Some(json) => serde_json::to_string_pretty(&json)
            .unwrap_or_default()
            .lines()
            .map(json_line)
            .collect(),
        None => text
            .lines()
            .map(|line| Line::raw(line.replace('\t', "    ")))
            .collect(),
    }
}

/// Colours one line of pretty-printed JSON: keys bold, strings green,
/// numbers blue, literals magenta, punctuation dim.
pub(crate) fn json_line(line: &str) -> Line<'static> {
    let mut spans = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    let dim = Style::new().fg(Color::DarkGray);
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            let start = i;
            i += 1;
            while i < chars.len() {
                if chars[i] == '\\' {
                    i += 2;
                    continue;
                }
                if chars[i] == '"' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            let token: String = chars[start..i.min(chars.len())].iter().collect();
            let rest: String = chars[i.min(chars.len())..].iter().collect();
            let style = if rest.trim_start().starts_with(':') {
                Style::new().add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(Color::Green)
            };
            spans.push(Span::styled(token, style));
        } else if c == '-' || c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || "+-.eE".contains(chars[i])) {
                i += 1;
            }
            spans.push(Span::styled(
                chars[start..i].iter().collect::<String>(),
                Style::new().fg(Color::Blue),
            ));
        } else if c.is_ascii_alphabetic() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_alphabetic() {
                i += 1;
            }
            spans.push(Span::styled(
                chars[start..i].iter().collect::<String>(),
                Style::new().fg(Color::Magenta),
            ));
        } else if c.is_whitespace() {
            let start = i;
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
            spans.push(Span::raw(chars[start..i].iter().collect::<String>()));
        } else {
            spans.push(Span::styled(c.to_string(), dim));
            i += 1;
        }
    }
    Line::from(spans)
}

/// A byte count for people.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_lines_split_into_coloured_tokens() {
        let line = json_line(r#"  "id": 7, "name": "Ada \"A\"", "ok": true"#);
        let texts: Vec<String> = line.spans.iter().map(|s| s.content.to_string()).collect();
        assert_eq!(
            texts.concat(),
            r#"  "id": 7, "name": "Ada \"A\"", "ok": true"#
        );
        assert!(
            line.spans
                .iter()
                .any(|s| s.content == "\"id\"" && s.style.add_modifier.contains(Modifier::BOLD))
        );
        assert!(
            line.spans
                .iter()
                .any(|s| s.content == "7" && s.style.fg == Some(Color::Blue))
        );
    }
}
