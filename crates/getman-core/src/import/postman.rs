//! Importing collections, environments and globals exported from Postman
//! (collection format v2.0 and v2.1).
//!
//! Folders become directories, requests become YAML files in the original
//! order, and collection-level variables, authentication and scripts move
//! to the workspace or to the folder the collection is imported into.
//! Saved example responses become the requests' examples. Scripts keep
//! working: `pm` is an alias of `gm`.

mod auth;

use serde::Serialize;
use serde_json::{Map, Value};

use crate::error::{Error, Result};
use crate::model::{
    ApiKeyLocation, Auth, Body, Environment, Example, ExampleRequest, Folder, FormField, KeyValue,
    Request, Scripts, Settings, Vars,
};
use crate::workspace::{EnvironmentFiles, Workspace, slugify};

/// What an import created.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ImportReport {
    /// What was imported: `collection`, `environment` or `globals`.
    pub kind: String,
    /// Display name of what was imported.
    pub name: String,
    /// Path of the folder created for a collection, or the environment's
    /// id. Empty when a collection went into the workspace root.
    pub path: String,
    /// Requests created.
    pub requests: usize,
    /// Folders created.
    pub folders: usize,
    /// Saved example responses kept.
    pub examples: usize,
    /// Things that could not be carried over exactly.
    pub warnings: Vec<String>,
}

/// Imports an exported collection, environment or globals file.
///
/// A collection goes into a new folder inside `parent`, named after it,
/// unless `into_root` is set: then its settings become the workspace's and
/// its items the workspace's top level.
pub fn import(
    workspace: &Workspace,
    parent: &str,
    json: &str,
    into_root: bool,
) -> Result<ImportReport> {
    let value: Value = serde_json::from_str(json)
        .map_err(|e| Error::Invalid(format!("the file is not valid JSON: {e}")))?;
    if value.get("info").is_some() && value.get("item").is_some() {
        import_collection(workspace, parent, &value, into_root)
    } else if value.get("values").is_some_and(Value::is_array) {
        let scope = value["_postman_variable_scope"]
            .as_str()
            .unwrap_or("environment");
        if scope == "globals" {
            import_globals(workspace, &value)
        } else {
            import_environment(workspace, &value)
        }
    } else if value
        .get("collection")
        .is_some_and(|c| c.get("item").is_some())
    {
        // Exports fetched from the API wrap the collection.
        import_collection(workspace, parent, &value["collection"], into_root)
    } else {
        Err(Error::Invalid(
            "this is not a collection, environment or globals export; export a collection as v2.1"
                .into(),
        ))
    }
}

fn import_collection(
    workspace: &Workspace,
    parent: &str,
    value: &Value,
    into_root: bool,
) -> Result<ImportReport> {
    let info = &value["info"];
    let name = text(&info["name"]).unwrap_or_else(|| "Imported collection".into());
    if let Some(schema) = info["schema"].as_str()
        && !schema.contains("v2.")
    {
        return Err(Error::Invalid(format!(
            "only collection format v2.0 and v2.1 can be imported, not {schema}; export the collection again as v2.1"
        )));
    }
    let mut report = ImportReport {
        kind: "collection".into(),
        name: name.clone(),
        ..ImportReport::default()
    };
    let mut meta = Folder {
        name: name.clone(),
        description: description(&info["description"]),
        variables: variables(&value["variable"]),
        auth: auth(&value["auth"], &name, &mut report.warnings).unwrap_or(Auth::Inherit),
        scripts: scripts(&value["event"]),
        ..Folder::default()
    };

    let folder = if into_root {
        let mut existing = workspace.read_folder("")?;
        existing.name = meta.name;
        if !meta.description.is_empty() {
            existing.description = meta.description;
        }
        existing.variables.extend(meta.variables);
        if !meta.auth.is_inherit() {
            existing.auth = meta.auth;
        }
        if !meta.scripts.is_empty() {
            existing.scripts = meta.scripts;
        }
        workspace.write_folder("", &existing)?;
        String::new()
    } else {
        let path = workspace.create_folder(parent, &name)?;
        report.folders += 1;
        meta.order = Vec::new();
        workspace.write_folder(&path, &meta)?;
        path
    };
    report.path.clone_from(&folder);

    let items = value["item"].as_array().cloned().unwrap_or_default();
    import_items(workspace, &folder, &items, &mut report)?;
    Ok(report)
}

