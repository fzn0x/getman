//! Writing a request as code in other languages and tools.
//!
//! Every value goes through the quoting rules of the language it lands
//! in, so quotes, backslashes, `$`, line breaks and non-ASCII text reach
//! the server as they were.

use std::fmt::Write as _;

use serde::Serialize;

use crate::http::{Header, Outgoing, OutgoingBody, Part};

/// A language code can be generated for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Language {
    /// Identifier passed to [`generate`].
    pub id: &'static str,
    /// Name to show.
    pub label: &'static str,
    /// Syntax for highlighting the snippet.
    pub syntax: &'static str,
    /// Whether the snippet has an indent that [`Options::indent`] changes.
    pub indent: bool,
    /// Whether the snippet has imports or a `main` function that
    /// [`Options::boilerplate`] leaves out.
    pub boilerplate: bool,
}

/// Every language [`generate`] supports.
pub const LANGUAGES: &[Language] = &[
    Language {
        id: "curl",
        label: "cURL",
        syntax: "shell",
        indent: true,
        boilerplate: false,
    },
    Language {
        id: "httpie",
        label: "HTTPie",
        syntax: "shell",
        indent: false,
        boilerplate: false,
    },
    Language {
        id: "wget",
        label: "Wget",
        syntax: "shell",
        indent: true,
        boilerplate: false,
    },
    Language {
        id: "powershell",
        label: "PowerShell",
        syntax: "powershell",
        indent: true,
        boilerplate: false,
    },
    Language {
        id: "http",
        label: "HTTP/1.1 (raw)",
        syntax: "http",
        indent: false,
        boilerplate: false,
    },
    Language {
        id: "fetch",
        label: "JavaScript (fetch)",
        syntax: "javascript",
        indent: true,
        boilerplate: true,
    },
    Language {
        id: "axios",
        label: "JavaScript (axios)",
        syntax: "javascript",
        indent: true,
        boilerplate: true,
    },
    Language {
        id: "node",
        label: "Node.js (node:https)",
        syntax: "javascript",
        indent: true,
        boilerplate: true,
    },
    Language {
        id: "python",
        label: "Python (requests)",
        syntax: "python",
        indent: true,
        boilerplate: true,
    },
    Language {
        id: "go",
        label: "Go (net/http)",
        syntax: "go",
        indent: true,
        boilerplate: true,
    },
    Language {
        id: "rust",
        label: "Rust (reqwest)",
        syntax: "rust",
        indent: true,
        boilerplate: true,
    },
    Language {
        id: "java",
        label: "Java (HttpClient)",
        syntax: "java",
        indent: true,
        boilerplate: true,
    },
    Language {
        id: "kotlin",
        label: "Kotlin (OkHttp)",
        syntax: "kotlin",
        indent: true,
        boilerplate: true,
    },
    Language {
        id: "csharp",
        label: "C# (HttpClient)",
        syntax: "csharp",
        indent: false,
        boilerplate: true,
    },
    Language {
        id: "swift",
        label: "Swift (URLSession)",
        syntax: "swift",
        indent: false,
        boilerplate: true,
    },
    Language {
        id: "php",
        label: "PHP (cURL)",
        syntax: "php",
        indent: true,
        boilerplate: false,
    },
    Language {
        id: "ruby",
        label: "Ruby (Net::HTTP)",
        syntax: "ruby",
        indent: true,
        boilerplate: true,
    },
    Language {
        id: "dart",
        label: "Dart (http)",
        syntax: "dart",
        indent: true,
        boilerplate: true,
    },
];

/// Headers that clients set themselves and that only add noise.
fn interesting(headers: &[Header]) -> impl Iterator<Item = &Header> {
    headers.iter().filter(|h| {
        !(h.name.eq_ignore_ascii_case("user-agent") && h.value.starts_with("getman/")
            || h.name.eq_ignore_ascii_case("accept") && h.value == "*/*"
            || h.name.eq_ignore_ascii_case("content-length")
            || h.name.eq_ignore_ascii_case("host"))
    })
}

/// Rewrites the files a request uploads relative to `root`, so code
/// written for it works from the workspace on any machine.
pub fn relativize(request: &mut Outgoing, root: &std::path::Path) {
    let Ok(root) = root.canonicalize() else {
        return;
    };
    let shorten = |path: &mut std::path::PathBuf| {
        if let Ok(relative) = path.strip_prefix(&root) {
            *path = relative.to_path_buf();
        }
    };
    match &mut request.body {
        OutgoingBody::File { path } => shorten(path),
        OutgoingBody::Multipart { parts } => {
            for part in parts {
                if let Some(path) = &mut part.file {
                    shorten(path);
                }
            }
        }
        OutgoingBody::None | OutgoingBody::Bytes { .. } => {}
    }
}

/// How a snippet is indented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Indent {
    /// Two spaces.
    Two,
    /// Four spaces.
    Four,
    /// One tab per level.
    Tab,
}

impl Indent {
    /// The text of one level.
    fn unit(self) -> &'static str {
        match self {
            Indent::Two => "  ",
            Indent::Four => "    ",
            Indent::Tab => "\t",
        }
    }
}

impl std::str::FromStr for Indent {
    type Err = String;

    fn from_str(text: &str) -> std::result::Result<Self, String> {
        match text.trim().to_ascii_lowercase().as_str() {
            "2" => Ok(Indent::Two),
            "4" => Ok(Indent::Four),
            "tab" | "tabs" => Ok(Indent::Tab),
            other => Err(format!("{other:?} is not an indent; use 2, 4 or tab")),
        }
    }
}

/// How to write a snippet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// The indent to use. `None` keeps each language's own habit: tabs
    /// for Go, two spaces for JavaScript, Ruby and Dart, four elsewhere.
    pub indent: Option<Indent>,
    /// Whether to write the imports and the `main` function around the
    /// request, in the languages that need them. Without them the
    /// snippet is the statements that make the request.
    pub boilerplate: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            indent: None,
            boilerplate: true,
        }
    }
}

/// Writes a request as code. Returns `None` for an unknown language.
pub fn generate(language: &str, request: &Outgoing) -> Option<String> {
    generate_with(language, request, &Options::default())
}

/// Writes a request as code with the given options. Returns `None` for
/// an unknown language.
pub fn generate_with(language: &str, request: &Outgoing, options: &Options) -> Option<String> {
    // A prepared multipart request carries the Content-Type with the
    // boundary its body was built with. Every snippet builds the body
    // anew, with the client's own boundary, so that header would not match.
    let own_boundary;
    let request = if is_multipart(request) {
        own_boundary = Outgoing {
            headers: request
                .headers
                .iter()
                .filter(|h| !h.name.eq_ignore_ascii_case("content-type"))
                .cloned()
                .collect(),
            ..request.clone()
        };
        &own_boundary
    } else {
        request
    };
    let full = options.boilerplate;
    // What a level of indent is in the text each writer produces, and
    // where its string literals can hold a real line break.
    let (code, native, literals) = match language {
        "curl" => (curl(request), "  ", Literals::SingleQuotes),
        "httpie" => (httpie(request), "", Literals::SingleQuotes),
        "wget" => (wget(request), "  ", Literals::SingleQuotes),
        "powershell" => (powershell(request), "    ", Literals::SingleQuotes),
        "http" => (raw_http(request), "", Literals::None),
        "fetch" => (fetch(request, full), "  ", Literals::None),
        "axios" => (axios(request, full), "  ", Literals::None),
        "node" => (node(request, full), "  ", Literals::None),
        "python" => (python(request, full), "    ", Literals::None),
        "go" => (go(request, full), "\t", Literals::Go),
        "rust" => (rust(request, full), "    ", Literals::None),
        "java" => (java(request, full), "    ", Literals::None),
        "kotlin" => (kotlin(request, full), "    ", Literals::None),
        "csharp" => (csharp(request, full), "", Literals::None),
        "swift" => (swift(request, full), "", Literals::None),
        "php" => (php(request), "    ", Literals::Escaped),
        "ruby" => (ruby(request, full), "  ", Literals::Escaped),
        "dart" => (dart(request, full), "  ", Literals::None),
        _ => return None,
    };
    Some(match options.indent {
        Some(indent) if !native.is_empty() && indent.unit() != native => {
            map_lines(&code, literals, |line| {
                let mut rest = line;
                let mut levels = 0;
                while let Some(after) = rest.strip_prefix(native) {
                    rest = after;
                    levels += 1;
                }
                format!("{}{rest}", indent.unit().repeat(levels))
            })
        }
        _ => code,
    })
}

/// Where a language's string literals can hold a real line break, so
/// that the lines inside one are left as they are.
#[derive(Clone, Copy)]
enum Literals {
    None,
    /// Single quotes without escapes: the shell and PowerShell.
    SingleQuotes,
    /// Single and double quotes with backslash escapes: PHP and Ruby.
    Escaped,
    /// Double quotes with backslash escapes, and raw backquotes.
    Go,
}

/// Rewrites each line of `code` with `change`, except the lines that
/// continue a string literal.
fn map_lines(code: &str, literals: Literals, mut change: impl FnMut(&str) -> String) -> String {
    // The quotes, whether a backslash escapes inside them, and whether
    // it does outside them (the shell writes a quote as `'\''`).
    let (quotes, escapes, escapes_outside): (&[char], bool, bool) = match literals {
        Literals::None => (&[], false, false),
        Literals::SingleQuotes => (&['\''], false, true),
        Literals::Escaped => (&['\'', '"'], true, false),
        Literals::Go => (&['"', '`'], true, false),
    };
    let mut open: Option<char> = None;
    let mut lines = Vec::new();
    for line in code.split('\n') {
        lines.push(if open.is_none() {
            change(line)
        } else {
            line.to_owned()
        });
        let mut chars = line.chars();
        while let Some(c) = chars.next() {
            match open {
                None if escapes_outside && c == '\\' => {
                    chars.next();
                }
                None if quotes.contains(&c) => open = Some(c),
                Some(quote) if escapes && c == '\\' && quote != '`' => {
                    chars.next();
                }
                Some(quote) if c == quote => open = None,
                _ => {}
            }
        }
    }
    lines.join("\n")
}

/// Takes one level of indent off every line, for code that was written
/// inside a function and is now the whole snippet.
fn dedent(code: &str, unit: &str, literals: Literals) -> String {
    map_lines(code, literals, |line| {
        line.strip_prefix(unit).unwrap_or(line).to_owned()
    })
}

