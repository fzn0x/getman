//! Importing Insomnia exports: the v4 JSON export and the v5 YAML
//! collection.
//!
//! Folders become directories and requests YAML files, ordered as they
//! were. The base environment's values become the collection folder's
//! variables, and each sub-environment a Getman environment.
//! `{{ _.name }}` references become `{{name}}`.

use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use super::{Collection, Import, Item, NewEnvironment, text, text_or_empty, warn_once};
use crate::error::{Error, Result};
use crate::model::{
    ApiKeyLocation, Auth, Body, ClientAuth, Folder, FormField, KeyValue, OAuth2, OAuth2Grant,
    Request, Scripts, Settings, Vars,
};

static VARIABLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{\{\s*(?:_\.)?([^{}\s]+?)\s*\}\}").expect("variable pattern"));
static TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{%\s*(\w+)\s*([^%]*?)\s*%\}").expect("tag pattern"));
static SCRIPT_API: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\binsomnia\.").expect("script pattern"));

/// Whether a parsed document is an Insomnia export.
pub(crate) fn matches(value: &Value) -> bool {
    (value["_type"] == "export" && value["resources"].is_array())
        || value["type"]
            .as_str()
            .is_some_and(|t| t.contains(".insomnia.rest/"))
}

/// Reads an Insomnia export, already parsed from JSON or YAML.
pub fn parse(doc: &Value) -> Result<Import> {
    let mut reader = Reader {
        warnings: Vec::new(),
        skipped: 0,
    };
    let import = if doc["_type"] == "export" {
        reader.v4(doc)?
    } else {
        reader.v5(doc)?
    };
    Ok(import)
}

struct Reader {
    warnings: Vec<String>,
    /// gRPC and WebSocket requests, which Getman cannot send.
    skipped: usize,
}

impl Reader {
    // ---- v4: a flat list of resources linked by parentId -----------------

    fn v4(&mut self, doc: &Value) -> Result<Import> {
        let Some(resources) = doc["resources"].as_array() else {
            return Err(Error::Invalid("the export has no resources".into()));
        };
        let children = |parent: &str| -> Vec<&Value> {
            let mut list: Vec<&Value> = resources
                .iter()
                .filter(|r| r["parentId"].as_str() == Some(parent))
                .collect();
            list.sort_by(|a, b| {
                let key = |v: &Value| v["metaSortKey"].as_f64().unwrap_or(0.0);
                key(a).total_cmp(&key(b))
            });
            list
        };
        let workspaces: Vec<&Value> = resources
            .iter()
            .filter(|r| r["_type"] == "workspace")
            .collect();
        if workspaces.is_empty() {
            // Exports of a single folder or request have no workspace.
            let roots: Vec<&Value> = resources
                .iter()
                .filter(|r| {
                    let parent = r["parentId"].as_str().unwrap_or_default();
                    !resources.iter().any(|o| o["_id"].as_str() == Some(parent))
                })
                .filter(|r| matches!(r["_type"].as_str(), Some("request" | "request_group")))
                .collect();
            if roots.is_empty() {
                return Err(Error::Invalid(
                    "the export has no workspace, folders or requests".into(),
                ));
            }
            let items = roots
                .into_iter()
                .filter_map(|r| self.v4_item(r, &children, 0))
                .collect();
            return Ok(self.finish(
                Folder {
                    name: "Imported requests".into(),
                    ..Folder::default()
                },
                items,
                Vec::new(),
                "Insomnia export v4",
            ));
        }

        let mut collections = Vec::new();
        for workspace in &workspaces {
            let id = workspace["_id"].as_str().unwrap_or_default();
            let mut folder = Folder {
                name: text_or_empty(&workspace["name"]),
                description: self.template(&text_or_empty(&workspace["description"])),
                ..Folder::default()
            };
            let mut environments = Vec::new();
            let mut items = Vec::new();
            for child in children(id) {
                match child["_type"].as_str() {
                    Some("environment") => {
                        folder.variables.extend(self.vars(&child["data"]));
                        for sub in children(child["_id"].as_str().unwrap_or_default()) {
                            if sub["_type"] == "environment" {
                                environments.push(self.environment(sub));
                            }
                        }
                    }
                    _ => {
                        if let Some(item) = self.v4_item(child, &children, 0) {
                            items.push(item);
                        }
                    }
                }
            }
            collections.push((folder, items, environments));
        }
        if collections.len() == 1 {
            let (folder, items, environments) = collections.remove(0);
            return Ok(self.finish(folder, items, environments, "Insomnia export v4"));
        }
        // Several workspaces: one folder each.
        let mut all_environments = Vec::new();
        let items = collections
            .into_iter()
            .map(|(folder, items, environments)| {
                all_environments.extend(environments);
                Item::Folder(folder, items)
            })
            .collect();
        Ok(self.finish(
            Folder {
                name: "Insomnia export".into(),
                ..Folder::default()
            },
            items,
            all_environments,
            "Insomnia export v4",
        ))
    }