fn import_items(
    workspace: &Workspace,
    folder: &str,
    items: &[Value],
    report: &mut ImportReport,
) -> Result<()> {
    let mut order = Vec::new();
    for item in items {
        let name = text(&item["name"]).unwrap_or_else(|| "Untitled".into());
        if let Some(children) = item["item"].as_array() {
            let path = workspace.create_folder(folder, &name)?;
            report.folders += 1;
            let meta = Folder {
                name: name.clone(),
                description: description(&item["description"]),
                auth: auth(&item["auth"], &name, &mut report.warnings).unwrap_or(Auth::Inherit),
                scripts: scripts(&item["event"]),
                variables: variables(&item["variable"]),
                ..Folder::default()
            };
            workspace.write_folder(&path, &meta)?;
            import_items(workspace, &path, children, report)?;
            order.push(path.rsplit('/').next().unwrap_or(&path).to_owned());
        } else if item.get("request").is_some() {
            let request = request(item, &name, &mut report.warnings);
            let path = workspace.create_request(folder, &name, &request)?;
            report.requests += 1;
            report.examples += request.examples.len();
            if !request.examples.is_empty() {
                crate::examples::place_long_bodies(workspace, &path)?;
            }
            let file = path.rsplit('/').next().unwrap_or(&path);
            order.push(file.trim_end_matches(".yaml").to_owned());
        }
    }
    if order.len() > 1 {
        workspace.set_order(folder, order)?;
    }
    Ok(())
}

fn request(item: &Value, name: &str, warnings: &mut Vec<String>) -> Request {
    let source = &item["request"];
    // A request may be just its URL.
    if let Some(url) = source.as_str() {
        return Request {
            name: name.to_owned(),
            url: url.to_owned(),
            ..Request::default()
        };
    }
    let mut request = Request {
        name: name.to_owned(),
        method: text(&source["method"])
            .unwrap_or_else(|| "GET".into())
            .to_ascii_uppercase(),
        description: description(&source["description"]),
        headers: key_values(&source["header"]),
        scripts: scripts(&item["event"]),
        ..Request::default()
    };
    let (url, disabled, path_params) = url(&source["url"]);
    request.url = url;
    request.disabled_params = disabled;
    request.path_params = path_params;
    request.body = body(&source["body"], &request.headers, name, warnings);
    if let Some(auth) = auth(&source["auth"], name, warnings) {
        request.auth = auth;
    }
    request.settings = settings(&item["protocolProfileBehavior"]);
    request.examples = examples(&item["response"]);
    request
}

/// Saved responses, which Postman keeps in an item's `response` list.
fn examples(responses: &Value) -> Vec<Example> {
    let mut examples: Vec<Example> = Vec::new();
    for (n, response) in responses.as_array().into_iter().flatten().enumerate() {
        let mut name = text(&response["name"])
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| format!("Example {}", n + 1));
        let base = name.clone();
        let mut copy = 2;
        while examples.iter().any(|e| e.name == name) {
            name = format!("{base} ({copy})");
            copy += 1;
        }
        let headers = match &response["header"] {
            Value::String(raw) => raw
                .lines()
                .filter_map(|line| line.split_once(':'))
                .map(|(name, value)| KeyValue::new(name.trim(), value.trim()))
                .collect(),
            other => key_values(other),
        };
        examples.push(Example {
            name,
            status: response["code"]
                .as_u64()
                .and_then(|code| u16::try_from(code).ok())
                .unwrap_or(200),
            headers,
            body: text(&response["body"]).unwrap_or_default(),
            body_file: None,
            request: example_request(&response["originalRequest"]),
        });
    }
    examples
}

fn example_request(source: &Value) -> Option<ExampleRequest> {
    if source.is_null() {
        return None;
    }
    if let Some(url) = source.as_str() {
        return Some(ExampleRequest {
            url: url.to_owned(),
            ..ExampleRequest::default()
        });
    }
    let body = &source["body"];
    Some(ExampleRequest {
        method: text(&source["method"])
            .unwrap_or_default()
            .to_ascii_uppercase(),
        url: url(&source["url"]).0,
        headers: key_values(&source["header"])
            .into_iter()
            .filter(|h| h.enabled)
            .collect(),
        body: if body["mode"].as_str().unwrap_or("raw") == "raw" {
            text(&body["raw"]).unwrap_or_default()
        } else {
            String::new()
        },
    })
}

