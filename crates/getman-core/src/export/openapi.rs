//! Writing an OpenAPI 3.1 document from a folder or the workspace.
//!
//! The document is inferred from the requests, so it is a starting point
//! rather than a full description: paths come from URLs relative to
//! `{{baseUrl}}`, with `:name` and `{{name}}` segments as path
//! parameters; query and header parameters come with their values as
//! examples; bodies are examples of their media type; tags are the
//! top-level folders; authentication becomes security schemes; and a
//! `status == N` check names the expected response.

use indexmap::IndexMap;
use serde_json::{Map, Value, json};

use super::{Exported, chain_of, items_of};
use crate::checks::Check;
use crate::error::Result;
use crate::model::{ApiKeyLocation, Auth, Body, Folder, KeyValue, OAuth2Grant, Request, Vars};
use crate::workspace::{Node, Workspace, slugify};

const METHODS: &[&str] = &[
    "get", "put", "post", "delete", "options", "head", "patch", "trace",
];

/// Writes the OpenAPI document for a folder, or the workspace for `""`,
/// as YAML when `yaml` is set and JSON otherwise.
pub fn document(workspace: &Workspace, folder: &str, yaml: bool) -> Result<Exported> {
    let chain = chain_of(workspace, folder)?;
    let meta = chain.last().cloned().unwrap_or_default();
    let title = if folder.is_empty() {
        workspace.name()
    } else {
        meta.name.clone()
    };
    let mut writer = Writer {
        workspace,
        warnings: Vec::new(),
        paths: Map::new(),
        schemes: IndexMap::new(),
        operation_ids: Vec::new(),
        tags: Vec::new(),
    };
    let root_auth = effective_auth(&chain);
    let mut variables = Vars::new();
    for level in &chain {
        variables.extend(level.variables.clone());
    }
    writer.walk(&items_of(workspace, folder)?, &chain, None, &root_auth)?;

    let mut doc = Map::new();
    doc.insert("openapi".into(), "3.1.0".into());
    let mut info = Map::new();
    info.insert("title".into(), title.clone().into());
    info.insert("version".into(), "1.0.0".into());
    if !meta.description.is_empty() {
        info.insert("description".into(), meta.description.clone().into());
    }
    doc.insert("info".into(), Value::Object(info));
    doc.insert(
        "servers".into(),
        Value::Array(servers(workspace, &variables, &mut writer.warnings)),
    );
    if let Some(requirement) = writer.security(&root_auth) {
        doc.insert("security".into(), requirement);
    }
    if !writer.tags.is_empty() {
        let tags: Vec<Value> = writer
            .tags
            .iter()
            .map(|(name, description)| {
                if description.is_empty() {
                    json!({ "name": name })
                } else {
                    json!({ "name": name, "description": description })
                }
            })
            .collect();
        doc.insert("tags".into(), Value::Array(tags));
    }
    doc.insert(
        "paths".into(),
        Value::Object(std::mem::take(&mut writer.paths)),
    );
    if !writer.schemes.is_empty() {
        let schemes: Map<String, Value> = writer
            .schemes
            .iter()
            .map(|(name, scheme)| (name.clone(), scheme.clone()))
            .collect();
        doc.insert(
            "components".into(),
            json!({ "securitySchemes": Value::Object(schemes) }),
        );
    }
    let doc = Value::Object(doc);
    let content = if yaml {
        serde_yaml_ng::to_string(&doc).unwrap_or_default()
    } else {
        serde_json::to_string_pretty(&doc).unwrap_or_default()
    };
    Ok(Exported {
        file_name: format!(
            "{}.openapi.{}",
            slugify(&title),
            if yaml { "yaml" } else { "json" }
        ),
        content,
        warnings: writer.warnings,
    })
}

