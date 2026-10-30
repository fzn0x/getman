//! Declarative checks and captures: assertions and variable extraction
//! written without code.
//!
//! A check is one line: a target, an operator and usually a value.
//!
//! ```text
//! status == 200
//! time < 500
//! header.content-type contains json
//! json.data[0].id exists
//! json.items.length >= 3
//! json.user.email matches ^[^@]+@example\.com$
//! json.role is string
//! ```
//!
//! Targets: `status`, `time` (milliseconds), `size` (body bytes), `url`,
//! `body`, `header.<name>`, `cookie.<name>`, `json` and `json.<path>`,
//! where a path is made of `.name`, `[index]` (negative counts from the
//! end), `["quoted name"]` and `.length`.
//!
//! Operators: `==` `!=` `<` `<=` `>` `>=` `contains` `!contains`
//! `startsWith` `endsWith` `matches` `!matches` `exists` `!exists` `is`,
//! `in` and `not in` (a value in a JSON array or a comma list),
//! `between A and B`, `is empty` and `is not empty`, `is truthy` and
//! `is falsy`. A value that parses as JSON is compared as JSON, so `== 42`
//! is a number and `== "42"` a string; anything else is text.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::http::Response;

/// The outcome of one check or script test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestResult {
    /// What was tested: the check's text or the test's name.
    pub name: String,
    /// Whether it passed.
    pub passed: bool,
    /// Why it failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Whether the test was skipped.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub skipped: bool,
}

impl TestResult {
    fn pass(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            passed: true,
            message: None,
            skipped: false,
        }
    }

    fn fail(name: &str, message: impl Into<String>) -> Self {
        Self {
            name: name.to_owned(),
            passed: false,
            message: Some(message.into()),
            skipped: false,
        }
    }
}

const OPERATORS: &[&str] = &[
    "==",
    "!=",
    "<=",
    ">=",
    "<",
    ">",
    "contains",
    "!contains",
    "startsWith",
    "endsWith",
    "matches",
    "!matches",
    "exists",
    "!exists",
    "is",
    "in",
    "not in",
    "between",
    "is empty",
    "is not empty",
    "is truthy",
    "is falsy",
];

/// The operators that take no value.
const UNARY: &[&str] = &[
    "exists",
    "!exists",
    "is empty",
    "is not empty",
    "is truthy",
    "is falsy",
];

/// A parsed check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    /// What to look at, such as `json.id`.
    pub target: String,
    /// The comparison.
    pub operator: String,
    /// The value to compare with; empty for `exists`.
    pub value: String,
}

impl Check {
    /// Parses a check line.
    pub fn parse(line: &str) -> Result<Self, String> {
        let line = line.trim();
        let (target, rest) = split_target(line);
        // Allow symbols without spaces, such as `status==200`.
        for symbol in ["==", "!=", "<=", ">=", "<", ">"] {
            if let Some((t, v)) = target.split_once(symbol)
                && !t.is_empty()
            {
                return Ok(Self {
                    target: t.to_owned(),
                    operator: symbol.to_owned(),
                    value: format!("{v} {rest}").trim().to_owned(),
                });
            }
        }
        let (operator, value) = split_operator(rest);
        if target.is_empty() || operator.is_empty() {
            return Err(format!(
                "{line:?} is not a check; write it as `target operator value`"
            ));
        }
        if !OPERATORS.contains(&operator) {
            return Err(format!(
                "{operator:?} is not an operator; use one of {}",
                OPERATORS.join(" ")
            ));
        }
        Ok(Self {
            target: target.to_owned(),
            operator: operator.to_owned(),
            value: if UNARY.contains(&operator) {
                String::new()
            } else {
                value.trim().to_owned()
            },
        })
    }
}

