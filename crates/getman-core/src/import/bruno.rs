//! Importing Bruno collections: a folder of `.bru` files with
//! `bruno.json`, `collection.bru`, `folder.bru` and `environments/*.bru`.
//!
//! Requests keep their variables, authentication, headers and bodies.
//! Assertions become checks where Getman has the same test, and
//! post-response variables become captures. Scripts are carried over;
//! their `bru.*`, `req.*` and `res.*` calls need rewriting with `gm.*`.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;

use super::{Collection, Import, Item, NewEnvironment, SourceFile, warn_once};
use crate::error::{Error, Result};
use crate::model::{
    ApiKeyLocation, Auth, Body, ClientAuth, Folder, FormField, KeyValue, OAuth2, OAuth2Grant,
    Request, Scripts, Vars,
};

/// Blocks whose content is text rather than `key: value` lines.
const TEXT_BLOCKS: &[&str] = &[
    "body:json",
    "body:text",
    "body:xml",
    "body:sparql",
    "body:graphql",
    "body:graphql:vars",
    "script:pre-request",
    "script:post-response",
    "tests",
    "docs",
];
const METHODS: &[&str] = &[
    "get", "post", "put", "delete", "patch", "options", "head", "connect", "trace",
];

static BLOCK_START: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([A-Za-z][\w:-]*)\s*([{\[])\s*$").expect("block pattern"));
static PROCESS_ENV: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\{\{\s*process\.env\.([A-Za-z_][A-Za-z0-9_]*)\s*\}\}").expect("env pattern")
});

/// Whether text looks like a `.bru` file: it starts with a block such as
/// `meta {` or `get {`.
pub(crate) fn looks_like_bru(text: &str) -> bool {
    let Some(first) = text
        .lines()
        .map(str::trim_end)
        .find(|l| !l.trim().is_empty())
    else {
        return false;
    };
    BLOCK_START.captures(first).is_some_and(|caps| {
        let name = &caps[1];
        name == "meta" || METHODS.contains(&name)
    }) && text.lines().any(|l| l.trim_end() == "}")
}