/// Servers: the value of `baseUrl` where it is set, and in each
/// environment that sets it.
fn servers(workspace: &Workspace, variables: &Vars, warnings: &mut Vec<String>) -> Vec<Value> {
    let mut list: Vec<Value> = Vec::new();
    let mut add = |url: &str, description: Option<&str>| {
        let url = url.trim().trim_end_matches('/');
        if url.is_empty() || url.contains("{{") || list.iter().any(|s| s["url"] == url) {
            return;
        }
        let mut server = Map::new();
        server.insert("url".into(), url.into());
        if let Some(description) = description {
            server.insert("description".into(), description.into());
        }
        list.push(Value::Object(server));
    };
    if let Some(url) = variables.get("baseUrl") {
        add(url, None);
    }
    for summary in workspace.environments().unwrap_or_default() {
        if let Ok(files) = workspace.read_environment(&summary.id)
            && let Some(url) = files.shared.variables.get("baseUrl")
        {
            add(url, Some(&summary.name));
        }
    }
    if list.is_empty() {
        warnings.push(
            "no baseUrl value was found, so the document lists the server as /; add the API's address under servers"
                .into(),
        );
        list.push(json!({ "url": "/" }));
    }
    list
}

/// The nearest authentication that is not `inherit`.
fn effective_auth(chain: &[Folder]) -> Auth {
    chain
        .iter()
        .rev()
        .map(|f| &f.auth)
        .find(|a| !a.is_inherit())
        .cloned()
        .unwrap_or(Auth::Inherit)
}

struct Writer<'a> {
    workspace: &'a Workspace,
    warnings: Vec<String>,
    paths: Map<String, Value>,
    /// Security schemes by name.
    schemes: IndexMap<String, Value>,
    operation_ids: Vec<String>,
    /// Tags and their descriptions, in order.
    tags: Vec<(String, String)>,
}

