//! Turning a cURL command into a request.
//!
//! Commands copied from browsers' developer tools, API documentation and
//! terminals are understood, in POSIX shell quoting (`bash`, `zsh`) and in
//! Windows `cmd` quoting with `^` line continuations.

use crate::error::{Error, Result};
use crate::model::{Auth, Body, FormField, KeyValue, Request, Settings};

/// Parses a cURL command into a request.
pub fn parse(command: &str) -> Result<Request> {
    let words = split_words(command)?;
    let mut args = words.into_iter().peekable();
    match args.next() {
        Some(first)
            if first == "curl" || first.ends_with("/curl") || first.ends_with("curl.exe") => {}
        _ => {
            return Err(Error::Invalid(
                "the command does not start with curl".into(),
            ));
        }
    }

    let mut request = Request::default();
    let mut method: Option<String> = None;
    let mut url: Option<String> = None;
    let mut data: Vec<String> = Vec::new();
    let mut data_file: Option<String> = None;
    let mut data_urlencode: Vec<String> = Vec::new();
    let mut form: Vec<FormField> = Vec::new();
    let mut as_get = false;
    let mut head = false;
    let mut user: Option<String> = None;
    let mut digest = false;
    let mut json = false;
    let mut settings = Settings::default();

    let take = |args: &mut std::iter::Peekable<std::vec::IntoIter<String>>, flag: &str| {
        args.next()
            .ok_or_else(|| Error::Invalid(format!("{flag} needs a value")))
    };

    while let Some(arg) = args.next() {
        // Long options may carry their value after `=`.
        let (flag, inline) = match arg.split_once('=') {
            Some((flag, value)) if arg.starts_with("--") => {
                (flag.to_owned(), Some(value.to_owned()))
            }
            _ => (arg.clone(), None),
        };
        let value = |args: &mut std::iter::Peekable<std::vec::IntoIter<String>>| match &inline {
            Some(value) => Ok(value.clone()),
            None => take(args, &flag),
        };
        match flag.as_str() {
            "-X" | "--request" => method = Some(value(&mut args)?.to_ascii_uppercase()),
            "-H" | "--header" => {
                // `Name: value` sets a header, `Name:` removes one and
                // `Name;` sends it empty.
                let header = value(&mut args)?;
                if let Some((name, val)) = header.split_once(':') {
                    if !val.trim().is_empty() {
                        request.headers.push(KeyValue::new(name.trim(), val.trim()));
                    }
                } else if let Some(name) = header.trim_end().strip_suffix(';') {
                    request.headers.push(KeyValue::new(name.trim(), ""));
                }
            }
            "-d" | "--data" | "--data-ascii" | "--data-binary" | "--data-raw" => {
                let text = value(&mut args)?;
                match text.strip_prefix('@') {
                    Some(file) if flag != "--data-raw" => data_file = Some(file.to_owned()),
                    _ => data.push(text),
                }
            }
            "--json" => {
                json = true;
                data.push(value(&mut args)?);
            }
            "--data-urlencode" => data_urlencode.push(value(&mut args)?),
            "-F" | "--form" | "--form-string" => {
                let field = value(&mut args)?;
                let (name, val) = field.split_once('=').unwrap_or((&field, ""));
                let file = (flag != "--form-string")
                    .then(|| val.strip_prefix('@'))
                    .flatten()
                    .map(|path| path.split(';').next().unwrap_or(path).to_owned());
                let content_type = val
                    .split(';')
                    .find_map(|part| part.trim().strip_prefix("type="))
                    .map(str::to_owned);
                form.push(FormField {
                    name: name.to_owned(),
                    value: if file.is_some() {
                        String::new()
                    } else {
                        val.split(';').next().unwrap_or(val).to_owned()
                    },
                    file,
                    content_type,
                    enabled: true,
                });
            }
            "-u" | "--user" => user = Some(value(&mut args)?),
            "--digest" => digest = true,
            "--basic" => digest = false,
            "-G" | "--get" => as_get = true,
            "-I" | "--head" => head = true,
            "-A" | "--user-agent" => request
                .headers
                .push(KeyValue::new("User-Agent", value(&mut args)?)),
            "-e" | "--referer" => request
                .headers
                .push(KeyValue::new("Referer", value(&mut args)?)),
            "-b" | "--cookie" => {
                let cookie = value(&mut args)?;
                if cookie.contains('=') {
                    request.headers.push(KeyValue::new("Cookie", cookie));
                }
            }
            "--url" => url = Some(value(&mut args)?),
            "-L" | "--location" => settings.follow_redirects = Some(true),
            "--max-redirs" => {
                settings.max_redirects = value(&mut args)?.parse().ok();
            }
            "-k" | "--insecure" => settings.verify_tls = Some(false),
            "-m" | "--max-time" => {
                let seconds: f64 = value(&mut args)?.parse().unwrap_or(0.0);
                if seconds > 0.0 {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let millis = (seconds * 1000.0) as u64;
                    settings.timeout_ms = Some(millis);
                }
            }
            "--oauth2-bearer" => {
                request.auth = Auth::Bearer {
                    token: value(&mut args)?,
                    prefix: String::new(),
                };
            }
            // Options that take a value but do not change the request.
            "-o" | "--output" | "-w" | "--write-out" | "--connect-timeout" | "-x" | "--proxy"
            | "--retry" | "--cacert" | "--cert" | "--key" | "-c" | "--cookie-jar" | "-T"
            | "--upload-file" | "-r" | "--range" | "--resolve" | "--interface" => {
                value(&mut args)?;
            }
            _ if flag.starts_with('-') && flag.len() > 2 && !flag.starts_with("--") => {
                // Bundled short flags such as `-sSL` or `-XPOST`.
                let rest = &flag[1..];
                if let Some(m) = rest.strip_prefix('X') {
                    method = Some(m.to_ascii_uppercase());
                } else if let Some(h) = rest.strip_prefix('H') {
                    if let Some((name, val)) = h.split_once(':') {
                        request.headers.push(KeyValue::new(name.trim(), val.trim()));
                    }
                } else {
                    for c in rest.chars() {
                        match c {
                            'L' => settings.follow_redirects = Some(true),
                            'k' => settings.verify_tls = Some(false),
                            'G' => as_get = true,
                            'I' => head = true,
                            _ => {}
                        }
                    }
                }
            }
            _ if flag.starts_with('-') => {}
            _ => {
                if url.is_none() {
                    url = Some(arg);
                }
            }
        }
    }

    let mut url = url.ok_or_else(|| Error::Invalid("the command has no URL".into()))?;
    if as_get && !(data.is_empty() && data_urlencode.is_empty()) {
        let mut parts = data;
        parts.extend(data_urlencode.iter().map(|d| urlencode_field(d)));
        url.push(if url.contains('?') { '&' } else { '?' });
        url.push_str(&parts.join("&"));
        data = Vec::new();
        data_urlencode = Vec::new();
    }

    let has_body =
        !data.is_empty() || !data_urlencode.is_empty() || !form.is_empty() || data_file.is_some();
    request.method = method.unwrap_or_else(|| {
        if head {
            "HEAD".into()
        } else if has_body {
            "POST".into()
        } else {
            "GET".into()
        }
    });

    let content_type = request
        .headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case("content-type"))
        .map(|h| h.value.to_ascii_lowercase());
    if !form.is_empty() {
        request
            .headers
            .retain(|h| !h.name.eq_ignore_ascii_case("content-type"));
        request.body = Body::Multipart { fields: form };
    } else if let Some(file) = data_file {
        request.body = Body::Binary {
            file,
            content_type: None,
        };
    } else if !data.is_empty() || !data_urlencode.is_empty() {
        let mut text = data.join("&");
        if !data_urlencode.is_empty() {
            if !text.is_empty() {
                text.push('&');
            }
            text.push_str(
                &data_urlencode
                    .iter()
                    .map(|d| urlencode_field(d))
                    .collect::<Vec<_>>()
                    .join("&"),
            );
        }
        let json_type = content_type.as_deref().is_some_and(|t| t.contains("json"));
        let form_type = content_type
            .as_deref()
            .is_none_or(|t| t.contains("x-www-form-urlencoded"));
        if json || json_type || (content_type.is_none() && looks_like_json(&text)) {
            if json
                && !request
                    .headers
                    .iter()
                    .any(|h| h.name.eq_ignore_ascii_case("accept"))
            {
                request
                    .headers
                    .push(KeyValue::new("Accept", "application/json"));
            }
            request.headers.retain(|h| {
                !h.name.eq_ignore_ascii_case("content-type") || !h.value.contains("json")
            });
            request.body = Body::Json {
                content: pretty_json(&text),
            };
        } else if form_type && !text.contains('\n') {
            request
                .headers
                .retain(|h| !h.name.eq_ignore_ascii_case("content-type"));
            request.body = Body::Form {
                fields: text
                    .split('&')
                    .filter(|pair| !pair.is_empty())
                    .map(|pair| {
                        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
                        KeyValue::new(decode(name), decode(value))
                    })
                    .collect(),
            };
        } else if content_type.as_deref().is_some_and(|t| t.contains("xml")) {
            request.headers.retain(|h| {
                !h.name.eq_ignore_ascii_case("content-type") || !h.value.contains("xml")
            });
            request.body = Body::Xml { content: text };
        } else {
            request.body = Body::Text {
                content: text,
                content_type: None,
            };
        }
    }

    if let Some(user) = user {
        let (username, password) = user.split_once(':').unwrap_or((&user, ""));
        request.auth = if digest {
            Auth::Digest {
                username: username.to_owned(),
                password: password.to_owned(),
            }
        } else {
            Auth::Basic {
                username: username.to_owned(),
                password: password.to_owned(),
            }
        };
    } else if let Some(at) = request.headers.iter().position(|h| {
        h.name.eq_ignore_ascii_case("authorization") && h.value.starts_with("Bearer ")
    }) {
        let header = request.headers.remove(at);
        request.auth = Auth::Bearer {
            token: header.value["Bearer ".len()..].trim().to_owned(),
            prefix: String::new(),
        };
    }

    request.url = url;
    request.settings = settings;
    request.name = default_name(&request.method, &request.url);
    Ok(request)
}