fn url(value: &Value) -> (String, Vec<KeyValue>, Vars) {
    if let Some(raw) = value.as_str() {
        return (raw.to_owned(), Vec::new(), Vars::new());
    }
    let raw = text(&value["raw"]).unwrap_or_else(|| {
        // Rebuild from the parts when the export has no raw form.
        let protocol = text(&value["protocol"])
            .map(|p| format!("{p}://"))
            .unwrap_or_default();
        let host = parts(&value["host"], ".");
        let port = text(&value["port"])
            .map(|p| format!(":{p}"))
            .unwrap_or_default();
        let path = parts(&value["path"], "/");
        format!(
            "{protocol}{host}{port}{}{path}",
            if path.is_empty() { "" } else { "/" }
        )
    });
    let mut disabled = Vec::new();
    let url = match value["query"].as_array() {
        Some(query) => {
            let base = raw.split('?').next().unwrap_or(&raw);
            let (base, fragment) = base.split_once('#').map_or((base, ""), |(b, f)| (b, f));
            let mut enabled = Vec::new();
            for param in query {
                let key = text(&param["key"]).unwrap_or_default();
                let value = text(&param["value"]);
                if param["disabled"].as_bool() == Some(true) {
                    disabled.push(KeyValue {
                        enabled: false,
                        ..KeyValue::new(key, value.unwrap_or_default())
                    });
                } else {
                    enabled.push(match value {
                        Some(value) => format!("{key}={value}"),
                        None => key,
                    });
                }
            }
            let mut url = base.to_owned();
            if !enabled.is_empty() {
                url.push('?');
                url.push_str(&enabled.join("&"));
            }
            if !fragment.is_empty() {
                url.push('#');
                url.push_str(fragment);
            }
            url
        }
        None => raw,
    };
    let path_params = value["variable"]
        .as_array()
        .map(|vars| {
            vars.iter()
                .filter_map(|v| Some((text(&v["key"])?, text(&v["value"]).unwrap_or_default())))
                .collect()
        })
        .unwrap_or_default();
    (url, disabled, path_params)
}

fn body(value: &Value, headers: &[KeyValue], name: &str, warnings: &mut Vec<String>) -> Body {
    if value.is_null() || value["disabled"].as_bool() == Some(true) {
        return Body::None;
    }
    match value["mode"].as_str().unwrap_or("raw") {
        "raw" => {
            let content = text(&value["raw"]).unwrap_or_default();
            if content.is_empty() {
                return Body::None;
            }
            let declared = headers
                .iter()
                .find(|h| h.enabled && h.name.eq_ignore_ascii_case("content-type"))
                .map(|h| h.value.to_ascii_lowercase())
                .unwrap_or_default();
            let language = value["options"]["raw"]["language"].as_str().unwrap_or("");
            match language {
                "json" => Body::Json { content },
                "xml" => Body::Xml { content },
                "html" => Body::Text {
                    content,
                    content_type: Some("text/html".into()),
                },
                "javascript" => Body::Text {
                    content,
                    content_type: Some("application/javascript".into()),
                },
                _ if declared.contains("json") => Body::Json { content },
                _ if declared.contains("xml") => Body::Xml { content },
                _ => Body::Text {
                    content,
                    content_type: None,
                },
            }
        }
        "urlencoded" => Body::Form {
            fields: key_values(&value["urlencoded"]),
        },
        "formdata" => Body::Multipart {
            fields: value["formdata"]
                .as_array()
                .map(|fields| {
                    fields
                        .iter()
                        .map(|field| {
                            let key = text(&field["key"]).unwrap_or_default();
                            let is_file = field["type"].as_str() == Some("file");
                            let src = match &field["src"] {
                                Value::Array(list) => list.first().and_then(text),
                                other => text(other),
                            };
                            FormField {
                                file: is_file
                                    .then(|| local_file(src.as_deref(), &key, name, warnings)),
                                value: if is_file {
                                    String::new()
                                } else {
                                    text(&field["value"]).unwrap_or_default()
                                },
                                content_type: text(&field["contentType"]),
                                enabled: field["disabled"].as_bool() != Some(true),
                                name: key,
                            }
                        })
                        .collect()
                })
                .unwrap_or_default(),
        },
        "file" => Body::Binary {
            file: local_file(
                text(&value["file"]["src"]).as_deref(),
                "body",
                name,
                warnings,
            ),
            content_type: None,
        },
        "graphql" => Body::Graphql {
            query: text(&value["graphql"]["query"]).unwrap_or_default(),
            variables: match &value["graphql"]["variables"] {
                Value::String(s) => s.clone(),
                Value::Null => String::new(),
                other => serde_json::to_string_pretty(other).unwrap_or_default(),
            },
        },
        other => {
            warnings.push(format!(
                "{name}: the body mode {other:?} is not supported and was dropped"
            ));
            Body::None
        }
    }
}

