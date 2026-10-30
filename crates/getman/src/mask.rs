//! Masking of secret values: in everything the MCP tools return, in the
//! request as sent that the browser UI shows, and in what runs report.
//!
//! For the tools ([`secret_values`]), a value counts as secret when it is
//!
//! - a value in `getman.local.yaml` (variables and globals),
//! - a value in an environment's `*.local.yaml`,
//! - the value of a variable an environment lists under `secrets`, or
//! - the value of an environment variable of the process named `GETMAN_*`,
//!   except the ones that set Getman's own options ([`OPTIONS`]).
//!
//! The UI and runs mask only the variables the environment lists under
//! `secrets`, the ones the UI's editors hide too ([`Secrets`]).
//!
//! Whatever its length, such a value is replaced by [`MASK`] wherever it
//! appears in text, also inside a longer string. So are its encodings: the
//! JSON-escaped form and the percent-encoded forms, and for a value of at
//! least [`MIN_ENCODED_BYTES`] bytes hexadecimal in either case and Base64
//! and Base64URL with or without padding, including the Base64 of a longer
//! text that has the value somewhere inside, in any of the three alignments
//! Base64 gives it.

use std::collections::BTreeSet;

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
use getman_core::data::Row;
use getman_core::execute::Execution;
use getman_core::http::{Header, Outgoing, OutgoingBody, Payload};
use getman_core::runner::RunEvent;
use getman_core::script::VarChange;
use getman_core::vars::VarStack;
use getman_core::{Scope, Workspace};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use regex::{Regex, RegexBuilder};
use serde_json::Value;

/// What replaces a secret value in results.
pub(crate) const MASK: &str = "[secret]";

/// `GETMAN_*` variables that set an option of Getman or its install
/// scripts rather than hold a value for requests.
const OPTIONS: [&str; 3] = ["GETMAN_ENVIRONMENT", "GETMAN_INSTALL_DIR", "GETMAN_VERSION"];

/// The hexadecimal and Base64 forms of a shorter value are a few digits or
/// letters that turn up everywhere, such as `61` for `a`, so only the value
/// itself is looked for.
const MIN_ENCODED_BYTES: usize = 4;

/// A piece of Base64 found by shifting the value inside a longer text must
/// be at least this long to be looked for, or it would match by chance.
const MIN_SHIFTED_CHARS: usize = 6;

/// Everything except `A-Z a-z 0-9 - . _ ~`, as RFC 3986 asks.
const RFC3986: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');
/// What `encodeURIComponent` leaves alone: `A-Z a-z 0-9 - _ . ! ~ * ' ( )`.
const URI_COMPONENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'!')
    .remove(b'~')
    .remove(b'*')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')');

/// Replaces secret values and their encodings in text.
pub(crate) struct Masker {
    /// Matches any of them, longest first; `None` when nothing is secret.
    pattern: Option<Regex>,
}

impl Masker {
    /// A masker for these secret values.
    pub(crate) fn new(secrets: impl IntoIterator<Item = String>) -> Self {
        let mut needles = BTreeSet::new();
        for secret in secrets {
            // A value read from a file or a variable may carry a line break
            // that the response does not.
            for text in [secret.as_str(), secret.trim()] {
                if !text.is_empty() {
                    forms(text, &mut needles);
                }
            }
        }
        if !needles.is_empty() {
            // A mask already in the text stays one mask, even when the mask
            // contains a secret, such as the word "secret".
            needles.insert(MASK.to_owned());
        }
        let mut needles: Vec<String> = needles.into_iter().collect();
        needles.sort_by_key(|n| std::cmp::Reverse(n.len()));
        let pattern = (!needles.is_empty())
            .then(|| {
                let alternatives: Vec<String> = needles.iter().map(|n| regex::escape(n)).collect();
                RegexBuilder::new(&alternatives.join("|"))
                    .size_limit(256 * 1024 * 1024)
                    .build()
                    .ok()
            })
            .flatten();
        Self { pattern }
    }

