//! Importing OpenAPI 3.0 and 3.1 documents and Swagger 2.0 documents, in
//! JSON or YAML.
//!
//! Each operation becomes a request, in a folder per tag (or per first
//! path segment when it has no tags). URLs start with `{{baseUrl}}`, set
//! from the first server; path parameters become `:name` segments, and
//! query and header parameters are listed, switched on when required.
//! Bodies come from the document's examples, or from a sample built from
//! the schema. Security schemes become authentication that reads its
//! secrets from variables such as `{{token}}`.

use indexmap::IndexMap;
use serde_json::{Map, Value};

use super::{Collection, Import, Item, text, warn_once};
use crate::error::{Error, Result};
use crate::model::{
    ApiKeyLocation, Auth, Body, ClientAuth, Folder, FormField, KeyValue, OAuth2, OAuth2Grant,
    Request, Vars,
};

/// Deepest nesting a generated sample body reaches.
const MAX_SAMPLE_DEPTH: usize = 8;
/// Most schema nodes visited for one sample body.
const SAMPLE_BUDGET: usize = 2_000;
/// Most properties of one object in a sample.
const MAX_PROPERTIES: usize = 64;
/// Methods an OpenAPI path item can hold.
const METHODS: &[&str] = &[
    "get", "put", "post", "delete", "options", "head", "patch", "trace",
];

static NULL: Value = Value::Null;

/// Reads an OpenAPI or Swagger document, already parsed from JSON or YAML.
pub fn parse(doc: &Value) -> Result<Import> {
    let swagger = doc["swagger"].is_string() || doc["swagger"].is_number();
    let version = text(if swagger {
        &doc["swagger"]
    } else {
        &doc["openapi"]
    })
    .unwrap_or_default();
    if !doc.is_object() || version.is_empty() {
        return Err(Error::Invalid(
            "this is not an OpenAPI document: it has no `openapi` or `swagger` version".into(),
        ));
    }
    if !swagger && !version.starts_with('3') {
        return Err(Error::Invalid(format!(
            "OpenAPI {version} is not supported; Getman reads 3.0, 3.1 and Swagger 2.0"
        )));
    }
    let mut reader = Reader {
        doc,
        swagger,
        warnings: Vec::new(),
        auth_vars: Vec::new(),
    };
    let collection = reader.collection(&version)?;
    Ok(Import::Collection(Box::new(collection)))
}

struct Reader<'a> {
    doc: &'a Value,
    swagger: bool,
    warnings: Vec<String>,
    /// Variables the authentication reads, to mention once.
    auth_vars: Vec<&'static str>,
}

impl<'a> Reader<'a> {
    fn collection(&mut self, version: &str) -> Result<Collection> {
        let info = &self.doc["info"];
        let title = text(&info["title"])
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| "API".into());
        let mut folder = Folder {
            name: title,
            description: text(&info["description"]).unwrap_or_default(),
            ..Folder::default()
        };
        folder.variables.insert("baseUrl".into(), self.base_url());
        if let Some(security) = self.doc.get("security")
            && let Some(auth) = self.auth_for(security)
        {
            folder.auth = auth;
        }

        let paths = self.doc["paths"].as_object();
        if paths.is_none() && self.doc.get("paths").is_some() {
            return Err(Error::Invalid(
                "the document's `paths` is not a mapping of paths to operations".into(),
            ));
        }
        let mut groups: IndexMap<String, Vec<Request>> = IndexMap::new();
        let mut top: Vec<Request> = Vec::new();
        for (path, item) in paths.into_iter().flatten() {
            let item = self.resolve(item);
            let Some(operations) = item.as_object() else {
                continue;
            };
            for (method, operation) in operations {
                if !METHODS.contains(&method.as_str()) {
                    continue;
                }
                let operation = self.resolve(operation);
                if !operation.is_object() {
                    continue;
                }
                let request = self.operation(path, method, operation, &item["parameters"]);
                let request = match (&folder.auth, &request.auth) {
                    (outer, inner) if outer == inner => Request {
                        auth: Auth::Inherit,
                        ..request
                    },
                    _ => request,
                };
                match group_of(path, operation) {
                    Some(group) => groups.entry(group).or_default().push(request),
                    None => top.push(request),
                }
            }
        }

        // Folders follow the document's list of tags, then first use.
        let tags: Vec<(String, String)> = self.doc["tags"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|t| {
                Some((
                    text(&t["name"])?,
                    text(&t["description"]).unwrap_or_default(),
                ))
            })
            .collect();
        groups.sort_by_cached_key(|name, _| {
            tags.iter()
                .position(|(tag, _)| tag == name)
                .unwrap_or(usize::MAX)
        });
        let mut items: Vec<Item> = groups
            .into_iter()
            .map(|(name, requests)| {
                let description = tags
                    .iter()
                    .find(|(tag, _)| *tag == name)
                    .map(|(_, d)| d.clone())
                    .unwrap_or_default();
                Item::Folder(
                    Folder {
                        name,
                        description,
                        ..Folder::default()
                    },
                    requests.into_iter().map(Item::Request).collect(),
                )
            })
            .collect();
        items.extend(top.into_iter().map(Item::Request));