/// Files in exports are absolute paths on the exporting machine; keep the
/// file name and ask for the file to be placed in the workspace.
fn local_file(src: Option<&str>, field: &str, name: &str, warnings: &mut Vec<String>) -> String {
    let Some(src) = src.filter(|s| !s.is_empty()) else {
        warnings.push(format!(
            "{name}: the file for {field} was not selected in the export"
        ));
        return String::new();
    };
    let relative = !src.starts_with('/') && !src.contains(':') && !src.contains("..");
    if relative {
        return src.replace('\\', "/");
    }
    let file_name = src.rsplit(['/', '\\']).next().unwrap_or(src).to_owned();
    warnings.push(format!(
        "{name}: {field} uploaded {src}; copy that file into the workspace as {file_name}"
    ));
    file_name
}

fn auth(value: &Value, name: &str, warnings: &mut Vec<String>) -> Option<Auth> {
    let kind = value["type"].as_str()?;
    let params = |section: &str| -> Map<String, Value> {
        match &value[section] {
            // v2.1: [{ "key": ..., "value": ... }]
            Value::Array(list) => list
                .iter()
                .filter_map(|p| Some((p["key"].as_str()?.to_owned(), p["value"].clone())))
                .collect(),
            // v2.0: { "key": value }
            Value::Object(map) => map.clone(),
            _ => Map::new(),
        }
    };
    let get = |map: &Map<String, Value>, key: &str| map.get(key).and_then(text).unwrap_or_default();
    Some(match kind {
        "noauth" => Auth::None,
        "inherit" => Auth::Inherit,
        "basic" => {
            let p = params("basic");
            Auth::Basic {
                username: get(&p, "username"),
                password: get(&p, "password"),
            }
        }
        "bearer" => {
            let p = params("bearer");
            Auth::Bearer {
                token: get(&p, "token"),
                prefix: String::new(),
            }
        }
        "apikey" => {
            let p = params("apikey");
            Auth::ApiKey {
                key: get(&p, "key"),
                value: get(&p, "value"),
                location: if get(&p, "in") == "query" {
                    ApiKeyLocation::Query
                } else {
                    ApiKeyLocation::Header
                },
            }
        }
        "digest" => {
            let p = params("digest");
            Auth::Digest {
                username: get(&p, "username"),
                password: get(&p, "password"),
            }
        }
        "oauth2" => {
            let p = params("oauth2");
            match auth::oauth2(&p, name, warnings) {
                Some(oauth2) => oauth2,
                None => {
                    // A saved token would end up in a shared file, so it is
                    // never imported.
                    warnings.push(format!(
                        "{name}: OAuth 2.0 with the {:?} grant is not supported; it now sends {{{{accessToken}}}} as a Bearer token",
                        get(&p, "grant_type"),
                    ));
                    Auth::Bearer {
                        token: "{{accessToken}}".into(),
                        prefix: String::new(),
                    }
                }
            }
        }
        other => match auth::other(other, &params(other), name, warnings) {
            Some(auth) => auth,
            None => {
                warnings.push(format!(
                    "{name}: {other} authentication is not supported; set the credentials in headers or a pre-request script"
                ));
                return None;
            }
        },
    })
}

fn scripts(events: &Value) -> Scripts {
    let mut scripts = Scripts::default();
    for event in events.as_array().into_iter().flatten() {
        if event["disabled"].as_bool() == Some(true) {
            continue;
        }
        let code = match &event["script"]["exec"] {
            Value::Array(lines) => lines
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("\n"),
            Value::String(code) => code.clone(),
            _ => continue,
        };
        if code.trim().is_empty() {
            continue;
        }
        match event["listen"].as_str() {
            Some("prerequest") => scripts.pre_request = code,
            Some("test") => scripts.post_response = code,
            _ => {}
        }
    }
    scripts
}

fn settings(behavior: &Value) -> Settings {
    Settings {
        follow_redirects: behavior["followRedirects"].as_bool(),
        max_redirects: behavior["maxRedirects"]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok()),
        verify_tls: behavior["strictSSL"].as_bool(),
        ..Settings::default()
    }
}

fn variables(value: &Value) -> Vars {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter(|v| v["disabled"].as_bool() != Some(true))
        .filter_map(|v| Some((text(&v["key"])?, text(&v["value"]).unwrap_or_default())))
        .collect()
}

fn key_values(value: &Value) -> Vec<KeyValue> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            Some(KeyValue {
                name: text(&item["key"])?,
                value: text(&item["value"]).unwrap_or_default(),
                enabled: item["disabled"].as_bool() != Some(true),
                description: description(&item["description"]),
            })
        })
        .collect()
}

fn description(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Object(map) => map
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        _ => String::new(),
    }
}