/// One block of a `.bru` file.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Content {
    /// `key: value` lines; `~` switches a line off.
    Pairs(Vec<Pair>),
    /// Text, such as a JSON body or a script.
    Text(String),
    /// A list, such as `vars:secret [ a, b ]`.
    List(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Pair {
    key: String,
    value: String,
    enabled: bool,
}

#[derive(Debug, Default)]
struct BruFile {
    blocks: Vec<(String, Content)>,
}

impl BruFile {
    fn parse(text: &str) -> Result<Self, String> {
        let lines: Vec<&str> = text.lines().map(|l| l.trim_end_matches('\r')).collect();
        let mut file = Self::default();
        let mut at = 0;
        while at < lines.len() {
            let line = lines[at];
            if line.trim().is_empty() {
                at += 1;
                continue;
            }
            let Some(caps) = BLOCK_START.captures(line.trim_end()) else {
                return Err(format!(
                    "line {}: expected a block such as `get {{`, found {:?}",
                    at + 1,
                    truncate(line)
                ));
            };
            let name = caps[1].to_owned();
            let list = &caps[2] == "[";
            let close = if list { "]" } else { "}" };
            let start = at + 1;
            let Some(end) = lines[start..]
                .iter()
                .position(|l| l.trim_end() == close)
                .map(|offset| start + offset)
            else {
                return Err(format!("the block {name} on line {} is not closed", at + 1));
            };
            let body = &lines[start..end];
            let content = if list {
                Content::List(
                    body.iter()
                        .map(|l| l.trim().trim_end_matches(',').trim().to_owned())
                        .filter(|l| !l.is_empty())
                        .collect(),
                )
            } else if TEXT_BLOCKS.contains(&name.as_str()) {
                Content::Text(dedent(body, 2))
            } else {
                Content::Pairs(pairs(body)?)
            };
            file.blocks.push((name, content));
            at = end + 1;
        }
        Ok(file)
    }

    fn pairs(&self, name: &str) -> &[Pair] {
        self.blocks
            .iter()
            .find_map(|(n, content)| match content {
                Content::Pairs(pairs) if n == name => Some(pairs.as_slice()),
                _ => None,
            })
            .unwrap_or_default()
    }

    fn text(&self, name: &str) -> Option<&str> {
        self.blocks.iter().find_map(|(n, content)| match content {
            Content::Text(text) if n == name => Some(text.as_str()),
            _ => None,
        })
    }

    fn list(&self, name: &str) -> &[String] {
        self.blocks
            .iter()
            .find_map(|(n, content)| match content {
                Content::List(list) if n == name => Some(list.as_slice()),
                _ => None,
            })
            .unwrap_or_default()
    }

    fn has(&self, name: &str) -> bool {
        self.blocks.iter().any(|(n, _)| n == name)
    }

    fn get(&self, block: &str, key: &str) -> Option<&str> {
        self.pairs(block)
            .iter()
            .find(|p| p.key == key && p.enabled)
            .map(|p| p.value.as_str())
    }
}

fn truncate(line: &str) -> String {
    line.chars().take(40).collect()
}

/// Removes up to `width` spaces from the start of every line.
fn dedent(lines: &[&str], width: usize) -> String {
    lines
        .iter()
        .map(|line| {
            let spaces = line.len() - line.trim_start_matches(' ').len();
            &line[spaces.min(width)..]
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn pairs(lines: &[&str]) -> Result<Vec<Pair>, String> {
    let mut pairs = Vec::new();
    let mut at = 0;
    while at < lines.len() {
        let line = lines[at].trim();
        at += 1;
        if line.is_empty() {
            continue;
        }
        let (enabled, line) = match line.strip_prefix('~') {
            Some(rest) => (false, rest),
            None => (true, line),
        };
        let (key, value) = line.split_once(':').unwrap_or((line, ""));
        let key = unquote(key.trim());
        let mut value = value.trim().to_owned();
        // Multi-line values sit between ''' lines.
        if value == "'''" {
            let start = at;
            let Some(end) = lines[start..]
                .iter()
                .position(|l| l.trim() == "'''")
                .map(|offset| start + offset)
            else {
                return Err(format!("the multi-line value of {key} is not closed"));
            };
            value = dedent(&lines[start..end], 4);
            at = end + 1;
        }
        pairs.push(Pair {
            key,
            value,
            enabled,
        });
    }
    Ok(pairs)
}

fn unquote(text: &str) -> String {
    text.strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .map_or_else(|| text.to_owned(), str::to_owned)
}

/// Reads a Bruno collection from the files of its folder, or one `.bru`
/// file as a request.
pub fn parse(files: &[SourceFile]) -> Result<Import> {
    let mut reader = Reader {
        warnings: Vec::new(),
    };
    let bru: Vec<&SourceFile> = files
        .iter()
        .filter(|f| super::has_extension(&f.path, "bru"))
        .collect();
    let config = files.iter().find(|f| f.path == "bruno.json");
    if config.is_none() && bru.len() == 1 && !bru[0].path.contains('/') {
        let file = bru[0];
        let parsed = BruFile::parse(&file.text).map_err(|e| invalid(&file.path, &e))?;
        if parsed.blocks.is_empty() {
            return Err(invalid(&file.path, "the file is empty"));
        }
        let request = reader.request(&parsed, stem(&file.path));
        return Ok(Import::Request {
            request,
            warnings: reader.warnings,
        });
    }

    let mut folder = Folder {
        name: "Bruno collection".into(),
        ..Folder::default()
    };
    if let Some(config) = config {
        let value: serde_json::Value = serde_json::from_str(&config.text)
            .map_err(|e| invalid("bruno.json", &format!("not valid JSON: {e}")))?;
        if let Some(name) = super::text(&value["name"]).filter(|n| !n.trim().is_empty()) {
            folder.name = name;
        }
    }

    // Folders by path, each with its settings and requests.
    let mut tree = Node::default();
    let mut environments = Vec::new();
    for file in bru {
        let path = file.path.trim_start_matches('/');
        let parsed = BruFile::parse(&file.text).map_err(|e| invalid(path, &e))?;
        let (dir, name) = path.rsplit_once('/').unwrap_or(("", path));
        if dir
            .split('/')
            .any(|part| part.starts_with('.') || part == "node_modules")
        {
            continue;
        }
        if dir == "environments" {
            environments.push(reader.environment(&parsed, stem(name)));
            continue;
        }
        let node = tree.at(dir);
        match name {
            "collection.bru" if dir.is_empty() => {
                let settings = reader.folder(&parsed, &folder.name);
                folder.description = settings.description;
                folder.headers = settings.headers;
                folder.auth = settings.auth;
                folder.variables = settings.variables;
                folder.scripts = settings.scripts;
            }
            "folder.bru" => {
                let fallback = dir.rsplit('/').next().unwrap_or(dir).to_owned();
                node.seq = parsed
                    .get("meta", "seq")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(f64::MAX);
                node.settings = Some(reader.folder(&parsed, &fallback));
            }
            _ => {
                let seq = parsed
                    .get("meta", "seq")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(f64::MAX);
                if parsed
                    .blocks
                    .iter()
                    .any(|(n, _)| METHODS.contains(&n.as_str()))
                {
                    let request = reader.request(&parsed, stem(name));
                    node.requests.push((seq, request));
                }
            }
        }
    }
    let items = tree.into_items();
    if items.is_empty() && environments.is_empty() {
        return Err(Error::Invalid(
            "the folder holds no Bruno requests or environments".into(),
        ));
    }
    Ok(Import::Collection(Collection {
        folder,
        items,
        environments,
        warnings: reader.warnings,
        format: "Bruno collection".into(),
    }))
}

fn invalid(path: &str, message: &str) -> Error {
    Error::Invalid(format!("{path} is not a valid .bru file: {message}"))
}

fn stem(file_name: &str) -> &str {
    let name = file_name.rsplit('/').next().unwrap_or(file_name);
    name.strip_suffix(".bru").unwrap_or(name)
}

#[derive(Default)]
struct Node {
    settings: Option<Folder>,
    seq: f64,
    folders: BTreeMap<String, Node>,
    requests: Vec<(f64, Request)>,
}

impl Node {
    fn at(&mut self, dir: &str) -> &mut Self {
        let mut node = self;
        for part in dir.split('/').filter(|p| !p.is_empty()) {
            node = node.folders.entry(part.to_owned()).or_insert_with(|| Self {
                seq: f64::MAX,
                ..Self::default()
            });
        }
        node
    }

    fn into_items(self) -> Vec<Item> {
        let mut folders: Vec<(f64, String, Self)> = self
            .folders
            .into_iter()
            .map(|(name, node)| (node.seq, name, node))
            .collect();
        folders.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        let mut items = Vec::new();
        for (_, name, node) in folders {
            let mut settings = node.settings.clone().unwrap_or_default();
            if settings.name.is_empty() {
                settings.name.clone_from(&name);
            }
            let children = node.into_items();
            if !children.is_empty() || settings != Folder::default() {
                items.push(Item::Folder(settings, children));
            }
        }
        let mut requests = self.requests;
        requests.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.name.cmp(&b.1.name)));
        items.extend(requests.into_iter().map(|(_, r)| Item::Request(r)));
        items
    }
}

struct Reader {
    warnings: Vec<String>,
}

impl Reader {
    fn request(&mut self, file: &BruFile, fallback: &str) -> Request {
        let name = file
            .get("meta", "name")
            .filter(|n| !n.trim().is_empty())
            .unwrap_or(fallback)
            .to_owned();
        let (method, http) = file
            .blocks
            .iter()
            .find(|(n, _)| METHODS.contains(&n.as_str()))
            .map_or(("get", ""), |(n, _)| (n.as_str(), n.as_str()));
        let mut request = Request {
            method: method.to_ascii_uppercase(),
            description: file.text("docs").unwrap_or_default().trim().to_owned(),
            headers: self.key_values(file.pairs("headers")),
            variables: self.vars(file.pairs("vars:pre-request")),
            ..Request::default()
        };
        let url = self.template(file.get(http, "url").unwrap_or_default());
        if file.has("params:query") {
            let base = url.split('?').next().unwrap_or(&url).to_owned();
            let params = self.key_values(file.pairs("params:query"));
            request.url = super::with_query(&base, &params);
            request.disabled_params = params.into_iter().filter(|p| !p.enabled).collect();
        } else {
            request.url = url;
        }
        for pair in file.pairs("params:path") {
            request
                .path_params
                .insert(pair.key.clone(), self.template(&pair.value));
        }
        let body_mode = file.get(http, "body").unwrap_or("none");
        let graphql = file.get("meta", "type") == Some("graphql");
        request.body = self.body(file, if graphql { "graphql" } else { body_mode }, &name);
        let auth_mode = file.get(http, "auth").unwrap_or("inherit");
        if let Some(auth) = self.auth(file, auth_mode, &name) {
            request.auth = auth;
        }
        request.scripts = self.scripts(file, &name);
        self.assertions(file, &mut request, &name);
        self.captures(file, &mut request, &name);
        super::drop_implied_content_type(&mut request);
        request.name = name;
        request
    }

    fn folder(&mut self, file: &BruFile, fallback: &str) -> Folder {
        let name = file
            .get("meta", "name")
            .filter(|n| !n.trim().is_empty())
            .unwrap_or(fallback)
            .to_owned();
        let mode = file.get("auth", "mode").unwrap_or("inherit");
        Folder {
            description: file.text("docs").unwrap_or_default().trim().to_owned(),
            headers: self.key_values(file.pairs("headers")),
            variables: self.vars(file.pairs("vars:pre-request")),
            auth: self.auth(file, mode, &name).unwrap_or(Auth::Inherit),
            scripts: self.scripts(file, &name),
            name,
            ..Folder::default()
        }
    }

    fn environment(&mut self, file: &BruFile, name: &str) -> NewEnvironment {
        let mut variables = self.vars(file.pairs("vars"));
        let secrets: Vec<String> = file.list("vars:secret").to_vec();
        for secret in &secrets {
            variables.entry(secret.clone()).or_default();
        }
        if !secrets.is_empty() {
            warn_once(
                &mut self.warnings,
                format!(
                    "the values of secret variables are not in the collection's files; set them in the local file of the {name} environment"
                ),
            );
        }
        NewEnvironment {
            name: name.to_owned(),
            variables,
            secrets,
        }
    }

    fn body(&mut self, file: &BruFile, mode: &str, name: &str) -> Body {
        let text = |block: &str| file.text(block).unwrap_or_default().to_owned();
        match mode {
            "json" => Body::Json {
                content: self.template(&text("body:json")),
            },
            "xml" => Body::Xml {
                content: self.template(&text("body:xml")),
            },
            "text" => Body::Text {
                content: self.template(&text("body:text")),
                content_type: None,
            },
            "sparql" => Body::Text {
                content: self.template(&text("body:sparql")),
                content_type: Some("application/sparql-query".into()),
            },
            "graphql" => Body::Graphql {
                query: self.template(&text("body:graphql")),
                variables: self.template(&text("body:graphql:vars")),
            },
            "formUrlEncoded" | "form-urlencoded" => Body::Form {
                fields: self.key_values(file.pairs("body:form-urlencoded")),
            },
            "multipartForm" | "multipart-form" => {
                let mut fields = Vec::new();
                for pair in file.pairs("body:multipart-form") {
                    let (value, content_type) = split_content_type(&pair.value);
                    let file_path = value
                        .strip_prefix("@file(")
                        .and_then(|v| v.strip_suffix(')'))
                        .map(|paths| paths.split('|').next().unwrap_or_default().to_owned());
                    fields.push(FormField {
                        name: self.template(&pair.key),
                        value: if file_path.is_some() {
                            String::new()
                        } else {
                            self.template(value)
                        },
                        file: file_path.map(|p| self.file_path(&p, name)),
                        content_type,
                        enabled: pair.enabled,
                    });
                }
                Body::Multipart { fields }
            }
            "file" => {
                let chosen = file
                    .pairs("body:file")
                    .iter()
                    .find(|p| p.enabled)
                    .or_else(|| file.pairs("body:file").first());
                let Some(chosen) = chosen else {
                    return Body::None;
                };
                let (value, content_type) = split_content_type(&chosen.value);
                let path = value
                    .strip_prefix("@file(")
                    .and_then(|v| v.strip_suffix(')'))
                    .unwrap_or(value);
                Body::Binary {
                    file: self.file_path(path, name),
                    content_type,
                }
            }
            _ => Body::None,
        }
    }

    /// Bruno keeps files relative to the collection; Getman relative to
    /// the workspace. Absolute paths keep only the file name.
    fn file_path(&mut self, path: &str, name: &str) -> String {
        let path = path.trim();
        let absolute = path.starts_with('/') || path.contains(':') || path.contains("..");
        let kept = if absolute {
            path.rsplit(['/', '\\']).next().unwrap_or(path).to_owned()
        } else {
            path.replace('\\', "/")
        };
        self.warnings.push(format!(
            "{name}: copy {path} into the workspace as {kept}; files are read from the workspace"
        ));
        kept
    }

    fn auth(&mut self, file: &BruFile, mode: &str, name: &str) -> Option<Auth> {
        let get = |key: &str| file.get(&format!("auth:{mode}"), key).unwrap_or_default();
        Some(match mode {
            "none" => Auth::None,
            "inherit" => Auth::Inherit,
            "basic" => Auth::Basic {
                username: self.template(get("username")),
                password: self.template(get("password")),
            },
            "digest" => Auth::Digest {
                username: self.template(get("username")),
                password: self.template(get("password")),
            },
            "bearer" => Auth::Bearer {
                token: self.template(get("token")),
                prefix: String::new(),
            },
            "apikey" => Auth::ApiKey {
                key: self.template(get("key")),
                value: self.template(get("value")),
                location: if get("placement") == "queryparams" {
                    ApiKeyLocation::Query
                } else {
                    ApiKeyLocation::Header
                },
            },
            "oauth2" => {
                let grant = match get("grant_type") {
                    "client_credentials" => OAuth2Grant::ClientCredentials,
                    "password" => OAuth2Grant::Password,
                    other => {
                        warn_once(
                            &mut self.warnings,
                            format!(
                                "{name}: OAuth 2.0 with the {other} grant needs a browser; it now sends {{{{accessToken}}}} as a Bearer token"
                            ),
                        );
                        return Some(Auth::Bearer {
                            token: "{{accessToken}}".into(),
                            prefix: String::new(),
                        });
                    }
                };
                Auth::Oauth2(OAuth2 {
                    grant,
                    token_url: self.template(get("access_token_url")),
                    client_id: self.template(get("client_id")),
                    client_secret: self.template(get("client_secret")),
                    scope: self.template(get("scope")),
                    audience: String::new(),
                    username: self.template(get("username")),
                    password: self.template(get("password")),
                    client_auth: if get("credentials_placement") == "body" {
                        ClientAuth::Body
                    } else {
                        ClientAuth::Basic
                    },
                })
            }
            other => {
                warn_once(
                    &mut self.warnings,
                    format!(
                        "{name}: {other} authentication is not supported; set the credentials in headers or a pre-request script"
                    ),
                );
                return None;
            }
        })
    }

    /// Scripts and tests, carried over as they are.
    fn scripts(&mut self, file: &BruFile, name: &str) -> Scripts {
        let pre = file.text("script:pre-request").unwrap_or_default().trim();
        let mut post = file
            .text("script:post-response")
            .unwrap_or_default()
            .trim()
            .to_owned();
        if let Some(tests) = file.text("tests").map(str::trim).filter(|t| !t.is_empty()) {
            if !post.is_empty() {
                post.push_str("\n\n");
            }
            post.push_str(tests);
        }
        let uses_api = |code: &str| {
            code.contains("bru.")
                || code.contains("req.")
                || code.contains("res.")
                || code.contains("res(")
        };
        if uses_api(pre) || uses_api(&post) {
            warn_once(
                &mut self.warnings,
                format!(
                    "{name}: its scripts call bru.*, req.* or res.*, which may need edits: Getman scripts use gm.*"
                ),
            );
        }
        Scripts {
            pre_request: pre.to_owned(),
            post_response: post,
        }
    }

    /// `assert` lines that Getman can check the same way become checks.
    fn assertions(&mut self, file: &BruFile, request: &mut Request, name: &str) {
        for pair in file.pairs("assert").iter().filter(|p| p.enabled) {
            match assertion(&pair.key, &pair.value) {
                Some(check) => request.checks.push(check),
                None => self.warnings.push(format!(
                    "{name}: the assertion `{}: {}` has no matching check; write it as a check or a test",
                    pair.key, pair.value
                )),
            }
        }
    }

    /// `vars:post-response` lines that read the response become captures.
    fn captures(&mut self, file: &BruFile, request: &mut Request, name: &str) {
        for pair in file
            .pairs("vars:post-response")
            .iter()
            .filter(|p| p.enabled)
        {
            match target(&pair.value) {
                Some(target) => {
                    request.captures.insert(pair.key.clone(), target);
                }
                None => self.warnings.push(format!(
                    "{name}: the post-response variable {} = {} has no matching capture; set it in a script",
                    pair.key, pair.value
                )),
            }
        }
    }

    fn key_values(&mut self, pairs: &[Pair]) -> Vec<KeyValue> {
        pairs
            .iter()
            .map(|p| KeyValue {
                enabled: p.enabled,
                ..KeyValue::new(self.template(&p.key), self.template(&p.value))
            })
            .collect()
    }

    fn vars(&mut self, pairs: &[Pair]) -> Vars {
        pairs
            .iter()
            .filter(|p| p.enabled)
            .map(|p| (p.key.clone(), self.template(&p.value)))
            .collect()
    }

    /// Rewrites `{{process.env.NAME}}` to Getman's `{{$env.GETMAN_NAME}}`.
    fn template(&mut self, text: &str) -> String {
        if !text.contains("process.env.") {
            return text.to_owned();
        }
        let mut renamed = Vec::new();
        let out = PROCESS_ENV.replace_all(text, |caps: &regex::Captures<'_>| {
            let name = &caps[1];
            if name.starts_with(crate::vars::ENV_PREFIX) {
                format!("{{{{$env.{name}}}}}")
            } else {
                renamed.push(name.to_owned());
                format!("{{{{$env.{}{name}}}}}", crate::vars::ENV_PREFIX)
            }
        });
        for name in renamed {
            warn_once(
                &mut self.warnings,
                format!(
                    "process.env.{name} is now read from GETMAN_{name}: Getman reads only environment variables that start with GETMAN_"
                ),
            );
        }
        out.into_owned()
    }
}

/// Splits `value @contentType(type)` into the value and the type.
fn split_content_type(value: &str) -> (&str, Option<String>) {
    match value.split_once("@contentType(") {
        Some((value, rest)) => (
            value.trim(),
            rest.strip_suffix(')')
                .map(|t| t.trim().to_owned())
                .filter(|t| !t.is_empty()),
        ),
        None => (value.trim(), None),
    }
}

/// Turns a Bruno response expression into a Getman check target, such as
/// `res.body.id` into `json.id`.
fn target(expression: &str) -> Option<String> {
    let expression = expression.trim();
    match expression {
        "res.status" | "res.getStatus()" => return Some("status".into()),
        "res.responseTime" | "res.getResponseTime()" => return Some("time".into()),
        "res.body" | "res.getBody()" => return Some("json".into()),
        _ => {}
    }
    if let Some(path) = expression.strip_prefix("res.body") {
        return (path.starts_with('.') || path.starts_with('['))
            .then(|| format!("json{path}"))
            .filter(|t| simple_path(t));
    }
    if let Some(inner) = expression
        .strip_prefix("res(")
        .and_then(|r| r.strip_suffix(')'))
    {
        let path = inner.trim().trim_matches(['"', '\'']);
        let target = format!("json.{path}");
        return simple_path(&target).then_some(target);
    }
    let header = expression
        .strip_prefix("res.headers.")
        .map(str::to_owned)
        .or_else(|| {
            expression
                .strip_prefix("res.headers[")
                .and_then(|r| r.strip_suffix(']'))
                .map(|r| r.trim().trim_matches(['"', '\'']).to_owned())
        });
    if let Some(name) = header.filter(|n| !n.is_empty() && !n.contains(['(', ' ', '['])) {
        return Some(format!("header.{name}"));
    }
    None
}

/// Whether a target is a plain path, with no calls or expressions.
fn simple_path(target: &str) -> bool {
    !target.contains(['(', ')', ' ', '+', '|', '&', '?', '='])
}

/// Turns an assertion into a check, or `None` when Getman has no check
/// for it.
fn assertion(key: &str, value: &str) -> Option<String> {
    let target = target(key)?;
    let value = value.trim();
    let (operator, operand) = value.split_once(' ').unwrap_or((value, ""));
    let operand = operand.trim();
    let symbol = match operator {
        "eq" => "==",
        "neq" => "!=",
        "gt" => ">",
        "gte" => ">=",
        "lt" => "<",
        "lte" => "<=",
        "contains" => "contains",
        "notContains" => "!contains",
        "startsWith" => "startsWith",
        "endsWith" => "endsWith",
        "matches" => "matches",
        "notMatches" => "!matches",
        "isDefined" => return Some(format!("{target} exists")),
        "isUndefined" => return Some(format!("{target} !exists")),
        "isNull" => return Some(format!("{target} == null")),
        "isString" => return Some(format!("{target} is string")),
        "isNumber" => return Some(format!("{target} is number")),
        "isBoolean" => return Some(format!("{target} is boolean")),
        "isArray" => return Some(format!("{target} is array")),
        "isJson" => return Some(format!("{target} is object")),
        "isEmpty" if target.starts_with("json") => {
            return Some(format!("{target}.length == 0"));
        }
        "isNotEmpty" if target.starts_with("json") => {
            return Some(format!("{target}.length > 0"));
        }
        "length" if target.starts_with("json") && !operand.is_empty() => {
            return Some(format!("{target}.length == {operand}"));
        }
        _ => return None,
    };
    if operand.is_empty() {
        return None;
    }
    Some(format!("{target} {symbol} {operand}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GET_USER: &str = include_str!("../../tests/fixtures/import/bruno/users/get-user.bru");

    fn fixture() -> Vec<SourceFile> {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/import/bruno");
        let mut files = Vec::new();
        let mut dirs = vec![std::path::PathBuf::from(root)];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    dirs.push(path);
                } else {
                    let relative = path.strip_prefix(root).unwrap();
                    files.push(SourceFile {
                        path: relative.to_string_lossy().replace('\\', "/"),
                        text: std::fs::read_to_string(&path).unwrap(),
                    });
                }
            }
        }
        files
    }

    #[test]
    fn bru_files_are_parsed_into_blocks() {
        let file = BruFile::parse(GET_USER).unwrap();
        assert_eq!(file.get("meta", "name"), Some("Get user"));
        assert_eq!(file.pairs("params:query")[1].key, "verbose");
        assert!(!file.pairs("params:query")[1].enabled);
        assert!(file.text("docs").unwrap().starts_with("# Get user"));
        assert!(
            file.text("script:pre-request")
                .unwrap()
                .contains("bru.setVar")
        );
    }

    #[test]
    fn a_collection_folder_becomes_folders_requests_and_environments() {
        let Import::Collection(c) = parse(&fixture()).unwrap() else {
            panic!()
        };
        assert_eq!(c.folder.name, "Shop");
        assert_eq!(c.folder.headers[0].name, "X-Client");
        assert!(matches!(&c.folder.auth, Auth::Bearer { token, .. } if token == "{{token}}"));
        assert_eq!(c.folder.variables["apiVersion"], "v2");

        assert_eq!(c.environments.len(), 1);
        let env = &c.environments[0];
        assert_eq!(env.name, "local");
        assert_eq!(env.variables["baseUrl"], "http://localhost:3000");
        assert!(!env.variables.contains_key("old"));
        assert_eq!(env.secrets, ["token"]);

        let Item::Folder(folder, items) = &c.items[0] else {
            panic!("{:?}", c.items)
        };
        assert_eq!(folder.name, "Users");
        assert!(matches!(&folder.auth, Auth::Basic { username, .. } if username == "ada"));
        let names: Vec<&str> = items
            .iter()
            .map(|i| match i {
                Item::Request(r) => r.name.as_str(),
                Item::Folder(f, _) => f.name.as_str(),
            })
            .collect();
        assert_eq!(names, ["Get user", "Create user"]);

        let Item::Request(get) = &items[0] else {
            panic!()
        };
        assert_eq!(get.url, "{{baseUrl}}/users/:id?page=1");
        assert_eq!(get.disabled_params[0].name, "verbose");
        assert_eq!(get.path_params["id"], "7");
        assert!(get.auth.is_inherit());
        assert_eq!(get.variables["userId"], "7");
        assert_eq!(
            get.checks,
            [
                "status == 200",
                "json.name is string",
                "header.content-type contains json",
                "json.items.length == 3"
            ]
        );
        assert_eq!(get.captures["userName"], "json.name");
        assert!(get.scripts.pre_request.contains("bru.setVar"));
        assert!(get.scripts.post_response.contains("test(\"ok\""));
        assert!(get.description.contains("Fetches one user"));

        let Item::Request(create) = &items[1] else {
            panic!()
        };
        assert_eq!(create.method, "POST");
        assert!(
            matches!(&create.body, Body::Json { content } if content.starts_with("{\n  \"name\": \"Ada\""))
        );
        assert!(
            create
                .headers
                .iter()
                .all(|h| !h.name.eq_ignore_ascii_case("content-type"))
        );
        assert_eq!(create.headers[0].value, "{{$env.GETMAN_TRACE_ID}}");

        let Item::Request(upload) = &c.items[1] else {
            panic!("{:?}", c.items)
        };
        let Body::Multipart { fields } = &upload.body else {
            panic!("{:?}", upload.body)
        };
        assert_eq!(fields[0].file.as_deref(), Some("files/cat.png"));
        assert_eq!(fields[0].content_type.as_deref(), Some("image/png"));
        assert!(!fields[1].enabled);
        assert!(matches!(
            &upload.auth,
            Auth::ApiKey {
                location: ApiKeyLocation::Query,
                ..
            }
        ));

        let warnings = c.warnings.join("\n");
        assert!(warnings.contains("bru.*"), "{warnings}");
        assert!(warnings.contains("isTruthy"), "{warnings}");
        assert!(warnings.contains("GETMAN_TRACE_ID"), "{warnings}");
        assert!(warnings.contains("files/cat.png"), "{warnings}");
    }

    #[test]
    fn one_bru_file_is_one_request() {
        let files = [SourceFile {
            path: "get-user.bru".into(),
            text: GET_USER.into(),
        }];
        let Import::Request { request, .. } = parse(&files).unwrap() else {
            panic!()
        };
        assert_eq!(request.name, "Get user");
    }

    #[test]
    fn assertions_translate_where_they_can() {
        assert_eq!(
            assertion("res.status", "eq 201").as_deref(),
            Some("status == 201")
        );
        assert_eq!(
            assertion("res.body.user.id", "isDefined").as_deref(),
            Some("json.user.id exists")
        );
        assert_eq!(
            assertion("res.headers['x-total']", "gte 1").as_deref(),
            Some("header.x-total >= 1")
        );
        assert_eq!(
            assertion("res('data.list')", "isNotEmpty").as_deref(),
            Some("json.data.list.length > 0")
        );
        assert_eq!(assertion("res.body.ok", "isTruthy"), None);
        assert_eq!(assertion("res.body.items.filter(x => x)", "isEmpty"), None);
        assert_eq!(assertion("res.body.name", "eq"), None);
    }

    #[test]
    fn broken_files_give_errors() {
        assert!(BruFile::parse("meta {\n  name: x\n").is_err());
        assert!(BruFile::parse("hello world").is_err());
        assert!(BruFile::parse("body:json {\n  {\n").is_err());
        assert!(BruFile::parse("vars {\n  a: '''\n    open\n}\n").is_err());
    }
}