/// Splits off the target, which may hold spaces inside `["quoted keys"]`.
fn split_target(line: &str) -> (&str, &str) {
    let mut depth = 0_u32;
    let mut quote = None;
    for (at, c) in line.char_indices() {
        match (c, quote) {
            ('"' | '\'', None) if depth > 0 => quote = Some(c),
            (c, Some(q)) if c == q => quote = None,
            ('[', None) => depth += 1,
            (']', None) => depth = depth.saturating_sub(1),
            (c, None) if c.is_whitespace() && depth == 0 => {
                return (&line[..at], line[at..].trim_start());
            }
            _ => {}
        }
    }
    (line, "")
}

fn split_token(text: &str) -> (&str, &str) {
    let text = text.trim_start();
    match text.find(char::is_whitespace) {
        Some(at) => (&text[..at], text[at..].trim_start()),
        None => (text, ""),
    }
}

/// Splits off the operator, which is several words for `not in`,
/// `is empty`, `is not empty`, `is truthy` and `is falsy`.
fn split_operator(text: &str) -> (&str, &str) {
    let text = text.trim_start();
    for operator in [
        "not in",
        "is not empty",
        "is empty",
        "is truthy",
        "is falsy",
    ] {
        let mut remaining = text;
        let mut matched = true;
        for word in operator.split(' ') {
            let (token, after) = split_token(remaining);
            if token != word {
                matched = false;
                break;
            }
            remaining = after;
        }
        if matched {
            return (operator, remaining);
        }
    }
    split_token(text)
}

/// Runs every check against a response.
pub fn run_checks(checks: &[String], response: &Response) -> Vec<TestResult> {
    run_checks_on(checks, response, None)
}

/// Runs every check against a response and, for a connection that stays
/// open, the messages it received: the `messages` target reads them (see
/// [`messages_value`]).
pub fn run_checks_on(
    checks: &[String],
    response: &Response,
    messages: Option<&Value>,
) -> Vec<TestResult> {
    checks
        .iter()
        .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
        .map(|line| run_check(line.trim(), response, messages))
        .collect()
}