fn parts(value: &Value, separator: &str) -> String {
    match value {
        Value::Array(list) => list
            .iter()
            .filter_map(text)
            .collect::<Vec<_>>()
            .join(separator),
        other => text(other).unwrap_or_default(),
    }
}

/// Any scalar as text.
fn text(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn environment_values(value: &Value) -> (Vars, Vec<String>) {
    let mut vars = Vars::new();
    let mut secrets = Vec::new();
    for entry in value["values"].as_array().into_iter().flatten() {
        if entry["enabled"].as_bool() == Some(false) {
            continue;
        }
        let Some(key) = text(&entry["key"]) else {
            continue;
        };
        if entry["type"].as_str() == Some("secret") {
            secrets.push(key.clone());
        }
        vars.insert(key, text(&entry["value"]).unwrap_or_default());
    }
    (vars, secrets)
}

fn import_environment(workspace: &Workspace, value: &Value) -> Result<ImportReport> {
    let name = text(&value["name"]).unwrap_or_else(|| "Imported".into());
    let (variables, secrets) = environment_values(value);
    let id = workspace.create_environment(&name)?;
    let shared = Environment {
        name: if slugify(&name) == id && name == id {
            String::new()
        } else {
            name.clone()
        },
        variables,
        secrets,
    };
    workspace.write_environment(
        &id,
        &EnvironmentFiles {
            shared,
            local: Vars::new(),
        },
    )?;
    Ok(ImportReport {
        kind: "environment".into(),
        name,
        path: id,
        ..ImportReport::default()
    })
}

fn import_globals(workspace: &Workspace, value: &Value) -> Result<ImportReport> {
    let (variables, _) = environment_values(value);
    let mut state = workspace.read_local_state()?;
    state.globals.extend(variables);
    workspace.write_local_state(&state)?;
    Ok(ImportReport {
        kind: "globals".into(),
        name: "Globals".into(),
        ..ImportReport::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const COLLECTION: &str = r#"{
      "info": {
        "name": "Pet Store",
        "description": "All about pets",
        "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
      },
      "variable": [
        { "key": "baseUrl", "value": "https://pets.example.com" },
        { "key": "off", "value": "x", "disabled": true }
      ],
      "auth": { "type": "bearer", "bearer": [{ "key": "token", "value": "{{token}}", "type": "string" }] },
      "event": [{ "listen": "prerequest", "script": { "exec": ["console.log('collection');"] } }],
      "item": [
        {
          "name": "Pets",
          "auth": { "type": "apikey", "apikey": [
            { "key": "key", "value": "X-Key" }, { "key": "value", "value": "{{key}}" }, { "key": "in", "value": "header" }
          ]},
          "item": [
            {
              "name": "List pets",
              "event": [{ "listen": "test", "script": { "exec": ["pm.test('ok', () => {", "  pm.response.to.have.status(200);", "});"] } }],
              "request": {
                "method": "GET",
                "header": [{ "key": "Accept", "value": "application/json" }, { "key": "X-Off", "value": "1", "disabled": true }],
                "url": {
                  "raw": "{{baseUrl}}/pets?limit=10&debug=1",
                  "host": ["{{baseUrl}}"], "path": ["pets"],
                  "query": [{ "key": "limit", "value": "10" }, { "key": "debug", "value": "1", "disabled": true }]
                }
              },
              "response": [
                {
                  "name": "Some pets",
                  "originalRequest": {
                    "method": "GET",
                    "header": [{ "key": "Accept", "value": "application/json" }],
                    "url": { "raw": "{{baseUrl}}/pets?limit=2", "host": ["{{baseUrl}}"], "path": ["pets"],
                      "query": [{ "key": "limit", "value": "2" }] }
                  },
                  "status": "OK",
                  "code": 200,
                  "_postman_previewlanguage": "json",
                  "header": [{ "key": "Content-Type", "value": "application/json" }],
                  "cookie": [],
                  "body": "[{\"id\": 1}, {\"id\": 2}]"
                },
                { "name": "Some pets", "code": 500, "header": null, "body": "oops" },
                { "code": 404 }
              ]
            },
            {
              "name": "Create pet",
              "request": {
                "method": "POST",
                "header": [],
                "body": { "mode": "raw", "raw": "{\"name\": \"Rex\"}", "options": { "raw": { "language": "json" } } },
                "url": { "raw": "{{baseUrl}}/pets/:id", "variable": [{ "key": "id", "value": "7" }] },
                "auth": { "type": "noauth" }
              },
              "protocolProfileBehavior": { "followRedirects": false, "strictSSL": false }
            }
          ]
        },
        {
          "name": "Upload",
          "request": {
            "method": "POST",
            "url": "{{baseUrl}}/upload",
            "body": { "mode": "formdata", "formdata": [
              { "key": "photo", "type": "file", "src": "/Users/ada/Desktop/rex.png" },
              { "key": "caption", "value": "Rex", "type": "text" }
            ]},
            "auth": { "type": "oauth2", "oauth2": [
              { "key": "grant_type", "value": "client_credentials" },
              { "key": "accessTokenUrl", "value": "https://id.example.com/token" },
              { "key": "clientId", "value": "app" },
              { "key": "client_authentication", "value": "body" }
            ]}
          }
        },
        {
          "name": "Login",
          "request": {
            "method": "POST",
            "url": "{{baseUrl}}/login",
            "body": { "mode": "urlencoded", "urlencoded": [{ "key": "user", "value": "ada" }] },
            "auth": { "type": "ntlm", "ntlm": [] }
          }
        }
      ]
    }"#;

    fn workspace() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::init(dir.path(), "Mine").unwrap();
        (dir, ws)
    }

    #[test]
    fn collections_become_folders_and_requests_in_order() {
        let (_dir, ws) = workspace();
        let report = import(&ws, "", COLLECTION, false).unwrap();
        assert_eq!(report.kind, "collection");
        assert_eq!(report.path, "pet-store");
        assert_eq!((report.folders, report.requests), (2, 4));

        let meta = ws.read_folder("pet-store").unwrap();
        assert_eq!(meta.variables.len(), 1);
        assert_eq!(meta.variables["baseUrl"], "https://pets.example.com");
        assert!(matches!(&meta.auth, Auth::Bearer { token, .. } if token == "{{token}}"));
        assert_eq!(meta.scripts.pre_request, "console.log('collection');");
        assert_eq!(meta.order, ["pets", "upload", "login"]);

        let pets = ws.read_folder("pet-store/pets").unwrap();
        assert!(
            matches!(&pets.auth, Auth::ApiKey { key, location: ApiKeyLocation::Header, .. } if key == "X-Key")
        );
        assert_eq!(pets.order, ["list-pets", "create-pet"]);

        let list = ws.read_request("pet-store/pets/list-pets.yaml").unwrap();
        assert_eq!(list.url, "{{baseUrl}}/pets?limit=10");
        assert_eq!(list.disabled_params[0].name, "debug");
        assert!(!list.headers[1].enabled);
        assert!(
            list.scripts
                .post_response
                .contains("pm.response.to.have.status(200);")
        );
        assert!(list.auth.is_inherit());

        let create = ws.read_request("pet-store/pets/create-pet.yaml").unwrap();
        assert_eq!(create.method, "POST");
        assert_eq!(create.path_params["id"], "7");
        assert!(matches!(create.body, Body::Json { .. }));
        assert_eq!(create.auth, Auth::None);
        assert_eq!(create.settings.follow_redirects, Some(false));
        assert_eq!(create.settings.verify_tls, Some(false));

        let upload = ws.read_request("pet-store/upload.yaml").unwrap();
        let Body::Multipart { fields } = &upload.body else {
            panic!()
        };
        assert_eq!(fields[0].file.as_deref(), Some("rex.png"));
        assert_eq!(fields[1].value, "Rex");
        let Auth::Oauth2(oauth) = &upload.auth else {
            panic!()
        };
        assert_eq!(oauth.client_auth, crate::model::ClientAuth::Body);

        let login = ws.read_request("pet-store/login.yaml").unwrap();
        assert!(matches!(login.body, Body::Form { .. }));
        assert!(login.auth.is_inherit());

        let warnings = report.warnings.join("\n");
        assert!(warnings.contains("rex.png"), "{warnings}");
        assert!(warnings.contains("ntlm"), "{warnings}");
        assert!(!warnings.contains("example"), "{warnings}");
    }

    #[test]
    fn saved_responses_become_examples() {
        let (_dir, ws) = workspace();
        let report = import(&ws, "", COLLECTION, false).unwrap();
        assert_eq!(report.examples, 3);
        let list = ws.read_request("pet-store/pets/list-pets.yaml").unwrap();
        let names: Vec<_> = list.examples.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Some pets", "Some pets (2)", "Example 3"]);

        let first = &list.examples[0];
        assert_eq!(first.status, 200);
        assert_eq!(first.headers[0].name, "Content-Type");
        assert_eq!(first.body, r#"[{"id": 1}, {"id": 2}]"#);
        let sent = first.request.as_ref().unwrap();
        assert_eq!(sent.method, "GET");
        assert_eq!(sent.url, "{{baseUrl}}/pets?limit=2");
        assert_eq!(sent.headers[0].value, "application/json");

        assert_eq!(list.examples[1].status, 500);
        assert!(list.examples[1].request.is_none());
        assert_eq!(list.examples[2].status, 404);
    }

    #[test]
    fn long_example_bodies_go_to_files() {
        let (dir, ws) = workspace();
        let body = "x".repeat(crate::examples::INLINE_LIMIT + 1);
        let json = serde_json::json!({
            "info": { "name": "Big", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json" },
            "item": [{ "name": "Dump", "request": { "method": "GET", "url": "https://x.test/dump" },
                "response": [{ "name": "All", "code": 200, "body": body }] }]
        });
        import(&ws, "", &json.to_string(), false).unwrap();
        let dump = ws.read_request("big/dump.yaml").unwrap();
        assert_eq!(dump.examples[0].body_file.as_deref(), Some("all.txt"));
        let file = dir.path().join("big/dump.examples/all.txt");
        assert_eq!(std::fs::read_to_string(file).unwrap(), body);
    }

    #[test]
    fn signed_and_token_auth_types_carry_over() {
        use crate::model::{
            HawkAlgorithm, JwtAlgorithm, OAuth1Location, OAuth1Method, OAuth2Grant, Pkce,
        };

        let (_dir, ws) = workspace();
        let fixture = include_str!("../../tests/fixtures/postman/auth.postman_collection.json");
        let report = import(&ws, "", fixture, false).unwrap();
        assert_eq!(report.requests, 11);
        let read = |file: &str| {
            ws.read_request(&format!("auth-types/{file}.yaml"))
                .unwrap()
                .auth
        };

        let Auth::AwsSigV4(aws) = read("aws-signature") else {
            panic!()
        };
        assert_eq!(aws.access_key, "{{awsAccessKey}}");
        assert_eq!(aws.session_token, "{{awsSessionToken}}");
        assert_eq!(
            (aws.region.as_str(), aws.service.as_str()),
            ("eu-west-1", "s3")
        );
        assert_eq!(aws.location, ApiKeyLocation::Query);

        let Auth::Oauth1(oauth1) = read("oauth-1-0") else {
            panic!()
        };
        assert_eq!(oauth1.signature_method, OAuth1Method::HmacSha256);
        assert_eq!(oauth1.consumer_key, "ck");
        assert_eq!(oauth1.token_secret, "{{tokenSecret}}");
        assert_eq!(oauth1.realm, "photos");
        assert!(oauth1.version.is_empty());
        assert!(oauth1.body_hash);
        assert_eq!(oauth1.location, OAuth1Location::Query);

        let Auth::Jwt(jwt) = read("jwt-bearer") else {
            panic!()
        };
        assert_eq!(jwt.algorithm, JwtAlgorithm::Rs256);
        assert_eq!(jwt.private_key, "{{jwtKey}}");
        assert_eq!(jwt.payload, "{\"sub\": \"{{userId}}\"}");
        assert_eq!(jwt.location, ApiKeyLocation::Query);
        assert_eq!(jwt.param, "jwt");
        assert!(jwt.prefix.is_empty());

        let Auth::Hawk(hawk) = read("hawk") else {
            panic!()
        };
        assert_eq!(
            (hawk.id.as_str(), hawk.key.as_str()),
            ("dh37fgj492je", "{{hawkKey}}")
        );
        assert_eq!(hawk.algorithm, HawkAlgorithm::Sha1);
        assert_eq!(hawk.ext, "some-app-ext-data");
        assert!(hawk.payload_hash);

        let Auth::Edgegrid(edgegrid) = read("akamai-edgegrid") else {
            panic!()
        };
        assert_eq!(edgegrid.client_token, "akab-client");
        assert_eq!(edgegrid.headers_to_sign, ["X-Test1", "X-Test2"]);
        assert_eq!(edgegrid.base_url, "https://akab-host.luna.akamaiapis.net");

        let Auth::Asap(asap) = read("atlassian-asap") else {
            panic!()
        };
        assert_eq!(asap.issuer, "billing");
        assert_eq!(asap.key_id, "billing/key1");
        assert_eq!(asap.algorithm, Some(JwtAlgorithm::Es256));
        assert_eq!(asap.expires_in, Some(900));

        let Auth::Oauth2(code) = read("oauth-2-0-code-with-pkce") else {
            panic!()
        };
        assert_eq!(code.grant, OAuth2Grant::AuthorizationCode);
        assert_eq!(code.pkce, Pkce::S256);
        assert_eq!(code.auth_url, "https://id.example.com/authorize");
        assert_eq!(code.callback_url, "http://localhost:5000/callback");
        assert_eq!(code.auth_params[0].name, "prompt");
        assert!(!code.auth_params[1].enabled);
        assert_eq!(code.token_params[0].name, "tenant");
        assert_eq!(code.token_in, ApiKeyLocation::Query);
        assert_eq!(code.prefix, "Token");
        assert_eq!(code.client_auth, crate::model::ClientAuth::Body);

        let Auth::Oauth2(plain) = read("oauth-2-0-code") else {
            panic!()
        };
        assert_eq!(plain.grant, OAuth2Grant::AuthorizationCode);
        assert_eq!(plain.pkce, Pkce::Off);
        assert!(plain.callback_url.is_empty());

        let Auth::Oauth2(implicit) = read("oauth-2-0-implicit") else {
            panic!()
        };
        assert_eq!(implicit.grant, OAuth2Grant::Implicit);
        let Auth::Oauth2(password) = read("oauth-2-0-password") else {
            panic!()
        };
        assert_eq!(password.grant, OAuth2Grant::Password);
        assert_eq!(password.username, "ada");
        let Auth::Oauth2(client) = read("oauth-2-0-client-credentials") else {
            panic!()
        };
        assert_eq!(client.grant, OAuth2Grant::ClientCredentials);
        assert_eq!(client.audience, "https://api.example.com");

        let warnings = report.warnings.join("\n");
        assert!(warnings.contains("oauth.pstmn.io"), "{warnings}");
        assert!(
            warnings.contains("saved OAuth 2.0 access token"),
            "{warnings}"
        );
        assert!(!warnings.contains("not supported"), "{warnings}");
        for file in std::fs::read_dir(ws.root().join("auth-types")).unwrap() {
            let text = std::fs::read_to_string(file.unwrap().path()).unwrap();
            assert!(!text.contains("saved-access-token"), "{text}");
        }
    }

    #[test]
    fn collections_can_become_the_workspace() {
        let (_dir, ws) = workspace();
        let report = import(&ws, "", COLLECTION, true).unwrap();
        assert_eq!(report.path, "");
        let meta = ws.read_folder("").unwrap();
        assert_eq!(meta.name, "Pet Store");
        assert_eq!(meta.variables["baseUrl"], "https://pets.example.com");
        assert!(ws.read_request("pets/list-pets.yaml").is_ok());
    }

    #[test]
    fn version_2_0_auth_objects_are_understood() {
        let (_dir, ws) = workspace();
        let json = r#"{"info": {"name": "Old", "schema": "https://schema.getpostman.com/json/collection/v2.0.0/collection.json"},
          "item": [{"name": "Me", "request": {"url": "https://x.test/me", "method": "GET",
            "auth": {"type": "basic", "basic": {"username": "ada", "password": "pw"}}}}]}"#;
        import(&ws, "", json, false).unwrap();
        let me = ws.read_request("old/me.yaml").unwrap();
        assert_eq!(
            me.auth,
            Auth::Basic {
                username: "ada".into(),
                password: "pw".into()
            }
        );
    }

    #[test]
    fn environments_keep_secrets_local() {
        let (_dir, ws) = workspace();
        let json = r#"{"name": "Production", "_postman_variable_scope": "environment", "values": [
            {"key": "baseUrl", "value": "https://api.example.com", "enabled": true},
            {"key": "token", "value": "s3cret", "type": "secret", "enabled": true},
            {"key": "old", "value": "x", "enabled": false}]}"#;
        let report = import(&ws, "", json, false).unwrap();
        assert_eq!(report.kind, "environment");
        let files = ws.read_environment(&report.path).unwrap();
        assert_eq!(files.shared.variables["token"], "");
        assert_eq!(files.local["token"], "s3cret");
        assert!(!files.shared.variables.contains_key("old"));

        let globals = r#"{"name": "Globals", "_postman_variable_scope": "globals", "values": [{"key": "g", "value": "1"}]}"#;
        import(&ws, "", globals, false).unwrap();
        assert_eq!(ws.read_local_state().unwrap().globals["g"], "1");
    }

    #[test]
    fn other_files_are_refused() {
        let (_dir, ws) = workspace();
        assert!(import(&ws, "", "{}", false).is_err());
        assert!(import(&ws, "", "not json", false).is_err());
        let v1 = r#"{"info": {"name": "x", "schema": "https://schema.getpostman.com/json/collection/v1.0.0/collection.json"}, "item": []}"#;
        assert!(
            import(&ws, "", v1, false)
                .unwrap_err()
                .to_string()
                .contains("v2.1")
        );
    }
}