/// A name for an imported request: the method and the URL's path.
fn default_name(method: &str, url: &str) -> String {
    let without_scheme = url.split_once("://").map_or(url, |(_, rest)| rest);
    let path = without_scheme
        .split_once('/')
        .map_or("/", |(_, path)| path)
        .split(['?', '#'])
        .next()
        .unwrap_or_default();
    let path = path.trim_matches('/');
    if path.is_empty() {
        format!(
            "{method} {}",
            without_scheme.split(['/', '?']).next().unwrap_or(url)
        )
    } else {
        format!("{method} /{path}")
    }
}

fn looks_like_json(text: &str) -> bool {
    let trimmed = text.trim_start();
    (trimmed.starts_with('{') || trimmed.starts_with('['))
        && serde_json::from_str::<serde_json::Value>(text).is_ok()
}

fn pretty_json(text: &str) -> String {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|value| serde_json::to_string_pretty(&value).ok())
        .unwrap_or_else(|| text.to_owned())
}

/// `--data-urlencode` accepts `content`, `=content`, `name=content`,
/// `@file` and `name@file`; files become a variable reference.
fn urlencode_field(field: &str) -> String {
    let encode =
        |text: &str| url::form_urlencoded::byte_serialize(text.as_bytes()).collect::<String>();
    if let Some(content) = field.strip_prefix('=') {
        return encode(content);
    }
    match field.split_once('=') {
        Some((name, content)) => format!("{name}={}", encode(content)),
        None => encode(field),
    }
}

