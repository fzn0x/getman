//! Writing a request as code in other languages and tools.

use std::fmt::Write as _;

use serde::Serialize;

use crate::http::{Header, Outgoing, OutgoingBody};

/// A language code can be generated for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Language {
    /// Identifier passed to [`generate`].
    pub id: &'static str,
    /// Name to show.
    pub label: &'static str,
    /// Syntax for highlighting the snippet.
    pub syntax: &'static str,
}

/// Every language [`generate`] supports.
pub const LANGUAGES: &[Language] = &[
    Language {
        id: "curl",
        label: "cURL",
        syntax: "shell",
    },
    Language {
        id: "httpie",
        label: "HTTPie",
        syntax: "shell",
    },
    Language {
        id: "powershell",
        label: "PowerShell",
        syntax: "powershell",
    },
    Language {
        id: "fetch",
        label: "JavaScript (fetch)",
        syntax: "javascript",
    },
    Language {
        id: "python",
        label: "Python (requests)",
        syntax: "python",
    },
    Language {
        id: "go",
        label: "Go (net/http)",
        syntax: "go",
    },
    Language {
        id: "rust",
        label: "Rust (reqwest)",
        syntax: "rust",
    },
    Language {
        id: "java",
        label: "Java (HttpClient)",
        syntax: "java",
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

/// Writes a request as code. Returns `None` for an unknown language.
pub fn generate(language: &str, request: &Outgoing) -> Option<String> {
    Some(match language {
        "curl" => curl(request),
        "httpie" => httpie(request),
        "powershell" => powershell(request),
        "fetch" => fetch(request),
        "python" => python(request),
        "go" => go(request),
        "rust" => rust(request),
        "java" => java(request),
        _ => return None,
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
    let mut lines = vec![format!("curl {}", sh(&request.url))];
    if request.method != "GET" {
        lines[0] = format!("curl -X {} {}", request.method, sh(&request.url));
    }
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
                let value = match &part.file {
                    Some(path) => format!("{}=@{}", part.name, path_text(path)),
                    None => format!("{}={}", part.name, part.value),
                };
                let value = match &part.content_type {
                    Some(t) => format!("{value};type={t}"),
                    None => value,
                };
                lines.push(format!("-F {}", sh(&value)));
            }
        }
    }
    lines.join(" \\\n  ")
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
    words.push(request.method.clone());
    words.push(sh(&request.url));
    for header in interesting(&request.headers) {
        words.push(sh(&format!("{}:{}", header.name, header.value)));
    }
    if let OutgoingBody::Multipart { parts } = &request.body {
        for part in parts {
            words.push(match &part.file {
                Some(path) => sh(&format!("{}@{}", part.name, path_text(path))),
                None => sh(&format!("{}={}", part.name, part.value)),
            });
        }
    }
    let mut command = words.join(" ");
    if let OutgoingBody::File { path } = &request.body {
        command.push_str(&format!(" < {}", sh(&path_text(path))));
    }
    command
}

fn ps(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

fn powershell(request: &Outgoing) -> String {
    let mut out = String::new();
    let headers: Vec<&Header> = interesting(&request.headers)
        .filter(|h| !h.name.eq_ignore_ascii_case("content-type"))
        .collect();
    if !headers.is_empty() {
        out.push_str("$headers = @{\n");
        for header in &headers {
            let _ = writeln!(out, "    {} = {}", ps(&header.name), ps(&header.value));
        }
        out.push_str("}\n");
    }
    let content_type = crate::http::find_header(&request.headers, "content-type");
    let mut args = vec![
        format!("-Method {}", request.method),
        format!("-Uri {}", ps(&request.url)),
    ];
    if !headers.is_empty() {
        args.push("-Headers $headers".into());
    }
    if let Some(content_type) = content_type
        && !matches!(request.body, OutgoingBody::Multipart { .. })
    {
        args.push(format!("-ContentType {}", ps(content_type)));
    }
    match &request.body {
        OutgoingBody::None => {}
        OutgoingBody::Bytes { .. } => {
            let _ = writeln!(
                out,
                "$body = {}",
                ps(&body_text(&request.body).unwrap_or_default())
            );
            args.push("-Body $body".into());
        }
        OutgoingBody::File { path } => args.push(format!("-InFile {}", ps(&path_text(path)))),
        OutgoingBody::Multipart { parts } => {
            out.push_str("$form = @{\n");
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

/// A string literal for JavaScript or Python: JSON's syntax is valid in
/// both.
fn js(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_default()
}

fn fetch(request: &Outgoing) -> String {
    let mut out = String::new();
    let mut options = vec![format!("  method: {}", js(&request.method))];
    let multipart = matches!(request.body, OutgoingBody::Multipart { .. });
    let headers: Vec<&Header> = interesting(&request.headers)
        .filter(|h| !(multipart && h.name.eq_ignore_ascii_case("content-type")))
        .collect();
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
            out.push_str("// In Node.js; in a browser, use a File from an <input>.\n");
            out.push_str("import { readFile } from \"node:fs/promises\";\n\n");
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

fn python(request: &Outgoing) -> String {
    let mut out = String::from("import requests\n\n");
    let multipart = matches!(request.body, OutgoingBody::Multipart { .. });
    let headers: Vec<&Header> = interesting(&request.headers)
        .filter(|h| !(multipart && h.name.eq_ignore_ascii_case("content-type")))
        .collect();
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
            let (files, fields): (Vec<_>, Vec<_>) = parts.iter().partition(|p| p.file.is_some());
            if !fields.is_empty() {
                out.push_str("data = {\n");
                for part in &fields {
                    let _ = writeln!(out, "    {}: {},", js(&part.name), js(&part.value));
                }
                out.push_str("}\n");
                args.push("data=data".into());
            }
            if !files.is_empty() {
                out.push_str("files = {\n");
                for part in &files {
                    let path = part
                        .file
                        .as_deref()
                        .unwrap_or_else(|| std::path::Path::new(""));
                    let _ = writeln!(
                        out,
                        "    {}: ({}, open({}, \"rb\")),",
                        js(&part.name),
                        js(&file_name(path)),
                        js(&path_text(path))
                    );
                }
                out.push_str("}\n");
                args.push("files=files".into());
            }
        }
    }
    let _ = write!(
        out,
        "\nresponse = requests.request({})\n\nprint(response.status_code)\nprint(response.text)",
        args.join(", ")
    );
    out
}

/// A Go string literal: a raw string when possible.
fn go_string(text: &str) -> String {
    if text.contains('`') || text.contains('\r') {
        serde_json::to_string(text).unwrap_or_default()
    } else if text.contains('"') || text.contains('\\') || text.contains('\n') {
        format!("`{text}`")
    } else {
        format!("\"{text}\"")
    }
}

fn go(request: &Outgoing) -> String {
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
            imports.extend(["\"bytes\"", "\"mime/multipart\"", "\"os\""]);
            setup.push_str("\tbody := &bytes.Buffer{}\n\twriter := multipart.NewWriter(body)\n");
            for part in parts {
                match &part.file {
                    Some(path) => {
                        let _ = writeln!(
                            setup,
                            "\tif data, err := os.ReadFile({}); err == nil {{\n\t\tpart, _ := writer.CreateFormFile({}, {})\n\t\tpart.Write(data)\n\t}}",
                            go_string(&path_text(path)),
                            go_string(&part.name),
                            go_string(&file_name(path))
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
    let mut out = format!(
        "package main\n\nimport (\n\t{}\n)\n\nfunc main() {{\n",
        imports.join("\n\t")
    );
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
            "\treq.Header.Set({}, {})",
            go_string(&header.name),
            go_string(&header.value)
        );
    }
    if matches!(request.body, OutgoingBody::Multipart { .. }) {
        out.push_str("\treq.Header.Set(\"Content-Type\", writer.FormDataContentType())\n");
    }
    out.push_str(
        "\n\tres, err := http.DefaultClient.Do(req)\n\tif err != nil {\n\t\tpanic(err)\n\t}\n\tdefer res.Body.Close()\n\n\tdata, _ := io.ReadAll(res.Body)\n\tfmt.Println(res.Status)\n\tfmt.Println(string(data))\n}",
    );
    out
}

/// A Rust string literal: Rust's own escaping, as `Debug` writes it.
fn rust_string(text: &str) -> String {
    format!("{text:?}")
}

fn rust(request: &Outgoing) -> String {
    let multipart = matches!(request.body, OutgoingBody::Multipart { .. });
    let mut out = String::from(
        "// Cargo.toml: reqwest = { version = \"0.13\", features = [\"multipart\"] }, tokio = { version = \"1\", features = [\"full\"] }\n\n#[tokio::main]\nasync fn main() -> Result<(), Box<dyn std::error::Error>> {\n",
    );
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
                        let _ = write!(
                            setup,
                            "\n        .part({}, reqwest::multipart::Part::bytes(std::fs::read({})?).file_name({}))",
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
    out.push_str(&setup);
    out.push_str(&chain);
    out.push_str(
        "\n        .send()\n        .await?;\n\n    println!(\"{}\", response.status());\n    println!(\"{}\", response.text().await?);\n    Ok(())\n}",
    );
    out
}

/// A Java string literal. Line terminators must not be written as
/// `\u000a`, which Java turns into a real line break before parsing.
fn java_string(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn java(request: &Outgoing) -> String {
    let mut out = String::from(
        "import java.net.URI;\nimport java.net.http.HttpClient;\nimport java.net.http.HttpRequest;\nimport java.net.http.HttpResponse;\n",
    );
    let publisher = match &request.body {
        OutgoingBody::None => "HttpRequest.BodyPublishers.noBody()".to_owned(),
        OutgoingBody::Bytes { .. } => format!(
            "HttpRequest.BodyPublishers.ofString({})",
            java_string(&body_text(&request.body).unwrap_or_default())
        ),
        OutgoingBody::File { path } => {
            out.push_str("import java.nio.file.Path;\n");
            format!("HttpRequest.BodyPublishers.ofFile(Path.of({}))", java_string(&path_text(path)))
        }
        OutgoingBody::Multipart { .. } => {
            "HttpRequest.BodyPublishers.noBody() /* Java's HttpClient has no multipart helper; build the body or use a library */"
                .to_owned()
        }
    };
    out.push_str(
        "\npublic class Main {\n    public static void main(String[] args) throws Exception {\n",
    );
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
    out.push_str(
        "\n            .build();\n\n        HttpResponse<String> response = HttpClient.newHttpClient()\n            .send(request, HttpResponse.BodyHandlers.ofString());\n\n        System.out.println(response.statusCode());\n        System.out.println(response.body());\n    }\n}",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{Part, Payload};

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

    #[test]
    fn every_language_generates_code() {
        for language in LANGUAGES {
            let code = generate(language.id, &post_json()).unwrap();
            assert!(code.contains("api.example.com"), "{}: {code}", language.id);
            assert!(!code.contains("getman/"), "{}: {code}", language.id);
        }
        assert_eq!(generate("cobol", &post_json()), None);
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
    fn curl_round_trips_through_the_importer() {
        let code = generate("curl", &post_json()).unwrap();
        let request = crate::import::curl::parse(&code).unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(request.url, "https://api.example.com/users?q=it's");
        let crate::model::Body::Json { content } = request.body else {
            panic!()
        };
        assert!(content.contains("O'Brien"));
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
    fn string_literals_escape_per_language() {
        assert_eq!(rust_string("a\"b\\c\n\u{1}"), r#""a\"b\\c\n\u{1}""#);
        assert_eq!(java_string("a\"b\n\u{1}"), r#""a\"b\n\u0001""#);
        assert_eq!(go_string("plain"), "\"plain\"");
        assert_eq!(go_string("has \"quotes\""), "`has \"quotes\"`");
        assert_eq!(ps("it's"), "'it''s'");
        assert_eq!(sh("simple/path"), "simple/path");
        assert_eq!(sh("a b"), "'a b'");
    }

    #[test]
    fn multipart_bodies() {
        let request = Outgoing {
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
                        value: "Ada".into(),
                        file: None,
                        content_type: None,
                    },
                ],
            },
        };
        let curl = generate("curl", &request).unwrap();
        assert!(
            curl.contains("-F 'avatar=@photos/me.png;type=image/png'"),
            "{curl}"
        );
        assert!(curl.contains("-F name=Ada"), "{curl}");
        let python = generate("python", &request).unwrap();
        assert!(
            python.contains("\"avatar\": (\"me.png\", open(\"photos/me.png\", \"rb\")),"),
            "{python}"
        );
        let go = generate("go", &request).unwrap();
        assert!(go.contains("writer.FormDataContentType()"), "{go}");
    }
}
