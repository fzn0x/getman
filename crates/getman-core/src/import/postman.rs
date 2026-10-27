//! Importing collections, environments and globals exported from Postman
//! (collection format v2.0 and v2.1).
//!
//! Folders become directories, requests become YAML files in the original
//! order, and collection-level variables, authentication and scripts move
//! to the workspace or to the folder the collection is imported into.
//! Scripts keep working: `pm` is an alias of `gm`.

use serde::Serialize;
use serde_json::{Map, Value};

use crate::error::{Error, Result};
use crate::model::{
    ApiKeyLocation, Auth, Body, ClientAuth, Environment, Folder, FormField, KeyValue, OAuth2,
    OAuth2Grant, Request, Scripts, Settings, Vars,
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

    let responses = count_examples(&items);
    if responses > 0 {
        report.warnings.push(format!(
            "{responses} saved example responses were not imported; Getman does not keep examples"
        ));
    }
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
    request
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
            let grant = match get(&p, "grant_type").as_str() {
                "client_credentials" => Some(OAuth2Grant::ClientCredentials),
                "password_credentials" => Some(OAuth2Grant::Password),
                _ => None,
            };
            match grant {
                Some(grant) => Auth::Oauth2(OAuth2 {
                    grant,
                    token_url: get(&p, "accessTokenUrl"),
                    client_id: get(&p, "clientId"),
                    client_secret: get(&p, "clientSecret"),
                    scope: get(&p, "scope"),
                    audience: get(&p, "audience"),
                    username: get(&p, "username"),
                    password: get(&p, "password"),
                    client_auth: if get(&p, "client_authentication") == "body" {
                        ClientAuth::Body
                    } else {
                        ClientAuth::Basic
                    },
                }),
                None => {
                    let token = get(&p, "accessToken");
                    warnings.push(format!(
                        "{name}: OAuth 2.0 with the {:?} grant needs a browser; it now sends {} as a Bearer token",
                        get(&p, "grant_type"),
                        if token.is_empty() { "{{accessToken}}" } else { "the saved token" }
                    ));
                    Auth::Bearer {
                        token: if token.is_empty() {
                            "{{accessToken}}".into()
                        } else {
                            token
                        },
                        prefix: String::new(),
                    }
                }
            }
        }
        other => {
            warnings.push(format!(
                "{name}: {other} authentication is not supported; set the credentials in headers or a pre-request script"
            ));
            return None;
        }
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

fn count_examples(items: &[Value]) -> usize {
    items
        .iter()
        .map(|item| {
            item["response"].as_array().map_or(0, Vec::len)
                + item["item"]
                    .as_array()
                    .map_or(0, |children| count_examples(children))
        })
        .sum()
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
              "response": [{ "name": "example" }]
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
            "auth": { "type": "awsv4", "awsv4": [] }
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
        assert_eq!(oauth.client_auth, ClientAuth::Body);

        let login = ws.read_request("pet-store/login.yaml").unwrap();
        assert!(matches!(login.body, Body::Form { .. }));
        assert!(login.auth.is_inherit());

        let warnings = report.warnings.join("\n");
        assert!(warnings.contains("rex.png"), "{warnings}");
        assert!(warnings.contains("awsv4"), "{warnings}");
        assert!(warnings.contains("1 saved example"), "{warnings}");
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