fn decode(text: &str) -> String {
    url::form_urlencoded::parse(format!("x={text}").as_bytes())
        .next()
        .map_or_else(|| text.to_owned(), |(_, v)| v.into_owned())
}

/// Splits a command line into words, following POSIX shell quoting, or
/// Windows `cmd` quoting when the command uses `^` continuations.
fn split_words(command: &str) -> Result<Vec<String>> {
    let windows = command.contains("^\n") || command.contains("^\r\n") || command.contains("^\"");
    if windows {
        return Ok(split_cmd(command));
    }
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut chars = command.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('\n') => {}
                Some('\r') => {
                    chars.next_if_eq(&'\n');
                }
                Some(next) => {
                    word.push(next);
                    in_word = true;
                }
                None => {}
            },
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(c) => word.push(c),
                        None => {
                            return Err(Error::Invalid("a single quote is never closed".into()));
                        }
                    }
                }
            }
            '$' if chars.peek() == Some(&'\'') => {
                // ANSI-C quoting: $'...' with backslash escapes.
                chars.next();
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some('\\') => match chars.next() {
                            Some('n') => word.push('\n'),
                            Some('t') => word.push('\t'),
                            Some('r') => word.push('\r'),
                            Some(other) => word.push(other),
                            None => {}
                        },
                        Some(c) => word.push(c),
                        None => return Err(Error::Invalid("a $' quote is never closed".into())),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(next @ ('"' | '\\' | '$' | '`')) => word.push(next),
                            Some('\n') => {}
                            Some(other) => {
                                word.push('\\');
                                word.push(other);
                            }
                            None => {}
                        },
                        Some(c) => word.push(c),
                        None => {
                            return Err(Error::Invalid("a double quote is never closed".into()));
                        }
                    }
                }
            }
            c if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            c => {
                word.push(c);
                in_word = true;
            }
        }
    }
    if in_word {
        words.push(word);
    }
    Ok(words)
}

