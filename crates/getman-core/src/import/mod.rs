//! Bringing requests in from other formats.
//!
//! Every format but Postman's is read into an [`Import`] first: folders,
//! requests and environments in memory, with a warning for each thing
//! that could not be carried over. [`save`] then writes it into the
//! workspace. [`detect`] tells the formats apart by their content, so
//! [`import_text`] takes any of them.

pub mod curl;
pub mod har;
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
        }
    }
}

/// What a format was read into, before it is written.
#[derive(Debug, Clone, PartialEq)]
pub enum Import {
    /// Folders and requests, and environments that came with them.
    Collection(Collection),
    /// Environments alone.
    Environments {
        /// The environments.
        environments: Vec<NewEnvironment>,
        /// Things that could not be carried over exactly.
        warnings: Vec<String>,
    },
    /// One request.
    Request {
        /// The request.
        request: Request,
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

/// Tells which format `text` is in. `name` is the file name, if known.
pub fn detect(_name: &str, text: &str) -> Option<Format> {
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    if trimmed.starts_with("curl ") || trimmed.starts_with("curl\n") || trimmed == "curl" {
        return Some(Format::Curl);
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
    if (value.get("info").is_some() && value.get("item").is_some())
        || value["values"].is_array()
        || value["collection"].get("item").is_some()
    {
        return Some(Format::Postman);
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
        let reason = parse_document(text.trim_start())
            .err()
            .map(|e| format!(" ({e})"))
            .unwrap_or_default();
        return Err(Error::Invalid(format!(
            "this is not a format Getman imports{reason}; use a Postman export, an OpenAPI or Swagger document, a HAR file or a cURL command"
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
        Format::Openapi | Format::Swagger => openapi::parse(&parse_document(text)?)?,
        Format::Har => har::parse(&parse_document(text)?)?,
    };
    save(workspace, parent, import, into_root, format)
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
        Import::Collection(collection) => save_collection(workspace, parent, collection, into_root),
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
                format: format.label().into(),
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

/// Adds a warning once.
pub(crate) fn warn_once(warnings: &mut Vec<String>, warning: String) {
    if !warnings.contains(&warning) {
        warnings.push(warning);
    }
}

#[cfg(test)]
mod tests {
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
                r#"{"info": {"name": "x"}, "item": []}"#,
                Some(Format::Postman),
            ),
            ("", r#"{"name": "e", "values": []}"#, Some(Format::Postman)),
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
            Import::Collection(collection),
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
