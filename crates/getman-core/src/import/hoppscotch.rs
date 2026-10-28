//! Importing Hoppscotch collections and environments (JSON exports).
//!
//! A collection's folders become directories and its requests YAML files.
//! `<<name>>` variable references become `{{name}}`. Scripts are carried
//! over as they are; their `pw.*` calls need rewriting with `gm.*`.

use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use super::{Collection, Import, Item, NewEnvironment, text, text_or_empty, warn_once};
use crate::error::{Error, Result};
use crate::model::{
    ApiKeyLocation, Auth, Body, ClientAuth, Folder, FormField, KeyValue, OAuth2, OAuth2Grant,
    Request, Scripts, Vars,
};

static VARIABLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<<\s*([^<>\s]+)\s*>>").expect("variable pattern"));

/// Whether a parsed document is a Hoppscotch collection or environment
/// export, or a list of them.
pub(crate) fn matches(value: &Value) -> bool {
    match value {
        Value::Array(list) => list
            .first()
            .is_some_and(|first| is_collection(first) || is_environment(first)),
        other => is_collection(other) || is_environment(other),
    }
}

fn is_collection(value: &Value) -> bool {
    value["requests"].is_array() && value["folders"].is_array()
}

fn is_environment(value: &Value) -> bool {
    value["variables"].is_array()
        && value.get("requests").is_none()
        && value.get("values").is_none()
        && value.get("info").is_none()
}

/// Reads a Hoppscotch export, already parsed from JSON.
pub fn parse(doc: &Value) -> Result<Import> {
    let list: Vec<&Value> = match doc {
        Value::Array(list) => list.iter().collect(),
        other => vec![other],
    };
    let mut reader = Reader {
        warnings: Vec::new(),
    };
    if list.iter().all(|v| is_environment(v)) && !list.is_empty() {
        let environments = list.iter().map(|v| Reader::environment(v)).collect();
        return Ok(Import::Environments {
            environments,
            warnings: reader.warnings,
        });
    }
    let collections: Vec<&Value> = list.into_iter().filter(|v| is_collection(v)).collect();
    if collections.is_empty() {
        return Err(Error::Invalid(
            "this Hoppscotch export holds no collections or environments".into(),
        ));
    }
    let (folder, items) = if collections.len() == 1 {
        reader.collection(collections[0], 0)
    } else {
        let items = collections
            .iter()
            .map(|c| {
                let (folder, items) = reader.collection(c, 1);
                Item::Folder(folder, items)
            })
            .collect();
        (
            Folder {
                name: "Hoppscotch collections".into(),
                ..Folder::default()
            },
            items,
        )
    };
    Ok(Import::Collection(Box::new(Collection {
        folder,
        items,
        environments: Vec::new(),
        warnings: reader.warnings,
        format: "Hoppscotch collection".into(),
    })))
}

struct Reader {
    warnings: Vec<String>,
}

