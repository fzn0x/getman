//! Bringing requests in from other formats.
//!
//! Every format but Postman's is read into an [`Import`] first: folders,
//! requests and environments in memory, with a warning for each thing
//! that could not be carried over. [`save`] then writes it into the
//! workspace. [`detect`] tells the formats apart by their content, so
//! [`import_text`] and [`import_files`] take any of them.

pub mod bruno;
pub mod curl;
pub mod har;
pub mod hoppscotch;
pub mod insomnia;
pub mod openapi;
pub mod postman;

use serde::Serialize;
use serde_json::Value;

use crate::error::{Error, Result};
use crate::model::{Body, Folder, Request, Vars};
use crate::workspace::{EnvironmentFiles, Workspace, slugify};
use crate::{Environment, KeyValue};

pub use postman::ImportReport;

/// A format Getman imports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    /// A cURL command.
    Curl,
    /// A Postman collection, environment or globals export.
    Postman,
    /// An OpenAPI 3.0 or 3.1 document, in JSON or YAML.
    Openapi,
    /// A Swagger 2.0 document, in JSON or YAML.
    Swagger,
    /// An HTTP Archive (HAR 1.2), as saved by browsers.
    Har,
    /// An Insomnia export: v4 JSON or v5 YAML.
    Insomnia,
    /// Hoppscotch collections or environments.
    Hoppscotch,
    /// A Bruno collection: a folder of `.bru` files, or one of them.
    Bruno,
}

impl Format {
    /// Name to show.
    pub fn label(self) -> &'static str {
        match self {
            Self::Curl => "cURL command",
            Self::Postman => "Postman export",
            Self::Openapi => "OpenAPI document",
            Self::Swagger => "Swagger 2.0 document",
            Self::Har => "HAR file",
            Self::Insomnia => "Insomnia export",
            Self::Hoppscotch => "Hoppscotch export",
            Self::Bruno => "Bruno collection",
        }
    }
}

/// What a format was read into, before it is written.
#[derive(Debug, Clone, PartialEq)]
pub enum Import {
    /// Folders and requests, and environments that came with them.
    Collection(Box<Collection>),
    /// Environments alone.
    Environments {
        /// The environments.
        environments: Vec<NewEnvironment>,
        /// Things that could not be carried over exactly.
        warnings: Vec<String>,
    },
    /// One request.
    Request {
        /// The request, boxed because it is far larger than the others.
        request: Box<Request>,
        /// Things that could not be carried over exactly.
        warnings: Vec<String>,
    },
}

/// A collection read from another format.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Collection {
    /// Settings of the folder the collection becomes, with its name.
    pub folder: Folder,
    /// Folders and requests, in order.
    pub items: Vec<Item>,
    /// Environments that came with the collection.
    pub environments: Vec<NewEnvironment>,
    /// Things that could not be carried over exactly.
    pub warnings: Vec<String>,
    /// The format read, such as `OpenAPI 3.1.0`.
    pub format: String,
}

/// A folder or a request of a [`Collection`].
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)] // Built once per import, then written.
pub enum Item {
    /// A folder: its settings and its items, in order.
    Folder(Folder, Vec<Item>),
    /// A request.
    Request(Request),
}

impl Item {
    /// Counts the requests in a list of items, folders included.
    pub fn count_requests(items: &[Self]) -> usize {
        items
            .iter()
            .map(|item| match item {
                Self::Folder(_, children) => Self::count_requests(children),
                Self::Request(_) => 1,
            })
            .sum()
    }
}

/// An environment read from another format.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewEnvironment {
    /// Display name.
    pub name: String,
    /// Variables. Values of secrets go to the git-ignored local file.
    pub variables: Vars,
    /// Names of the variables that are secret.
    pub secrets: Vec<String>,
}

/// A file of a folder being imported, such as a Bruno collection.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct SourceFile {
    /// Path of the file inside the folder, with `/` separators.
    pub path: String,
    /// The file's text.
    pub text: String,
}

/// Tells which format `text` is in. `name` is the file name, if known.
pub fn detect(name: &str, text: &str) -> Option<Format> {
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    if trimmed.starts_with("curl ") || trimmed.starts_with("curl\n") || trimmed == "curl" {
        return Some(Format::Curl);
    }
    if has_extension(name, "bru") || bruno::looks_like_bru(trimmed) {
        return Some(Format::Bruno);
    }
    let value = parse_document(trimmed).ok()?;
    detect_value(&value)
}

