//! Masking of secret values in everything the tools return.
//!
//! A value counts as secret when it is
//!
//! - a value in `getman.local.yaml` (variables and globals),
//! - a value in an environment's `*.local.yaml`,
//! - the value of a variable an environment lists under `secrets`, or
//! - the value of an environment variable of the process named `GETMAN_*`.
//!
//! Whatever its length, such a value is replaced by [`MASK`] wherever it
//! appears in text, also inside a longer string. So are its encodings: the
//! JSON-escaped form, the percent-encoded forms, hexadecimal in either case,
//! and Base64 and Base64URL with or without padding, including the Base64
//! of a longer text that has the value somewhere inside, in any of the
//! three alignments Base64 gives it.

use std::collections::BTreeSet;

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
use getman_core::Workspace;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use regex::{Regex, RegexBuilder};
use serde_json::Value;

/// What replaces a secret value in results.
pub(super) const MASK: &str = "[secret]";

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
pub(super) struct Masker {
    /// Matches any of them, longest first; `None` when nothing is secret.
    pattern: Option<Regex>,
}

impl Masker {
    /// The secrets of a workspace, as they are now.
    pub(super) fn load(workspace: &Workspace) -> Self {
        Self::new(secret_values(workspace))
    }

    /// A masker for these secret values.
    pub(super) fn new(secrets: impl IntoIterator<Item = String>) -> Self {
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
    pub(super) fn mask(&self, text: &str) -> String {
        match &self.pattern {
            Some(pattern) => pattern.replace_all(text, MASK).into_owned(),
            None => text.to_owned(),
        }
    }

    /// Masks every string in a JSON value, and the names of its fields.
    pub(super) fn mask_value(&self, value: &mut Value) {
        if self.pattern.is_none() {
            return;
        }
        match value {
            Value::String(text) => {
                if self.pattern.as_ref().is_some_and(|p| p.is_match(text)) {
                    *text = self.mask(text);
                }
            }
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
}

/// The values that must not leave the machine.
fn secret_values(workspace: &Workspace) -> Vec<String> {
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
        if name.starts_with(getman_core::vars::ENV_PREFIX) {
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
    fn short_values_are_masked_in_their_encodings_too() {
        let masker = masking(&["a"]);
        assert_eq!(
            masker.mask("YQ== 61 %61 YQ"),
            "[secret] [secret] %[secret] [secret]"
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
}