    /// The text with every secret replaced.
    pub(crate) fn mask(&self, text: &str) -> String {
        match &self.pattern {
            Some(pattern) => pattern.replace_all(text, MASK).into_owned(),
            None => text.to_owned(),
        }
    }

    /// Masks every string in a JSON value, and the names of its fields.
    pub(crate) fn mask_value(&self, value: &mut Value) {
        if self.pattern.is_none() {
            return;
        }
        match value {
            Value::String(text) => self.mask_text(text),
            Value::Array(list) => list.iter_mut().for_each(|v| self.mask_value(v)),
            Value::Object(map) => {
                let fields = std::mem::take(map);
                for (name, mut field) in fields {
                    self.mask_value(&mut field);
                    map.insert(self.mask(&name), field);
                }
            }
            _ => {}
        }
    }

    /// Masks the text in place, leaving it alone when nothing in it is
    /// secret.
    fn mask_text(&self, text: &mut String) {
        if self.pattern.as_ref().is_some_and(|p| p.is_match(text)) {
            *text = self.mask(text);
        }
    }

    pub(crate) fn mask_headers(&self, headers: &mut [Header]) {
        for header in headers {
            self.mask_text(&mut header.value);
        }
    }

    /// Masks a body that is text; one that is not stays as it is.
    fn mask_payload(&self, payload: &mut Payload) {
        if let Ok(text) = std::str::from_utf8(&payload.0) {
            let mut text = text.to_owned();
            self.mask_text(&mut text);
            payload.0 = text.into_bytes();
        }
    }

    /// Masks a request as sent: its URL, headers and body.
    pub(crate) fn mask_outgoing(&self, outgoing: &mut Outgoing) {
        if self.pattern.is_none() {
            return;
        }
        self.mask_text(&mut outgoing.url);
        self.mask_headers(&mut outgoing.headers);
        match &mut outgoing.body {
            OutgoingBody::Bytes { data } => self.mask_payload(data),
            OutgoingBody::Multipart { parts } => {
                for part in parts {
                    self.mask_text(&mut part.name);
                    self.mask_text(&mut part.value);
                }
            }
            OutgoingBody::None | OutgoingBody::File { .. } => {}
        }
    }

    /// Masks everything a run reports of a request: the request as sent,
    /// the response, the messages of a connection or call, what the
    /// scripts logged and set, and every error and result. Binary data
    /// stays as it is.
    pub(crate) fn mask_execution(&self, execution: &mut Execution) {
        if self.pattern.is_none() {
            return;
        }
        if let Some(request) = &mut execution.request {
            self.mask_outgoing(request);
        }
        if let Some(response) = &mut execution.response {
            self.mask_text(&mut response.url);
            self.mask_headers(&mut response.headers);
            self.mask_headers(&mut response.added_headers);
            self.mask_payload(&mut response.body);
            for hop in &mut response.hops {
                self.mask_text(&mut hop.url);
            }
        }
        for text in execution.error.iter_mut().chain(&mut execution.warnings) {
            self.mask_text(text);
        }
        for log in &mut execution.logs {
            self.mask_text(&mut log.message);
        }
        for error in &mut execution.script_errors {
            self.mask_text(&mut error.message);
        }
        for result in execution.tests.iter_mut().chain(&mut execution.checks) {
            self.mask_text(&mut result.name);
            if let Some(message) = &mut result.message {
                self.mask_text(message);
            }
        }
        for change in &mut execution.changes {
            if let Some(value) = &mut change.value {
                self.mask_text(value);
            }
        }
        if let Some(call) = &mut execution.grpc {
            self.mask_headers(&mut call.headers);
            self.mask_headers(&mut call.trailers);
            if let Some(status) = &mut call.status {
                self.mask_text(&mut status.message);
            }
            for message in &mut call.messages {
                self.mask_value(&mut message.message);
                if let Some(error) = &mut message.error {
                    self.mask_text(error);
                }
            }
        }
        if let Some(call) = &mut execution.realtime {
            self.mask_text(&mut call.url);
            self.mask_headers(&mut call.request_headers);
            self.mask_headers(&mut call.headers);
            for message in call.messages.iter_mut().filter(|m| !m.binary) {
                self.mask_text(&mut message.data);
            }
            for text in call.notes.iter_mut().chain(&mut call.warnings) {
                self.mask_text(text);
            }
        }
    }
}