fn detect_value(value: &Value) -> Option<Format> {
    if value["openapi"]
        .as_str()
        .is_some_and(|v| v.starts_with('3'))
    {
        return Some(Format::Openapi);
    }
    if value["swagger"]
        .as_str()
        .is_some_and(|v| v.starts_with('2'))
        || value["swagger"].as_f64() == Some(2.0)
    {
        return Some(Format::Swagger);
    }
    if value["log"]["entries"].is_array() {
        return Some(Format::Har);
    }
    if insomnia::matches(value) {
        return Some(Format::Insomnia);
    }
    if (value.get("info").is_some() && value.get("item").is_some())
        || value["values"].is_array()
        || value["collection"].get("item").is_some()
    {
        return Some(Format::Postman);
    }
    if hoppscotch::matches(value) {
        return Some(Format::Hoppscotch);
    }
    None
}

/// Imports one file in any format Getman reads, detected from its content.
///
/// A collection goes into a new folder inside `parent`, named after it,
/// unless `into_root` is set: then its settings become the workspace's
/// and its items the workspace's top level.
pub fn import_text(
    workspace: &Workspace,
    parent: &str,
    name: &str,
    text: &str,
    into_root: bool,
) -> Result<ImportReport> {
    let text = text.trim_start_matches('\u{feff}');
    if text.trim().is_empty() {
        return Err(Error::Invalid("there is nothing to import".into()));
    }
    let Some(format) = detect(name, text) else {
        // Only JSON's errors help: other text was not meant to be YAML.
        let looks_like_json = text.trim_start().starts_with(['{', '[']);
        let reason = parse_document(text.trim_start())
            .err()
            .filter(|_| looks_like_json)
            .map(|e| format!(" ({e})"))
            .unwrap_or_default();
        return Err(Error::Invalid(format!(
            "this is not a format Getman imports{reason}; use a Postman, Insomnia or Hoppscotch export, an OpenAPI or Swagger document, a HAR file, a Bruno collection or a cURL command"
        )));
    };
    let import = match format {
        Format::Curl => {
            let request = curl::parse(text)?;
            let path = workspace.create_request(parent, &request.name, &request)?;
            return Ok(ImportReport {
                kind: "request".into(),
                name: request.name,
                path,
                requests: 1,
                format: format.label().into(),
                ..ImportReport::default()
            });
        }
        Format::Postman => return postman::import(workspace, parent, text, into_root),
        Format::Bruno => {
            let file = SourceFile {
                path: if name.is_empty() {
                    "request.bru".into()
                } else {
                    name.to_owned()
                },
                text: text.to_owned(),
            };
            bruno::parse(&[file])?
        }
        Format::Openapi | Format::Swagger => openapi::parse(&parse_document(text)?)?,
        Format::Har => har::parse(&parse_document(text)?)?,
        Format::Insomnia => insomnia::parse(&parse_document(text)?)?,
        Format::Hoppscotch => hoppscotch::parse(&parse_document(text)?)?,
    };
    save(workspace, parent, import, into_root, format)
}

/// Imports several files at once, such as a folder. A folder holding
/// `.bru` files or a `bruno.json` is one Bruno collection; otherwise each
/// file in a known format is imported on its own.
pub fn import_files(
    workspace: &Workspace,
    parent: &str,
    files: &[SourceFile],
    into_root: bool,
) -> Result<Vec<ImportReport>> {
    let files: Vec<SourceFile> = strip_common_folder(files);
    let is_bruno = files.iter().any(|f| {
        let lower = f.path.to_ascii_lowercase();
        lower == "bruno.json" || has_extension(&lower, "bru")
    });
    if is_bruno {
        let import = bruno::parse(&files)?;
        return Ok(vec![save(
            workspace,
            parent,
            import,
            into_root,
            Format::Bruno,
        )?]);
    }
    // Files of no known format, such as a README, are passed over, unless
    // there is only one.
    let known: Vec<&SourceFile> = files
        .iter()
        .filter(|f| files.len() == 1 || detect(&f.path, &f.text).is_some())
        .collect();
    if known.is_empty() {
        return Err(Error::Invalid(
            "none of the files is in a format Getman imports".into(),
        ));
    }
    let mut reports = Vec::new();
    for file in known {
        let report = import_text(workspace, parent, &file.path, &file.text, into_root)
            .map_err(|e| Error::Invalid(format!("{}: {e}", file.path)))?;
        reports.push(report);
    }
    Ok(reports)
}