impl Writer<'_> {
    fn walk(
        &mut self,
        nodes: &[Node],
        chain: &[Folder],
        tag: Option<&str>,
        root_auth: &Auth,
    ) -> Result<()> {
        for node in nodes {
            match node {
                Node::Folder {
                    path,
                    name,
                    children,
                } => {
                    let folder = self.workspace.read_folder(path)?;
                    let tag = match tag {
                        Some(tag) => tag.to_owned(),
                        None => {
                            self.tags.push((name.clone(), folder.description.clone()));
                            name.clone()
                        }
                    };
                    let mut inner = chain.to_vec();
                    inner.push(folder);
                    self.walk(children, &inner, Some(&tag), root_auth)?;
                }
                Node::Request {
                    path, error: None, ..
                } => {
                    let request = self.workspace.read_request(path)?;
                    self.operation(&request, chain, tag, root_auth);
                }
                Node::Request { .. } => {}
            }
        }
        Ok(())
    }

    fn operation(
        &mut self,
        request: &Request,
        chain: &[Folder],
        tag: Option<&str>,
        root_auth: &Auth,
    ) {
        let name = request.name.as_str();
        let method = request.method.trim().to_ascii_lowercase();
        if !METHODS.contains(&method.as_str()) {
            self.warnings.push(format!(
                "{name}: OpenAPI 3.1 has no {} method; the request was left out",
                request.method
            ));
            return;
        }
        let (path, path_names, query) = split_url(&request.url, &mut self.warnings, name);
        if self
            .paths
            .get(&path)
            .is_some_and(|item| item.get(&method).is_some())
        {
            self.warnings.push(format!(
                "{name}: {} {path} is already described by another request; it was left out",
                request.method.to_ascii_uppercase()
            ));
            return;
        }

        let mut parameters = Vec::new();
        for parameter in &path_names {
            let mut entry = json!({ "name": parameter, "in": "path", "required": true, "schema": { "type": "string" } });
            if let Some(value) = request
                .path_params
                .get(parameter)
                .filter(|v| !v.contains("{{") && !v.is_empty())
            {
                entry["example"] = value.clone().into();
            }
            parameters.push(entry);
        }
        for (key, value) in query.iter().chain(
            request
                .disabled_params
                .iter()
                .map(|p| (p.name.clone(), p.value.clone()))
                .collect::<Vec<_>>()
                .iter(),
        ) {
            if parameters
                .iter()
                .any(|p| p["name"] == key.as_str() && p["in"] == "query")
            {
                continue;
            }
            parameters.push(parameter(key, "query", value, ""));
        }
        let mut headers: Vec<KeyValue> = Vec::new();
        for level in chain
            .iter()
            .map(|f| &f.headers)
            .chain(std::iter::once(&request.headers))
        {
            for header in level.iter().filter(|h| h.enabled) {
                headers.retain(|h| !h.name.eq_ignore_ascii_case(&header.name));
                headers.push(header.clone());
            }
        }
        for header in &headers {
            let lower = header.name.to_ascii_lowercase();
            if matches!(lower.as_str(), "content-type" | "accept" | "authorization") {
                continue;
            }
            parameters.push(parameter(
                &header.name,
                "header",
                &header.value,
                &header.description,
            ));
        }

        let mut operation = Map::new();
        if let Some(tag) = tag {
            operation.insert("tags".into(), json!([tag]));
        }
        operation.insert("summary".into(), name.into());
        if !request.description.is_empty() {
            operation.insert("description".into(), request.description.clone().into());
        }
        operation.insert("operationId".into(), self.operation_id(name).into());
        if !parameters.is_empty() {
            operation.insert("parameters".into(), Value::Array(parameters));
        }
        if let Some(body) = request_body(&request.body) {
            operation.insert("requestBody".into(), body);
        }
        let mut auth = request.auth.clone();
        if auth.is_inherit() {
            auth = effective_auth(chain);
        }
        if auth != *root_auth
            && let Some(requirement) = self.security(&auth)
        {
            operation.insert("security".into(), requirement);
        }
        let statuses: Vec<String> = request
            .checks
            .iter()
            .filter_map(|line| Check::parse(line).ok())
            .filter(|c| c.target == "status" && c.operator == "==")
            .map(|c| c.value)
            .collect();
        if !statuses.is_empty() {
            let responses: Map<String, Value> = statuses
                .into_iter()
                .map(|status| {
                    (
                        status,
                        json!({ "description": "As the request's checks expect" }),
                    )
                })
                .collect();
            operation.insert("responses".into(), Value::Object(responses));
        }
        let item = self
            .paths
            .entry(path)
            .or_insert_with(|| Value::Object(Map::new()));
        if let Some(item) = item.as_object_mut() {
            item.insert(method, Value::Object(operation));
        }
    }

    /// A unique operationId in camelCase, from the request's name.
    fn operation_id(&mut self, name: &str) -> String {
        let mut id = String::new();
        for (i, word) in slugify(name).split('-').enumerate() {
            let mut chars = word.chars();
            if let Some(first) = chars.next() {
                if i == 0 {
                    id.push(first);
                } else {
                    id.extend(first.to_uppercase());
                }
                id.push_str(chars.as_str());
            }
        }
        let mut unique = id.clone();
        let mut n = 2;
        while self.operation_ids.contains(&unique) {
            unique = format!("{id}{n}");
            n += 1;
        }
        self.operation_ids.push(unique.clone());
        unique
    }

    /// A security requirement for some authentication, registering its
    /// scheme. `[]` for none; nothing when there is no authentication.
    fn security(&mut self, auth: &Auth) -> Option<Value> {
        let (base, scheme, scopes) = match auth {
            Auth::Inherit => return None,
            Auth::None => return Some(json!([])),
            Auth::Basic { .. } => (
                "basicAuth",
                json!({ "type": "http", "scheme": "basic" }),
                Vec::new(),
            ),
            Auth::Digest { .. } => (
                "digestAuth",
                json!({ "type": "http", "scheme": "digest" }),
                Vec::new(),
            ),
            Auth::Bearer { prefix, .. } => {
                let scheme = if prefix.trim().is_empty() {
                    "bearer".to_owned()
                } else {
                    prefix.trim().to_ascii_lowercase()
                };
                (
                    "bearerAuth",
                    json!({ "type": "http", "scheme": scheme }),
                    Vec::new(),
                )
            }
            Auth::Jwt(_) | Auth::Asap(_) => (
                "jwtAuth",
                json!({ "type": "http", "scheme": "bearer", "bearerFormat": "JWT" }),
                Vec::new(),
            ),
            // OpenAPI has no security scheme for signing each request.
            Auth::AwsSigV4(_) | Auth::Oauth1(_) | Auth::Hawk(_) | Auth::Edgegrid(_) => {
                return None;
            }
            Auth::ApiKey { key, location, .. } => (
                "apiKeyAuth",
                json!({
                    "type": "apiKey",
                    "in": if *location == ApiKeyLocation::Query { "query" } else { "header" },
                    "name": key,
                }),
                Vec::new(),
            ),
            Auth::Oauth2(oauth) => {
                let scopes: Vec<String> =
                    oauth.scope.split_whitespace().map(str::to_owned).collect();
                let scope_map: Map<String, Value> = scopes
                    .iter()
                    .map(|s| (s.clone(), Value::from("")))
                    .collect();
                let (flow, settings) = match oauth.grant {
                    OAuth2Grant::ClientCredentials => (
                        "clientCredentials",
                        json!({ "tokenUrl": oauth.token_url, "scopes": Value::Object(scope_map) }),
                    ),
                    OAuth2Grant::Password => (
                        "password",
                        json!({ "tokenUrl": oauth.token_url, "scopes": Value::Object(scope_map) }),
                    ),
                    OAuth2Grant::AuthorizationCode => (
                        "authorizationCode",
                        json!({
                            "authorizationUrl": oauth.auth_url,
                            "tokenUrl": oauth.token_url,
                            "scopes": Value::Object(scope_map),
                        }),
                    ),
                    OAuth2Grant::Implicit => (
                        "implicit",
                        json!({ "authorizationUrl": oauth.auth_url, "scopes": Value::Object(scope_map) }),
                    ),
                };
                let mut flows = Map::new();
                flows.insert(flow.into(), settings);
                (
                    "oauth2",
                    json!({ "type": "oauth2", "flows": Value::Object(flows) }),
                    scopes,
                )
            }
        };
        // The same settings share a name; different ones get a number.
        let name = match self.schemes.iter().find(|(_, s)| **s == scheme) {
            Some((name, _)) => name.clone(),
            None => {
                let mut name = base.to_owned();
                let mut n = 2;
                while self.schemes.contains_key(&name) {
                    name = format!("{base}{n}");
                    n += 1;
                }
                self.schemes.insert(name.clone(), scheme);
                name
            }
        };
        let mut requirement = Map::new();
        requirement.insert(name, json!(scopes));
        Some(json!([Value::Object(requirement)]))
    }
}