/// The values of the variables an environment lists under `secrets`, the
/// ones the UI's editors hide: what the UI hides in the request as sent
/// until asked, and what runs leave out of what they report.
pub(crate) struct Secrets {
    names: Vec<String>,
    values: Vec<String>,
    masker: Masker,
}

impl Secrets {
    /// The secrets of an environment, with the values its files hold now.
    pub(crate) fn of(workspace: &Workspace, environment: Option<&str>) -> Self {
        let Some(files) = environment.and_then(|id| workspace.read_environment(id).ok()) else {
            return Self::new(Vec::new(), Vec::new());
        };
        let mut stack = VarStack::new();
        stack.push(Scope::Environment, files.effective());
        let mut values = Vec::new();
        for name in &files.shared.secrets {
            for value in [files.shared.variables.get(name), files.local.get(name)]
                .into_iter()
                .flatten()
            {
                values.push(value.clone());
                // A value such as `{{$env.GETMAN_TOKEN}}` goes out as the
                // value it names.
                let rendered = stack.render(value);
                if rendered != *value {
                    values.push(rendered);
                }
            }
        }
        Self::new(files.shared.secrets, values)
    }

    fn new(names: Vec<String>, values: Vec<String>) -> Self {
        let masker = Masker::new(values.clone());
        Self {
            names,
            values,
            masker,
        }
    }

    /// Takes in values given to variables some other way, such as by
    /// scripts or on the command line, keeping the ones of secrets.
    pub(crate) fn learn<'a>(&mut self, values: impl IntoIterator<Item = (&'a str, &'a str)>) {
        let before = self.values.len();
        for (name, value) in values {
            if self.names.iter().any(|n| n == name)
                && !value.trim().is_empty()
                && !self.values.iter().any(|v| v == value)
            {
                self.values.push(value.to_owned());
            }
        }
        if self.values.len() > before {
            self.masker = Masker::new(self.values.clone());
        }
    }

    /// Takes in the values scripts and captures gave secrets.
    pub(crate) fn learn_changes(&mut self, changes: &[VarChange]) {
        self.learn(
            changes
                .iter()
                .filter_map(|c| Some((c.name.as_str(), c.value.as_deref()?))),
        );
    }

    /// Takes in the values of a run's data file, which name their
    /// variables by column.
    pub(crate) fn learn_rows(&mut self, rows: &[Row]) {
        let values: Vec<(&str, String)> = rows
            .iter()
            .flatten()
            .map(|(name, value)| {
                let text = match value {
                    Value::String(text) => text.clone(),
                    other => other.to_string(),
                };
                (name.as_str(), text)
            })
            .collect();
        self.learn(values.iter().map(|(name, value)| (*name, value.as_str())));
    }

    pub(crate) fn masker(&self) -> &Masker {
        &self.masker
    }

    /// Masks what an event of a run reports, after taking in the values
    /// the request's scripts gave secrets.
    pub(crate) fn mask_event(&mut self, event: &mut RunEvent) {
        match event {
            RunEvent::Request { execution, .. } => {
                self.learn_changes(&execution.changes);
                self.masker.mask_execution(execution);
            }
            RunEvent::Finish { summary } => {
                for text in summary.warnings.iter_mut().chain(&mut summary.stopped) {
                    self.masker.mask_text(text);
                }
            }
            RunEvent::Start { .. } | RunEvent::Iteration { .. } => {}
        }
    }
}