/// Drops the folder every path starts with, as a browser's folder upload
/// adds the folder's own name in front.
fn strip_common_folder(files: &[SourceFile]) -> Vec<SourceFile> {
    let normalized: Vec<SourceFile> = files
        .iter()
        .map(|f| SourceFile {
            path: f
                .path
                .replace('\\', "/")
                .trim_start_matches("./")
                .to_owned(),
            text: f.text.clone(),
        })
        .collect();
    let first = normalized
        .first()
        .and_then(|f| f.path.split_once('/'))
        .map(|(head, _)| format!("{head}/"));
    let Some(prefix) = first else {
        return normalized;
    };
    if normalized.iter().all(|f| f.path.starts_with(&prefix)) {
        normalized
            .into_iter()
            .map(|f| SourceFile {
                path: f.path[prefix.len()..].to_owned(),
                text: f.text,
            })
            .collect()
    } else {
        normalized
    }
}

/// Writes what was read into the workspace.
pub fn save(
    workspace: &Workspace,
    parent: &str,
    import: Import,
    into_root: bool,
    format: Format,
) -> Result<ImportReport> {
    match import {
        Import::Collection(collection) => {
            save_collection(workspace, parent, *collection, into_root)
        }
        Import::Request { request, warnings } => {
            let name = if request.name.trim().is_empty() {
                "Untitled".to_owned()
            } else {
                request.name.clone()
            };
            let path = workspace.create_request(parent, &name, &request)?;
            Ok(ImportReport {
                kind: "request".into(),
                name,
                path,
                requests: 1,
                warnings,
                format: if format == Format::Bruno {
                    ".bru file".into()
                } else {
                    format.label().into()
                },
                ..ImportReport::default()
            })
        }
        Import::Environments {
            environments,
            warnings,
        } => {
            if environments.is_empty() {
                return Err(Error::Invalid("the file holds no environments".into()));
            }
            let mut report = ImportReport {
                kind: "environment".into(),
                format: format.label().into(),
                warnings,
                ..ImportReport::default()
            };
            let names: Vec<&str> = environments.iter().map(|e| e.name.as_str()).collect();
            report.name = names.join(", ");
            for environment in &environments {
                report
                    .environments
                    .push(save_environment(workspace, environment)?);
            }
            report.path = report.environments[0].clone();
            Ok(report)
        }
    }
}

fn save_collection(
    workspace: &Workspace,
    parent: &str,
    collection: Collection,
    into_root: bool,
) -> Result<ImportReport> {
    let Collection {
        folder: mut meta,
        items,
        environments,
        warnings,
        format,
    } = collection;
    if items.is_empty() && environments.is_empty() {
        return Err(Error::Invalid(
            "the file holds no requests or environments to import".into(),
        ));
    }
    if meta.name.trim().is_empty() {
        meta.name = "Imported collection".into();
    }
    let mut report = ImportReport {
        kind: "collection".into(),
        name: meta.name.clone(),
        warnings,
        format,
        ..ImportReport::default()
    };
    let folder = if into_root {
        let mut existing = workspace.read_folder("")?;
        existing.name = meta.name;
        if !meta.description.is_empty() {
            existing.description = meta.description;
        }
        existing.variables.extend(meta.variables);
        existing.headers.extend(meta.headers);
        if !meta.auth.is_inherit() {
            existing.auth = meta.auth;
        }
        if !meta.scripts.is_empty() {
            existing.scripts = meta.scripts;
        }
        if !meta.settings.is_empty() {
            existing.settings = meta.settings;
        }
        workspace.write_folder("", &existing)?;
        String::new()
    } else {
        let path = workspace.create_folder(parent, &meta.name)?;
        report.folders += 1;
        meta.order = Vec::new();
        workspace.write_folder(&path, &meta)?;
        path
    };
    report.path.clone_from(&folder);
    save_items(workspace, &folder, items, &mut report)?;
    for environment in &environments {
        report
            .environments
            .push(save_environment(workspace, environment)?);
    }
    Ok(report)
}