        if !self.auth_vars.is_empty() {
            let names: Vec<String> = self
                .auth_vars
                .iter()
                .map(|n| format!("{{{{{n}}}}}"))
                .collect();
            self.warnings.push(format!(
                "authentication reads {}; set them in an environment, as secrets",
                names.join(", ")
            ));
        }
        Ok(Collection {
            folder,
            items,
            environments: Vec::new(),
            warnings: std::mem::take(&mut self.warnings),
            format: if self.swagger {
                format!("Swagger {version}")
            } else {
                format!("OpenAPI {version}")
            },
        })
    }

    /// Follows `$ref`s to what they point at. Broken and external
    /// references give `null`, with a warning.
    fn resolve(&mut self, mut value: &'a Value) -> &'a Value {
        for _ in 0..32 {
            let Some(reference) = value.get("$ref").and_then(Value::as_str) else {
                return value;
            };
            match self.pointer(reference) {
                Some(target) => value = target,
                None => {
                    let warning = if reference.starts_with('#') {
                        format!("the reference {reference} points at nothing and was skipped")
                    } else {
                        format!(
                            "the reference {reference} is in another file and was skipped; bundle the document into one file first"
                        )
                    };
                    warn_once(&mut self.warnings, warning);
                    return &NULL;
                }
            }
        }
        warn_once(
            &mut self.warnings,
            "references that point at each other were skipped".into(),
        );
        &NULL
    }

    fn pointer(&self, reference: &str) -> Option<&'a Value> {
        let fragment = reference.strip_prefix('#')?;
        let decoded = percent_encoding::percent_decode_str(fragment).decode_utf8_lossy();
        self.doc.pointer(&decoded)
    }

    fn base_url(&mut self) -> String {
        if self.swagger {
            let schemes: Vec<String> = self.doc["schemes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(text)
                .collect();
            let scheme = if schemes.is_empty() || schemes.iter().any(|s| s == "https") {
                "https".to_owned()
            } else {
                schemes[0].clone()
            };
            let base_path = text(&self.doc["basePath"]).unwrap_or_default();
            let base_path = base_path.trim_end_matches('/');
            return match text(&self.doc["host"]) {
                Some(host) if !host.is_empty() => format!("{scheme}://{host}{base_path}"),
                _ => {
                    self.warnings.push(
                        "the document names no host; set the baseUrl variable to the API's address"
                            .into(),
                    );
                    base_path.to_owned()
                }
            };
        }
        let servers: Vec<&Value> = self.doc["servers"]
            .as_array()
            .into_iter()
            .flatten()
            .collect();
        let Some(first) = servers.first() else {
            self.warnings.push(
                "the document lists no servers; set the baseUrl variable to the API's address"
                    .into(),
            );
            return String::new();
        };
        let url = server_url(first);
        if servers.len() > 1 {
            let others: Vec<String> = servers[1..].iter().map(|s| server_url(s)).collect();
            self.warnings.push(format!(
                "baseUrl is the first server, {url}; the document also lists {}",
                others.join(", ")
            ));
        }
        if url.starts_with('/') || url.is_empty() {
            self.warnings.push(format!(
                "the server URL {url:?} is relative; put the API's address in front of it in the baseUrl variable"
            ));
        }
        url
    }

    fn operation(
        &mut self,
        path: &str,
        method: &str,
        operation: &'a Value,
        shared: &'a Value,
    ) -> Request {
        let method_upper = method.to_ascii_uppercase();
        let summary = text(&operation["summary"]).unwrap_or_default();
        let name = if !summary.trim().is_empty() && summary.chars().count() <= 80 {
            summary.trim().to_owned()
        } else if let Some(id) = text(&operation["operationId"]).filter(|id| !id.is_empty()) {
            id
        } else {
            format!("{method_upper} {path}")
        };
        let mut description: Vec<String> = Vec::new();
        if !summary.trim().is_empty() {
            description.push(summary.trim().to_owned());
        }
        if let Some(text) = text(&operation["description"]).filter(|d| !d.trim().is_empty()) {
            description.push(text.trim().to_owned());
        }
        if operation["deprecated"].as_bool() == Some(true) {
            description.push("Deprecated.".into());
        }

        let mut request = Request {
            name: name.clone(),
            method: method_upper,
            description: description.join("\n\n"),
            ..Request::default()
        };
        let (url, variables) = path_template(path);
        request.variables = variables;

        // Operation parameters replace path-level ones with the same name
        // and location.
        let mut parameters: Vec<&'a Value> = Vec::new();
        for list in [shared, &operation["parameters"]] {
            for parameter in list.as_array().into_iter().flatten() {
                let parameter = self.resolve(parameter);
                let key = (text(&parameter["name"]), text(&parameter["in"]));
                parameters.retain(|p| (text(&p["name"]), text(&p["in"])) != key);
                parameters.push(parameter);
            }
        }
        let mut query: Vec<KeyValue> = Vec::new();
        let mut cookies: Vec<String> = Vec::new();
        let mut cookies_required = false;
        let mut form: Vec<&'a Value> = Vec::new();
        for parameter in parameters {
            let Some(param_name) = text(&parameter["name"]) else {
                continue;
            };
            let required = parameter["required"].as_bool() == Some(true);
            let location = parameter["in"].as_str().unwrap_or_default();
            if location == "body" {
                let schema = &parameter["schema"];
                let consumes = self.consumes(operation);
                self.body_from_example(&mut request, &consumes, None, schema, &name);
                continue;
            }
            if location == "formData" {
                form.push(parameter);
                continue;
            }
            let value = self.parameter_value(parameter);
            match location {
                "path" => {
                    if request.variables.contains_key(&param_name) {
                        request.variables.insert(param_name, value);
                    } else {
                        request.path_params.insert(param_name, value);
                    }
                }
                "query" => query.push(KeyValue {
                    enabled: required,
                    description: text(&parameter["description"]).unwrap_or_default(),
                    ..KeyValue::new(param_name, value)
                }),
                "header" => {
                    let ignored = ["accept", "content-type", "authorization"];
                    if !ignored.contains(&param_name.to_ascii_lowercase().as_str()) {
                        request.headers.push(KeyValue {
                            enabled: required,
                            description: text(&parameter["description"]).unwrap_or_default(),
                            ..KeyValue::new(param_name, value)
                        });
                    }
                }
                "cookie" => {
                    cookies_required |= required;
                    cookies.push(format!("{param_name}={value}"));
                }
                _ => {}
            }
        }
        if !cookies.is_empty() {
            request.headers.push(KeyValue {
                enabled: cookies_required,
                ..KeyValue::new("Cookie", cookies.join("; "))
            });
        }
        let (enabled, disabled): (Vec<KeyValue>, Vec<KeyValue>) =
            query.into_iter().partition(|p| p.enabled);
        request.url = super::with_query(&format!("{{{{baseUrl}}}}{url}"), &enabled);
        request.disabled_params = disabled;
        // Descriptions of enabled parameters have nowhere to go in the URL.
        if !form.is_empty() {
            let consumes = self.consumes(operation);
            self.form_parameters(&mut request, &form, &consumes, &name);
        }
        if !self.swagger
            && let Some(body) = operation.get("requestBody")
        {
            self.request_body(&mut request, body, &name);
        }
        if let Some(security) = operation.get("security") {
            if let Some(auth) = self.auth_for(security) {
                request.auth = auth;
            }
        } else if self.doc.get("security").is_some() {
            // Inherits the document's security, which is the folder's.
            request.auth = Auth::Inherit;
        }
        request
    }

    fn parameter_value(&mut self, parameter: &'a Value) -> String {
        let schema = self.resolve(&parameter["schema"]);
        let candidates = [
            parameter.get("example"),
            parameter.get("x-example"),
            parameter["examples"]
                .as_object()
                .and_then(|examples| examples.values().next())
                .map(|e| &e["value"]),
            schema.get("example"),
            schema["examples"].as_array().and_then(|e| e.first()),
            schema.get("default"),
            parameter.get("default"),
            schema["enum"].as_array().and_then(|e| e.first()),
            parameter["enum"].as_array().and_then(|e| e.first()),
        ];
        candidates
            .into_iter()
            .flatten()
            .find(|v| !v.is_null())
            .map(parameter_text)
            .unwrap_or_default()
    }

    fn consumes(&self, operation: &Value) -> Vec<String> {
        let list = if operation["consumes"].is_array() {
            &operation["consumes"]
        } else {
            &self.doc["consumes"]
        };
        list.as_array()
            .into_iter()
            .flatten()
            .filter_map(text)
            .collect()
    }

    /// Swagger 2.0 `formData` parameters: a form, or multipart when files
    /// are uploaded.
    fn form_parameters(
        &mut self,
        request: &mut Request,
        parameters: &[&'a Value],
        consumes: &[String],
        name: &str,
    ) {
        let multipart = consumes.iter().any(|c| c.contains("multipart"))
            || parameters.iter().any(|p| p["type"] == "file");
        if multipart {
            let mut fields = Vec::new();
            for parameter in parameters {
                let field_name = text(&parameter["name"]).unwrap_or_default();
                let is_file = parameter["type"] == "file";
                fields.push(FormField {
                    value: if is_file {
                        String::new()
                    } else {
                        self.parameter_value(parameter)
                    },
                    file: is_file.then(String::new),
                    content_type: None,
                    enabled: true,
                    name: field_name,
                });
            }
            if fields.iter().any(|f| f.file.is_some()) {
                self.warnings.push(format!(
                    "{name}: choose the files to upload in its multipart body"
                ));
            }
            request.body = Body::Multipart { fields };
        } else {
            let fields = parameters
                .iter()
                .map(|p| KeyValue {
                    enabled: p["required"].as_bool() == Some(true),
                    ..KeyValue::new(
                        text(&p["name"]).unwrap_or_default(),
                        self.parameter_value(p),
                    )
                })
                .collect();
            request.body = Body::Form { fields };
        }
    }

    /// An OpenAPI 3 request body.
    fn request_body(&mut self, request: &mut Request, body: &'a Value, name: &str) {
        let body = self.resolve(body);
        let Some(content) = body["content"].as_object() else {
            return;
        };
        let types: Vec<String> = content.keys().cloned().collect();
        let Some(chosen) = choose_media_type(&types) else {
            return;
        };
        let media = self.resolve(&content[&chosen]);
        let example = self.media_example(media);
        self.body_from_example(request, &[chosen], example, &media["schema"], name);
    }

    fn media_example(&mut self, media: &'a Value) -> Option<Value> {
        if let Some(example) = media.get("example") {
            return Some(example.clone());
        }
        let first = media["examples"].as_object()?.values().next()?;
        let example = self.resolve(first);
        if example.get("externalValue").is_some() && example.get("value").is_none() {
            warn_once(
                &mut self.warnings,
                "examples kept in other files were skipped; a sample built from the schema is used instead"
                    .into(),
            );
            return None;
        }
        example.get("value").cloned()
    }

    /// Sets the body from an example, or from a sample of the schema.
    fn body_from_example(
        &mut self,
        request: &mut Request,
        types: &[String],
        example: Option<Value>,
        schema: &'a Value,
        name: &str,
    ) {
        let content_type = choose_media_type(types).unwrap_or_else(|| "application/json".into());
        let essence = content_type.to_ascii_lowercase();
        let value = match example {
            Some(value) => value,
            None => self.sample(schema),
        };
        let as_text = |value: &Value| match value {
            Value::String(s) => s.clone(),
            Value::Null => String::new(),
            other => serde_json::to_string_pretty(other).unwrap_or_default(),
        };
        request.body = if essence.contains("json") {
            if essence != "application/json" {
                request
                    .headers
                    .push(KeyValue::new("Content-Type", &content_type));
            }
            Body::Json {
                content: match &value {
                    Value::String(s) => super::pretty_json(s),
                    other => serde_json::to_string_pretty(other).unwrap_or_default(),
                },
            }
        } else if essence == "application/x-www-form-urlencoded" {
            Body::Form {
                fields: object_fields(&value)
                    .into_iter()
                    .map(|(k, v)| KeyValue::new(k, v))
                    .collect(),
            }
        } else if essence.starts_with("multipart/") {
            let schema = self.resolve(schema);
            let mut fields = Vec::new();
            for (field, field_value) in object_fields(&value) {
                let property = self.resolve(&schema["properties"][&field]);
                let property = if property["type"] == "array" {
                    self.resolve(&property["items"])
                } else {
                    property
                };
                let is_file = matches!(property["format"].as_str(), Some("binary" | "base64"))
                    || property.get("contentMediaType").is_some();
                fields.push(FormField {
                    value: if is_file { String::new() } else { field_value },
                    file: is_file.then(String::new),
                    content_type: None,
                    enabled: true,
                    name: field,
                });
            }
            if fields.iter().any(|f| f.file.is_some()) {
                self.warnings.push(format!(
                    "{name}: choose the files to upload in its multipart body"
                ));
            }
            Body::Multipart { fields }
        } else if essence.ends_with("/xml") || essence.ends_with("+xml") {
            let content = match &value {
                Value::String(s) => s.clone(),
                _ => {
                    self.warnings.push(format!(
                        "{name}: the document has no XML example; write the body yourself"
                    ));
                    String::new()
                }
            };
            Body::Xml { content }
        } else if essence.starts_with("text/") {
            Body::Text {
                content: as_text(&value),
                content_type: (essence != "text/plain").then(|| content_type.clone()),
            }
        } else {
            self.warnings.push(format!(
                "{name}: choose the file to send as its {content_type} body"
            ));
            Body::Binary {
                file: String::new(),
                content_type: Some(content_type.clone()),
            }
        };
    }

    fn sample(&mut self, schema: &'a Value) -> Value {
        let mut budget = SAMPLE_BUDGET;
        let mut stack = Vec::new();
        self.sample_at(schema, 0, &mut stack, &mut budget)
    }

    fn sample_at(
        &mut self,
        schema: &'a Value,
        depth: usize,
        stack: &mut Vec<&'a str>,
        budget: &mut usize,
    ) -> Value {
        if depth > MAX_SAMPLE_DEPTH || *budget == 0 {
            return Value::Null;
        }
        *budget -= 1;
        if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
            if stack.contains(&reference) {
                // A schema that contains itself: stop here.
                return Value::Null;
            }
            stack.push(reference);
            let target = self.resolve(schema);
            let value = self.sample_at(target, depth, stack, budget);
            stack.pop();
            return value;
        }
        let Some(object) = schema.as_object() else {
            return Value::Null;
        };
        for key in ["example", "x-example", "default", "const"] {
            if let Some(value) = object.get(key).filter(|v| !v.is_null()) {
                return value.clone();
            }
        }
        if let Some(first) = object
            .get("examples")
            .and_then(Value::as_array)
            .and_then(|e| e.first())
        {
            return first.clone();
        }
        if let Some(first) = object
            .get("enum")
            .and_then(Value::as_array)
            .and_then(|e| e.first())
        {
            return first.clone();
        }
        if let Some(all) = object.get("allOf").and_then(Value::as_array) {
            let mut merged = Map::new();
            let mut last = Value::Null;
            for part in all {
                match self.sample_at(part, depth, stack, budget) {
                    Value::Object(map) => merged.extend(map),
                    other if !other.is_null() => last = other,
                    _ => {}
                }
            }
            return if merged.is_empty() {
                last
            } else {
                Value::Object(merged)
            };
        }
        for key in ["oneOf", "anyOf"] {
            if let Some(first) = object
                .get(key)
                .and_then(Value::as_array)
                .and_then(|list| list.first())
            {
                return self.sample_at(first, depth, stack, budget);
            }
        }
        let kind = match object.get("type") {
            Some(Value::String(kind)) => kind.as_str(),
            Some(Value::Array(kinds)) => kinds
                .iter()
                .filter_map(Value::as_str)
                .find(|k| *k != "null")
                .unwrap_or("null"),
            _ if object.contains_key("properties") => "object",
            _ if object.contains_key("items") => "array",
            _ => "",
        };
        match kind {
            "object" => {
                let mut map = Map::new();
                let properties = object.get("properties").and_then(Value::as_object);
                for (name, property) in properties.into_iter().flatten().take(MAX_PROPERTIES) {
                    let resolved = self.resolve(property);
                    if resolved["readOnly"].as_bool() == Some(true) {
                        continue;
                    }
                    map.insert(
                        name.clone(),
                        self.sample_at(property, depth + 1, stack, budget),
                    );
                }
                Value::Object(map)
            }
            "array" => match object.get("items") {
                Some(items) => {
                    let item = self.sample_at(items, depth + 1, stack, budget);
                    if item.is_null() {
                        Value::Array(Vec::new())
                    } else {
                        Value::Array(vec![item])
                    }
                }
                None => Value::Array(Vec::new()),
            },
            "string" => Value::String(
                match object.get("format").and_then(Value::as_str) {
                    Some("date-time") => "2024-01-01T00:00:00Z",
                    Some("date") => "2024-01-01",
                    Some("time") => "00:00:00",
                    Some("email") => "user@example.com",
                    Some("uuid") => "00000000-0000-4000-8000-000000000000",
                    Some("uri" | "url") => "https://example.com",
                    Some("hostname") => "example.com",
                    Some("ipv4") => "192.0.2.1",
                    Some("ipv6") => "2001:db8::1",
                    Some("binary" | "byte" | "base64") => "",
                    Some("password") => "password",
                    _ => "string",
                }
                .to_owned(),
            ),
            "integer" | "number" => object
                .get("minimum")
                .filter(|m| m.is_number())
                .cloned()
                .unwrap_or_else(|| Value::from(0)),
            "boolean" => Value::Bool(true),
            _ => Value::Null,
        }
    }

    /// Authentication for a list of security requirements, which are
    /// alternatives. `[]` means none; `None` means it could not be mapped.
    fn auth_for(&mut self, requirements: &'a Value) -> Option<Auth> {
        let list = requirements.as_array()?;
        if list.is_empty() {
            return Some(Auth::None);
        }
        let mut optional = false;
        for requirement in list {
            let Some(schemes) = requirement.as_object() else {
                continue;
            };
            if schemes.is_empty() {
                optional = true;
                continue;
            }
            let (scheme_name, scopes) = schemes.iter().next()?;
            let scopes: Vec<String> = scopes
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(text)
                .collect();
            if let Some(auth) = self.scheme(scheme_name, &scopes) {
                if schemes.len() > 1 {
                    let others: Vec<&str> = schemes.keys().skip(1).map(String::as_str).collect();
                    warn_once(
                        &mut self.warnings,
                        format!(
                            "{scheme_name} is used together with {}; only {scheme_name} was set, so add the others' credentials to headers",
                            others.join(", ")
                        ),
                    );
                }
                return Some(auth);
            }
        }
        optional.then_some(Auth::None)
    }

    fn scheme(&mut self, name: &str, scopes: &[String]) -> Option<Auth> {
        let schemes = if self.swagger {
            &self.doc["securityDefinitions"]
        } else {
            &self.doc["components"]["securitySchemes"]
        };
        let scheme = self.resolve(&schemes[name]);
        if !scheme.is_object() {
            warn_once(
                &mut self.warnings,
                format!("the security scheme {name} is not defined and was skipped"),
            );
            return None;
        }
        let kind = scheme["type"].as_str().unwrap_or_default();
        let auth = match kind {
            "http" => match scheme["scheme"]
                .as_str()
                .unwrap_or_default()
                .to_ascii_lowercase()
                .as_str()
            {
                "bearer" => self.bearer("token"),
                "basic" => self.basic(),
                "digest" => {
                    self.note_vars(&["username", "password"]);
                    Auth::Digest {
                        username: "{{username}}".into(),
                        password: "{{password}}".into(),
                    }
                }
                other => {
                    warn_once(
                        &mut self.warnings,
                        format!(
                            "{name}: HTTP {other} authentication is not supported; set the Authorization header yourself"
                        ),
                    );
                    return None;
                }
            },
            "basic" => self.basic(),
            "apiKey" => {
                let location = match scheme["in"].as_str() {
                    Some("query") => ApiKeyLocation::Query,
                    Some("header") => ApiKeyLocation::Header,
                    _ => {
                        warn_once(
                            &mut self.warnings,
                            format!(
                                "{name}: API keys sent in a cookie are not supported; add a Cookie header instead"
                            ),
                        );
                        return None;
                    }
                };
                self.note_vars(&["apiKey"]);
                Auth::ApiKey {
                    key: text(&scheme["name"]).unwrap_or_default(),
                    value: "{{apiKey}}".into(),
                    location,
                }
            }
            "oauth2" => self.oauth2(name, scheme, scopes),
            "openIdConnect" => {
                warn_once(
                    &mut self.warnings,
                    format!(
                        "{name}: OpenID Connect needs a browser; requests send {{{{accessToken}}}} as a Bearer token"
                    ),
                );
                self.bearer("accessToken")
            }
            other => {
                warn_once(
                    &mut self.warnings,
                    format!("{name}: {other} authentication is not supported"),
                );
                return None;
            }
        };
        Some(auth)
    }

    fn oauth2(&mut self, name: &str, scheme: &'a Value, scopes: &[String]) -> Auth {
        // OpenAPI 3 lists flows; Swagger 2.0 names one.
        let flows: Vec<(&str, &'a Value)> = if self.swagger {
            let flow = match scheme["flow"].as_str() {
                Some("application") => "clientCredentials",
                Some("password") => "password",
                Some("accessCode") => "authorizationCode",
                Some("implicit") => "implicit",
                _ => "",
            };
            vec![(flow, scheme)]
        } else {
            scheme["flows"]
                .as_object()
                .into_iter()
                .flatten()
                .map(|(flow, settings)| (flow.as_str(), settings))
                .collect()
        };
        let pick = flows
            .iter()
            .find(|(flow, _)| *flow == "clientCredentials")
            .or_else(|| flows.iter().find(|(flow, _)| *flow == "password"));
        let Some((flow, settings)) = pick else {
            let flow = flows.first().map_or("unnamed", |(flow, _)| *flow);
            warn_once(
                &mut self.warnings,
                format!(
                    "{name}: the OAuth 2.0 {flow} flow needs a browser; requests send {{{{accessToken}}}} as a Bearer token"
                ),
            );
            return self.bearer("accessToken");
        };
        let scope = if scopes.is_empty() {
            settings["scopes"]
                .as_object()
                .map(|s| s.keys().cloned().collect::<Vec<_>>().join(" "))
                .unwrap_or_default()
        } else {
            scopes.join(" ")
        };
        let password = *flow == "password";
        self.note_vars(&["clientId", "clientSecret"]);
        if password {
            self.note_vars(&["username", "password"]);
        }
        Auth::Oauth2(OAuth2 {
            grant: if password {
                OAuth2Grant::Password
            } else {
                OAuth2Grant::ClientCredentials
            },
            token_url: text(&settings["tokenUrl"]).unwrap_or_default(),
            client_id: "{{clientId}}".into(),
            client_secret: "{{clientSecret}}".into(),
            scope,
            audience: String::new(),
            username: if password {
                "{{username}}".into()
            } else {
                String::new()
            },
            password: if password {
                "{{password}}".into()
            } else {
                String::new()
            },
            client_auth: ClientAuth::Basic,
            ..OAuth2::default()
        })
    }

    fn bearer(&mut self, variable: &'static str) -> Auth {
        self.note_vars(&[variable]);
        Auth::Bearer {
            token: format!("{{{{{variable}}}}}"),
            prefix: String::new(),
        }
    }

    fn basic(&mut self) -> Auth {
        self.note_vars(&["username", "password"]);
        Auth::Basic {
            username: "{{username}}".into(),
            password: "{{password}}".into(),
        }
    }

    fn note_vars(&mut self, names: &[&'static str]) {
        for name in names {
            if !self.auth_vars.contains(name) {
                self.auth_vars.push(name);
            }
        }
    }
}

/// The folder an operation goes in: its first tag, or the first segment of
/// its path that is not a parameter.
fn group_of(path: &str, operation: &Value) -> Option<String> {
    if let Some(tag) = operation["tags"]
        .as_array()
        .and_then(|tags| tags.first())
        .and_then(text)
        .filter(|t| !t.trim().is_empty())
    {
        return Some(tag);
    }
    path.split('/')
        .find(|segment| !segment.is_empty() && !segment.starts_with('{'))
        .map(str::to_owned)
}

/// Turns `/users/{id}` into `/users/:id`. Parameters that share a segment
/// with other text, as in `/files/{name}.{ext}`, become `{{variables}}`
/// instead, returned with empty values.
fn path_template(path: &str) -> (String, Vars) {
    let mut variables = Vars::new();
    let segments: Vec<String> = path
        .split('/')
        .map(|segment| {
            if let Some(name) = segment
                .strip_prefix('{')
                .and_then(|s| s.strip_suffix('}'))
                .filter(|n| !n.contains(['{', '}']))
            {
                return format!(":{name}");
            }
            let mut out = String::new();
            let mut rest = segment;
            while let Some(start) = rest.find('{') {
                let Some(end) = rest[start..].find('}') else {
                    break;
                };
                let name = &rest[start + 1..start + end];
                out.push_str(&rest[..start]);
                out.push_str(&format!("{{{{{name}}}}}"));
                variables.insert(name.to_owned(), String::new());
                rest = &rest[start + end + 1..];
            }
            out.push_str(rest);
            out
        })
        .collect();
    (segments.join("/"), variables)
}

/// A server's URL with its variables set to their defaults.
fn server_url(server: &Value) -> String {
    let mut url = text(&server["url"]).unwrap_or_default();
    if let Some(variables) = server["variables"].as_object() {
        for (name, variable) in variables {
            let value = text(&variable["default"])
                .or_else(|| {
                    variable["enum"]
                        .as_array()
                        .and_then(|e| e.first())
                        .and_then(text)
                })
                .unwrap_or_default();
            url = url.replace(&format!("{{{name}}}"), &value);
        }
    }
    url.trim_end_matches('/').to_owned()
}

/// Picks the media type to send: JSON first, then forms, XML and text.
fn choose_media_type(types: &[String]) -> Option<String> {
    let lower: Vec<String> = types.iter().map(|t| t.to_ascii_lowercase()).collect();
    let preferences: [&dyn Fn(&str) -> bool; 6] = [
        &|t| t == "application/json",
        &|t| t.contains("json"),
        &|t| t == "application/x-www-form-urlencoded",
        &|t| t.starts_with("multipart/"),
        &|t| t.ends_with("/xml") || t.ends_with("+xml"),
        &|t| t.starts_with("text/"),
    ];
    for prefer in preferences {
        if let Some(at) = lower.iter().position(|t| prefer(t)) {
            return Some(types[at].clone());
        }
    }
    types.first().cloned()
}

/// The fields of an object example, as text.
fn object_fields(value: &Value) -> Vec<(String, String)> {
    value
        .as_object()
        .into_iter()
        .flatten()
        .map(|(k, v)| (k.clone(), parameter_text(v)))
        .collect()
}

fn parameter_text(value: &Value) -> String {
    match value {
        Value::Array(items) => items
            .iter()
            .map(parameter_text)
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => value.to_string(),
        Value::Null => String::new(),
        other => text(other).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::parse_document;

    const PETSTORE_3: &str = include_str!("../../tests/fixtures/import/petstore-openapi-3.1.yaml");
    const PETSTORE_2: &str = include_str!("../../tests/fixtures/import/petstore-swagger-2.0.json");

    fn collection(text: &str) -> Collection {
        let Import::Collection(collection) = parse(&parse_document(text).unwrap()).unwrap() else {
            panic!("expected a collection");
        };
        *collection
    }

    fn folder<'c>(collection: &'c Collection, name: &str) -> &'c [Item] {
        collection
            .items
            .iter()
            .find_map(|item| match item {
                Item::Folder(folder, items) if folder.name == name => Some(items.as_slice()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no folder {name}"))
    }

    fn request<'c>(items: &'c [Item], name: &str) -> &'c Request {
        items
            .iter()
            .find_map(|item| match item {
                Item::Request(request) if request.name == name => Some(request),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no request {name}"))
    }

    #[test]
    fn operations_become_requests_in_folders_per_tag() {
        let c = collection(PETSTORE_3);
        assert_eq!(c.format, "OpenAPI 3.1.0");
        assert_eq!(c.folder.name, "Pet Store");
        assert_eq!(c.folder.variables["baseUrl"], "https://pets.example.com/v1");
        assert!(matches!(&c.folder.auth, Auth::Bearer { token, .. } if token == "{{token}}"));
        // Folders follow the document's tag list, then requests without one.
        let names: Vec<&str> = c
            .items
            .iter()
            .map(|item| match item {
                Item::Folder(folder, _) => folder.name.as_str(),
                Item::Request(request) => request.name.as_str(),
            })
            .collect();
        assert_eq!(names, ["pets", "store", "health"]);
        let pets = folder(&c, "pets");
        assert_eq!(pets.len(), 4);

        let list = request(pets, "List pets");
        assert_eq!(list.method, "GET");
        assert_eq!(list.url, "{{baseUrl}}/pets?limit=20");
        assert_eq!(list.disabled_params[0].name, "tag");
        assert_eq!(list.disabled_params[0].value, "dog");
        assert!(
            list.headers
                .iter()
                .any(|h| h.name == "X-Request-Id" && !h.enabled)
        );
        assert!(list.description.contains("Lists every pet"));
        assert!(list.auth.is_inherit());

        let get = request(pets, "Find a pet");
        assert_eq!(get.url, "{{baseUrl}}/pets/:petId");
        assert_eq!(get.path_params["petId"], "42");
        assert_eq!(get.auth, Auth::None);
    }

    #[test]
    fn bodies_come_from_examples_or_schemas() {
        let c = collection(PETSTORE_3);
        let pets = folder(&c, "pets");
        let create = request(pets, "Add a pet");
        let Body::Json { content } = &create.body else {
            panic!("{:?}", create.body)
        };
        let body: Value = serde_json::from_str(content).unwrap();
        // From the schema: readOnly id left out, nested ref, cycle cut.
        assert_eq!(body["name"], "Rex");
        assert!(body.get("id").is_none(), "{content}");
        assert_eq!(body["status"], "available");
        assert_eq!(body["owner"]["email"], "user@example.com");
        assert_eq!(body["tags"][0], "string");
        assert!(body["parent"]["parent"].is_object() || body["parent"]["parent"].is_null());

        let store = folder(&c, "store");
        let order = request(store, "Place an order");
        let Body::Json { content } = &order.body else {
            panic!()
        };
        assert!(content.contains("\"quantity\": 2"), "{content}");
        assert!(
            matches!(&order.auth, Auth::ApiKey { key, location: ApiKeyLocation::Header, .. } if key == "X-Api-Key")
        );

        let upload = request(pets, "Upload a photo");
        let Body::Multipart { fields } = &upload.body else {
            panic!("{:?}", upload.body)
        };
        assert_eq!(fields[0].file.as_deref(), Some(""));
        assert_eq!(fields[1].value, "a good dog");
        assert!(c.warnings.iter().any(|w| w.contains("choose the files")));
        assert!(c.warnings.iter().any(|w| w.contains("{{token}}")));
    }

    #[test]
    fn operations_without_tags_go_by_their_first_path_segment() {
        let c = collection(PETSTORE_3);
        let health = folder(&c, "health");
        assert_eq!(request(health, "GET /health").url, "{{baseUrl}}/health");
    }

    #[test]
    fn swagger_2_documents_are_read() {
        let c = collection(PETSTORE_2);
        assert_eq!(c.format, "Swagger 2.0");
        assert_eq!(
            c.folder.variables["baseUrl"],
            "https://petstore.example.com/api"
        );
        let pets = folder(&c, "pet");
        let add = request(pets, "addPet");
        assert_eq!(add.method, "POST");
        let Body::Json { content } = &add.body else {
            panic!("{:?}", add.body)
        };
        assert!(content.contains("\"name\": \"doggie\""), "{content}");
        let Auth::Oauth2(oauth) = &add.auth else {
            panic!("{:?}", add.auth)
        };
        assert_eq!(oauth.grant, OAuth2Grant::ClientCredentials);
        assert_eq!(oauth.token_url, "https://petstore.example.com/oauth/token");
        assert_eq!(oauth.scope, "write:pets");

        let form = request(pets, "updatePetWithForm");
        assert_eq!(form.url, "{{baseUrl}}/pet/:petId");
        let Body::Form { fields } = &form.body else {
            panic!("{:?}", form.body)
        };
        assert_eq!(fields[0].name, "name");

        let upload = request(pets, "uploadFile");
        assert!(matches!(&upload.body, Body::Multipart { fields } if fields[1].file.is_some()));
        assert!(matches!(&c.folder.auth, Auth::Basic { .. }));
    }

    #[test]
    fn segments_with_several_parameters_become_variables() {
        let (url, vars) = path_template("/files/{name}.{ext}/raw/{id}");
        assert_eq!(url, "/files/{{name}}.{{ext}}/raw/:id");
        assert_eq!(vars.keys().collect::<Vec<_>>(), ["name", "ext"]);
    }

    #[test]
    fn external_and_broken_references_are_reported() {
        let doc = parse_document(
            r##"{"openapi": "3.0.0", "info": {"title": "T"}, "servers": [{"url": "/api"}],
              "paths": {"/a": {"post": {"requestBody": {"content": {"application/json": {"schema": {"$ref": "other.yaml#/X"}}}}}},
                        "/b": {"$ref": "#/nothing/here"}}}"##,
        )
        .unwrap();
        let Import::Collection(c) = parse(&doc).unwrap() else {
            panic!()
        };
        let warnings = c.warnings.join("\n");
        assert!(warnings.contains("another file"), "{warnings}");
        assert!(warnings.contains("points at nothing"), "{warnings}");
        assert!(warnings.contains("relative"), "{warnings}");
        assert_eq!(Item::count_requests(&c.items), 1);
    }

    #[test]
    fn deep_and_wide_schemas_stay_small() {
        // Each level refers to the next twice; without limits this would
        // grow exponentially.
        let mut schemas = Map::new();
        for level in 0..30 {
            schemas.insert(
                format!("L{level}"),
                serde_json::json!({"type": "object", "properties": {
                    "a": {"$ref": format!("#/components/schemas/L{}", level + 1)},
                    "b": {"$ref": format!("#/components/schemas/L{}", level + 1)}
                }}),
            );
        }
        let doc = serde_json::json!({"openapi": "3.0.0", "info": {"title": "T"}, "paths": {"/x": {"post": {
            "requestBody": {"content": {"application/json": {"schema": {"$ref": "#/components/schemas/L0"}}}}}}},
            "components": {"schemas": schemas}});
        let Import::Collection(c) = parse(&doc).unwrap() else {
            panic!()
        };
        let Item::Folder(_, items) = &c.items[0] else {
            panic!()
        };
        let Item::Request(request) = &items[0] else {
            panic!()
        };
        let Body::Json { content } = &request.body else {
            panic!()
        };
        assert!(content.len() < 200_000, "{} bytes", content.len());
    }

    #[test]
    fn other_versions_are_refused() {
        let doc = parse_document(r#"{"openapi": "4.0.0", "paths": {}}"#).unwrap();
        assert!(parse(&doc).unwrap_err().to_string().contains("4.0.0"));
        let doc = parse_document(r#"{"openapi": "3.0.0", "paths": []}"#).unwrap();
        assert!(parse(&doc).is_err());
    }
}