fn run_check(line: &str, response: &Response, messages: Option<&Value>) -> TestResult {
    let check = match Check::parse(line) {
        Ok(check) => check,
        Err(message) => return TestResult::fail(line, message),
    };
    let actual = match target_value_in(&check.target, response, messages) {
        Ok(actual) => actual,
        Err(message) => return TestResult::fail(line, message),
    };
    let expected = parse_value(&check.value);
    let shown =
        |value: &Option<Value>| value.as_ref().map_or_else(|| "nothing".to_owned(), display);

    let outcome: Result<bool, String> = match check.operator.as_str() {
        "exists" => Ok(actual.is_some()),
        "!exists" => Ok(actual.is_none()),
        _ if actual.is_none() => Err(format!("{} does not exist", check.target)),
        "==" => Ok(loose_eq(actual.as_ref().unwrap(), &expected)),
        "!=" => Ok(!loose_eq(actual.as_ref().unwrap(), &expected)),
        "<" | "<=" | ">" | ">=" => {
            let (Some(a), Some(b)) = (as_number(actual.as_ref().unwrap()), as_number(&expected))
            else {
                return TestResult::fail(
                    line,
                    format!(
                        "cannot compare {} with {} as numbers",
                        shown(&actual),
                        display(&expected)
                    ),
                );
            };
            Ok(match check.operator.as_str() {
                "<" => a < b,
                "<=" => a <= b,
                ">" => a > b,
                _ => a >= b,
            })
        }
        "contains" | "!contains" => {
            let found = contains(actual.as_ref().unwrap(), &expected);
            Ok(found == (check.operator == "contains"))
        }
        "startsWith" => Ok(as_text(actual.as_ref().unwrap()).starts_with(&as_text(&expected))),
        "endsWith" => Ok(as_text(actual.as_ref().unwrap()).ends_with(&as_text(&expected))),
        "matches" | "!matches" => match regex::Regex::new(&check.value) {
            Ok(pattern) => Ok(pattern.is_match(&as_text(actual.as_ref().unwrap()))
                == (check.operator == "matches")),
            Err(e) => Err(format!("invalid pattern: {e}")),
        },
        "in" | "not in" => {
            if check.value.is_empty() {
                return TestResult::fail(
                    line,
                    format!(
                        "`{}` needs a list, such as [200, 201] or 200, 201",
                        check.operator
                    ),
                );
            }
            let value = actual.as_ref().unwrap();
            let found = list_items(&check.value)
                .iter()
                .any(|item| loose_eq(value, item));
            Ok(found == (check.operator == "in"))
        }
        "between" => {
            let Some((low, high)) = between_bounds(&check.value) else {
                return TestResult::fail(
                    line,
                    "`between` needs two numbers, as in `between 1 and 10`",
                );
            };
            let Some(number) = as_number(actual.as_ref().unwrap()) else {
                return TestResult::fail(
                    line,
                    format!(
                        "cannot compare {} with {low} and {high} as numbers",
                        shown(&actual)
                    ),
                );
            };
            Ok(number >= low && number <= high)
        }
        "is empty" => Ok(is_empty(actual.as_ref().unwrap())),
        "is not empty" => Ok(!is_empty(actual.as_ref().unwrap())),
        "is truthy" => Ok(is_truthy(actual.as_ref().unwrap())),
        "is falsy" => Ok(!is_truthy(actual.as_ref().unwrap())),
        "is" => {
            let kind = type_name(actual.as_ref().unwrap());
            let wanted = check.value.to_ascii_lowercase();
            let number_is_integer = wanted == "integer"
                && actual
                    .as_ref()
                    .unwrap()
                    .as_f64()
                    .is_some_and(|n| n.fract() == 0.0);
            Ok(kind == wanted || number_is_integer)
        }
        other => Err(format!("unknown operator {other}")),
    };
    match outcome {
        Ok(true) => TestResult::pass(line),
        Ok(false) => {
            let message = match check.operator.as_str() {
                "exists" => format!("{} does not exist", check.target),
                "!exists" => format!("{} exists: {}", check.target, shown(&actual)),
                "is" => format!(
                    "{} is {}, not {}",
                    check.target,
                    type_name(actual.as_ref().unwrap()),
                    check.value
                ),
                "in" | "not in" => format!(
                    "{} is {}; expected {} {}",
                    check.target,
                    shown(&actual),
                    if check.operator == "in" {
                        "one of"
                    } else {
                        "none of"
                    },
                    display(&Value::Array(list_items(&check.value)))
                ),
                "between" => format!(
                    "{} is {}; expected between {}",
                    check.target,
                    shown(&actual),
                    check.value
                ),
                "is empty" => format!("{} is {}, not empty", check.target, shown(&actual)),
                "is not empty" => format!("{} is empty", check.target),
                "is truthy" => format!("{} is {}, not truthy", check.target, shown(&actual)),
                "is falsy" => format!("{} is {}, not falsy", check.target, shown(&actual)),
                _ => format!(
                    "{} is {}; expected {} {}",
                    check.target,
                    shown(&actual),
                    check.operator,
                    display(&expected)
                ),
            };
            TestResult::fail(line, message)
        }
        Err(message) => TestResult::fail(line, message),
    }
}

/// Values copied out of a response, by variable name, and the captures
/// that found nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Captured {
    /// Variable names and their new values.
    pub values: Vec<(String, String)>,
    /// Messages for captures that failed.
    pub failures: Vec<String>,
}

/// Evaluates captures such as `token: json.access_token`.
pub fn run_captures(captures: &crate::model::Vars, response: &Response) -> Captured {
    run_captures_on(captures, response, None)
}

/// Evaluates captures against a response and the messages of a connection
/// that stays open, as [`run_checks_on`] does for checks.
pub fn run_captures_on(
    captures: &crate::model::Vars,
    response: &Response,
    messages: Option<&Value>,
) -> Captured {
    let mut out = Captured::default();
    for (name, target) in captures {
        match target_value_in(target.trim(), response, messages) {
            Ok(Some(value)) => out.values.push((name.clone(), as_text(&value))),
            Ok(None) => out.failures.push(format!(
                "{name}: {} does not exist in the response",
                target.trim()
            )),
            Err(message) => out.failures.push(format!("{name}: {message}")),
        }
    }
    out
}