impl Reader {
    fn collection(&mut self, source: &Value, depth: usize) -> (Folder, Vec<Item>) {
        let name = text(&source["name"])
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| "Untitled".into());
        let folder = Folder {
            description: text_or_empty(&source["description"]),
            headers: Self::pairs(&source["headers"]),
            auth: self.auth(&source["auth"], &name).unwrap_or(Auth::Inherit),
            variables: Self::variables(&source["variables"]).0,
            scripts: self.scripts(source, &name),
            name,
            ..Folder::default()
        };
        let mut items = Vec::new();
        if depth < 32 {
            for child in source["folders"].as_array().into_iter().flatten() {
                let (folder, children) = self.collection(child, depth + 1);
                items.push(Item::Folder(folder, children));
            }
        }
        for request in source["requests"].as_array().into_iter().flatten() {
            items.push(Item::Request(self.request(request)));
        }
        (folder, items)
    }

    fn request(&mut self, source: &Value) -> Request {
        let name = text(&source["name"])
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| "Untitled".into());
        // GraphQL collections hold `url` and `query` instead.
        if source.get("query").is_some() && source.get("endpoint").is_none() {
            let mut request = Request {
                method: "POST".into(),
                url: template(&text_or_empty(&source["url"])),
                headers: Self::pairs(&source["headers"]),
                body: Body::Graphql {
                    query: template(&text_or_empty(&source["query"])),
                    variables: template(&text_or_empty(&source["variables"])),
                },
                ..Request::default()
            };
            if let Some(auth) = self.auth(&source["auth"], &name) {
                request.auth = auth;
            }
            request.name = name;
            return request;
        }
        let params = Self::pairs(&source["params"]);
        let mut request = Request {
            method: text(&source["method"])
                .unwrap_or_else(|| "GET".into())
                .to_ascii_uppercase(),
            description: text_or_empty(&source["description"]),
            headers: Self::pairs(&source["headers"]),
            variables: Self::variables(&source["requestVariables"]).0,
            scripts: self.scripts(source, &name),
            ..Request::default()
        };
        let base = template(&text_or_empty(&source["endpoint"]));
        request.url = super::with_query(&base, &params);
        request.disabled_params = params.into_iter().filter(|p| !p.enabled).collect();
        request.body = self.body(&source["body"], &name);
        if let Some(auth) = self.auth(&source["auth"], &name) {
            request.auth = auth;
        }
        super::drop_implied_content_type(&mut request);
        request.name = name;
        request
    }

    fn body(&mut self, body: &Value, name: &str) -> Body {
        let Some(content_type) = text(&body["contentType"]).filter(|t| !t.is_empty()) else {
            return Body::None;
        };
        let essence = content_type
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        match essence.as_str() {
            "application/x-www-form-urlencoded" => Body::Form {
                fields: match &body["body"] {
                    Value::Array(_) => Self::pairs(&body["body"]),
                    other => Self::raw_pairs(&text_or_empty(other)),
                },
            },
            "multipart/form-data" => {
                let mut fields = Vec::new();
                for field in body["body"].as_array().into_iter().flatten() {
                    let is_file = field["isFile"].as_bool() == Some(true);
                    fields.push(FormField {
                        name: template(&text_or_empty(&field["key"])),
                        value: if is_file {
                            String::new()
                        } else {
                            template(&text_or_empty(&field["value"]))
                        },
                        file: is_file.then(String::new),
                        content_type: text(&field["contentType"]).filter(|t| !t.is_empty()),
                        enabled: field["active"].as_bool() != Some(false),
                    });
                }
                if fields.iter().any(|f| f.file.is_some()) {
                    self.warnings.push(format!(
                        "{name}: exports do not hold the files of a multipart body; choose them again"
                    ));
                }
                Body::Multipart { fields }
            }
            "application/octet-stream" => {
                self.warnings.push(format!(
                    "{name}: exports do not hold the file of a binary body; choose it again"
                ));
                Body::Binary {
                    file: String::new(),
                    content_type: None,
                }
            }
            _ => {
                let content = template(&text_or_empty(&body["body"]));
                super::text_body(&content_type, content)
            }
        }
    }

    /// Hoppscotch keeps form bodies as `key: value` lines; a line that
    /// starts with `#` is switched off.
    fn raw_pairs(raw: &str) -> Vec<KeyValue> {
        let mut pairs = Vec::new();
        for line in raw.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let (enabled, line) = match line.strip_prefix('#') {
                Some(rest) => (false, rest.trim_start()),
                None => (true, line),
            };
            let (key, value) = line.split_once(':').unwrap_or((line, ""));
            pairs.push(KeyValue {
                enabled,
                ..KeyValue::new(template(key.trim()), template(value.trim()))
            });
        }
        pairs
    }

    fn auth(&mut self, auth: &Value, name: &str) -> Option<Auth> {
        let kind = auth["authType"].as_str()?;
        if auth["authActive"].as_bool() == Some(false) && kind != "inherit" {
            return Some(Auth::None);
        }
        let get = |key: &str| text_or_empty(&auth[key]);
        Some(match kind {
            "none" => Auth::None,
            "inherit" => Auth::Inherit,
            "basic" => Auth::Basic {
                username: template(&get("username")),
                password: template(&get("password")),
            },
            "digest" => Auth::Digest {
                username: template(&get("username")),
                password: template(&get("password")),
            },
            "bearer" => Auth::Bearer {
                token: template(&get("token")),
                prefix: String::new(),
            },
            "api-key" => Auth::ApiKey {
                key: template(&get("key")),
                value: template(&get("value")),
                location: if get("addTo").to_ascii_lowercase().contains("query") {
                    ApiKeyLocation::Query
                } else {
                    ApiKeyLocation::Header
                },
            },
            "oauth-2" => {
                let info = &auth["grantTypeInfo"];
                let field = |key: &str| text_or_empty(&info[key]);
                let grant = match info["grantType"].as_str() {
                    Some("CLIENT_CREDENTIALS") => OAuth2Grant::ClientCredentials,
                    Some("PASSWORD") => OAuth2Grant::Password,
                    other => {
                        let token = text(&info["token"])
                            .or_else(|| text(&auth["token"]))
                            .filter(|t| !t.is_empty());
                        warn_once(
                            &mut self.warnings,
                            format!(
                                "{name}: OAuth 2.0 with the {} grant needs a browser; it now sends {} as a Bearer token",
                                other
                                    .unwrap_or("authorization code")
                                    .to_ascii_lowercase()
                                    .replace('_', " "),
                                if token.is_some() {
                                    "the saved token"
                                } else {
                                    "{{accessToken}}"
                                }
                            ),
                        );
                        return Some(Auth::Bearer {
                            token: token.unwrap_or_else(|| "{{accessToken}}".into()),
                            prefix: String::new(),
                        });
                    }
                };
                Auth::Oauth2(OAuth2 {
                    grant,
                    token_url: template(&field("tokenEndpoint")),
                    client_id: template(&field("clientID")),
                    client_secret: template(&field("clientSecret")),
                    scope: template(&field("scopes")),
                    audience: String::new(),
                    username: template(&field("username")),
                    password: template(&field("password")),
                    client_auth: if field("clientAuthentication") == "IN_BODY" {
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
        let scripts = Scripts {
            pre_request: text_or_empty(&source["preRequestScript"]),
            post_response: text_or_empty(&source["testScript"]),
        };
        let uses_api = |code: &str| code.contains("pw.") || code.contains("hopp.");
        if uses_api(&scripts.pre_request) || uses_api(&scripts.post_response) {
            warn_once(
                &mut self.warnings,
                format!(
                    "{name}: its scripts call pw.* or hopp.*, which Getman does not have; rewrite them with gm.*"
                ),
            );
        }
        scripts
    }

    fn pairs(list: &Value) -> Vec<KeyValue> {
        let mut pairs = Vec::new();
        for item in list.as_array().into_iter().flatten() {
            let Some(key) = text(&item["key"]).filter(|k| !k.is_empty()) else {
                continue;
            };
            pairs.push(KeyValue {
                name: template(&key),
                value: template(&text_or_empty(&item["value"])),
                enabled: item["active"].as_bool() != Some(false),
                description: text_or_empty(&item["description"]),
            });
        }
        pairs
    }

    /// Variables, and which of them are secret. Newer exports keep an
    /// initial and a current value; the current one wins when set.
    fn variables(list: &Value) -> (Vars, Vec<String>) {
        let mut vars = Vars::new();
        let mut secrets = Vec::new();
        for item in list.as_array().into_iter().flatten() {
            let Some(key) = text(&item["key"]).filter(|k| !k.is_empty()) else {
                continue;
            };
            if item["active"].as_bool() == Some(false) {
                continue;
            }
            let value = text(&item["currentValue"])
                .filter(|v| !v.is_empty())
                .or_else(|| text(&item["initialValue"]))
                .or_else(|| text(&item["value"]))
                .unwrap_or_default();
            if item["secret"].as_bool() == Some(true) {
                secrets.push(key.clone());
            }
            vars.insert(key, template(&value));
        }
        (vars, secrets)
    }

    fn environment(source: &Value) -> NewEnvironment {
        let (variables, secrets) = Self::variables(&source["variables"]);
        NewEnvironment {
            name: text(&source["name"])
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| "Globals".into()),
            variables,
            secrets,
        }
    }
}

/// Rewrites `<<name>>` references as `{{name}}`.
fn template(text: &str) -> String {
    if text.contains("<<") {
        VARIABLE.replace_all(text, "{{$1}}").into_owned()
    } else {
        text.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::parse_document;

    const COLLECTION: &str = include_str!("../../tests/fixtures/import/hoppscotch-collection.json");
    const ENVIRONMENTS: &str =
        include_str!("../../tests/fixtures/import/hoppscotch-environments.json");

    #[test]
    fn collections_become_folders_and_requests() {
        let Import::Collection(c) = parse(&parse_document(COLLECTION).unwrap()).unwrap() else {
            panic!()
        };
        assert_eq!(c.folder.name, "Shop");
        assert!(matches!(&c.folder.auth, Auth::Bearer { token, .. } if token == "{{token}}"));
        assert_eq!(c.folder.headers[0].name, "X-Team");

        let Item::Folder(folder, items) = &c.items[0] else {
            panic!("{:?}", c.items[0])
        };
        assert_eq!(folder.name, "Orders");
        let Item::Request(list) = &items[0] else {
            panic!()
        };
        assert_eq!(list.url, "{{baseUrl}}/orders?page=1");
        assert_eq!(list.disabled_params[0].name, "debug");
        assert!(list.auth.is_inherit());
        assert_eq!(list.variables["limit"], "10");

        let Item::Request(create) = &items[1] else {
            panic!()
        };
        assert!(matches!(&create.body, Body::Json { content } if content.contains("BOOK-42")));
        assert!(create.headers.is_empty());
        assert!(create.scripts.post_response.contains("pw.test"));

        let Item::Request(login) = &c.items[1] else {
            panic!()
        };
        let Body::Form { fields } = &login.body else {
            panic!("{:?}", login.body)
        };
        assert_eq!(fields.len(), 2);
        assert!(!fields[1].enabled);
        let Auth::Oauth2(oauth) = &login.auth else {
            panic!("{:?}", login.auth)
        };
        assert_eq!(oauth.grant, OAuth2Grant::ClientCredentials);
        assert_eq!(oauth.token_url, "https://id.example.com/token");
        assert_eq!(oauth.client_auth, ClientAuth::Body);

        let Item::Request(upload) = &c.items[2] else {
            panic!()
        };
        assert!(matches!(&upload.body, Body::Multipart { fields } if fields[0].file.is_some()));
        assert!(matches!(
            &upload.auth,
            Auth::ApiKey {
                location: ApiKeyLocation::Query,
                ..
            }
        ));

        let warnings = c.warnings.join("\n");
        assert!(warnings.contains("pw.*"), "{warnings}");
        assert!(warnings.contains("choose them again"), "{warnings}");
        assert!(warnings.contains("aws-signature"), "{warnings}");
    }

    #[test]
    fn environments_keep_secrets_apart() {
        let Import::Environments { environments, .. } =
            parse(&parse_document(ENVIRONMENTS).unwrap()).unwrap()
        else {
            panic!()
        };
        assert_eq!(environments.len(), 2);
        assert_eq!(environments[0].name, "Staging");
        assert_eq!(
            environments[0].variables["baseUrl"],
            "https://staging.example.com"
        );
        assert_eq!(environments[0].secrets, ["token"]);
        assert_eq!(environments[1].variables["host"], "{{base}}.example.com");
    }
}