fn save_items(
    workspace: &Workspace,
    folder: &str,
    items: Vec<Item>,
    report: &mut ImportReport,
) -> Result<()> {
    let mut order = Vec::new();
    for item in items {
        match item {
            Item::Folder(mut meta, children) => {
                if meta.name.trim().is_empty() {
                    meta.name = "Untitled".into();
                }
                let path = workspace.create_folder(folder, &meta.name)?;
                report.folders += 1;
                meta.order = Vec::new();
                workspace.write_folder(&path, &meta)?;
                save_items(workspace, &path, children, report)?;
                order.push(path.rsplit('/').next().unwrap_or(&path).to_owned());
            }
            Item::Request(request) => {
                let name = if request.name.trim().is_empty() {
                    "Untitled".to_owned()
                } else {
                    request.name.clone()
                };
                let path = workspace.create_request(folder, &name, &request)?;
                report.requests += 1;
                let file = path.rsplit('/').next().unwrap_or(&path);
                order.push(file.trim_end_matches(".yaml").to_owned());
            }
        }
    }
    if order.len() > 1 {
        workspace.set_order(folder, order)?;
    }
    Ok(())
}

fn save_environment(workspace: &Workspace, environment: &NewEnvironment) -> Result<String> {
    let name = if environment.name.trim().is_empty() {
        "Imported"
    } else {
        environment.name.trim()
    };
    let id = workspace.create_environment(name)?;
    let shared = Environment {
        name: if slugify(name) == id && name == id {
            String::new()
        } else {
            name.to_owned()
        },
        variables: environment.variables.clone(),
        secrets: environment.secrets.clone(),
    };
    workspace.write_environment(
        &id,
        &EnvironmentFiles {
            shared,
            local: Vars::new(),
        },
    )?;
    Ok(id)
}

/// Parses JSON, or YAML when the text is not JSON. YAML mapping keys that
/// are numbers or booleans, such as status codes, become strings.
pub fn parse_document(text: &str) -> Result<Value> {
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return serde_json::from_str(trimmed)
            .map_err(|e| Error::Invalid(format!("the file is not valid JSON: {e}")));
    }
    let mut yaml: serde_yaml_ng::Value = serde_yaml_ng::from_str(trimmed)
        .map_err(|e| Error::Invalid(format!("the file is neither JSON nor valid YAML: {e}")))?;
    yaml.apply_merge()
        .map_err(|e| Error::Invalid(format!("the YAML file has a broken merge key: {e}")))?;
    Ok(yaml_to_json(yaml))
}

fn yaml_to_json(value: serde_yaml_ng::Value) -> Value {
    use serde_yaml_ng::Value as Y;
    match value {
        Y::Null => Value::Null,
        Y::Bool(b) => Value::Bool(b),
        Y::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::from(i)
            } else if let Some(u) = n.as_u64() {
                Value::from(u)
            } else {
                n.as_f64()
                    .and_then(serde_json::Number::from_f64)
                    .map_or(Value::Null, Value::Number)
            }
        }
        Y::String(s) => Value::String(s),
        Y::Sequence(list) => Value::Array(list.into_iter().map(yaml_to_json).collect()),
        Y::Mapping(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| (yaml_key(key), yaml_to_json(value)))
                .collect(),
        ),
        Y::Tagged(tagged) => yaml_to_json(tagged.value),
    }
}

fn yaml_key(key: serde_yaml_ng::Value) -> String {
    use serde_yaml_ng::Value as Y;
    match key {
        Y::String(s) => s,
        Y::Bool(b) => b.to_string(),
        Y::Number(n) => n.to_string(),
        Y::Null => "null".into(),
        other => serde_yaml_ng::to_string(&other)
            .unwrap_or_default()
            .trim()
            .to_owned(),
    }
}

// ---- Helpers shared by the formats ------------------------------------------