fn body_text(body: &OutgoingBody) -> Option<String> {
    match body {
        OutgoingBody::Bytes { data } => Some(data.text().into_owned()),
        _ => None,
    }
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn path_text(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn content_type(request: &Outgoing) -> Option<&str> {
    crate::http::find_header(&request.headers, "content-type")
}

fn is_multipart(request: &Outgoing) -> bool {
    matches!(request.body, OutgoingBody::Multipart { .. })
}

/// The body as JSON, when it is JSON.
fn json_body(request: &Outgoing) -> Option<serde_json::Value> {
    content_type(request).filter(|t| t.contains("json"))?;
    serde_json::from_str(&body_text(&request.body)?).ok()
}

/// Headers with repeated names joined into one, for languages that keep
/// headers in a map. Multipart bodies leave out the Content-Type, which
/// the client writes with its boundary.
fn merged_headers(request: &Outgoing) -> Vec<Header> {
    let mut out: Vec<Header> = Vec::new();
    for header in interesting(&request.headers) {
        if is_multipart(request) && header.name.eq_ignore_ascii_case("content-type") {
            continue;
        }
        match out
            .iter_mut()
            .find(|h| h.name.eq_ignore_ascii_case(&header.name))
        {
            Some(existing) => {
                existing.value.push_str(", ");
                existing.value.push_str(&header.value);
            }
            None => out.push(header.clone()),
        }
    }
    out
}

/// A multipart body written out by hand, for clients without a helper:
/// the text before and after each file, and the files' paths.
const BOUNDARY: &str = "----GetmanFormBoundary7MA4YWxkTrZu0gW";

enum Chunk {
    Text(String),
    File(String),
}

fn multipart_chunks(parts: &[Part]) -> Vec<Chunk> {
    // Quotes in names are written as %22, as browsers do.
    let quoted = |text: &str| text.replace('"', "%22").replace(['\r', '\n'], " ");
    let mut chunks = Vec::new();
    let mut text = String::new();
    for part in parts {
        let _ = write!(
            text,
            "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{}\"",
            quoted(&part.name)
        );
        match &part.file {
            Some(path) => {
                let _ = write!(text, "; filename=\"{}\"\r\n", quoted(&file_name(path)));
                let content_type = part.content_type.clone().unwrap_or_else(|| {
                    mime_guess::from_path(path)
                        .first_or_octet_stream()
                        .essence_str()
                        .to_owned()
                });
                let _ = write!(text, "Content-Type: {content_type}\r\n\r\n");
                chunks.push(Chunk::Text(std::mem::take(&mut text)));
                chunks.push(Chunk::File(path_text(path)));
                text.push_str("\r\n");
            }
            None => {
                text.push_str("\r\n");
                if let Some(content_type) = &part.content_type {
                    let _ = write!(text, "Content-Type: {content_type}\r\n");
                }
                let _ = write!(text, "\r\n{}\r\n", part.value);
            }
        }
    }
    let _ = write!(text, "--{BOUNDARY}--\r\n");
    chunks.push(Chunk::Text(text));
    chunks
}

/// Quotes a word for POSIX shells.
fn sh(text: &str) -> String {
    let plain = !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:@%+=,".contains(c));
    if plain {
        text.to_owned()
    } else {
        format!("'{}'", text.replace('\'', r"'\''"))
    }
}

fn curl(request: &Outgoing) -> String {
    let mut first = String::from("curl");
    // Braces and brackets are curl's URL globbing, as in `{{baseUrl}}`.
    if request.url.contains(['{', '}', '[', ']']) {
        first.push_str(" --globoff");
    }
    match request.method.as_str() {
        // With data, curl would switch to POST; say GET to keep it.
        "GET" if matches!(request.body, OutgoingBody::None) => {}
        // -X HEAD would wait for a body that never comes.
        "HEAD" => first.push_str(" --head"),
        method => {
            let _ = write!(first, " -X {}", sh(method));
        }
    }
    let _ = write!(first, " {}", sh(&request.url));
    let mut lines = vec![first];
    for header in interesting(&request.headers) {
        lines.push(format!(
            "-H {}",
            sh(&format!("{}: {}", header.name, header.value))
        ));
    }
    match &request.body {
        OutgoingBody::None => {}
        OutgoingBody::Bytes { .. } => {
            lines.push(format!(
                "--data-raw {}",
                sh(&body_text(&request.body).unwrap_or_default())
            ));
        }
        OutgoingBody::File { path } => lines.push(format!(
            "--data-binary {}",
            sh(&format!("@{}", path_text(path)))
        )),
        OutgoingBody::Multipart { parts } => {
            for part in parts {
                match &part.file {
                    Some(path) => {
                        let mut value = format!("{}=@{}", part.name, curl_form_file(path));
                        if let Some(content_type) = &part.content_type {
                            let _ = write!(value, ";type={content_type}");
                        }
                        lines.push(format!("-F {}", sh(&value)));
                    }
                    // -F would read a file for a value starting with @ or <.
                    None => lines.push(format!(
                        "--form-string {}",
                        sh(&format!("{}={}", part.name, part.value))
                    )),
                }
            }
        }
    }
    lines.join(" \\\n  ")
}

/// A file for `-F`, in quotes when its name holds `;`, `,` or `"`, which
/// curl would read as separators.
fn curl_form_file(path: &std::path::Path) -> String {
    let path = path_text(path);
    if path.contains([';', ',', '"']) {
        format!("\"{}\"", path.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        path
    }
}

/// Escapes the characters HTTPie reads as separators in a field name.
/// (A backslash right before a separator in a value cannot be written.)
fn httpie_key(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if matches!(c, ':' | '=' | '@' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn httpie(request: &Outgoing) -> String {
    let mut words = vec!["http".to_owned()];
    match &request.body {
        OutgoingBody::Multipart { .. } => words.push("--multipart".into()),
        OutgoingBody::Bytes { .. } => {
            words.push(format!(
                "--raw {}",
                sh(&body_text(&request.body).unwrap_or_default())
            ));
        }
        _ => {}
    }
    words.push(sh(&request.method));
    words.push(sh(&request.url));
    for header in interesting(&request.headers) {
        if is_multipart(request) && header.name.eq_ignore_ascii_case("content-type") {
            continue;
        }
        // `Name:` would remove the header; `Name;` sends it empty.
        words.push(if header.value.is_empty() {
            sh(&format!("{};", header.name))
        } else {
            sh(&format!("{}:{}", header.name, header.value))
        });
    }
    if let OutgoingBody::Multipart { parts } = &request.body {
        for part in parts {
            words.push(match &part.file {
                Some(path) => {
                    let mut field = format!("{}@{}", httpie_key(&part.name), path_text(path));
                    if let Some(content_type) = &part.content_type {
                        let _ = write!(field, ";type={content_type}");
                    }
                    sh(&field)
                }
                None => sh(&format!("{}={}", httpie_key(&part.name), part.value)),
            });
        }
    }
    let mut command = words.join(" ");
    if let OutgoingBody::File { path } = &request.body {
        command.push_str(&format!(" < {}", sh(&path_text(path))));
    }
    command
}

fn wget(request: &Outgoing) -> String {
    let mut lines = vec!["wget --quiet --output-document=- --content-on-error".to_owned()];
    let has_body = !matches!(request.body, OutgoingBody::None);
    if request.method != "GET" || has_body {
        lines.push(format!("--method={}", sh(&request.method)));
    }
    for header in interesting(&request.headers) {
        lines.push(format!(
            "--header={}",
            sh(&format!("{}: {}", header.name, header.value))
        ));
    }
    let mut note = "";
    match &request.body {
        OutgoingBody::None => {}
        OutgoingBody::Bytes { .. } => lines.push(format!(
            "--body-data={}",
            sh(&body_text(&request.body).unwrap_or_default())
        )),
        OutgoingBody::File { path } => {
            lines.push(format!("--body-file={}", sh(&path_text(path))));
        }
        OutgoingBody::Multipart { .. } => {
            note = "# Wget cannot send a multipart form; use cURL or HTTPie for this request.\n";
        }
    }
    lines.push(sh(&request.url));
    format!("{note}{}", lines.join(" \\\n  "))
}

/// A PowerShell single-quoted string. PowerShell also reads the
/// typographic quotes ‘ ’ ‚ ‛ as single quotes, so they are doubled too.
fn ps(text: &str) -> String {
    let mut out = String::from("'");
    for c in text.chars() {
        if matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201a}' | '\u{201b}') {
            out.push(c);
        }
        out.push(c);
    }
    out.push('\'');
    out
}

fn powershell(request: &Outgoing) -> String {
    let mut out = String::new();
    // A hashtable holds each name once.
    let headers: Vec<Header> = merged_headers(request)
        .into_iter()
        .filter(|h| !h.name.eq_ignore_ascii_case("content-type"))
        .collect();
    if !headers.is_empty() {
        out.push_str("$headers = @{\n");
        for header in &headers {
            let _ = writeln!(out, "    {} = {}", ps(&header.name), ps(&header.value));
        }
        out.push_str("}\n");
    }
    let standard = [
        "GET", "HEAD", "POST", "PUT", "DELETE", "TRACE", "OPTIONS", "MERGE", "PATCH",
    ];
    let mut args = vec![
        if standard.contains(&request.method.as_str()) {
            format!("-Method {}", request.method)
        } else {
            format!("-CustomMethod {}", ps(&request.method))
        },
        format!("-Uri {}", ps(&request.url)),
    ];
    if !headers.is_empty() {
        args.push("-Headers $headers".into());
    }
    if let Some(content_type) = content_type(request)
        && !is_multipart(request)
    {
        args.push(format!("-ContentType {}", ps(content_type)));
    }
    match &request.body {
        OutgoingBody::None => {}
        OutgoingBody::Bytes { .. } => {
            let text = body_text(&request.body).unwrap_or_default();
            // Windows PowerShell sends strings as ISO-8859-1; bytes keep
            // other text intact.
            if text.is_ascii() {
                let _ = writeln!(out, "$body = {}", ps(&text));
            } else {
                let _ = writeln!(
                    out,
                    "$body = [System.Text.Encoding]::UTF8.GetBytes({})",
                    ps(&text)
                );
            }
            args.push("-Body $body".into());
        }
        OutgoingBody::File { path } => args.push(format!("-InFile {}", ps(&path_text(path)))),
        OutgoingBody::Multipart { parts } => {
            // Ordered, so the parts go out in the order they are written.
            out.push_str("$form = [ordered]@{\n");
            for part in parts {
                match &part.file {
                    Some(path) => {
                        let _ = writeln!(
                            out,
                            "    {} = Get-Item {}",
                            ps(&part.name),
                            ps(&path_text(path))
                        );
                    }
                    None => {
                        let _ = writeln!(out, "    {} = {}", ps(&part.name), ps(&part.value));
                    }
                }
            }
            out.push_str("}\n");
            args.push("-Form $form".into());
        }
    }
    let _ = write!(
        out,
        "$response = Invoke-RestMethod {}\n$response",
        args.join(" `\n    ")
    );
    out
}

/// The request as it goes over the wire with HTTP/1.1.
fn raw_http(request: &Outgoing) -> String {
    let rest = request
        .url
        .split_once("://")
        .map_or(request.url.as_str(), |(_, rest)| rest);
    let (authority, target) = match rest.find(['/', '?', '#']) {
        Some(at) => rest.split_at(at),
        None => (rest, ""),
    };
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let target = target.split('#').next().unwrap_or_default();
    let target = if target.starts_with('/') {
        target.to_owned()
    } else {
        format!("/{target}")
    };
    let mut out = format!("{} {target} HTTP/1.1\nHost: {host}\n", request.method);
    for header in interesting(&request.headers) {
        let _ = writeln!(out, "{}: {}", header.name, header.value);
    }
    match &request.body {
        OutgoingBody::None => {}
        OutgoingBody::Bytes { .. } => {
            let text = body_text(&request.body).unwrap_or_default();
            let _ = write!(out, "Content-Length: {}\n\n{text}", text.len());
        }
        OutgoingBody::File { path } => {
            let _ = write!(out, "\n<the contents of {}>", path_text(path));
        }
        OutgoingBody::Multipart { parts } => {
            let _ = writeln!(
                out,
                "Content-Type: multipart/form-data; boundary={BOUNDARY}\n"
            );
            for chunk in multipart_chunks(parts) {
                match chunk {
                    Chunk::Text(text) => out.push_str(&text.replace("\r\n", "\n")),
                    Chunk::File(path) => {
                        let _ = write!(out, "<the contents of {path}>");
                    }
                }
            }
        }
    }
    // A text body's own trailing line breaks count in its Content-Length.
    match &request.body {
        OutgoingBody::Bytes { .. } if !body_text(&request.body).unwrap_or_default().is_empty() => {
            out
        }
        _ => out.trim_end_matches('\n').to_owned(),
    }
}

/// A string literal for JavaScript or Python: JSON's syntax is valid in
/// both.
fn js(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_default()
}

fn fetch(request: &Outgoing, full: bool) -> String {
    let mut out = String::new();
    let mut options = vec![format!("  method: {}", js(&request.method))];
    let headers = merged_headers(request);
    if !headers.is_empty() {
        let lines: Vec<String> = headers
            .iter()
            .map(|h| format!("    {}: {}", js(&h.name), js(&h.value)))
            .collect();
        options.push(format!("  headers: {{\n{},\n  }}", lines.join(",\n")));
    }
    match &request.body {
        OutgoingBody::None => {}
        OutgoingBody::Bytes { .. } => {
            let text = body_text(&request.body).unwrap_or_default();
            let is_json = crate::http::find_header(&request.headers, "content-type")
                .is_some_and(|t| t.contains("json"));
            match serde_json::from_str::<serde_json::Value>(&text) {
                Ok(value) if is_json => {
                    let pretty = serde_json::to_string_pretty(&value).unwrap_or(text);
                    options.push(format!(
                        "  body: JSON.stringify({})",
                        pretty.replace('\n', "\n  ")
                    ));
                }
                _ => options.push(format!("  body: {}", js(&text))),
            }
        }
        OutgoingBody::File { path } => {
            if full {
                out.push_str("// In Node.js; in a browser, use a File from an <input>.\n");
                out.push_str("import { readFile } from \"node:fs/promises\";\n\n");
            }
            options.push(format!("  body: await readFile({})", js(&path_text(path))));
        }
        OutgoingBody::Multipart { parts } => {
            out.push_str("const form = new FormData();\n");
            for part in parts {
                match &part.file {
                    Some(path) => {
                        let _ = writeln!(
                            out,
                            "form.append({}, fileInput.files[0], {}); // {}",
                            js(&part.name),
                            js(&file_name(path)),
                            path_text(path)
                        );
                    }
                    None => {
                        let _ =
                            writeln!(out, "form.append({}, {});", js(&part.name), js(&part.value));
                    }
                }
            }
            out.push('\n');
            options.push("  body: form".into());
        }
    }
    let _ = write!(
        out,
        "const response = await fetch({}, {{\n{},\n}});\n\nconsole.log(response.status);\nconsole.log(await response.text());",
        js(&request.url),
        options.join(",\n")
    );
    out
}

fn python(request: &Outgoing, full: bool) -> String {
    let mut out = String::new();
    if full {
        out.push_str("import requests\n\n");
    }
    let headers = merged_headers(request);
    let mut args = vec![js(&request.method), "url".to_owned()];
    let _ = writeln!(out, "url = {}", js(&request.url));
    if !headers.is_empty() {
        out.push_str("headers = {\n");
        for header in &headers {
            let _ = writeln!(out, "    {}: {},", js(&header.name), js(&header.value));
        }
        out.push_str("}\n");
        args.push("headers=headers".into());
    }
    match &request.body {
        OutgoingBody::None => {}
        OutgoingBody::Bytes { .. } => {
            let _ = writeln!(
                out,
                "data = {}",
                js(&body_text(&request.body).unwrap_or_default())
            );
            args.push("data=data.encode()".into());
        }
        OutgoingBody::File { path } => {
            let _ = writeln!(out, "data = open({}, \"rb\")", js(&path_text(path)));
            args.push("data=data".into());
        }
        OutgoingBody::Multipart { parts } => {
            // One list, so the form is multipart even with no file in it
            // (`data=` would send it URL-encoded), and names can repeat.
            out.push_str("files = [\n");
            for part in parts {
                match &part.file {
                    Some(path) => {
                        let content_type = part
                            .content_type
                            .as_ref()
                            .map(|t| format!(", {}", js(t)))
                            .unwrap_or_default();
                        let _ = writeln!(
                            out,
                            "    ({}, ({}, open({}, \"rb\"){content_type})),",
                            js(&part.name),
                            js(&file_name(path)),
                            js(&path_text(path))
                        );
                    }
                    None => {
                        let _ = writeln!(
                            out,
                            "    ({}, (None, {})),",
                            js(&part.name),
                            js(&part.value)
                        );
                    }
                }
            }
            out.push_str("]\n");
            args.push("files=files".into());
        }
    }
    let _ = write!(
        out,
        "\nresponse = requests.request({})\n\nprint(response.status_code)\nprint(response.text)",
        args.join(", ")
    );
    out
}

/// A Go string literal: a raw string when that reads better and holds
/// the text exactly (raw strings drop carriage returns and cannot hold a
/// backquote), else an interpreted one.
fn go_string(text: &str) -> String {
    let raw_safe = !text.contains('`')
        && !text.contains('\u{feff}')
        && text
            .chars()
            .all(|c| c == '\n' || c == '\t' || !c.is_control());
    if raw_safe && (text.contains('"') || text.contains('\\') || text.contains('\n')) {
        return format!("`{text}`");
    }
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() || c == '\u{feff}' => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn go(request: &Outgoing, full: bool) -> String {
    let mut imports = vec!["\"fmt\"", "\"io\"", "\"net/http\""];
    let mut setup = String::new();
    let body_var = match &request.body {
        OutgoingBody::None => "nil".to_owned(),
        OutgoingBody::Bytes { .. } => {
            imports.push("\"strings\"");
            let _ = writeln!(
                setup,
                "\tbody := strings.NewReader({})",
                go_string(&body_text(&request.body).unwrap_or_default())
            );
            "body".to_owned()
        }
        OutgoingBody::File { path } => {
            imports.push("\"os\"");
            let _ = writeln!(
                setup,
                "\tbody, err := os.Open({})\n\tif err != nil {{\n\t\tpanic(err)\n\t}}\n\tdefer body.Close()",
                go_string(&path_text(path))
            );
            "body".to_owned()
        }
        OutgoingBody::Multipart { parts } => {
            imports.extend(["\"bytes\"", "\"mime/multipart\""]);
            setup.push_str("\tbody := &bytes.Buffer{}\n\twriter := multipart.NewWriter(body)\n");
            for part in parts {
                match &part.file {
                    Some(path) => {
                        imports.push("\"os\"");
                        // CreateFormFile always says application/octet-stream.
                        let create = match &part.content_type {
                            Some(content_type) => {
                                imports.push("\"net/textproto\"");
                                format!(
                                    "header := textproto.MIMEHeader{{}}\n\t\theader.Set(\"Content-Disposition\", fmt.Sprintf(`form-data; name=%q; filename=%q`, {}, {}))\n\t\theader.Set(\"Content-Type\", {})\n\t\tpart, _ := writer.CreatePart(header)",
                                    go_string(&part.name),
                                    go_string(&file_name(path)),
                                    go_string(content_type)
                                )
                            }
                            None => format!(
                                "part, _ := writer.CreateFormFile({}, {})",
                                go_string(&part.name),
                                go_string(&file_name(path))
                            ),
                        };
                        let _ = writeln!(
                            setup,
                            "\tif data, err := os.ReadFile({}); err == nil {{\n\t\t{create}\n\t\tpart.Write(data)\n\t}}",
                            go_string(&path_text(path))
                        );
                    }
                    None => {
                        let _ = writeln!(
                            setup,
                            "\twriter.WriteField({}, {})",
                            go_string(&part.name),
                            go_string(&part.value)
                        );
                    }
                }
            }
            setup.push_str("\twriter.Close()\n");
            "body".to_owned()
        }
    };
    imports.sort_unstable();
    imports.dedup();
    let mut out = String::new();
    out.push_str(&setup);
    let _ = writeln!(
        out,
        "\treq, err := http.NewRequest({}, {}, {body_var})\n\tif err != nil {{\n\t\tpanic(err)\n\t}}",
        go_string(&request.method),
        go_string(&request.url)
    );
    for header in interesting(&request.headers) {
        if matches!(request.body, OutgoingBody::Multipart { .. })
            && header.name.eq_ignore_ascii_case("content-type")
        {
            continue;
        }
        let _ = writeln!(
            out,
            "\treq.Header.Add({}, {})",
            go_string(&header.name),
            go_string(&header.value)
        );
    }
    if matches!(request.body, OutgoingBody::Multipart { .. }) {
        out.push_str("\treq.Header.Set(\"Content-Type\", writer.FormDataContentType())\n");
    }
    out.push_str(
        "\n\tres, err := http.DefaultClient.Do(req)\n\tif err != nil {\n\t\tpanic(err)\n\t}\n\tdefer res.Body.Close()\n\n\tdata, _ := io.ReadAll(res.Body)\n\tfmt.Println(res.Status)\n\tfmt.Println(string(data))",
    );
    if !full {
        return dedent(&out, "\t", Literals::Go);
    }
    format!(
        "package main\n\nimport (\n\t{}\n)\n\nfunc main() {{\n{out}\n}}",
        imports.join("\n\t")
    )
}

/// A Rust string literal: Rust's own escaping, as `Debug` writes it.
fn rust_string(text: &str) -> String {
    format!("{text:?}")
}

fn rust(request: &Outgoing, full: bool) -> String {
    let multipart = matches!(request.body, OutgoingBody::Multipart { .. });
    let mut setup = String::new();
    let method = match request.method.as_str() {
        m @ ("GET" | "POST" | "PUT" | "DELETE" | "PATCH" | "HEAD" | "OPTIONS") => {
            format!("reqwest::Method::{m}")
        }
        other => format!("reqwest::Method::from_bytes(b{})?", rust_string(other)),
    };
    let mut chain = format!(
        "    let response = reqwest::Client::new()\n        .request({method}, {})",
        rust_string(&request.url)
    );
    for header in interesting(&request.headers) {
        if multipart && header.name.eq_ignore_ascii_case("content-type") {
            continue;
        }
        let _ = write!(
            chain,
            "\n        .header({}, {})",
            rust_string(&header.name),
            rust_string(&header.value)
        );
    }
    match &request.body {
        OutgoingBody::None => {}
        OutgoingBody::Bytes { .. } => {
            let _ = write!(
                chain,
                "\n        .body({})",
                rust_string(&body_text(&request.body).unwrap_or_default())
            );
        }
        OutgoingBody::File { path } => {
            let _ = writeln!(
                setup,
                "    let body = std::fs::read({})?;",
                rust_string(&path_text(path))
            );
            chain.push_str("\n        .body(body)");
        }
        OutgoingBody::Multipart { parts } => {
            setup.push_str("    let form = reqwest::multipart::Form::new()");
            for part in parts {
                match &part.file {
                    Some(path) => {
                        let mime = part
                            .content_type
                            .as_ref()
                            .map(|t| format!(".mime_str({})?", rust_string(t)))
                            .unwrap_or_default();
                        let _ = write!(
                            setup,
                            "\n        .part({}, reqwest::multipart::Part::bytes(std::fs::read({})?).file_name({}){mime})",
                            rust_string(&part.name),
                            rust_string(&path_text(path)),
                            rust_string(&file_name(path))
                        );
                    }
                    None => {
                        let _ = write!(
                            setup,
                            "\n        .text({}, {})",
                            rust_string(&part.name),
                            rust_string(&part.value)
                        );
                    }
                }
            }
            setup.push_str(";\n");
            chain.push_str("\n        .multipart(form)");
        }
    }
    let mut out = setup;
    out.push_str(&chain);
    out.push_str(
        "\n        .send()\n        .await?;\n\n    println!(\"{}\", response.status());\n    println!(\"{}\", response.text().await?);",
    );
    if !full {
        return dedent(&out, "    ", Literals::None);
    }
    format!(
        "// Cargo.toml: reqwest = {{ version = \"0.13\", features = [\"multipart\"] }}, tokio = {{ version = \"1\", features = [\"full\"] }}\n\n#[tokio::main]\nasync fn main() -> Result<(), Box<dyn std::error::Error>> {{\n{out}\n    Ok(())\n}}"
    )
}

/// A Java string literal. Line terminators must not be written as
/// `\u000a`, which Java turns into a real line break before parsing.
/// Other characters outside ASCII are written as `\u` escapes, because
/// Java 17 and older read source files in the system's encoding.
fn java_string(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() || !c.is_ascii() => {
                let mut units = [0_u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    let _ = write!(out, "\\u{unit:04x}");
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn java(request: &Outgoing, full: bool) -> String {
    let mut imports = String::from(
        "import java.net.URI;\nimport java.net.http.HttpClient;\nimport java.net.http.HttpRequest;\nimport java.net.http.HttpResponse;\n",
    );
    let publisher = match &request.body {
        OutgoingBody::None => "HttpRequest.BodyPublishers.noBody()".to_owned(),
        OutgoingBody::Bytes { .. } => format!(
            "HttpRequest.BodyPublishers.ofString({})",
            java_string(&body_text(&request.body).unwrap_or_default())
        ),
        OutgoingBody::File { path } => {
            imports.push_str("import java.nio.file.Path;\n");
            format!(
                "HttpRequest.BodyPublishers.ofFile(Path.of({}))",
                java_string(&path_text(path))
            )
        }
        // Java's client has no multipart helper: write the form by hand.
        OutgoingBody::Multipart { parts } => {
            imports.push_str("import java.nio.charset.StandardCharsets;\n");
            let chunks = multipart_chunks(parts);
            if chunks.iter().any(|c| matches!(c, Chunk::File(_))) {
                imports.push_str("import java.nio.file.Files;\nimport java.nio.file.Path;\n");
            }
            imports.push_str("import java.util.List;\n");
            let items: Vec<String> = chunks
                .iter()
                .map(|chunk| match chunk {
                    Chunk::Text(text) => {
                        format!("{}.getBytes(StandardCharsets.UTF_8)", java_string(text))
                    }
                    Chunk::File(path) => {
                        format!("Files.readAllBytes(Path.of({}))", java_string(path))
                    }
                })
                .collect();
            format!(
                "HttpRequest.BodyPublishers.ofByteArrays(List.of(\n                {}))",
                items.join(",\n                ")
            )
        }
    };
    let mut out = String::new();
    let _ = write!(
        out,
        "        HttpRequest request = HttpRequest.newBuilder()\n            .uri(URI.create({}))\n            .method({}, {publisher})",
        java_string(&request.url),
        java_string(&request.method)
    );
    for header in interesting(&request.headers) {
        let _ = write!(
            out,
            "\n            .header({}, {})",
            java_string(&header.name),
            java_string(&header.value)
        );
    }
    if is_multipart(request) {
        let _ = write!(
            out,
            "\n            .header(\"Content-Type\", \"multipart/form-data; boundary={BOUNDARY}\")"
        );
    }
    out.push_str(
        "\n            .build();\n\n        HttpResponse<String> response = HttpClient.newHttpClient()\n            .send(request, HttpResponse.BodyHandlers.ofString());\n\n        System.out.println(response.statusCode());\n        System.out.println(response.body());",
    );
    if !full {
        return dedent(
            &dedent(&out, "    ", Literals::None),
            "    ",
            Literals::None,
        );
    }
    format!(
        "{imports}\npublic class Main {{\n    public static void main(String[] args) throws Exception {{\n{out}\n    }}\n}}"
    )
}

/// Headers as the lines of a JavaScript object literal.
fn js_headers(headers: &[Header], indent: &str) -> String {
    headers
        .iter()
        .map(|h| format!("{indent}{}: {},\n", js(&h.name), js(&h.value)))
        .collect()
}

/// A `FormData` built in Node.js, with files read from disk.
fn node_form(parts: &[Part]) -> String {
    let mut out = String::from("const form = new FormData();\n");
    for part in parts {
        match &part.file {
            Some(path) => {
                let _ = writeln!(
                    out,
                    "form.append({}, await openAsBlob({}{}), {});",
                    js(&part.name),
                    js(&path_text(path)),
                    part.content_type
                        .as_ref()
                        .map(|t| format!(", {{ type: {} }}", js(t)))
                        .unwrap_or_default(),
                    js(&file_name(path))
                );
            }
            None => {
                let _ = writeln!(out, "form.append({}, {});", js(&part.name), js(&part.value));
            }
        }
    }
    out
}

fn axios(request: &Outgoing, full: bool) -> String {
    let mut imports = vec!["import axios from \"axios\";".to_owned()];
    let mut setup = String::new();
    let mut options = format!(
        "  method: {},\n  url: {},\n",
        js(&request.method.to_ascii_lowercase()),
        js(&request.url)
    );
    let headers = merged_headers(request);
    if !headers.is_empty() {
        let _ = write!(
            options,
            "  headers: {{\n{}  }},\n",
            js_headers(&headers, "    ")
        );
    }
    match &request.body {
        OutgoingBody::None => {}
        OutgoingBody::Bytes { .. } => match json_body(request) {
            Some(value) => {
                let pretty = serde_json::to_string_pretty(&value).unwrap_or_default();
                let _ = writeln!(options, "  data: {},", pretty.replace('\n', "\n  "));
            }
            None => {
                let _ = writeln!(
                    options,
                    "  data: {},",
                    js(&body_text(&request.body).unwrap_or_default())
                );
            }
        },
        OutgoingBody::File { path } => {
            imports.push("import { readFile } from \"node:fs/promises\";".into());
            let _ = writeln!(options, "  data: await readFile({}),", js(&path_text(path)));
        }
        OutgoingBody::Multipart { parts } => {
            if parts.iter().any(|p| p.file.is_some()) {
                imports.push("import { openAsBlob } from \"node:fs\";".into());
            }
            setup = node_form(parts);
            setup.push('\n');
            options.push_str("  data: form,\n");
        }
    }
    // Answer with the response whatever its status, as the other snippets do.
    options.push_str("  validateStatus: () => true,\n");
    let code = format!(
        "{setup}const response = await axios.request({{\n{options}}});\n\nconsole.log(response.status);\nconsole.log(response.data);"
    );
    if full {
        format!("{}\n\n{code}", imports.join("\n"))
    } else {
        code
    }
}

fn node(request: &Outgoing, full: bool) -> String {
    let module = if request.url.starts_with("http://") {
        "http"
    } else {
        "https"
    };
    let mut imports = vec![format!("import {module} from \"node:{module}\";")];
    let headers = merged_headers(request);
    let mut out = format!("const headers = {{\n{}}};\n", js_headers(&headers, "  "));
    let mut has_body = true;
    match &request.body {
        OutgoingBody::None => has_body = false,
        OutgoingBody::Bytes { .. } => {
            let _ = writeln!(
                out,
                "const body = {};",
                js(&body_text(&request.body).unwrap_or_default())
            );
        }
        OutgoingBody::File { path } => {
            imports.push("import { readFile } from \"node:fs/promises\";".into());
            let _ = writeln!(
                out,
                "const body = await readFile({});",
                js(&path_text(path))
            );
        }
        OutgoingBody::Multipart { parts } => {
            if parts.iter().any(|p| p.file.is_some()) {
                imports.push("import { openAsBlob } from \"node:fs\";".into());
            }
            out.push_str(&node_form(parts));
            out.push_str(
                "// Encode the form the way fetch would, boundary included.\nconst encoded = new Response(form);\nheaders[\"Content-Type\"] = encoded.headers.get(\"content-type\");\nconst body = Buffer.from(await encoded.arrayBuffer());\n",
            );
        }
    }
    if has_body {
        out.push_str("headers[\"Content-Length\"] = Buffer.byteLength(body);\n");
    }
    let _ = write!(
        out,
        "\nconst request = {module}.request({}, {{ method: {}, headers }}, (response) => {{\n  let data = \"\";\n  response.setEncoding(\"utf8\");\n  response.on(\"data\", (chunk) => (data += chunk));\n  response.on(\"end\", () => {{\n    console.log(response.statusCode);\n    console.log(data);\n  }});\n}});\nrequest.on(\"error\", (error) => console.error(error));\n",
        js(&request.url),
        js(&request.method)
    );
    if has_body {
        out.push_str("request.write(body);\n");
    }
    out.push_str("request.end();");
    if full {
        format!("{}\n\n{out}", imports.join("\n"))
    } else {
        out
    }
}

/// Writes `\uXXXX` for the characters a C-like string literal cannot hold
/// as they are.
fn escape_c_like(text: &str, out: &mut String, extra: &[(char, &str)], unicode: fn(u32) -> String) {
    for c in text.chars() {
        if let Some((_, escaped)) = extra.iter().find(|(e, _)| *e == c) {
            out.push_str(escaped);
            continue;
        }
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() || matches!(c, '\u{2028}' | '\u{2029}' | '\u{feff}') => {
                out.push_str(&unicode(c as u32));
            }
            c => out.push(c),
        }
    }
}

/// A Kotlin string literal: `$` starts a template, so it is escaped too.
fn kotlin_string(text: &str) -> String {
    let mut out = String::from("\"");
    escape_c_like(text, &mut out, &[('"', "\\\""), ('$', "\\$")], |c| {
        format!("\\u{c:04x}")
    });
    out.push('"');
    out
}

fn kotlin(request: &Outgoing, full: bool) -> String {
    let mut imports = vec![
        "okhttp3.MediaType.Companion.toMediaTypeOrNull",
        "okhttp3.OkHttpClient",
        "okhttp3.Request",
        "okhttp3.RequestBody.Companion.toRequestBody",
    ];
    let media = |t: Option<&str>| match t {
        Some(t) => format!("{}.toMediaTypeOrNull()", kotlin_string(t)),
        None => "null".to_owned(),
    };
    let mut note = "";
    let body = match &request.body {
        OutgoingBody::None
            if matches!(
                request.method.as_str(),
                "POST" | "PUT" | "PATCH" | "PROPPATCH" | "REPORT"
            ) =>
        {
            // OkHttp needs a body, even an empty one, for these methods.
            "ByteArray(0).toRequestBody()".to_owned()
        }
        _ if matches!(request.method.as_str(), "GET" | "HEAD")
            && !matches!(request.body, OutgoingBody::None) =>
        {
            note = "    // OkHttp sends no body with GET or HEAD.\n";
            "null".to_owned()
        }
        OutgoingBody::None => "null".to_owned(),
        // The bytes, so OkHttp adds no charset to the content type.
        OutgoingBody::Bytes { .. } => format!(
            "{}.toByteArray().toRequestBody({})",
            kotlin_string(&body_text(&request.body).unwrap_or_default()),
            media(content_type(request))
        ),
        OutgoingBody::File { path } => {
            imports.extend([
                "okhttp3.RequestBody.Companion.asRequestBody",
                "java.io.File",
            ]);
            format!(
                "File({}).asRequestBody({})",
                kotlin_string(&path_text(path)),
                media(content_type(request))
            )
        }
        OutgoingBody::Multipart { parts } => {
            imports.push("okhttp3.MultipartBody");
            let mut builder =
                String::from("MultipartBody.Builder()\n        .setType(MultipartBody.FORM)");
            for part in parts {
                match &part.file {
                    Some(path) => {
                        if !imports.contains(&"java.io.File") {
                            imports.extend([
                                "okhttp3.RequestBody.Companion.asRequestBody",
                                "java.io.File",
                            ]);
                        }
                        let _ = write!(
                            builder,
                            "\n        .addFormDataPart({}, {}, File({}).asRequestBody({}))",
                            kotlin_string(&part.name),
                            kotlin_string(&file_name(path)),
                            kotlin_string(&path_text(path)),
                            media(part.content_type.as_deref())
                        );
                    }
                    None => {
                        let _ = write!(
                            builder,
                            "\n        .addFormDataPart({}, {})",
                            kotlin_string(&part.name),
                            kotlin_string(&part.value)
                        );
                    }
                }
            }
            builder.push_str("\n        .build()");
            builder
        }
    };
    imports.sort_unstable();
    imports.dedup();
    let imports: String = imports.iter().map(|i| format!("import {i}\n")).collect();
    let mut out = String::new();
    let _ = write!(
        out,
        "    val client = OkHttpClient()\n{note}    val body = {body}\n    val request = Request.Builder()\n        .url({})\n        .method({}, body)",
        kotlin_string(&request.url),
        kotlin_string(&request.method)
    );
    for header in interesting(&request.headers) {
        // The body's media type is the Content-Type.
        if header.name.eq_ignore_ascii_case("content-type") && body != "null" {
            continue;
        }
        let _ = write!(
            out,
            "\n        .addHeader({}, {})",
            kotlin_string(&header.name),
            kotlin_string(&header.value)
        );
    }
    out.push_str(
        "\n        .build()\n\n    client.newCall(request).execute().use { response ->\n        println(response.code)\n        println(response.body?.string())\n    }",
    );
    if !full {
        return dedent(&out, "    ", Literals::None);
    }
    format!("{imports}\nfun main() {{\n{out}\n}}")
}

/// A C# string literal. C# also ends a line at U+0085, U+2028 and U+2029,
/// which are escaped.
fn csharp_string(text: &str) -> String {
    let mut out = String::from("\"");
    escape_c_like(
        text,
        &mut out,
        &[('"', "\\\""), ('\u{85}', "\\u0085")],
        |c| format!("\\u{c:04x}"),
    );
    out.push('"');
    out
}

fn csharp(request: &Outgoing, full: bool) -> String {
    let mut out = String::new();
    if full {
        out.push_str("using System.Text;\n\n");
    }
    out.push_str("using var client = new HttpClient();\n");
    let _ = writeln!(
        out,
        "using var request = new HttpRequestMessage(new HttpMethod({}), {});",
        csharp_string(&request.method),
        csharp_string(&request.url)
    );
    let has_content = !matches!(request.body, OutgoingBody::None);
    match &request.body {
        OutgoingBody::None => {}
        OutgoingBody::Bytes { .. } => {
            let _ = writeln!(
                out,
                "request.Content = new ByteArrayContent(Encoding.UTF8.GetBytes({}));",
                csharp_string(&body_text(&request.body).unwrap_or_default())
            );
        }
        OutgoingBody::File { path } => {
            let _ = writeln!(
                out,
                "request.Content = new ByteArrayContent(File.ReadAllBytes({}));",
                csharp_string(&path_text(path))
            );
        }
        OutgoingBody::Multipart { parts } => {
            out.push_str("var form = new MultipartFormDataContent();\n");
            for (index, part) in parts.iter().enumerate() {
                let content = match &part.file {
                    Some(path) => format!(
                        "new ByteArrayContent(File.ReadAllBytes({}))",
                        csharp_string(&path_text(path))
                    ),
                    None => format!("new StringContent({})", csharp_string(&part.value)),
                };
                let name = format!("part{}", index + 1);
                let _ = writeln!(out, "var {name} = {content};");
                if let Some(content_type) = &part.content_type {
                    let _ = writeln!(
                        out,
                        "{name}.Headers.Remove(\"Content-Type\");\n{name}.Headers.TryAddWithoutValidation(\"Content-Type\", {});",
                        csharp_string(content_type)
                    );
                }
                match &part.file {
                    Some(path) => {
                        let _ = writeln!(
                            out,
                            "form.Add({name}, {}, {});",
                            csharp_string(&part.name),
                            csharp_string(&file_name(path))
                        );
                    }
                    None => {
                        let _ = writeln!(out, "form.Add({name}, {});", csharp_string(&part.name));
                    }
                }
            }
            out.push_str("request.Content = form;\n");
        }
    }
    for header in interesting(&request.headers) {
        // Content headers belong to the content.
        let target = if header.name.to_ascii_lowercase().starts_with("content-") {
            if !has_content || is_multipart(request) {
                continue;
            }
            "request.Content.Headers"
        } else {
            "request.Headers"
        };
        let _ = writeln!(
            out,
            "{target}.TryAddWithoutValidation({}, {});",
            csharp_string(&header.name),
            csharp_string(&header.value)
        );
    }
    out.push_str(
        "\nusing var response = await client.SendAsync(request);\nConsole.WriteLine((int)response.StatusCode);\nConsole.WriteLine(await response.Content.ReadAsStringAsync());",
    );
    out
}

/// A Swift string literal; escaping `\` also stops `\(` interpolation.
fn swift_string(text: &str) -> String {
    let mut out = String::from("\"");
    escape_c_like(text, &mut out, &[('"', "\\\""), ('\0', "\\0")], |c| {
        format!("\\u{{{c:x}}}")
    });
    out.push('"');
    out
}

fn swift(request: &Outgoing, full: bool) -> String {
    let mut out = String::new();
    if full {
        out.push_str(
            "import Foundation\n#if canImport(FoundationNetworking)\nimport FoundationNetworking\n#endif\n\n",
        );
    }
    let _ = writeln!(
        out,
        "var request = URLRequest(url: URL(string: {})!)\nrequest.httpMethod = {}",
        swift_string(&request.url),
        swift_string(&request.method)
    );
    for header in interesting(&request.headers) {
        if is_multipart(request) && header.name.eq_ignore_ascii_case("content-type") {
            continue;
        }
        let _ = writeln!(
            out,
            "request.addValue({}, forHTTPHeaderField: {})",
            swift_string(&header.value),
            swift_string(&header.name)
        );
    }
    match &request.body {
        OutgoingBody::None => {}
        OutgoingBody::Bytes { .. } => {
            let _ = writeln!(
                out,
                "request.httpBody = Data({}.utf8)",
                swift_string(&body_text(&request.body).unwrap_or_default())
            );
        }
        OutgoingBody::File { path } => {
            let _ = writeln!(
                out,
                "request.httpBody = try Data(contentsOf: URL(fileURLWithPath: {}))",
                swift_string(&path_text(path))
            );
        }
        OutgoingBody::Multipart { parts } => {
            let _ = writeln!(
                out,
                "request.setValue(\"multipart/form-data; boundary={BOUNDARY}\", forHTTPHeaderField: \"Content-Type\")\nvar body = Data()"
            );
            for chunk in multipart_chunks(parts) {
                match chunk {
                    Chunk::Text(text) => {
                        let _ = writeln!(out, "body.append(Data({}.utf8))", swift_string(&text));
                    }
                    Chunk::File(path) => {
                        let _ = writeln!(
                            out,
                            "body.append(try Data(contentsOf: URL(fileURLWithPath: {})))",
                            swift_string(&path)
                        );
                    }
                }
            }
            out.push_str("request.httpBody = body\n");
        }
    }
    out.push_str(
        "\nlet (data, response) = try await URLSession.shared.data(for: request)\nprint((response as! HTTPURLResponse).statusCode)\nprint(String(decoding: data, as: UTF8.self))",
    );
    out
}

/// A PHP single-quoted string: only `\` and `'` are special, and `$` is
/// not read.
fn php_string(text: &str) -> String {
    format!("'{}'", text.replace('\\', "\\\\").replace('\'', "\\'"))
}

fn php(request: &Outgoing) -> String {
    let mut options = vec![format!("    CURLOPT_URL => {},", php_string(&request.url))];
    match request.method.as_str() {
        "GET" => {}
        // Without NOBODY, cURL would wait for a body that never comes.
        "HEAD" => options.push("    CURLOPT_NOBODY => true,".into()),
        method => options.push(format!(
            "    CURLOPT_CUSTOMREQUEST => {},",
            php_string(method)
        )),
    }
    let headers: Vec<String> = interesting(&request.headers)
        .filter(|h| !(is_multipart(request) && h.name.eq_ignore_ascii_case("content-type")))
        .map(|h| {
            format!(
                "        {},",
                php_string(&format!("{}: {}", h.name, h.value))
            )
        })
        .collect();
    if !headers.is_empty() {
        options.push(format!(
            "    CURLOPT_HTTPHEADER => [\n{}\n    ],",
            headers.join("\n")
        ));
    }
    match &request.body {
        OutgoingBody::None => {}
        OutgoingBody::Bytes { .. } => options.push(format!(
            "    CURLOPT_POSTFIELDS => {},",
            php_string(&body_text(&request.body).unwrap_or_default())
        )),
        OutgoingBody::File { path } => options.push(format!(
            "    CURLOPT_POSTFIELDS => file_get_contents({}),",
            php_string(&path_text(path))
        )),
        OutgoingBody::Multipart { parts } => {
            // An array holds each field name once.
            let fields: Vec<String> = parts
                .iter()
                .map(|part| {
                    let value = match &part.file {
                        Some(path) => format!(
                            "new CURLFile({}, {}, {})",
                            php_string(&path_text(path)),
                            php_string(part.content_type.as_deref().unwrap_or_default()),
                            php_string(&file_name(path))
                        ),
                        None => php_string(&part.value),
                    };
                    format!("        {} => {value},", php_string(&part.name))
                })
                .collect();
            options.push(format!(
                "    CURLOPT_POSTFIELDS => [\n{}\n    ],",
                fields.join("\n")
            ));
        }
    }
    options.push("    CURLOPT_RETURNTRANSFER => true,".into());
    format!(
        "<?php\n\n$curl = curl_init();\ncurl_setopt_array($curl, [\n{}\n]);\n$response = curl_exec($curl);\n$status = curl_getinfo($curl, CURLINFO_RESPONSE_CODE);\ncurl_close($curl);\n\necho $status, \"\\n\", $response, \"\\n\";",
        options.join("\n")
    )
}

/// A Ruby single-quoted string: only `\` and `'` are special, and `#{` is
/// not read.
fn ruby_string(text: &str) -> String {
    format!("'{}'", text.replace('\\', "\\\\").replace('\'', "\\'"))
}

fn ruby(request: &Outgoing, full: bool) -> String {
    let class = match request.method.as_str() {
        "GET" => Some("Get"),
        "POST" => Some("Post"),
        "PUT" => Some("Put"),
        "DELETE" => Some("Delete"),
        "PATCH" => Some("Patch"),
        "HEAD" => Some("Head"),
        "OPTIONS" => Some("Options"),
        "TRACE" => Some("Trace"),
        _ => None,
    };
    let mut out = String::new();
    if full {
        out.push_str("require \"net/http\"\nrequire \"uri\"\n\n");
    }
    let _ = writeln!(out, "uri = URI({})", ruby_string(&request.url));
    match class {
        Some(class) => {
            let _ = writeln!(out, "request = Net::HTTP::{class}.new(uri)");
        }
        None => {
            let _ = writeln!(
                out,
                "request = Net::HTTPGenericRequest.new({}, true, true, uri)",
                ruby_string(&request.method)
            );
        }
    }
    let mut seen: Vec<String> = Vec::new();
    for header in interesting(&request.headers) {
        if is_multipart(request) && header.name.eq_ignore_ascii_case("content-type") {
            continue;
        }
        // `[]=` replaces Ruby's own default; later ones add to it.
        let lower = header.name.to_ascii_lowercase();
        if seen.contains(&lower) {
            let _ = writeln!(
                out,
                "request.add_field({}, {})",
                ruby_string(&header.name),
                ruby_string(&header.value)
            );
        } else {
            let _ = writeln!(
                out,
                "request[{}] = {}",
                ruby_string(&header.name),
                ruby_string(&header.value)
            );
            seen.push(lower);
        }
    }
    match &request.body {
        OutgoingBody::None => {}
        OutgoingBody::Bytes { .. } => {
            let _ = writeln!(
                out,
                "request.body = {}",
                ruby_string(&body_text(&request.body).unwrap_or_default())
            );
        }
        OutgoingBody::File { path } => {
            let _ = writeln!(
                out,
                "request.body = File.binread({})",
                ruby_string(&path_text(path))
            );
        }
        OutgoingBody::Multipart { parts } => {
            let fields: Vec<String> = parts
                .iter()
                .map(|part| match &part.file {
                    Some(path) => format!(
                        "  [{}, File.open({}), {{ filename: {}{} }}],",
                        ruby_string(&part.name),
                        ruby_string(&path_text(path)),
                        ruby_string(&file_name(path)),
                        part.content_type
                            .as_ref()
                            .map(|t| format!(", content_type: {}", ruby_string(t)))
                            .unwrap_or_default()
                    ),
                    None => format!(
                        "  [{}, {}],",
                        ruby_string(&part.name),
                        ruby_string(&part.value)
                    ),
                })
                .collect();
            let _ = writeln!(
                out,
                "request.set_form([\n{}\n], \"multipart/form-data\")",
                fields.join("\n")
            );
        }
    }
    out.push_str(
        "\nresponse = Net::HTTP.start(uri.hostname, uri.port, use_ssl: uri.scheme == \"https\") do |http|\n  http.request(request)\nend\n\nputs response.code\nputs response.body",
    );
    out
}

/// A Dart string literal in single quotes: `$` starts interpolation, so
/// it is escaped too.
fn dart_string(text: &str) -> String {
    let mut out = String::from("'");
    escape_c_like(text, &mut out, &[('\'', "\\'"), ('$', "\\$")], |c| {
        format!("\\u{{{c:x}}}")
    });
    out.push('\'');
    out
}

fn dart(request: &Outgoing, full: bool) -> String {
    let mut imports = vec!["dart:convert", "package:http/http.dart"];
    let mut setup = String::new();
    let headers = merged_headers(request);
    let constructor = if is_multipart(request) {
        "http.MultipartRequest"
    } else {
        "http.Request"
    };
    let _ = writeln!(
        setup,
        "  final request = {constructor}({}, Uri.parse({}));",
        dart_string(&request.method),
        dart_string(&request.url)
    );
    if !headers.is_empty() {
        setup.push_str("  request.headers.addAll({\n");
        for header in &headers {
            let _ = writeln!(
                setup,
                "    {}: {},",
                dart_string(&header.name),
                dart_string(&header.value)
            );
        }
        setup.push_str("  });\n");
    }
    match &request.body {
        OutgoingBody::None => {}
        // Bytes, so the client adds no charset to the content type.
        OutgoingBody::Bytes { .. } => {
            let _ = writeln!(
                setup,
                "  request.bodyBytes = utf8.encode({});",
                dart_string(&body_text(&request.body).unwrap_or_default())
            );
        }
        OutgoingBody::File { path } => {
            imports.push("dart:io");
            let _ = writeln!(
                setup,
                "  request.bodyBytes = await File({}).readAsBytes();",
                dart_string(&path_text(path))
            );
        }
        OutgoingBody::Multipart { parts } => {
            for part in parts {
                match &part.file {
                    Some(path) => {
                        let content_type = match &part.content_type {
                            Some(t) => {
                                imports.push("package:http_parser/http_parser.dart");
                                format!(", contentType: MediaType.parse({})", dart_string(t))
                            }
                            None => String::new(),
                        };
                        let _ = writeln!(
                            setup,
                            "  request.files.add(await http.MultipartFile.fromPath({}, {}, filename: {}{content_type}));",
                            dart_string(&part.name),
                            dart_string(&path_text(path)),
                            dart_string(&file_name(path))
                        );
                    }
                    // A map holds each field name once.
                    None => {
                        let _ = writeln!(
                            setup,
                            "  request.fields[{}] = {};",
                            dart_string(&part.name),
                            dart_string(&part.value)
                        );
                    }
                }
            }
        }
    }
    imports.sort_unstable();
    imports.dedup();
    let imports: String = imports
        .iter()
        .map(|i| {
            if *i == "package:http/http.dart" {
                format!("import '{i}' as http;\n")
            } else {
                format!("import '{i}';\n")
            }
        })
        .collect();
    let code = format!(
        "{setup}\n  final response = await http.Response.fromStream(await request.send());\n  print(response.statusCode);\n  print(utf8.decode(response.bodyBytes));"
    );
    if !full {
        return dedent(&code, "  ", Literals::None);
    }
    format!("{imports}\nFuture<void> main() async {{\n{code}\n}}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::Payload;

    /// Quotes, a backslash, `$` and other template starters, a line break,
    /// non-ASCII letters, typographic quotes, a line separator and a
    /// control character.
    const NASTY: &str = "it's \"q\" \\ $HOME #{x} ${y}\nZoë ‘’ \u{2028}\u{1}";

    fn post_json() -> Outgoing {
        Outgoing {
            method: "POST".into(),
            url: "https://api.example.com/users?q=it's".into(),
            headers: vec![
                Header::new("Content-Type", "application/json"),
                Header::new("Authorization", "Bearer t0k\"en"),
                Header::new("User-Agent", crate::http::USER_AGENT),
                Header::new("Accept", "*/*"),
            ],
            body: OutgoingBody::Bytes {
                data: Payload(b"{\"name\":\"O'Brien\",\"note\":\"line\\nbreak\"}".to_vec()),
            },
        }
    }

    fn nasty() -> Outgoing {
        Outgoing {
            method: "POST".into(),
            url: "https://api.example.com/notes?q=Zo%C3%AB".into(),
            headers: vec![
                Header::new("Content-Type", "text/plain"),
                Header::new("X-Note", NASTY),
            ],
            body: OutgoingBody::Bytes {
                data: Payload(NASTY.as_bytes().to_vec()),
            },
        }
    }

    fn upload() -> Outgoing {
        Outgoing {
            method: "POST".into(),
            url: "https://x.test/upload".into(),
            headers: vec![],
            body: OutgoingBody::Multipart {
                parts: vec![
                    Part {
                        name: "avatar".into(),
                        value: String::new(),
                        file: Some("photos/me.png".into()),
                        content_type: Some("image/png".into()),
                    },
                    Part {
                        name: "name".into(),
                        value: "@Ada".into(),
                        file: None,
                        content_type: None,
                    },
                ],
            },
        }
    }

    #[test]
    fn every_language_generates_code() {
        for request in [post_json(), nasty(), upload()] {
            for language in LANGUAGES {
                let code = generate(language.id, &request).unwrap();
                assert!(
                    code.contains("api.example.com") || code.contains("x.test"),
                    "{}: {code}",
                    language.id
                );
                assert!(!code.contains("getman/"), "{}: {code}", language.id);
            }
        }
        assert_eq!(generate("cobol", &post_json()), None);
    }

    #[test]
    fn string_literals_escape_per_language() {
        assert_eq!(
            sh(NASTY),
            "'it'\\''s \"q\" \\ $HOME #{x} ${y}\nZoë ‘’ \u{2028}\u{1}'"
        );
        assert_eq!(
            ps(NASTY),
            "'it''s \"q\" \\ $HOME #{x} ${y}\nZoë ‘‘’’ \u{2028}\u{1}'"
        );
        assert_eq!(
            js(NASTY),
            "\"it's \\\"q\\\" \\\\ $HOME #{x} ${y}\\nZoë ‘’ \u{2028}\\u0001\""
        );
        assert_eq!(
            go_string(NASTY),
            "\"it's \\\"q\\\" \\\\ $HOME #{x} ${y}\\nZoë ‘’ \u{2028}\\u0001\""
        );
        assert_eq!(
            rust_string(NASTY),
            "\"it's \\\"q\\\" \\\\ $HOME #{x} ${y}\\nZoë ‘’ \\u{2028}\\u{1}\""
        );
        assert_eq!(
            java_string(NASTY),
            "\"it's \\\"q\\\" \\\\ $HOME #{x} ${y}\\nZo\\u00eb \\u2018\\u2019 \\u2028\\u0001\""
        );
        assert_eq!(java_string("🎉"), "\"\\ud83c\\udf89\"");
        assert_eq!(
            kotlin_string(NASTY),
            "\"it's \\\"q\\\" \\\\ \\$HOME #{x} \\${y}\\nZoë ‘’ \\u2028\\u0001\""
        );
        assert_eq!(
            csharp_string(NASTY),
            "\"it's \\\"q\\\" \\\\ $HOME #{x} ${y}\\nZoë ‘’ \\u2028\\u0001\""
        );
        assert_eq!(
            swift_string(NASTY),
            "\"it's \\\"q\\\" \\\\ $HOME #{x} ${y}\\nZoë ‘’ \\u{2028}\\u{1}\""
        );
        assert_eq!(
            php_string(NASTY),
            "'it\\'s \"q\" \\\\ $HOME #{x} ${y}\nZoë ‘’ \u{2028}\u{1}'"
        );
        assert_eq!(
            ruby_string(NASTY),
            "'it\\'s \"q\" \\\\ $HOME #{x} ${y}\nZoë ‘’ \u{2028}\u{1}'"
        );
        assert_eq!(
            dart_string(NASTY),
            "'it\\'s \"q\" \\\\ \\$HOME #{x} \\${y}\\nZoë ‘’ \\u{2028}\\u{1}'"
        );
        assert_eq!(csharp_string("\u{85}\0"), "\"\\u0085\\u0000\"");
        assert_eq!(swift_string("\0\r\t"), "\"\\0\\r\\t\"");
        assert_eq!(go_string("plain"), "\"plain\"");
        assert_eq!(go_string("has \"quotes\""), "`has \"quotes\"`");
        assert_eq!(go_string("tick ` and \"q\""), "\"tick ` and \\\"q\\\"\"");
        assert_eq!(sh("simple/path"), "simple/path");
        assert_eq!(sh("a b"), "'a b'");
    }

    #[test]
    fn every_language_quotes_tricky_values() {
        let request = nasty();
        let header = format!("X-Note: {NASTY}");
        let expected: &[(&str, Vec<String>)] = &[
            (
                "curl",
                vec![sh(&header), format!("--data-raw {}", sh(NASTY))],
            ),
            (
                "httpie",
                vec![
                    sh(&format!("X-Note:{NASTY}")),
                    format!("--raw {}", sh(NASTY)),
                ],
            ),
            (
                "wget",
                vec![
                    format!("--header={}", sh(&header)),
                    format!("--body-data={}", sh(NASTY)),
                ],
            ),
            (
                "powershell",
                vec![
                    format!("'X-Note' = {}", ps(NASTY)),
                    format!("[System.Text.Encoding]::UTF8.GetBytes({})", ps(NASTY)),
                ],
            ),
            ("http", vec![header.clone(), format!("\n\n{NASTY}")]),
            ("fetch", vec![js(NASTY), format!("body: {}", js(NASTY))]),
            ("axios", vec![js(NASTY), format!("data: {}", js(NASTY))]),
            (
                "node",
                vec![js(NASTY), format!("const body = {};", js(NASTY))],
            ),
            ("python", vec![js(NASTY), format!("data = {}", js(NASTY))]),
            (
                "go",
                vec![
                    go_string(NASTY),
                    format!("strings.NewReader({})", go_string(NASTY)),
                ],
            ),
            (
                "rust",
                vec![rust_string(NASTY), format!(".body({})", rust_string(NASTY))],
            ),
            (
                "java",
                vec![
                    java_string(NASTY),
                    format!("ofString({})", java_string(NASTY)),
                ],
            ),
            (
                "kotlin",
                vec![
                    kotlin_string(NASTY),
                    format!("{}.toByteArray()", kotlin_string(NASTY)),
                ],
            ),
            (
                "csharp",
                vec![
                    csharp_string(NASTY),
                    format!("GetBytes({})", csharp_string(NASTY)),
                ],
            ),
            (
                "swift",
                vec![
                    swift_string(NASTY),
                    format!("Data({}.utf8)", swift_string(NASTY)),
                ],
            ),
            (
                "php",
                vec![
                    php_string(&header),
                    format!("CURLOPT_POSTFIELDS => {}", php_string(NASTY)),
                ],
            ),
            (
                "ruby",
                vec![
                    ruby_string(NASTY),
                    format!("request.body = {}", ruby_string(NASTY)),
                ],
            ),
            (
                "dart",
                vec![
                    dart_string(NASTY),
                    format!("utf8.encode({})", dart_string(NASTY)),
                ],
            ),
        ];
        assert_eq!(expected.len(), LANGUAGES.len());
        for (language, needles) in expected {
            let code = generate(language, &request).unwrap();
            for needle in needles {
                assert!(
                    code.contains(needle.as_str()),
                    "{language}: no {needle:?} in\n{code}"
                );
            }
        }
    }

    #[test]
    fn curl_quotes_for_the_shell() {
        let code = generate("curl", &post_json()).unwrap();
        assert_eq!(
            code,
            "curl -X POST 'https://api.example.com/users?q=it'\\''s' \\\n  -H 'Content-Type: application/json' \\\n  -H 'Authorization: Bearer t0k\"en' \\\n  --data-raw '{\"name\":\"O'\\''Brien\",\"note\":\"line\\nbreak\"}'"
        );
    }

    #[test]
    fn curl_keeps_braces_methods_and_form_values_literal() {
        let mut request = Outgoing {
            method: "GET".into(),
            url: "{{baseUrl}}/items".into(),
            headers: vec![],
            body: OutgoingBody::Bytes {
                data: Payload(b"x".to_vec()),
            },
        };
        let code = generate("curl", &request).unwrap();
        assert!(
            code.starts_with("curl --globoff -X GET '{{baseUrl}}/items'"),
            "{code}"
        );
        request.method = "HEAD".into();
        request.body = OutgoingBody::None;
        assert!(generate("curl", &request).unwrap().contains("--head"));

        let code = generate("curl", &upload()).unwrap();
        assert!(
            code.contains("-F 'avatar=@photos/me.png;type=image/png'"),
            "{code}"
        );
        // A value starting with @ is sent as text, not read from a file.
        assert!(code.contains("--form-string name=@Ada"), "{code}");
    }

    #[test]
    fn curl_round_trips_through_the_importer() {
        for request in [post_json(), nasty()] {
            let code = generate("curl", &request).unwrap();
            let imported = crate::import::curl::parse(&code).unwrap();
            assert_eq!(imported.method, "POST");
            assert_eq!(imported.url, request.url);
            let content = match imported.body {
                crate::model::Body::Json { content } => content,
                crate::model::Body::Text { content, .. } => content,
                other => panic!("{other:?}"),
            };
            if request.url.contains("notes") {
                assert_eq!(content, NASTY);
                assert_eq!(imported.headers[1].value, NASTY);
            } else {
                assert!(content.contains("O'Brien"));
            }
        }
    }

    #[test]
    fn fetch_pretty_prints_json() {
        let code = generate("fetch", &post_json()).unwrap();
        assert!(
            code.contains("body: JSON.stringify({\n    \"name\": \"O'Brien\","),
            "{code}"
        );
        assert!(
            code.contains("\"Authorization\": \"Bearer t0k\\\"en\""),
            "{code}"
        );
    }

    #[test]
    fn raw_http_shows_the_wire() {
        let code = generate("http", &post_json()).unwrap();
        assert_eq!(
            code,
            "POST /users?q=it's HTTP/1.1\nHost: api.example.com\nContent-Type: application/json\nAuthorization: Bearer t0k\"en\nContent-Length: 39\n\n{\"name\":\"O'Brien\",\"note\":\"line\\nbreak\"}"
        );
        let code = generate("http", &upload()).unwrap();
        assert!(code.contains(&format!("boundary={BOUNDARY}")), "{code}");
        assert!(
            code.contains(
                "filename=\"me.png\"\nContent-Type: image/png\n\n<the contents of photos/me.png>"
            ),
            "{code}"
        );
        // The last line break of a body is part of it: Content-Length
        // counts it, so the text must keep it.
        let mut request = post_json();
        request.body = OutgoingBody::Bytes {
            data: Payload(b"one\n\n".to_vec()),
        };
        let code = generate("http", &request).unwrap();
        assert!(code.ends_with("Content-Length: 5\n\none\n\n"), "{code:?}");
    }

    #[test]
    fn maps_join_repeated_headers() {
        let mut request = post_json();
        request.headers.push(Header::new("accept", "text/html"));
        request
            .headers
            .push(Header::new("Accept", "application/json"));
        let code = generate("python", &request).unwrap();
        assert!(
            code.contains("\"accept\": \"text/html, application/json\","),
            "{code}"
        );
        let code = generate("ruby", &request).unwrap();
        assert!(code.contains("request['accept'] = 'text/html'"), "{code}");
        assert!(
            code.contains("request.add_field('Accept', 'application/json')"),
            "{code}"
        );
    }

    #[test]
    fn methods_without_a_shortcut() {
        let mut request = post_json();
        request.method = "PURGE".into();
        assert!(
            generate("powershell", &request)
                .unwrap()
                .contains("-CustomMethod 'PURGE'")
        );
        assert!(
            generate("ruby", &request)
                .unwrap()
                .contains("Net::HTTPGenericRequest.new('PURGE', true, true, uri)")
        );
        assert!(
            generate("php", &request)
                .unwrap()
                .contains("CURLOPT_CUSTOMREQUEST => 'PURGE'")
        );
        request.method = "GET".into();
        request.body = OutgoingBody::None;
        assert!(!generate("php", &request).unwrap().contains("CUSTOMREQUEST"));
        request.method = "POST".into();
        assert!(
            generate("kotlin", &request)
                .unwrap()
                .contains("val body = ByteArray(0).toRequestBody()")
        );
    }

    #[test]
    fn a_prepared_multipart_boundary_stays_out_of_snippets() {
        let mut request = upload();
        request.headers.push(Header {
            name: "Content-Type".into(),
            value: "multipart/form-data; boundary=----getmanprepared".into(),
        });
        for language in LANGUAGES {
            let code = generate(language.id, &request).unwrap();
            assert!(!code.contains("getmanprepared"), "{}: {code}", language.id);
        }
    }

    #[test]
    fn multipart_bodies() {
        let request = upload();
        let python = generate("python", &request).unwrap();
        assert!(
            python.contains(
                "(\"avatar\", (\"me.png\", open(\"photos/me.png\", \"rb\"), \"image/png\")),"
            ),
            "{python}"
        );
        // Fields go in the same list, or requests would URL-encode a form
        // with no file in it.
        assert!(python.contains("(\"name\", (None, \"@Ada\")),"), "{python}");
        assert!(!python.contains("data = {"), "{python}");
        let powershell = generate("powershell", &request).unwrap();
        assert!(powershell.contains("$form = [ordered]@{"), "{powershell}");
        let rust = generate("rust", &request).unwrap();
        assert!(rust.contains(".mime_str(\"image/png\")?"), "{rust}");
        let httpie = generate("httpie", &request).unwrap();
        assert!(
            httpie.contains("'avatar@photos/me.png;type=image/png'"),
            "{httpie}"
        );
        let go = generate("go", &request).unwrap();
        assert!(go.contains("writer.FormDataContentType()"), "{go}");
        // A part with a type needs its own header; CreateFormFile has none.
        assert!(go.contains("\"Content-Type\", \"image/png\""), "{go}");
        assert!(go.contains("\"net/textproto\""), "{go}");
        // With no file, "os" would be an unused import, a compile error.
        let fields_only = Outgoing {
            body: OutgoingBody::Multipart {
                parts: vec![Part {
                    name: "name".into(),
                    value: "Ada".into(),
                    file: None,
                    content_type: None,
                }],
            },
            ..upload()
        };
        let go = generate("go", &fields_only).unwrap();
        assert!(!go.contains("\"os\""), "{go}");
        assert!(!go.contains("textproto"), "{go}");
        let java = generate("java", &request).unwrap();
        assert!(
            java.contains("Files.readAllBytes(Path.of(\"photos/me.png\"))"),
            "{java}"
        );
        assert!(java.contains(&format!("boundary={BOUNDARY}")), "{java}");
        let kotlin = generate("kotlin", &request).unwrap();
        assert!(
            kotlin.contains(".addFormDataPart(\"avatar\", \"me.png\", File(\"photos/me.png\").asRequestBody(\"image/png\".toMediaTypeOrNull()))"),
            "{kotlin}"
        );
        let csharp = generate("csharp", &request).unwrap();
        assert!(
            csharp.contains("form.Add(part1, \"avatar\", \"me.png\");"),
            "{csharp}"
        );
        let php = generate("php", &request).unwrap();
        assert!(
            php.contains("'avatar' => new CURLFile('photos/me.png', 'image/png', 'me.png'),"),
            "{php}"
        );
        let dart = generate("dart", &request).unwrap();
        assert!(dart.contains("http.MultipartRequest('POST'"), "{dart}");
        assert!(
            dart.contains("contentType: MediaType.parse('image/png')"),
            "{dart}"
        );
        let node = generate("node", &request).unwrap();
        assert!(
            node.contains("await openAsBlob(\"photos/me.png\", { type: \"image/png\" })"),
            "{node}"
        );
        let wget = generate("wget", &request).unwrap();
        assert!(
            wget.starts_with("# Wget cannot send a multipart form"),
            "{wget}"
        );
    }

    /// A body whose second line starts with four spaces, which must reach
    /// the server as it is whatever the indent of the code around it.
    fn spaced_body() -> Outgoing {
        Outgoing {
            method: "POST".into(),
            url: "https://api.example.com/notes".into(),
            headers: vec![Header::new("Content-Type", "text/plain")],
            body: OutgoingBody::Bytes {
                data: Payload(b"{\n    \"a\": 1,\n\t\"b\": 2\n}".to_vec()),
            },
        }
    }

    fn with(indent: Option<Indent>, boilerplate: bool) -> Options {
        Options {
            indent,
            boilerplate,
        }
    }

    /// The leading whitespace of the lines that start with some.
    fn indents(code: &str) -> Vec<&str> {
        code.lines()
            .filter(|l| l.starts_with([' ', '\t']))
            .map(|l| &l[..l.len() - l.trim_start().len()])
            .collect()
    }

    #[test]
    fn indent_parses_two_four_and_tab() {
        assert_eq!("2".parse(), Ok(Indent::Two));
        assert_eq!("4".parse(), Ok(Indent::Four));
        assert_eq!("Tab".parse(), Ok(Indent::Tab));
        assert!("3".parse::<Indent>().is_err());
    }

    #[test]
    fn default_options_change_nothing() {
        for language in LANGUAGES {
            for request in [post_json(), upload()] {
                assert_eq!(
                    generate_with(language.id, &request, &Options::default()),
                    generate(language.id, &request),
                    "{}",
                    language.id
                );
            }
        }
    }

    #[test]
    fn indent_applies_to_every_language_that_indents() {
        let flat = |text: &str| {
            text.lines()
                .map(|l| l.trim_start().to_owned())
                .collect::<Vec<_>>()
        };
        for language in LANGUAGES {
            for request in [post_json(), upload()] {
                let plain = generate(language.id, &request).unwrap();
                for (indent, unit) in [
                    (Indent::Two, "  "),
                    (Indent::Four, "    "),
                    (Indent::Tab, "\t"),
                ] {
                    let code =
                        generate_with(language.id, &request, &with(Some(indent), true)).unwrap();
                    // Every indented line uses only the chosen unit.
                    for lead in indents(&code) {
                        assert!(
                            lead.replace(unit, "").is_empty(),
                            "{} {indent:?}: {lead:?}\n{code}",
                            language.id
                        );
                    }
                    // Nothing but the indent changed.
                    assert_eq!(flat(&code), flat(&plain), "{}", language.id);
                }
            }
        }
        // Languages with no indent have nothing to change.
        for id in ["httpie", "http", "csharp", "swift"] {
            assert_eq!(
                generate_with(id, &post_json(), &with(Some(Indent::Tab), true)),
                generate(id, &post_json()),
                "{id}"
            );
        }
    }

    #[test]
    fn indent_keeps_string_literals_exact() {
        let request = spaced_body();
        let body = "{\n    \"a\": 1,\n\t\"b\": 2\n}";
        for language in ["curl", "wget", "powershell", "go", "php", "ruby"] {
            for indent in [Indent::Two, Indent::Four, Indent::Tab] {
                for full in [true, false] {
                    let code =
                        generate_with(language, &request, &with(Some(indent), full)).unwrap();
                    assert!(code.contains(body), "{language} {indent:?} {full}:\n{code}");
                }
            }
        }
    }

    #[test]
    fn indent_rewrites_levels() {
        let write = |language: &str, indent| {
            generate_with(language, &post_json(), &with(Some(indent), true)).unwrap()
        };
        let go = write("go", Indent::Four);
        assert!(go.contains("\n    req, err := http.NewRequest("), "{go}");
        assert!(go.contains("\n        panic(err)\n"), "{go}");
        let python = write("python", Indent::Tab);
        assert!(
            python.contains("\n\t\"Content-Type\": \"application/json\","),
            "{python}"
        );
        let java = write("java", Indent::Two);
        assert!(java.contains("\n  public static void main("), "{java}");
        assert!(java.contains("\n      .uri(URI.create("), "{java}");
        let fetch = write("fetch", Indent::Tab);
        assert!(fetch.contains("\n\tmethod: \"POST\","), "{fetch}");
        assert!(fetch.contains("\n\t\t\"Content-Type\": "), "{fetch}");
        let curl = write("curl", Indent::Four);
        assert!(curl.contains(" \\\n    -H "), "{curl}");
    }

    #[test]
    fn no_boilerplate_leaves_the_request_alone() {
        let request = upload();
        let bare = |language: &str| generate_with(language, &request, &with(None, false)).unwrap();
        let go = bare("go");
        assert!(go.starts_with("body := &bytes.Buffer{}"), "{go}");
        for word in ["package main", "import (", "func main"] {
            assert!(!go.contains(word), "{go}");
        }
        assert!(go.ends_with("fmt.Println(string(data))"), "{go}");
        let rust = bare("rust");
        assert!(
            rust.starts_with("let form = reqwest::multipart::Form::new()"),
            "{rust}"
        );
        assert!(
            !rust.contains("fn main") && !rust.contains("Ok(())"),
            "{rust}"
        );
        let java = bare("java");
        assert!(
            java.starts_with("HttpRequest request = HttpRequest.newBuilder()"),
            "{java}"
        );
        assert!(
            !java.contains("import ") && !java.contains("class Main"),
            "{java}"
        );
        let kotlin = bare("kotlin");
        assert!(
            kotlin.starts_with("val client = OkHttpClient()"),
            "{kotlin}"
        );
        assert!(
            !kotlin.contains("import ") && !kotlin.contains("fun main"),
            "{kotlin}"
        );
        let dart = bare("dart");
        assert!(
            dart.starts_with("final request = http.MultipartRequest("),
            "{dart}"
        );
        assert!(
            !dart.contains("import ") && !dart.contains("main()"),
            "{dart}"
        );
        assert!(bare("python").starts_with("url = "));
        assert!(bare("ruby").starts_with("uri = URI("));
        let csharp = bare("csharp");
        assert!(csharp.starts_with("using var client"), "{csharp}");
        assert!(!csharp.contains("using System.Text"), "{csharp}");
        assert!(bare("swift").starts_with("var request = URLRequest"));
        let node = bare("node");
        assert!(!node.contains("import "), "{node}");
        assert!(node.contains("await openAsBlob("), "{node}");
        let axios = bare("axios");
        assert!(!axios.contains("import "), "{axios}");
        assert!(axios.contains("axios.request("), "{axios}");
        let fetch = generate_with("fetch", &post_json(), &with(None, false)).unwrap();
        assert!(
            fetch.starts_with("const response = await fetch("),
            "{fetch}"
        );
    }

    #[test]
    fn no_boilerplate_changes_only_languages_that_have_some() {
        for id in ["curl", "httpie", "wget", "powershell", "http", "php"] {
            assert_eq!(
                generate_with(id, &post_json(), &with(None, false)),
                generate(id, &post_json()),
                "{id}"
            );
        }
        for id in [
            "go", "rust", "java", "kotlin", "dart", "python", "ruby", "swift", "csharp", "axios",
            "node",
        ] {
            let full = generate(id, &post_json()).unwrap();
            let bare = generate_with(id, &post_json(), &with(None, false)).unwrap();
            assert!(bare.len() < full.len(), "{id}: {bare}");
            assert!(bare.contains("api.example.com"), "{id}: {bare}");
        }
    }

    #[test]
    fn no_boilerplate_dedents_the_body_of_main() {
        let go = generate_with("go", &spaced_body(), &with(Some(Indent::Tab), false)).unwrap();
        // The raw string's own lines stay as they were.
        assert!(go.contains("\n\t\"b\": 2\n}`)"), "{go}");
        assert!(go.contains("\nreq, err := http.NewRequest("), "{go}");
        assert!(go.contains("\n\tpanic(err)\n"), "{go}");
    }
}