    fn v4_item<'a>(
        &mut self,
        resource: &'a Value,
        children: &dyn Fn(&str) -> Vec<&'a Value>,
        depth: usize,
    ) -> Option<Item> {
        match resource["_type"].as_str()? {
            "request_group" if depth < 32 => {
                let id = resource["_id"].as_str().unwrap_or_default();
                let folder = self.folder(resource, resource);
                let items = children(id)
                    .into_iter()
                    .filter_map(|child| self.v4_item(child, children, depth + 1))
                    .collect();
                Some(Item::Folder(folder, items))
            }
            "request" => Some(Item::Request(self.request(resource, false))),
            "grpc_request" | "websocket_request" | "socketio_request" => {
                self.skipped += 1;
                None
            }
            _ => None,
        }
    }

    // ---- v5: a nested YAML document --------------------------------------

    fn v5(&mut self, doc: &Value) -> Result<Import> {
        let kind = doc["type"].as_str().unwrap_or_default();
        let version = format!(
            "Insomnia export v{}",
            kind.rsplit('/').next().unwrap_or("5")
        );
        if !kind.starts_with("collection.") && !kind.starts_with("environment.") {
            return Err(Error::Invalid(format!(
                "{kind} exports cannot be imported; export a request collection"
            )));
        }
        let mut folder = Folder {
            name: text_or_empty(&doc["name"]),
            description: self.template(&text_or_empty(&doc["meta"]["description"])),
            ..Folder::default()
        };
        let mut environments = Vec::new();
        let base = &doc["environments"];
        if base.is_object() {
            folder.variables = self.vars(&base["data"]);
            for sub in base["subEnvironments"].as_array().into_iter().flatten() {
                environments.push(self.environment(sub));
            }
        }
        if kind.starts_with("environment.") {
            let mut list = environments;
            if !folder.variables.is_empty() {
                list.insert(
                    0,
                    NewEnvironment {
                        name: if folder.name.is_empty() {
                            "Base Environment".into()
                        } else {
                            folder.name.clone()
                        },
                        variables: folder.variables,
                        secrets: Vec::new(),
                    },
                );
            }
            return Ok(Import::Environments {
                environments: list,
                warnings: std::mem::take(&mut self.warnings),
            });
        }
        let items = self.v5_items(&doc["collection"], 0);
        Ok(self.finish(folder, items, environments, &version))
    }

    fn v5_items(&mut self, list: &Value, depth: usize) -> Vec<Item> {
        let mut entries: Vec<&Value> = list.as_array().into_iter().flatten().collect();
        entries.sort_by(|a, b| {
            let key = |v: &Value| v["meta"]["sortKey"].as_f64().unwrap_or(0.0);
            key(a).total_cmp(&key(b))
        });
        let mut items = Vec::new();
        for entry in entries {
            if entry.get("children").is_some() {
                if depth >= 32 {
                    continue;
                }
                let folder = self.folder(entry, &entry["meta"]);
                let children = self.v5_items(&entry["children"], depth + 1);
                items.push(Item::Folder(folder, children));
            } else if entry.get("method").is_some() && entry.get("url").is_some() {
                items.push(Item::Request(self.request(entry, true)));
            } else if entry.get("url").is_some() {
                self.skipped += 1;
            }
        }
        items
    }

    // ---- Shared by both versions -----------------------------------------

    fn finish(
        &mut self,
        folder: Folder,
        items: Vec<Item>,
        environments: Vec<NewEnvironment>,
        format: &str,
    ) -> Import {
        if self.skipped > 0 {
            self.warnings.push(format!(
                "skipped {}: Getman sends HTTP requests",
                super::count(
                    self.skipped,
                    "a gRPC or WebSocket request",
                    "gRPC and WebSocket requests"
                )
            ));
        }
        Import::Collection(Box::new(Collection {
            folder,
            items,
            environments,
            warnings: std::mem::take(&mut self.warnings),
            format: format.into(),
        }))
    }

    /// A folder's settings. `meta` holds the description in v5.
    fn folder(&mut self, source: &Value, meta: &Value) -> Folder {
        let name = text_or_empty(&source["name"]);
        let auth = self.auth(&source["authentication"], &name);
        Folder {
            description: self.template(&text_or_empty(&meta["description"])),
            variables: self.vars(&source["environment"]),
            headers: self.pairs(&source["headers"]),
            auth: auth.unwrap_or(Auth::Inherit),
            scripts: self.scripts(source, &name),
            name,
            ..Folder::default()
        }
    }

    fn request(&mut self, source: &Value, v5: bool) -> Request {
        let name = text(&source["name"])
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| "Untitled".into());
        let description = if v5 {
            text_or_empty(&source["meta"]["description"])
        } else {
            text_or_empty(&source["description"])
        };
        let mut request = Request {
            method: text(&source["method"])
                .unwrap_or_else(|| "GET".into())
                .to_ascii_uppercase(),
            description: self.template(&description),
            headers: self.pairs(&source["headers"]),
            ..Request::default()
        };
        let base = self.template(&text_or_empty(&source["url"]));
        let params = self.pairs(&source["parameters"]);
        request.url = super::with_query(&base, &params);
        request.disabled_params = params.into_iter().filter(|p| !p.enabled).collect();
        for param in source["pathParameters"].as_array().into_iter().flatten() {
            if let Some(key) = text(&param["name"]) {
                let value = self.template(&text_or_empty(&param["value"]));
                request.path_params.insert(key, value);
            }
        }
        request.body = self.body(&source["body"], &name);
        if let Some(auth) = self.auth(&source["authentication"], &name) {
            request.auth = auth;
        }
        request.scripts = self.scripts(source, &name);
        let follow = if v5 {
            source["settings"]["followRedirects"].as_str()
        } else {
            source["settingFollowRedirects"].as_str()
        };
        request.settings = Settings {
            follow_redirects: match follow {
                Some("on") => Some(true),
                Some("off") => Some(false),
                _ => None,
            },
            ..Settings::default()
        };
        super::drop_implied_content_type(&mut request);
        request.name = name;
        request
    }

    fn body(&mut self, body: &Value, name: &str) -> Body {
        let mime = text_or_empty(&body["mimeType"]);
        let essence = mime
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        let content = self.template(&text_or_empty(&body["text"]));
        match essence.as_str() {
            "application/x-www-form-urlencoded" => Body::Form {
                fields: self.pairs(&body["params"]),
            },
            "multipart/form-data" => {
                let mut fields = Vec::new();
                for param in body["params"].as_array().into_iter().flatten() {
                    let is_file = param["type"] == "file";
                    let field = text_or_empty(&param["name"]);
                    fields.push(FormField {
                        file: is_file.then(|| {
                            self.local_file(&text_or_empty(&param["fileName"]), &field, name)
                        }),
                        value: if is_file {
                            String::new()
                        } else {
                            self.template(&text_or_empty(&param["value"]))
                        },
                        content_type: None,
                        enabled: param["disabled"].as_bool() != Some(true),
                        name: field,
                    });
                }
                Body::Multipart { fields }
            }
            "application/graphql" => {
                let parsed: Value = serde_json::from_str(&content).unwrap_or(Value::Null);
                Body::Graphql {
                    query: text(&parsed["query"]).unwrap_or(content),
                    variables: match &parsed["variables"] {
                        Value::Null => String::new(),
                        Value::String(s) => s.clone(),
                        other => serde_json::to_string_pretty(other).unwrap_or_default(),
                    },
                }
            }
            _ if body.get("fileName").is_some() => Body::Binary {
                file: self.local_file(&text_or_empty(&body["fileName"]), "body", name),
                content_type: (!mime.is_empty()).then(|| mime.clone()),
            },
            _ if content.is_empty() => Body::None,
            _ => super::text_body(&mime, content),
        }
    }

    /// Files in exports are paths on the exporting machine; keep the file
    /// name and ask for the file to be placed in the workspace.
    fn local_file(&mut self, path: &str, field: &str, name: &str) -> String {
        if path.is_empty() {
            self.warnings.push(format!(
                "{name}: no file was chosen for {field} in the export"
            ));
            return String::new();
        }
        let file_name = path.rsplit(['/', '\\']).next().unwrap_or(path).to_owned();
        self.warnings.push(format!(
            "{name}: {field} uploads {path}; copy that file into the workspace as {file_name}"
        ));
        file_name
    }

    fn auth(&mut self, auth: &Value, name: &str) -> Option<Auth> {
        let get = |key: &str| text_or_empty(&auth[key]);
        let kind = auth["type"].as_str()?;
        if auth["disabled"].as_bool() == Some(true) {
            return Some(Auth::None);
        }
        Some(match kind {
            "none" => Auth::None,
            "inherit" => Auth::Inherit,
            "basic" => Auth::Basic {
                username: self.template(&get("username")),
                password: self.template(&get("password")),
            },
            "digest" => Auth::Digest {
                username: self.template(&get("username")),
                password: self.template(&get("password")),
            },
            "bearer" => Auth::Bearer {
                token: self.template(&get("token")),
                prefix: get("prefix"),
            },
            "apikey" => {
                let location = match auth["addTo"].as_str() {
                    Some("queryParams") => ApiKeyLocation::Query,
                    Some("cookie") => {
                        warn_once(
                            &mut self.warnings,
                            format!(
                                "{name}: API keys sent as a cookie are not supported; add a Cookie header instead"
                            ),
                        );
                        return None;
                    }
                    _ => ApiKeyLocation::Header,
                };
                Auth::ApiKey {
                    key: self.template(&get("key")),
                    value: self.template(&get("value")),
                    location,
                }
            }
            "oauth2" => {
                let grant = match auth["grantType"].as_str() {
                    Some("client_credentials") => OAuth2Grant::ClientCredentials,
                    Some("password") => OAuth2Grant::Password,
                    other => {
                        warn_once(
                            &mut self.warnings,
                            format!(
                                "{name}: OAuth 2.0 with the {} grant needs a browser; it now sends {{{{accessToken}}}} as a Bearer token",
                                other.unwrap_or("unnamed")
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
                    token_url: self.template(&get("accessTokenUrl")),
                    client_id: self.template(&get("clientId")),
                    client_secret: self.template(&get("clientSecret")),
                    scope: self.template(&get("scope")),
                    audience: self.template(&get("audience")),
                    username: self.template(&get("username")),
                    password: self.template(&get("password")),
                    client_auth: if auth["credentialsInBody"].as_bool() == Some(true) {
                        ClientAuth::Body
                    } else {
                        ClientAuth::Basic
                    },
                    ..OAuth2::default()
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

    fn scripts(&mut self, source: &Value, name: &str) -> Scripts {
        let pre = text(&source["preRequestScript"])
            .or_else(|| text(&source["scripts"]["preRequest"]))
            .unwrap_or_default();
        let post = text(&source["afterResponseScript"])
            .or_else(|| text(&source["scripts"]["afterResponse"]))
            .unwrap_or_default();
        let mut convert = |code: String| {
            if SCRIPT_API.is_match(&code) {
                warn_once(
                    &mut self.warnings,
                    format!(
                        "{name}: its scripts' insomnia.* calls now read gm.*; check that they still do what they did"
                    ),
                );
                SCRIPT_API.replace_all(&code, "gm.").into_owned()
            } else {
                code
            }
        };
        Scripts {
            pre_request: convert(pre),
            post_response: convert(post),
        }
    }

    fn pairs(&mut self, list: &Value) -> Vec<KeyValue> {
        let mut pairs = Vec::new();
        for item in list.as_array().into_iter().flatten() {
            let Some(name) = text(&item["name"]).filter(|n| !n.is_empty()) else {
                continue;
            };
            pairs.push(KeyValue {
                name: self.template(&name),
                value: self.template(&text_or_empty(&item["value"])),
                enabled: item["disabled"].as_bool() != Some(true),
                description: text_or_empty(&item["description"]),
            });
        }
        pairs
    }

    /// Environment data, which may nest: `{"db": {"host": "x"}}` becomes
    /// the variable `db.host`, as Insomnia reads it with `{{ _.db.host }}`.
    fn vars(&mut self, data: &Value) -> Vars {
        let mut vars = Vars::new();
        self.flatten("", data, &mut vars, 0);
        vars
    }

    fn flatten(&mut self, prefix: &str, value: &Value, vars: &mut Vars, depth: usize) {
        match value {
            Value::Object(map) if depth < 16 => {
                for (key, child) in map {
                    let name = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{prefix}.{key}")
                    };
                    self.flatten(&name, child, vars, depth + 1);
                }
            }
            Value::Null if prefix.is_empty() => {}
            Value::Array(_) | Value::Object(_) => {
                vars.insert(prefix.to_owned(), value.to_string());
            }
            other if !prefix.is_empty() => {
                let value = self.template(&text_or_empty(other));
                vars.insert(prefix.to_owned(), value);
            }
            _ => {}
        }
    }

    fn environment(&mut self, source: &Value) -> NewEnvironment {
        let variables = self.vars(&source["data"]);
        let secrets = if source["isPrivate"].as_bool() == Some(true) {
            variables.keys().cloned().collect()
        } else {
            Vec::new()
        };
        NewEnvironment {
            name: text(&source["name"]).unwrap_or_else(|| "Imported".into()),
            variables,
            secrets,
        }
    }

    /// Rewrites `{{ _.name }}` as `{{name}}` and the template tags Getman
    /// has a dynamic value for.
    fn template(&mut self, text: &str) -> String {
        if !text.contains("{{") && !text.contains("{%") {
            return text.to_owned();
        }
        let text = VARIABLE.replace_all(text, "{{$1}}");
        let mut unknown = Vec::new();
        let text = TAG.replace_all(&text, |caps: &regex::Captures<'_>| {
            let args = caps[2].replace(['\'', '"'], "");
            let args = args.trim();
            match (&caps[1], args) {
                ("uuid", _) => "{{$uuid}}".to_owned(),
                ("now", "unix") => "{{$timestamp}}".to_owned(),
                ("now", "millis") => "{{$timestampMs}}".to_owned(),
                ("now", _) => "{{$isoTimestamp}}".to_owned(),
                ("faker", "randomEmail") => "{{$randomEmail}}".to_owned(),
                ("faker", "randomFirstName") => "{{$randomFirstName}}".to_owned(),
                ("faker", "randomLastName") => "{{$randomLastName}}".to_owned(),
                ("faker", "randomInt") => "{{$randomInt}}".to_owned(),
                (tag, _) => {
                    unknown.push(tag.to_owned());
                    caps[0].to_owned()
                }
            }
        });
        for tag in unknown {
            warn_once(
                &mut self.warnings,
                format!(
                    "the {{% {tag} %}} template tag is not supported and was kept as written; use a variable or a script instead"
                ),
            );
        }
        text.into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::parse_document;

    const V4: &str = include_str!("../../tests/fixtures/import/insomnia-v4.json");
    const V5: &str = include_str!("../../tests/fixtures/import/insomnia-v5.yaml");

    fn collection(text: &str) -> Collection {
        let Import::Collection(c) = parse(&parse_document(text).unwrap()).unwrap() else {
            panic!("expected a collection")
        };
        *c
    }

    fn find<'c>(items: &'c [Item], name: &str) -> &'c Item {
        items
            .iter()
            .find(|item| match item {
                Item::Folder(folder, _) => folder.name == name,
                Item::Request(request) => request.name == name,
            })
            .unwrap_or_else(|| panic!("no item {name}"))
    }

    #[test]
    fn v4_exports_become_folders_requests_and_environments() {
        let c = collection(V4);
        assert_eq!(c.folder.name, "Shop API");
        assert_eq!(c.folder.variables["baseUrl"], "https://shop.example.com");
        assert_eq!(c.folder.variables["db.host"], "localhost");
        assert_eq!(c.environments.len(), 2);
        assert_eq!(c.environments[0].name, "Production");
        assert_eq!(c.environments[1].secrets, ["token"]);

        // Ordered by metaSortKey: the folder sorts before the request.
        let Item::Folder(folder, items) = &c.items[0] else {
            panic!("{:?}", c.items[0])
        };
        assert_eq!(folder.name, "Orders");
        assert_eq!(folder.variables["page"], "1");
        assert!(matches!(&folder.auth, Auth::Bearer { token, .. } if token == "{{token}}"));
        let Item::Request(list) = find(items, "List orders") else {
            panic!()
        };
        assert_eq!(list.url, "{{baseUrl}}/orders?page={{page}}");
        assert_eq!(list.disabled_params[0].name, "debug");
        assert!(list.auth.is_inherit());
        assert_eq!(list.settings.follow_redirects, Some(false));

        let Item::Request(create) = find(items, "Create order") else {
            panic!()
        };
        assert!(matches!(&create.body, Body::Json { content } if content.contains("{{$uuid}}")));
        assert!(create.scripts.post_response.contains("gm.test("));
        assert!(create.headers.is_empty());

        let Item::Request(login) = find(&c.items, "Login") else {
            panic!()
        };
        let Auth::Oauth2(oauth) = &login.auth else {
            panic!("{:?}", login.auth)
        };
        assert_eq!(oauth.grant, OAuth2Grant::Password);
        assert_eq!(oauth.client_auth, ClientAuth::Body);
        assert!(matches!(&login.body, Body::Form { fields } if fields[0].name == "user"));

        let Item::Request(upload) = find(&c.items, "Upload") else {
            panic!()
        };
        assert!(
            matches!(&upload.body, Body::Multipart { fields } if fields[0].file.as_deref() == Some("cat.png"))
        );

        let Item::Request(graph) = find(&c.items, "Graph") else {
            panic!()
        };
        assert!(
            matches!(&graph.body, Body::Graphql { query, variables } if query.contains("orders") && variables.contains("\"first\""))
        );

        let warnings = c.warnings.join("\n");
        assert!(warnings.contains("cat.png"), "{warnings}");
        assert!(warnings.contains("response"), "{warnings}");
        assert!(
            warnings.contains("a gRPC or WebSocket request"),
            "{warnings}"
        );
        assert!(warnings.contains("hawk"), "{warnings}");
    }

    #[test]
    fn v5_collections_are_read() {
        let c = collection(V5);
        assert_eq!(c.format, "Insomnia export v5.0");
        assert_eq!(c.folder.name, "Shop API");
        assert_eq!(c.folder.variables["baseUrl"], "https://shop.example.com");
        assert_eq!(c.environments[0].name, "Staging");
        assert_eq!(
            c.environments[0].variables["baseUrl"],
            "https://staging.example.com"
        );

        let Item::Folder(folder, items) = find(&c.items, "Users") else {
            panic!()
        };
        assert_eq!(folder.description, "People who buy things");
        assert_eq!(folder.headers[0].name, "X-Team");
        let Item::Request(get) = &items[0] else {
            panic!()
        };
        assert_eq!(get.name, "Get user");
        assert_eq!(get.url, "{{baseUrl}}/users/:id");
        assert_eq!(get.path_params["id"], "7");
        assert_eq!(get.scripts.pre_request, "gm.environment.set('a', 1);");
        let Item::Request(health) = &c.items[1] else {
            panic!()
        };
        assert!(
            matches!(&health.auth, Auth::ApiKey { location: ApiKeyLocation::Query, key, .. } if key == "key")
        );
    }
}