/// Any scalar as text.
pub(crate) fn text(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// A scalar as text, or the empty string.
pub(crate) fn text_or_empty(value: &Value) -> String {
    text(value).unwrap_or_default()
}

/// JSON text, pretty-printed when it parses.
pub(crate) fn pretty_json(text: &str) -> String {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|value| serde_json::to_string_pretty(&value).ok())
        .unwrap_or_else(|| text.to_owned())
}

/// A body for text sent with a content type.
pub(crate) fn text_body(content_type: &str, content: String) -> Body {
    let essence = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if essence.contains("json") {
        Body::Json {
            content: pretty_json(&content),
        }
    } else if essence.ends_with("/xml") || essence.ends_with("+xml") {
        Body::Xml { content }
    } else {
        Body::Text {
            content,
            content_type: (!essence.is_empty() && essence != "text/plain")
                .then(|| content_type.trim().to_owned()),
        }
    }
}

/// Removes a `Content-Type` header that only repeats what the body sends
/// anyway, so imported requests stay short.
pub(crate) fn drop_implied_content_type(request: &mut Request) {
    let implied = match &request.body {
        Body::Json { .. } | Body::Graphql { .. } => Some("application/json"),
        Body::Xml { .. } => Some("application/xml"),
        Body::Form { .. } => Some("application/x-www-form-urlencoded"),
        // The boundary is written when sending; a copied one would be wrong.
        Body::Multipart { .. } => Some("multipart/form-data"),
        _ => None,
    };
    let Some(implied) = implied else { return };
    request.headers.retain(|h| {
        if !h.name.eq_ignore_ascii_case("content-type") {
            return true;
        }
        let essence = h.value.split(';').next().unwrap_or_default().trim();
        let multipart = implied == "multipart/form-data";
        !(multipart || essence.eq_ignore_ascii_case(implied) && !h.value.contains(';'))
    });
}

/// A name for a request: its method and its URL's path.
pub(crate) fn name_from_url(method: &str, url: &str) -> String {
    let without_scheme = url.split_once("://").map_or(url, |(_, rest)| rest);
    let path = without_scheme
        .split_once('/')
        .map_or("", |(_, path)| path)
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim_matches('/');
    if path.is_empty() {
        let host = without_scheme.split(['/', '?', '#']).next().unwrap_or(url);
        format!("{method} {host}")
    } else {
        format!("{method} /{path}")
    }
}

/// Builds a URL from a base and the enabled query parameters; disabled
/// ones go to the request's `disabled_params`.
pub(crate) fn with_query(base: &str, params: &[KeyValue]) -> String {
    let enabled: Vec<String> = params
        .iter()
        .filter(|p| p.enabled)
        .map(|p| {
            if p.value.is_empty() {
                p.name.clone()
            } else {
                format!("{}={}", p.name, p.value)
            }
        })
        .collect();
    if enabled.is_empty() {
        return base.to_owned();
    }
    let separator = if base.contains('?') { '&' } else { '?' };
    format!("{base}{separator}{}", enabled.join("&"))
}

/// A count with its noun: `1 request`, `2 requests`. A singular that
/// starts with `a ` is written without the number.
pub(crate) fn count(n: usize, one: &str, many: &str) -> String {
    match n {
        1 if one.starts_with("a ") => one.to_owned(),
        1 => format!("1 {one}"),
        n => format!("{n} {many}"),
    }
}

/// Whether a file name ends with `.extension`, in any case.
pub(crate) fn has_extension(name: &str, extension: &str) -> bool {
    std::path::Path::new(name)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(extension))
}