/// What a target points at in a response; `None` if it is absent.
pub fn target_value(target: &str, response: &Response) -> Result<Option<Value>, String> {
    target_value_in(target, response, None)
}

/// The messages a connection received, as the `messages` target sees
/// them: an array with, for each message, its `direction`, `data`,
/// `time` (milliseconds after the connection opened), `size`, any `event`,
/// `id` and `ack`, and `json` when the data is JSON.
pub fn messages_value(messages: &[crate::script::ScriptMessage]) -> Value {
    Value::Array(
        messages
            .iter()
            .map(|message| {
                let mut object = serde_json::to_value(message)
                    .ok()
                    .and_then(|value| match value {
                        Value::Object(map) => Some(map),
                        _ => None,
                    })
                    .unwrap_or_default();
                if !message.binary
                    && let Ok(json) = serde_json::from_str::<Value>(&message.data)
                {
                    object.insert("json".into(), json);
                }
                Value::Object(object)
            })
            .collect(),
    )
}

/// What a target points at in a response, or in the messages of a
/// connection that stays open; `None` if it is absent.
pub fn target_value_in(
    target: &str,
    response: &Response,
    messages: Option<&Value>,
) -> Result<Option<Value>, String> {
    let (head, rest) = match target.find(['.', '[']) {
        Some(at) => (&target[..at], &target[at..]),
        None => (target, ""),
    };
    let simple = |value: Value| -> Result<Option<Value>, String> {
        if rest.is_empty() {
            Ok(Some(value))
        } else {
            Err(format!("{head} has no parts; remove {rest:?}"))
        }
    };
    match head {
        "status" => simple(response.status.into()),
        "time" => simple(Value::from(response.time_ms)),
        "size" => simple(response.size.body.into()),
        "url" => simple(response.url.clone().into()),
        "body" | "text" => simple(response.text().into_owned().into()),
        "header" | "headers" => {
            let name = rest.strip_prefix('.').unwrap_or(rest);
            if name.is_empty() {
                return Err("name the header, as in header.content-type".into());
            }
            Ok(response.header(name).map(|v| Value::String(v.to_owned())))
        }
        "cookie" | "cookies" => {
            let name = rest.strip_prefix('.').unwrap_or(rest);
            Ok(response
                .headers
                .iter()
                .filter(|h| h.name.eq_ignore_ascii_case("set-cookie"))
                .find_map(|h| {
                    let pair = h.value.split(';').next()?;
                    let (n, v) = pair.split_once('=')?;
                    (n.trim() == name).then(|| Value::String(v.trim().to_owned()))
                }))
        }
        "json" => {
            let Some(json) = response.json() else {
                return Err("the response body is not JSON".into());
            };
            let path = parse_path(rest)?;
            Ok(walk(&json, &path))
        }
        "messages" => {
            let Some(messages) = messages else {
                return Err(
                    "messages are only there for WebSocket, Server-Sent Events and Socket.IO requests"
                        .into(),
                );
            };
            Ok(walk(messages, &parse_path(rest)?))
        }
        other => Err(format!(
            "{other:?} is not a target; use status, time, size, url, body, header.<name>, cookie.<name>, json.<path> or messages.<path>"
        )),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Step {
    Key(String),
    Index(i64),
}

fn parse_path(path: &str) -> Result<Vec<Step>, String> {
    let mut steps = Vec::new();
    let mut chars = path.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '.' => {
                let key: String =
                    std::iter::from_fn(|| chars.next_if(|c| *c != '.' && *c != '[')).collect();
                if key.is_empty() {
                    return Err(format!("empty name in path {path:?}"));
                }
                steps.push(Step::Key(key));
            }
            '[' => {
                let inner: String = std::iter::from_fn(|| chars.next_if(|c| *c != ']')).collect();
                if chars.next() != Some(']') {
                    return Err(format!("missing ] in path {path:?}"));
                }
                let inner = inner.trim();
                let quoted = inner
                    .strip_prefix('"')
                    .and_then(|s| s.strip_suffix('"'))
                    .or_else(|| inner.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')));
                if let Some(key) = quoted {
                    steps.push(Step::Key(key.to_owned()));
                } else if let Ok(index) = inner.parse() {
                    steps.push(Step::Index(index));
                } else {
                    return Err(format!("{inner:?} is not an index in path {path:?}"));
                }
            }
            _ => return Err(format!("unexpected {c:?} in path {path:?}")),
        }
    }
    Ok(steps)
}

fn walk(value: &Value, path: &[Step]) -> Option<Value> {
    let mut current = value;
    let mut owned;
    for step in path {
        match (step, current) {
            (Step::Key(key), Value::Object(map)) => current = map.get(key)?,
            (Step::Key(key), Value::Array(items)) if key == "length" => {
                owned = Value::from(items.len());
                current = &owned;
            }
            (Step::Key(key), Value::String(text)) if key == "length" => {
                owned = Value::from(text.chars().count());
                current = &owned;
            }
            (Step::Index(index), Value::Array(items)) => {
                let len = i64::try_from(items.len()).ok()?;
                let at = if *index < 0 { len + index } else { *index };
                current = items.get(usize::try_from(at).ok()?)?;
            }
            _ => return None,
        }
    }
    Some(current.clone())
}

/// Reads a check's value: JSON if it parses, else text.
fn parse_value(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_owned()))
}