/// Windows `cmd` quoting: `^` escapes the next character and continues
/// lines; inside double quotes, `^"` and `""` stand for a quote.
fn split_cmd(command: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut quoted = false;
    let mut chars = command.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '^' => match chars.next() {
                Some('\r') => {
                    chars.next_if_eq(&'\n');
                }
                Some('\n') => {}
                Some('"') => {
                    // Chrome's "Copy as cURL (cmd)" writes ^" for quotes
                    // that delimit words and \^" for literal quotes.
                    if word.ends_with('\\') {
                        word.pop();
                        word.push('"');
                    } else {
                        quoted = !quoted;
                    }
                    in_word = true;
                }
                Some(next) => {
                    word.push(next);
                    in_word = true;
                }
                None => {}
            },
            '"' => {
                if quoted && chars.peek() == Some(&'"') {
                    chars.next();
                    word.push('"');
                } else {
                    quoted = !quoted;
                }
                in_word = true;
            }
            c if c.is_whitespace() && !quoted => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            c => {
                word.push(c);
                in_word = true;
            }
        }
    }
    if in_word {
        words.push(word);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header<'a>(request: &'a Request, name: &str) -> Option<&'a str> {
        request
            .headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case(name))
            .map(|h| h.value.as_str())
    }

    #[test]
    fn a_simple_get() {
        let request = parse("curl https://api.example.com/users?page=2").unwrap();
        assert_eq!(request.method, "GET");
        assert_eq!(request.url, "https://api.example.com/users?page=2");
        assert_eq!(request.name, "GET /users");
    }

    #[test]
    fn json_posts_from_documentation() {
        let request = parse(
            r#"curl -X POST "https://api.example.com/v1/users" \
              -H 'Content-Type: application/json' \
              -H "Authorization: Bearer abc.def" \
              -d '{"name":"Ada","tags":["x"]}'"#,
        )
        .unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(header(&request, "content-type"), None);
        assert_eq!(
            request.auth,
            Auth::Bearer {
                token: "abc.def".into(),
                prefix: String::new()
            }
        );
        let Body::Json { content } = &request.body else {
            panic!("{:?}", request.body)
        };
        assert!(content.contains("\"name\": \"Ada\""), "{content}");
    }

    #[test]
    fn data_without_a_type_is_a_form() {
        let request = parse(
            "curl https://x.test/login -d user=ada -d 'pass=p%40ss' --data-urlencode 'note=a b'",
        )
        .unwrap();
        assert_eq!(request.method, "POST");
        let Body::Form { fields } = &request.body else {
            panic!("{:?}", request.body)
        };
        let pairs: Vec<_> = fields
            .iter()
            .map(|f| (f.name.as_str(), f.value.as_str()))
            .collect();
        assert_eq!(pairs, [("user", "ada"), ("pass", "p@ss"), ("note", "a b")]);
    }

    #[test]
    fn multipart_forms_and_files() {
        let request = parse(
            "curl -F 'avatar=@photos/me.png;type=image/png' -F name=Ada https://x.test/profile",
        )
        .unwrap();
        let Body::Multipart { fields } = &request.body else {
            panic!("{:?}", request.body)
        };
        assert_eq!(fields[0].file.as_deref(), Some("photos/me.png"));
        assert_eq!(fields[0].content_type.as_deref(), Some("image/png"));
        assert_eq!(fields[1].value, "Ada");
        assert_eq!(request.method, "POST");
    }

    #[test]
    fn flags_map_to_settings_and_auth() {
        let request =
            parse("curl -sSLk --max-time 2.5 -u ada:secret --digest -XPATCH https://x.test/a")
                .unwrap();
        assert_eq!(request.method, "PATCH");
        assert_eq!(request.settings.follow_redirects, Some(true));
        assert_eq!(request.settings.verify_tls, Some(false));
        assert_eq!(request.settings.timeout_ms, Some(2500));
        assert_eq!(
            request.auth,
            Auth::Digest {
                username: "ada".into(),
                password: "secret".into()
            }
        );
    }

    #[test]
    fn get_moves_data_into_the_query() {
        let request =
            parse("curl -G https://x.test/search -d q=rust --data-urlencode 'tag=a b'").unwrap();
        assert_eq!(request.method, "GET");
        assert_eq!(request.url, "https://x.test/search?q=rust&tag=a+b");
        assert!(request.body.is_none());
    }

    #[test]
    fn chrome_copy_as_curl_for_bash() {
        let request = parse(
            "curl 'https://x.test/api' \\\n  -H 'accept: */*' \\\n  -H 'cookie: a=1; b=2' \\\n  --data-raw $'{\"q\":\"it\\'s\"}' \\\n  --compressed",
        )
        .unwrap();
        assert_eq!(header(&request, "cookie"), Some("a=1; b=2"));
        let Body::Json { content } = &request.body else {
            panic!("{:?}", request.body)
        };
        assert!(content.contains("it's"), "{content}");
    }

    #[test]
    fn chrome_copy_as_curl_for_cmd() {
        let request = parse(
            "curl ^\"https://x.test/api?a=1^&b=2^\" ^\r\n  -H ^\"content-type: application/json^\" ^\r\n  --data-raw ^\"^{^\\^\"name^\\^\":^\\^\"Ada^\\^\"^}^\"",
        )
        .unwrap();
        assert_eq!(request.url, "https://x.test/api?a=1&b=2");
        let Body::Json { content } = &request.body else {
            panic!("{:?}", request.body)
        };
        assert!(content.contains("\"name\": \"Ada\""), "{content}");
    }

    #[test]
    fn errors_are_reported() {
        assert!(parse("wget https://x.test").is_err());
        assert!(parse("curl -H 'X: y'").is_err());
        assert!(parse("curl 'https://x.test").is_err());
    }
}