/// The values of a workspace that must not leave the machine, as they are
/// now.
pub(crate) fn secret_values(workspace: &Workspace) -> Vec<String> {
    let mut secrets = Vec::new();
    if let Ok(local) = workspace.read_local_state() {
        secrets.extend(local.variables.into_values());
        secrets.extend(local.globals.into_values());
    }
    for summary in workspace.environments().unwrap_or_default() {
        if let Ok(files) = workspace.read_environment(&summary.id) {
            for name in &files.shared.secrets {
                if let Some(value) = files.shared.variables.get(name) {
                    secrets.push(value.clone());
                }
            }
            secrets.extend(files.local.into_values());
        }
    }
    for (name, value) in std::env::vars() {
        if name.starts_with(getman_core::vars::ENV_PREFIX) && !OPTIONS.contains(&name.as_str()) {
            secrets.push(value);
        }
    }
    secrets
}

/// Adds the value and its encodings to `out`.
fn forms(secret: &str, out: &mut BTreeSet<String>) {
    out.insert(secret.to_owned());

    // Inside a JSON string, quotes, backslashes and control characters are
    // escaped, and a response body is often JSON inside a JSON string.
    if let Ok(json) = serde_json::to_string(secret) {
        out.insert(json[1..json.len() - 1].to_owned());
    }

    for set in [RFC3986, URI_COMPONENT] {
        let encoded = utf8_percent_encode(secret, set).to_string();
        out.insert(encoded.to_ascii_lowercase());
        out.insert(encoded);
    }
    // HTML forms write a space as `+`.
    let form = utf8_percent_encode(secret, RFC3986)
        .to_string()
        .replace("%20", "+");
    out.insert(form.to_ascii_lowercase());
    out.insert(form);

    let bytes = secret.as_bytes();
    if bytes.len() < MIN_ENCODED_BYTES {
        return;
    }
    out.insert(hex::encode(bytes));
    out.insert(hex::encode_upper(bytes));

    out.insert(STANDARD.encode(bytes));
    out.insert(STANDARD_NO_PAD.encode(bytes));
    out.insert(URL_SAFE.encode(bytes));
    out.insert(URL_SAFE_NO_PAD.encode(bytes));
    shifted_base64(bytes, out);
}