fn parameter(name: &str, location: &str, value: &str, description: &str) -> Value {
    let mut entry = Map::new();
    entry.insert("name".into(), name.into());
    entry.insert("in".into(), location.into());
    if !description.is_empty() {
        entry.insert("description".into(), description.into());
    }
    entry.insert("schema".into(), json!({ "type": "string" }));
    if !value.is_empty() && !value.contains("{{") {
        entry.insert("example".into(), value.into());
    }
    Value::Object(entry)
}

/// The path template, its parameter names and the query pairs of a URL.
/// The part before the path, `{{baseUrl}}` or a scheme and host, is
/// dropped; it belongs to the servers.
fn split_url(
    url: &str,
    warnings: &mut Vec<String>,
    name: &str,
) -> (String, Vec<String>, Vec<(String, String)>) {
    let url = url.trim();
    let url = url.split('#').next().unwrap_or(url);
    let (base, query) = url.split_once('?').unwrap_or((url, ""));
    let path = if let Some(rest) = base.strip_prefix("{{baseUrl}}") {
        rest.to_owned()
    } else if let Some((_, rest)) = base.split_once("://") {
        rest.find('/')
            .map_or(String::new(), |at| rest[at..].to_owned())
    } else if base.starts_with("{{") {
        let end = base.find("}}").map_or(base.len(), |at| at + 2);
        warnings.push(format!(
            "{name}: its URL starts with {} rather than {{{{baseUrl}}}}; the path is taken as relative to the servers",
            &base[..end]
        ));
        base[end..].to_owned()
    } else {
        base.find('/')
            .map_or(String::new(), |at| base[at..].to_owned())
    };
    let mut names = Vec::new();
    let segments: Vec<String> = path
        .split('/')
        .map(|segment| {
            if let Some(param) = segment.strip_prefix(':').filter(|p| !p.is_empty()) {
                names.push(param.to_owned());
                return format!("{{{param}}}");
            }
            // `{{name}}` inside a segment is a parameter too.
            let mut out = String::new();
            let mut rest = segment;
            while let Some(start) = rest.find("{{") {
                let Some(end) = rest[start..].find("}}") else {
                    break;
                };
                let param = rest[start + 2..start + end].trim();
                out.push_str(&rest[..start]);
                out.push_str(&format!("{{{param}}}"));
                if !names.iter().any(|n| n == param) {
                    names.push(param.to_owned());
                }
                rest = &rest[start + end + 2..];
            }
            out.push_str(rest);
            out
        })
        .collect();
    let mut path = segments.join("/");
    if !path.starts_with('/') {
        path.insert(0, '/');
    }
    let query = query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (k.to_owned(), v.to_owned())
        })
        .collect();
    (path, names, query)
}