fn loose_eq(actual: &Value, expected: &Value) -> bool {
    if actual == expected {
        return true;
    }
    match (actual, expected) {
        (Value::Number(_), Value::Number(_)) => as_number(actual) == as_number(expected),
        (Value::String(a), other) | (other, Value::String(a)) => *a == display(other),
        _ => false,
    }
}

/// The values of a list written in a check: a JSON array, or items
/// separated by commas, each read as a check value.
pub fn list_items(text: &str) -> Vec<Value> {
    match serde_json::from_str(text) {
        Ok(Value::Array(items)) => items,
        _ => text
            .split(',')
            .map(|item| parse_value(item.trim()))
            .collect(),
    }
}

/// The two numbers of `A and B`.
pub fn between_bounds(text: &str) -> Option<(f64, f64)> {
    let mut words = text.split_whitespace();
    let low = words.next()?.parse().ok()?;
    if !words.next()?.eq_ignore_ascii_case("and") {
        return None;
    }
    let high = words.next()?.parse().ok()?;
    words.next().is_none().then_some((low, high))
}

/// Whether a value is nothing, or a string, array or object with nothing
/// in it. Numbers and booleans are never empty.
fn is_empty(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(text) => text.is_empty(),
        Value::Array(items) => items.is_empty(),
        Value::Object(map) => map.is_empty(),
        Value::Bool(_) | Value::Number(_) => false,
    }
}

/// Truthiness as in JavaScript: `null`, `false`, `0` and `""` are falsy;
/// everything else is truthy, empty arrays and objects included.
fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().is_some_and(|n| n != 0.0 && !n.is_nan()),
        Value::String(text) => !text.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

fn contains(haystack: &Value, needle: &Value) -> bool {
    match haystack {
        Value::Array(items) => items.iter().any(|item| loose_eq(item, needle)),
        Value::Object(map) => map.contains_key(&as_text(needle)),
        other => as_text(other).contains(&as_text(needle)),
    }
}

fn as_number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// A value as text: strings without quotes, everything else as JSON.
pub fn as_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn display(value: &Value) -> String {
    match value {
        Value::String(s) => format!("{s:?}"),
        other => {
            let text = other.to_string();
            if text.len() > 80 {
                let mut end = 80;
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                format!("{}…", &text[..end])
            } else {
                text
            }
        }
    }
}