/// Base64 encodes three bytes as four characters, so the Base64 of a text
/// with the value inside it holds the value's own Base64 only if the value
/// starts at a multiple of three bytes. For the other two starting points
/// the characters that depend on the value alone are looked for instead.
fn shifted_base64(secret: &[u8], out: &mut BTreeSet<String>) {
    // Characters at the start of the encoding that bytes before the value
    // take part in, by the number of such bytes in the first group.
    const LEADING: [usize; 3] = [0, 2, 3];
    for (shift, from) in LEADING.into_iter().enumerate() {
        let mut text = vec![0_u8; shift];
        text.extend_from_slice(secret);
        // The last group is cut to the characters that no later byte can
        // change, unless the value ends the encoded text.
        let settled = match text.len() % 3 {
            0 => 4 * (text.len() / 3),
            1 => 4 * (text.len() / 3) + 1,
            _ => 4 * (text.len() / 3) + 2,
        };
        for (padded, plain) in [(&STANDARD, &STANDARD_NO_PAD), (&URL_SAFE, &URL_SAFE_NO_PAD)] {
            let plain = plain.encode(&text);
            let padded = padded.encode(&text);
            let mut candidates = vec![&plain[from..settled]];
            if from > 0 {
                // The value ends the encoded text, so the last group is whole.
                candidates.extend([&plain[from..], &padded[from..]]);
            }
            for needle in candidates {
                if needle.len() >= MIN_SHIFTED_CHARS {
                    out.insert(needle.to_owned());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn masking(secrets: &[&str]) -> Masker {
        Masker::new(secrets.iter().map(|s| (*s).to_owned()))
    }

    #[test]
    fn a_value_is_masked_whatever_its_length_and_wherever_it_is() {
        let masker = masking(&["tok", "a-much-longer-api-key"]);
        assert_eq!(
            masker.mask("Bearer tok and key=a-much-longer-api-key&x=1, tokens"),
            "Bearer [secret] and key=[secret]&x=1, [secret]ens"
        );
        // One character is enough to be hidden, and everywhere.
        assert_eq!(masking(&["x"]).mask("xyz-x"), "[secret]yz-[secret]");
        assert_eq!(masking(&[]).mask("nothing"), "nothing");
        assert_eq!(masking(&["", "  "]).mask("nothing"), "nothing");
    }

    #[test]
    fn a_trimmed_value_is_masked_too() {
        let masker = masking(&["secret-value\n"]);
        assert_eq!(masker.mask("a secret-value b"), "a [secret] b");
        assert_eq!(masker.mask("a secret-value\n b"), "a [secret] b");
    }

    #[test]
    fn the_longest_value_wins_and_masks_are_not_rescanned() {
        let masker = masking(&["abc", "abcdef", "secret"]);
        assert_eq!(masker.mask("abcdef abc"), "[secret] [secret]");
        // The mask itself is not a secret, even if the secret is "secret".
        assert_eq!(masker.mask("secret secret"), "[secret] [secret]");
        assert_eq!(masker.mask(&masker.mask("abc")), "[secret]");
    }

    #[test]
    fn encoded_forms_are_masked() {
        let secret = "p@ss w/rd&\"x\"=é";
        let masker = masking(&[secret]);
        let encoded = [
            secret.to_owned(),
            r#"p@ss w/rd&\"x\"=é"#.to_owned(),
            "p%40ss%20w%2Frd%26%22x%22%3D%C3%A9".to_owned(),
            "p%40ss%20w%2frd%26%22x%22%3d%c3%a9".to_owned(),
            "p%40ss+w%2Frd%26%22x%22%3D%C3%A9".to_owned(),
            hex::encode(secret),
            hex::encode_upper(secret),
            STANDARD.encode(secret),
            STANDARD_NO_PAD.encode(secret),
            URL_SAFE.encode(secret),
            URL_SAFE_NO_PAD.encode(secret),
        ];
        for form in &encoded {
            let text = format!("before {form} after");
            assert_eq!(masker.mask(&text), "before [secret] after", "{form}");
        }
    }

    #[test]
    fn very_short_values_are_masked_but_not_their_hex_or_base64() {
        let masker = masking(&["a"]);
        assert_eq!(masker.mask("YQ== 61 %61 a"), "YQ== 61 %61 [secret]");
        let masker = masking(&["abcd"]);
        assert_eq!(
            masker.mask("YWJjZA== 61626364 abcd"),
            "[secret] [secret] [secret]"
        );
    }

    #[test]
    fn base64_of_a_longer_text_with_the_value_inside_is_masked() {
        let secret = "hunter2-hunter2";
        let masker = masking(&[secret]);
        for prefix in ["", "u", "us", "use", "user:", "a-longer-user-name:"] {
            for suffix in ["", "x", "xy", "xyz", ";more"] {
                let text = format!("{prefix}{secret}{suffix}");
                for encoded in [
                    STANDARD.encode(&text),
                    STANDARD_NO_PAD.encode(&text),
                    URL_SAFE.encode(&text),
                    URL_SAFE_NO_PAD.encode(&text),
                ] {
                    let shown = format!("Authorization: Basic {encoded}");
                    let hidden = masker.mask(&shown);
                    assert!(
                        hidden.contains(MASK),
                        "{prefix:?} + secret + {suffix:?}: {shown} -> {hidden}"
                    );
                }
            }
        }
    }

    #[test]
    fn unrelated_text_is_left_alone() {
        let masker = masking(&["hunter2-hunter2"]);
        let text = "Authorization: Basic dXNlcjpwYXNzd29yZA== and {\"id\": 7, \"ok\": true}";
        assert_eq!(masker.mask(text), text);
    }

    #[test]
    fn values_are_masked_in_text_nested_in_json_and_in_field_names() {
        let masker = masking(&["s3cr3t", "z"]);
        let mut value = json!({
            "url": "https://x.test/?key=s3cr3t&a=z",
            "list": ["a", { "deep": "xs3cr3tx", "n": 5, "ok": true, "none": null }],
            "s3cr3t": "field name",
        });
        masker.mask_value(&mut value);
        assert_eq!(
            value,
            json!({
                "url": "https://x.test/?key=[secret]&a=[secret]",
                "list": ["a", { "deep": "x[secret]x", "n": 5, "ok": true, "none": null }],
                "[secret]": "field name",
            })
        );
    }

    #[test]
    fn a_run_reports_text_masked_and_binary_data_as_it_is() {
        use getman_core::checks::TestResult;
        use getman_core::realtime::{Direction, Entry, RealtimeCall};
        use getman_core::script::LogEntry;

        let masker = masking(&["s3cr3t"]);
        let binary = Payload(vec![0xff, b's', b'3', b'c', b'r', b'3', b't']);
        let mut response = crate::report::sample().requests[0]
            .1
            .response
            .clone()
            .unwrap();
        response.body = binary.clone();
        let mut execution = Execution {
            request: Some(Outgoing {
                method: "GET".into(),
                url: "https://x.test/?key=s3cr3t".into(),
                headers: vec![Header::new("Authorization", "Bearer s3cr3t")],
                body: OutgoingBody::Bytes {
                    data: Payload(b"{\"key\": \"s3cr3t\"}".to_vec()),
                },
            }),
            response: Some(response),
            logs: vec![LogEntry {
                level: "log".into(),
                message: "key s3cr3t".into(),
                origin: "request".into(),
            }],
            tests: vec![TestResult {
                name: "has s3cr3t".into(),
                passed: false,
                message: Some("expected s3cr3t".into()),
                skipped: false,
            }],
            realtime: Some(RealtimeCall {
                messages: vec![
                    Entry {
                        direction: Direction::Sent,
                        data: "s3cr3t".into(),
                        time: 0.0,
                        size: 6,
                        binary: false,
                        name: None,
                        event: None,
                        id: None,
                        ack: None,
                    },
                    Entry {
                        direction: Direction::Received,
                        // Base64 that happens to hold the value.
                        data: "AAs3cr3tAA==".into(),
                        time: 0.0,
                        size: 8,
                        binary: true,
                        name: None,
                        event: None,
                        id: None,
                        ack: None,
                    },
                ],
                ..RealtimeCall::default()
            }),
            ..Execution::default()
        };
        masker.mask_execution(&mut execution);
        let text = serde_json::to_string(&execution).unwrap();
        assert!(text.contains("https://x.test/?key=[secret]"), "{text}");
        assert!(text.contains("Bearer [secret]"), "{text}");
        assert!(text.contains(r#"{\"key\": \"[secret]\"}"#), "{text}");
        assert!(text.contains("key [secret]"), "{text}");
        assert!(text.contains("has [secret]") && text.contains("expected [secret]"));
        assert_eq!(execution.response.unwrap().body, binary);
        let call = execution.realtime.unwrap();
        assert_eq!(call.messages[0].data, "[secret]");
        assert_eq!(call.messages[1].data, "AAs3cr3tAA==");
    }

    #[test]
    fn only_the_values_given_to_secrets_are_learned() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = Workspace::init(dir.path(), "Test").unwrap();
        let id = workspace.create_environment("Dev").unwrap();
        let mut files = workspace.read_environment(&id).unwrap();
        files.shared.secrets = vec!["token".into()];
        files.local.insert("token".into(), "first-token".into());
        workspace.write_environment(&id, &files).unwrap();

        let mut secrets = Secrets::of(&workspace, Some(&id));
        secrets.learn([("token", "second-token"), ("user", "ada-lovelace")]);
        secrets.learn_changes(&[VarChange {
            scope: getman_core::script::ChangeScope::Environment,
            name: "token".into(),
            value: Some("third-token".into()),
        }]);
        let mut row = Row::new();
        row.insert("token".into(), json!(4_242_424));
        secrets.learn_rows(&[row]);
        assert_eq!(
            secrets
                .masker()
                .mask("first-token second-token third-token 4242424 ada-lovelace"),
            "[secret] [secret] [secret] [secret] ada-lovelace"
        );
        let none = Secrets::of(&workspace, None);
        assert_eq!(none.masker().mask("first-token"), "first-token");
    }
}