/// A request body, with its content as an example.
fn request_body(body: &Body) -> Option<Value> {
    let (media, schema, example) = match body {
        Body::None => return None,
        Body::Json { content } => (
            "application/json".to_owned(),
            None,
            Some(serde_json::from_str(content).unwrap_or_else(|_| Value::from(content.as_str()))),
        ),
        Body::Xml { content } => (
            "application/xml".to_owned(),
            None,
            Some(content.as_str().into()),
        ),
        Body::Text {
            content,
            content_type,
        } => (
            content_type.clone().unwrap_or_else(|| "text/plain".into()),
            None,
            Some(content.as_str().into()),
        ),
        Body::Form { fields } => {
            let example: Map<String, Value> = fields
                .iter()
                .filter(|f| f.enabled)
                .map(|f| (f.name.clone(), Value::from(f.value.as_str())))
                .collect();
            let properties: Map<String, Value> = example
                .keys()
                .map(|k| (k.clone(), json!({ "type": "string" })))
                .collect();
            (
                "application/x-www-form-urlencoded".to_owned(),
                Some(json!({ "type": "object", "properties": Value::Object(properties) })),
                Some(Value::Object(example)),
            )
        }
        Body::Multipart { fields } => {
            let mut properties = Map::new();
            let mut example = Map::new();
            for field in fields.iter().filter(|f| f.enabled) {
                if field.file.is_some() {
                    let media = field
                        .content_type
                        .clone()
                        .unwrap_or_else(|| "application/octet-stream".into());
                    properties.insert(
                        field.name.clone(),
                        json!({ "type": "string", "contentMediaType": media }),
                    );
                } else {
                    properties.insert(field.name.clone(), json!({ "type": "string" }));
                    example.insert(field.name.clone(), field.value.clone().into());
                }
            }
            (
                "multipart/form-data".to_owned(),
                Some(json!({ "type": "object", "properties": Value::Object(properties) })),
                (!example.is_empty()).then_some(Value::Object(example)),
            )
        }
        Body::Binary { content_type, .. } => {
            let media = content_type
                .clone()
                .unwrap_or_else(|| "application/octet-stream".into());
            (
                media.clone(),
                Some(json!({ "contentMediaType": media })),
                None,
            )
        }
        Body::Graphql { query, variables } => {
            let mut example = json!({ "query": query });
            if !variables.trim().is_empty() {
                example["variables"] = serde_json::from_str(variables)
                    .unwrap_or_else(|_| Value::from(variables.as_str()));
            }
            ("application/json".to_owned(), None, Some(example))
        }
    };
    let mut content = Map::new();
    if let Some(schema) = schema {
        content.insert("schema".into(), schema);
    }
    if let Some(example) = example {
        content.insert("example".into(), example);
    }
    let mut media_types = Map::new();
    media_types.insert(media, Value::Object(content));
    Some(json!({ "content": Value::Object(media_types) }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::FormField;

    fn workspace() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::init(dir.path(), "Shop").unwrap();
        let mut root = ws.read_folder("").unwrap();
        root.variables
            .insert("baseUrl".into(), "https://shop.example.com/v1/".into());
        root.auth = Auth::Bearer {
            token: "{{token}}".into(),
            prefix: String::new(),
        };
        ws.write_folder("", &root).unwrap();
        let orders = ws.create_folder("", "Orders").unwrap();
        let mut folder = ws.read_folder(&orders).unwrap();
        folder.description = "Buying things".into();
        ws.write_folder(&orders, &folder).unwrap();
        ws.create_request(
            &orders,
            "Get order",
            &Request {
                url: "{{baseUrl}}/orders/:id/lines/{{line}}?expand=items".into(),
                path_params: [("id".to_owned(), "7".to_owned())].into_iter().collect(),
                headers: vec![
                    KeyValue::new("X-Trace", "abc"),
                    KeyValue::new("Accept", "application/json"),
                ],
                checks: vec!["status == 200".into()],
                ..Request::default()
            },
        )
        .unwrap();
        ws.create_request(
            &orders,
            "Create order",
            &Request {
                method: "POST".into(),
                url: "{{baseUrl}}/orders".into(),
                body: Body::Json {
                    content: "{\"sku\": \"BOOK-42\"}".into(),
                },
                auth: Auth::ApiKey {
                    key: "X-Key".into(),
                    value: "{{key}}".into(),
                    location: ApiKeyLocation::Header,
                },
                ..Request::default()
            },
        )
        .unwrap();
        ws.create_request(
            "",
            "Upload",
            &Request {
                method: "PUT".into(),
                url: "https://files.example.com/upload".into(),
                body: Body::Multipart {
                    fields: vec![
                        FormField {
                            name: "photo".into(),
                            value: String::new(),
                            file: Some("cat.png".into()),
                            content_type: Some("image/png".into()),
                            enabled: true,
                        },
                        FormField {
                            name: "caption".into(),
                            value: "Tom".into(),
                            file: None,
                            content_type: None,
                            enabled: true,
                        },
                    ],
                },
                auth: Auth::None,
                ..Request::default()
            },
        )
        .unwrap();
        ws.create_request(
            "",
            "Purge",
            &Request {
                method: "PURGE".into(),
                url: "{{baseUrl}}/cache".into(),
                ..Request::default()
            },
        )
        .unwrap();
        (dir, ws)
    }

    #[test]
    fn requests_become_paths_and_operations() {
        let (_dir, ws) = workspace();
        let exported = document(&ws, "", false).unwrap();
        assert_eq!(exported.file_name, "shop.openapi.json");
        let doc: Value = serde_json::from_str(&exported.content).unwrap();
        assert_eq!(doc["openapi"], "3.1.0");
        assert_eq!(doc["servers"][0]["url"], "https://shop.example.com/v1");
        assert_eq!(doc["security"][0]["bearerAuth"], json!([]));
        assert_eq!(doc["tags"][0]["description"], "Buying things");

        let get = &doc["paths"]["/orders/{id}/lines/{line}"]["get"];
        assert_eq!(get["tags"][0], "Orders");
        assert_eq!(get["operationId"], "getOrder");
        let params = get["parameters"].as_array().unwrap();
        assert_eq!(params[0]["name"], "id");
        assert_eq!(params[0]["example"], "7");
        assert_eq!(params[1]["name"], "line");
        assert_eq!(params[2]["in"], "query");
        assert_eq!(params[2]["example"], "items");
        assert_eq!(params[3]["name"], "X-Trace");
        assert_eq!(params.len(), 4);
        assert!(get["responses"]["200"].is_object());
        assert!(get.get("security").is_none());

        let create = &doc["paths"]["/orders"]["post"];
        assert_eq!(
            create["requestBody"]["content"]["application/json"]["example"]["sku"],
            "BOOK-42"
        );
        assert_eq!(create["security"][0]["apiKeyAuth"], json!([]));
        assert_eq!(
            doc["components"]["securitySchemes"]["apiKeyAuth"]["name"],
            "X-Key"
        );

        let upload = &doc["paths"]["/upload"]["put"];
        assert_eq!(upload["security"], json!([]));
        let form = &upload["requestBody"]["content"]["multipart/form-data"];
        assert_eq!(
            form["schema"]["properties"]["photo"]["contentMediaType"],
            "image/png"
        );
        assert_eq!(form["example"]["caption"], "Tom");

        assert!(exported.warnings.iter().any(|w| w.contains("PURGE")));
        let yaml = document(&ws, "orders", true).unwrap();
        assert_eq!(yaml.file_name, "orders.openapi.yaml");
        assert!(yaml.content.contains("openapi: 3.1.0"), "{}", yaml.content);
    }

    #[test]
    fn exported_documents_import_back() {
        let (_dir, ws) = workspace();
        let exported = document(&ws, "", false).unwrap();
        let other = tempfile::tempdir().unwrap();
        let back = Workspace::init(other.path(), "Back").unwrap();
        let report = crate::import::import_text(&back, "", "", &exported.content, false).unwrap();
        assert_eq!(report.requests, 3);
        let get = back.read_request("shop/orders/get-order.yaml").unwrap();
        assert_eq!(get.url, "{{baseUrl}}/orders/:id/lines/:line");
        assert_eq!(get.path_params["id"], "7");
    }
}