/// Adds a warning once.
pub(crate) fn warn_once(warnings: &mut Vec<String>, warning: String) {
    if !warnings.contains(&warning) {
        warnings.push(warning);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Map;

    use super::*;

    fn workspace() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::init(dir.path(), "Mine").unwrap();
        (dir, ws)
    }

    #[test]
    fn formats_are_told_apart_by_content() {
        let cases: &[(&str, &str, Option<Format>)] = &[
            ("", "curl https://x.test", Some(Format::Curl)),
            (
                "",
                "openapi: 3.1.0\ninfo: {title: x}\npaths: {}",
                Some(Format::Openapi),
            ),
            (
                "",
                r#"{"swagger": "2.0", "paths": {}}"#,
                Some(Format::Swagger),
            ),
            (
                "",
                r#"{"log": {"version": "1.2", "entries": []}}"#,
                Some(Format::Har),
            ),
            (
                "",
                r#"{"_type": "export", "__export_format": 4, "resources": []}"#,
                Some(Format::Insomnia),
            ),
            (
                "",
                "type: collection.insomnia.rest/5.0\nname: x\n",
                Some(Format::Insomnia),
            ),
            (
                "",
                r#"{"info": {"name": "x"}, "item": []}"#,
                Some(Format::Postman),
            ),
            ("", r#"{"name": "e", "values": []}"#, Some(Format::Postman)),
            (
                "",
                r#"{"v": 2, "name": "c", "folders": [], "requests": []}"#,
                Some(Format::Hoppscotch),
            ),
            (
                "",
                r#"[{"name": "env", "variables": [{"key": "a", "value": "1"}]}]"#,
                Some(Format::Hoppscotch),
            ),
            ("get.bru", "meta {\n  name: x\n}\n", Some(Format::Bruno)),
            (
                "",
                "meta {\n  name: x\n  type: http\n}\n\nget {\n  url: x\n}\n",
                Some(Format::Bruno),
            ),
            ("", "{\"just\": \"json\"}", None),
            ("", "hello", None),
        ];
        for (name, text, expected) in cases {
            assert_eq!(detect(name, text), *expected, "{text}");
        }
    }

    #[test]
    fn yaml_keys_that_are_numbers_become_strings() {
        let value =
            parse_document("responses:\n  200:\n    description: ok\n  true: yes\n").unwrap();
        assert_eq!(value["responses"]["200"]["description"], "ok");
        assert_eq!(value["responses"]["true"], "yes");
    }

    #[test]
    fn unknown_text_is_refused_with_the_formats_listed() {
        let (_dir, ws) = workspace();
        let error = import_text(&ws, "", "", "{\"a\": 1}", false).unwrap_err();
        assert!(error.to_string().contains("OpenAPI"), "{error}");
        assert!(import_text(&ws, "", "", "   ", false).is_err());
    }

    #[test]
    fn collections_are_written_with_their_environments_and_order() {
        let (_dir, ws) = workspace();
        let collection = Collection {
            folder: Folder {
                name: "Shop".into(),
                ..Folder::default()
            },
            items: vec![
                Item::Request(Request {
                    name: "Zeta".into(),
                    url: "https://x.test/z".into(),
                    ..Request::default()
                }),
                Item::Folder(
                    Folder {
                        name: "Alpha".into(),
                        ..Folder::default()
                    },
                    vec![Item::Request(Request {
                        name: String::new(),
                        url: "https://x.test/a".into(),
                        ..Request::default()
                    })],
                ),
            ],
            environments: vec![NewEnvironment {
                name: "Live".into(),
                variables: [("token".to_owned(), "s3cret".to_owned())]
                    .into_iter()
                    .collect(),
                secrets: vec!["token".into()],
            }],
            warnings: vec!["a warning".into()],
            format: "Test".into(),
        };
        let report = save(
            &ws,
            "",
            Import::Collection(Box::new(collection)),
            false,
            Format::Openapi,
        )
        .unwrap();
        assert_eq!((report.folders, report.requests), (2, 2));
        assert_eq!(report.environments, ["live"]);
        assert_eq!(ws.read_folder("shop").unwrap().order, ["zeta", "alpha"]);
        assert!(ws.read_request("shop/alpha/untitled.yaml").is_ok());
        let env = ws.read_environment("live").unwrap();
        assert_eq!(env.shared.variables["token"], "");
        assert_eq!(env.local["token"], "s3cret");
    }

    #[test]
    fn folders_import_each_file_in_a_known_format() {
        let (_dir, ws) = workspace();
        let files = vec![
            SourceFile {
                path: "exports/README.md".into(),
                text: "# Notes".into(),
            },
            SourceFile {
                path: "exports/staging.json".into(),
                text: r#"{"name": "Staging", "values": [{"key": "a", "value": "1"}]}"#.into(),
            },
            SourceFile {
                path: "exports/api.har".into(),
                text: include_str!("../../tests/fixtures/import/browser.har").into(),
            },
        ];
        let reports = import_files(&ws, "", &files, false).unwrap();
        let kinds: Vec<&str> = reports.iter().map(|r| r.kind.as_str()).collect();
        assert_eq!(kinds, ["environment", "collection"]);
        assert!(import_files(&ws, "", &files[..1], false).is_err());
    }

    #[test]
    fn a_folder_upload_loses_its_own_name() {
        let files = vec![
            SourceFile {
                path: "api/bruno.json".into(),
                text: String::new(),
            },
            SourceFile {
                path: "api\\users\\get.bru".into(),
                text: String::new(),
            },
        ];
        let stripped = strip_common_folder(&files);
        assert_eq!(stripped[0].path, "bruno.json");
        assert_eq!(stripped[1].path, "users/get.bru");
    }

    const FIXTURES: &[(&str, &str)] = &[
        (
            "petstore-openapi-3.1.yaml",
            include_str!("../../tests/fixtures/import/petstore-openapi-3.1.yaml"),
        ),
        (
            "petstore-swagger-2.0.json",
            include_str!("../../tests/fixtures/import/petstore-swagger-2.0.json"),
        ),
        (
            "browser.har",
            include_str!("../../tests/fixtures/import/browser.har"),
        ),
        (
            "insomnia-v4.json",
            include_str!("../../tests/fixtures/import/insomnia-v4.json"),
        ),
        (
            "insomnia-v5.yaml",
            include_str!("../../tests/fixtures/import/insomnia-v5.yaml"),
        ),
        (
            "hoppscotch-collection.json",
            include_str!("../../tests/fixtures/import/hoppscotch-collection.json"),
        ),
        (
            "hoppscotch-environments.json",
            include_str!("../../tests/fixtures/import/hoppscotch-environments.json"),
        ),
        (
            "get-user.bru",
            include_str!("../../tests/fixtures/import/bruno/users/get-user.bru"),
        ),
    ];

    /// Runs the parser for a format directly, without writing anything.
    fn parse_as(format: Format, name: &str, text: &str) -> Result<Import> {
        let document = || parse_document(text);
        match format {
            Format::Openapi | Format::Swagger => openapi::parse(&document()?),
            Format::Har => har::parse(&document()?),
            Format::Insomnia => insomnia::parse(&document()?),
            Format::Hoppscotch => hoppscotch::parse(&document()?),
            Format::Bruno => bruno::parse(&[SourceFile {
                path: name.into(),
                text: text.into(),
            }]),
            Format::Curl | Format::Postman => unreachable!(),
        }
    }

    #[test]
    fn every_fixture_imports_into_a_workspace() {
        for (name, text) in FIXTURES {
            let (_dir, ws) = workspace();
            let report =
                import_text(&ws, "", name, text, false).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(
                report.requests > 0 || !report.environments.is_empty(),
                "{name}"
            );
        }
    }

    #[test]
    fn truncated_and_garbage_input_gives_errors_not_panics() {
        let (_dir, ws) = workspace();
        for (name, text) in FIXTURES {
            let format = detect(name, text).unwrap();
            // Every cut point along the file.
            let cuts: Vec<usize> = (1..40).map(|i| text.len() * i / 40).collect();
            for cut in cuts {
                let mut end = cut;
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                let truncated = &text[..end];
                let _ = parse_as(format, name, truncated);
                let result = import_text(&ws, "", name, truncated, false);
                if has_extension(name, "json") || has_extension(name, "har") {
                    assert!(result.is_err(), "{name} cut at {end} was accepted");
                }
            }
        }
        let garbage: &[&str] = &[
            "",
            "{",
            "[",
            "null",
            "42",
            "\"text\"",
            "[]",
            "{}",
            "[1, 2, 3]",
            "{\"openapi\": 3}",
            "{\"openapi\": \"3.0.0\"}",
            "{\"log\": {\"entries\": [1, null, {\"request\": \"x\"}, {\"request\": {\"url\": 5}}]}}",
            "{\"_type\": \"export\", \"resources\": [{\"_type\": \"workspace\", \"_id\": \"w\"}, {\"_type\": \"request_group\", \"_id\": \"g\", \"parentId\": \"g\"}]}",
            "type: collection.insomnia.rest/5.0\ncollection: 5\nenvironments: [1]\n",
            "type: spec.insomnia.rest/5.0\n",
            "[{\"variables\": 5}]",
            "meta {\n  name: x\n",
            "get {\n  url: '''\n}\n",
            "\u{feff}\u{0}\u{1}garbage\u{7f}",
            "- just\n- a\n- list\n",
            "key: [unclosed\n",
            "&a [*a]",
        ];
        for text in garbage {
            assert!(
                import_text(&ws, "", "", text, false).is_err(),
                "{text:?} was accepted"
            );
        }
        // Odd but readable: these import what they can, without panicking.
        let odd: &[&str] = &[
            "{\"openapi\": \"3.0.0\", \"paths\": {\"/a\": 5, \"/b\": {\"get\": 7, \"post\": {\"parameters\": [1, null, \"x\", {\"in\": 5}], \"requestBody\": {\"$ref\": 9}}}}}",
            "{\"swagger\": \"2.0\", \"paths\": {\"/a\": {\"get\": {\"parameters\": [{\"in\": \"body\", \"schema\": {\"$ref\": \"#/definitions/X\"}}]}}}, \"definitions\": {\"X\": {\"$ref\": \"#/definitions/X\"}}}",
            "{\"_type\": \"export\", \"resources\": [1, null, {\"_type\": \"request\", \"parentId\": 5}]}",
            "{\"folders\": [1, null], \"requests\": [null, 5, {\"auth\": 5, \"body\": [], \"params\": {}}]}",
        ];
        for text in odd {
            let _ = import_text(&ws, "", "", text, false);
        }
        // Random bytes.
        let mut rng = fastrand::Rng::with_seed(7);
        for _ in 0..200 {
            let length = rng.usize(0..400);
            let bytes: Vec<u8> = (0..length).map(|_| rng.u8(..)).collect();
            let text = String::from_utf8_lossy(&bytes);
            let _ = import_text(&ws, "", "", &text, false);
            let _ = bruno::parse(&[SourceFile {
                path: "x.bru".into(),
                text: text.clone().into_owned(),
            }]);
        }
    }

    #[test]
    fn values_of_the_wrong_type_anywhere_give_errors_not_panics() {
        // Replaces each part of each fixture, one at a time, with values
        // of other types, and parses the result.
        fn paths(value: &Value, at: &str, out: &mut Vec<String>) {
            out.push(at.to_owned());
            match value {
                Value::Object(map) => {
                    for (key, child) in map {
                        let key = key.replace('~', "~0").replace('/', "~1");
                        paths(child, &format!("{at}/{key}"), out);
                    }
                }
                Value::Array(list) => {
                    for (index, child) in list.iter().enumerate() {
                        paths(child, &format!("{at}/{index}"), out);
                    }
                }
                _ => {}
            }
        }
        let replacements = [
            Value::Null,
            Value::from(7),
            Value::from("x"),
            Value::Array(Vec::new()),
            Value::Object(Map::new()),
        ];
        for (name, text) in FIXTURES {
            let Ok(original) = parse_document(text) else {
                continue;
            };
            let format = detect(name, text).unwrap();
            let mut all = Vec::new();
            paths(&original, "", &mut all);
            for path in all {
                for replacement in &replacements {
                    let mut mutated = original.clone();
                    if path.is_empty() {
                        mutated = replacement.clone();
                    } else if let Some(slot) = mutated.pointer_mut(&path) {
                        *slot = replacement.clone();
                    }
                    let text = serde_json::to_string(&mutated).unwrap();
                    let _ = parse_as(format, name, &text);
                }
            }
        }
    }

    #[test]
    fn implied_content_types_are_dropped() {
        let mut request = Request {
            headers: vec![
                KeyValue::new("Content-Type", "application/json"),
                KeyValue::new("Accept", "application/json"),
            ],
            body: Body::Json {
                content: "{}".into(),
            },
            ..Request::default()
        };
        drop_implied_content_type(&mut request);
        assert_eq!(request.headers.len(), 1);
        let mut multipart = Request {
            headers: vec![KeyValue::new(
                "content-type",
                "multipart/form-data; boundary=x",
            )],
            body: Body::Multipart { fields: vec![] },
            ..Request::default()
        };
        drop_implied_content_type(&mut multipart);
        assert!(multipart.headers.is_empty());
    }
}