fn type_name(value: &Value) -> String {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{Header, Payload, Size};

    fn response(body: &str) -> Response {
        Response {
            status: 201,
            status_text: "Created".into(),
            http_version: "HTTP/1.1".into(),
            url: "https://example.com/users".into(),
            headers: vec![
                Header::new("Content-Type", "application/json; charset=utf-8"),
                Header::new("Set-Cookie", "session=abc123; Path=/; HttpOnly"),
            ],
            body: Payload(body.as_bytes().to_vec()),
            truncated: false,
            time_ms: 120.5,
            headers_ms: 100.0,
            size: Size {
                headers: 80,
                body: body.len() as u64,
            },
            remote_addr: None,
            timings: None,
            tls: None,
            hops: Vec::new(),
            http3_available: false,
            stream_open: false,
            added_headers: Vec::new(),
        }
    }

    const BODY: &str = r#"{"data": [{"id": 1, "name": "Ada"}, {"id": 2, "name": "Grace"}],
        "meta": {"total": 2, "next": null}, "role": "admin", "score": 9.5, "count": "3",
        "with space": true}"#;

    fn outcomes(lines: &[&str]) -> Vec<(String, bool, Option<String>)> {
        let lines: Vec<String> = lines.iter().map(|l| (*l).to_owned()).collect();
        run_checks(&lines, &response(BODY))
            .into_iter()
            .map(|r| (r.name, r.passed, r.message))
            .collect()
    }

    #[test]
    fn passing_checks() {
        let lines = [
            "status == 201",
            "status==201",
            "status >= 200",
            "status < 300",
            "time < 500",
            "size > 10",
            "url endsWith /users",
            "header.content-type contains json",
            "header.Content-Type startsWith application/json",
            "header.x-missing !exists",
            "cookie.session == abc123",
            "json.data.length == 2",
            "json.data[0].name == Ada",
            "json.data[-1].name == \"Grace\"",
            "json.data[1].id is integer",
            "json.meta.next exists",
            "json.meta.next == null",
            "json.meta is object",
            "json.role != user",
            "json.role matches ^ad",
            "json.role !matches ^user",
            "json.score > 9",
            "json.count == 3",
            "json.count > 2",
            "json[\"with space\"] == true",
            "json.data contains {\"id\": 1, \"name\": \"Ada\"}",
            "json.meta contains total",
            "body contains Grace",
            "body !contains Linus",
            "status in 200, 201, 204",
            "status in [200, 201]",
            "status not in 400, 404",
            "json.role in admin, editor",
            "json.role in [\"admin\", \"editor\"]",
            "json.count in 3, 4",
            "json.role not in user, guest",
            "json.score between 9 and 10",
            "status between 200 and 299",
            "json.count between 1 AND 5",
            "json.data is not empty",
            "json.meta.next is empty",
            "json.data[0].name is not empty",
            "json.role is truthy",
            "json[\"with space\"] is truthy",
            "json.meta.next is falsy",
            "# comments are skipped",
        ];
        for (name, passed, message) in outcomes(&lines) {
            assert!(passed, "{name}: {message:?}");
        }
    }

    #[test]
    fn failing_checks_explain_themselves() {
        let results = outcomes(&[
            "status == 200",
            "json.nope exists",
            "json.role is number",
            "json.role > 3",
            "json.data[9].id == 1",
            "frobs == 1",
            "status equals 201",
            "json.role",
        ]);
        let messages: Vec<String> = results
            .iter()
            .map(|(_, passed, message)| {
                assert!(!passed);
                message.clone().unwrap()
            })
            .collect();
        assert_eq!(messages[0], "status is 201; expected == 200");
        assert_eq!(messages[1], "json.nope does not exist");
        assert_eq!(messages[2], "json.role is string, not number");
        assert!(messages[3].starts_with("cannot compare"), "{}", messages[3]);
        assert_eq!(messages[4], "json.data[9].id does not exist");
        assert!(messages[5].contains("is not a target"));
        assert!(messages[6].contains("is not an operator"));
        assert!(messages[7].contains("is not a check"));
    }

    #[test]
    fn multi_word_operators_parse() {
        let parse = |line: &str| {
            let check = Check::parse(line).unwrap();
            (check.target, check.operator, check.value)
        };
        let t = |a: &str, b: &str, c: &str| (a.to_owned(), b.to_owned(), c.to_owned());
        assert_eq!(
            parse("status not in 400, 404"),
            t("status", "not in", "400, 404")
        );
        assert_eq!(parse("status   in   [1, 2]"), t("status", "in", "[1, 2]"));
        assert_eq!(
            parse("time between 1 and 9"),
            t("time", "between", "1 and 9")
        );
        assert_eq!(parse("json.a is empty"), t("json.a", "is empty", ""));
        assert_eq!(
            parse("json.a is not empty"),
            t("json.a", "is not empty", "")
        );
        assert_eq!(parse("json.a is truthy"), t("json.a", "is truthy", ""));
        assert_eq!(parse("json.a is falsy"), t("json.a", "is falsy", ""));
        assert_eq!(parse("json.a is string"), t("json.a", "is", "string"));
        assert!(Check::parse("json.a is nothing").is_ok());
    }

    #[test]
    fn the_new_operators_fail_with_a_reason() {
        let results = outcomes(&[
            "status in 200, 204",
            "status not in 200, 201",
            "json.score between 1 and 5",
            "json.role between 1 and 5",
            "json.role between 1",
            "json.role in",
            "json.data is empty",
            "json.meta.total is falsy",
            "json.meta.next is not empty",
            "json.nope is empty",
            "json.score is empty",
        ]);
        let messages: Vec<String> = results
            .iter()
            .map(|(name, passed, message)| {
                assert!(!passed, "{name}");
                message.clone().unwrap()
            })
            .collect();
        assert_eq!(messages[0], "status is 201; expected one of [200,204]");
        assert_eq!(messages[1], "status is 201; expected none of [200,201]");
        assert_eq!(messages[2], "json.score is 9.5; expected between 1 and 5");
        assert!(messages[3].starts_with("cannot compare"), "{}", messages[3]);
        assert!(messages[4].contains("between 1 and 10"), "{}", messages[4]);
        assert!(messages[5].contains("needs a list"), "{}", messages[5]);
        assert!(messages[6].ends_with(", not empty"), "{}", messages[6]);
        assert_eq!(messages[7], "json.meta.total is 2, not falsy");
        assert_eq!(messages[8], "json.meta.next is empty");
        assert_eq!(messages[9], "json.nope does not exist");
        assert_eq!(messages[10], "json.score is 9.5, not empty");
    }

    #[test]
    fn checks_on_json_need_a_json_body() {
        let results = run_checks(&["json.a == 1".to_owned()], &response("<html>"));
        assert_eq!(
            results[0].message.as_deref(),
            Some("the response body is not JSON")
        );
    }

    #[test]
    fn captures_copy_values_into_variables() {
        let captures = [
            ("first".to_owned(), "json.data[0].name".to_owned()),
            ("total".to_owned(), "json.meta".to_owned()),
            ("type".to_owned(), "header.content-type".to_owned()),
            ("gone".to_owned(), "json.nope".to_owned()),
        ]
        .into_iter()
        .collect();
        let out = run_captures(&captures, &response(BODY));
        assert_eq!(
            out.values,
            [
                ("first".to_owned(), "Ada".to_owned()),
                ("total".to_owned(), r#"{"total":2,"next":null}"#.to_owned()),
                (
                    "type".to_owned(),
                    "application/json; charset=utf-8".to_owned()
                ),
            ]
        );
        assert_eq!(
            out.failures,
            ["gone: json.nope does not exist in the response"]
        );
    }
}
