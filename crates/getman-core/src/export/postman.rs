//! Exporting a folder or the workspace as a Postman collection (format
//! v2.1), and environments and globals as Postman environment files.
//!
//! What Postman has no place for is translated where it can be: folder
//! headers and request variables become pre-request script lines, and
//! checks and captures become `pm.test` and `pm.*.set` lines in the test
//! script. Values kept in git-ignored local files are left out unless
//! asked for.

use serde_json::{Map, Value, json};

use super::{Exported, chain_of, items_of};
use crate::checks::Check;
use crate::error::Result;
use crate::model::{
    ApiKeyLocation, Auth, Body, ClientAuth, Folder, HawkAlgorithm, JwtAlgorithm, KeyValue,
    OAuth1Location, OAuth1Method, OAuth2Grant, Pkce, Request, Scripts, Settings, Vars,
};
use crate::workspace::{Node, Workspace, slugify};

const SCHEMA: &str = "https://schema.getpostman.com/json/collection/v2.1.0/collection.json";
const EXPORTED_USING: &str = concat!("Getman/", env!("CARGO_PKG_VERSION"));

/// Exports a folder, or the whole workspace for `""`, as a collection.
///
/// A folder carries what it inherits: the variables, headers,
/// authentication and scripts of the folders around it and of the
/// workspace become the collection's own.
pub fn collection(workspace: &Workspace, folder: &str) -> Result<Exported> {
    let chain = chain_of(workspace, folder)?;
    let meta = chain.last().cloned().unwrap_or_default();
    let name = if folder.is_empty() {
        workspace.name()
    } else {
        meta.name.clone()
    };
    let mut writer = Writer {
        workspace,
        warnings: Vec::new(),
    };

    // The collection's own settings, merged down the chain.
    let mut variables = Vars::new();
    let mut headers: Vec<KeyValue> = Vec::new();
    let mut auth = Auth::Inherit;
    let mut pre = Vec::new();
    let mut post = Vec::new();
    for level in &chain {
        variables.extend(level.variables.clone());
        for header in level.headers.iter().filter(|h| h.enabled) {
            headers.retain(|h| !h.name.eq_ignore_ascii_case(&header.name));
            headers.push(header.clone());
        }
        if !level.auth.is_inherit() {
            auth = level.auth.clone();
        }
        if !level.scripts.pre_request.trim().is_empty() {
            pre.push(level.scripts.pre_request.clone());
        }
        if !level.scripts.post_response.trim().is_empty() {
            post.push(level.scripts.post_response.clone());
        }
        writer.settings_warning(&level.settings, &level.name);
    }
    let scripts = Scripts {
        pre_request: pre.join("\n\n"),
        post_response: post.join("\n\n"),
    };

    let mut out = Map::new();
    let mut info = Map::new();
    info.insert("_postman_id".into(), crate::dynamic::uuid_v4().into());
    info.insert("name".into(), name.clone().into());
    if !meta.description.is_empty() {
        info.insert("description".into(), meta.description.clone().into());
    }
    info.insert("schema".into(), SCHEMA.into());
    out.insert("info".into(), Value::Object(info));
    out.insert(
        "item".into(),
        Value::Array(writer.items(&items_of(workspace, folder)?)?),
    );
    if let Some(auth) = writer.auth(&auth, &name) {
        out.insert("auth".into(), auth);
    }
    let events = events(&scripts, &headers, &Vars::new(), &[]);
    if !events.is_empty() {
        out.insert("event".into(), Value::Array(events));
    }
    if !variables.is_empty() {
        out.insert("variable".into(), variable_list(&variables));
    }
    let content = serde_json::to_string_pretty(&Value::Object(out)).unwrap_or_default();
    Ok(Exported {
        file_name: format!("{}.postman_collection.json", slugify(&name)),
        content,
        warnings: writer.warnings,
    })
}

/// Exports an environment. Values kept in its git-ignored local file,
/// such as secrets, are left out unless `with_secrets` is set.
pub fn environment(workspace: &Workspace, id: &str, with_secrets: bool) -> Result<Exported> {
    let files = workspace.read_environment(id)?;
    let mut warnings = Vec::new();
    let mut values = Vec::new();
    let mut names: Vec<&String> = files.shared.variables.keys().collect();
    if with_secrets {
        names.extend(
            files
                .local
                .keys()
                .filter(|k| !files.shared.variables.contains_key(*k)),
        );
    }
    let mut left_out = 0;
    for name in names {
        let secret = files.shared.secrets.contains(name);
        let shared = files
            .shared
            .variables
            .get(name)
            .cloned()
            .unwrap_or_default();
        let local = files.local.get(name);
        let value = match local {
            Some(local) if with_secrets => local.clone(),
            Some(_) if secret || shared.is_empty() => {
                left_out += 1;
                String::new()
            }
            _ => shared,
        };
        values.push(json!({
            "key": name,
            "value": value,
            "type": if secret { "secret" } else { "default" },
            "enabled": true,
        }));
    }
    if left_out > 0 {
        warnings.push(format!(
            "{} kept in {id}.local.yaml {} left out; export with secrets to include {}",
            crate::import::count(left_out, "value", "values"),
            if left_out == 1 { "was" } else { "were" },
            if left_out == 1 { "it" } else { "them" },
        ));
    }
    let document = json!({
        "id": crate::dynamic::uuid_v4(),
        "name": files.shared.name,
        "values": values,
        "_postman_variable_scope": "environment",
        "_postman_exported_at": crate::time::iso8601(std::time::SystemTime::now()),
        "_postman_exported_using": EXPORTED_USING,
    });
    Ok(Exported {
        file_name: format!("{id}.postman_environment.json"),
        content: serde_json::to_string_pretty(&document).unwrap_or_default(),
        warnings,
    })
}

/// Exports the globals. They live in the git-ignored `getman.local.yaml`,
/// so their values are left out unless `with_secrets` is set.
pub fn globals(workspace: &Workspace, with_secrets: bool) -> Result<Exported> {
    let state = workspace.read_local_state()?;
    let values: Vec<Value> = state
        .globals
        .iter()
        .map(|(key, value)| {
            json!({
                "key": key,
                "value": if with_secrets { value.as_str() } else { "" },
                "type": "default",
                "enabled": true,
            })
        })
        .collect();
    let mut warnings = Vec::new();
    if !with_secrets && state.globals.values().any(|v| !v.is_empty()) {
        warnings.push(
            "the globals' values live in getman.local.yaml and were left out; export with secrets to include them"
                .into(),
        );
    }
    let document = json!({
        "id": crate::dynamic::uuid_v4(),
        "name": "Globals",
        "values": values,
        "_postman_variable_scope": "globals",
        "_postman_exported_at": crate::time::iso8601(std::time::SystemTime::now()),
        "_postman_exported_using": EXPORTED_USING,
    });
    Ok(Exported {
        file_name: "globals.postman_globals.json".into(),
        content: serde_json::to_string_pretty(&document).unwrap_or_default(),
        warnings,
    })
}

struct Writer<'a> {
    workspace: &'a Workspace,
    warnings: Vec<String>,
}

impl Writer<'_> {
    fn items(&mut self, nodes: &[Node]) -> Result<Vec<Value>> {
        let mut items = Vec::new();
        for node in nodes {
            match node {
                Node::Folder {
                    path,
                    name,
                    children,
                } => {
                    let folder = self.workspace.read_folder(path)?;
                    items.push(self.folder(&folder, name, children)?);
                }
                Node::Request {
                    path, error: None, ..
                } => {
                    let request = self.workspace.read_request(path)?;
                    items.push(self.request(&request));
                }
                Node::Request {
                    path,
                    error: Some(error),
                    ..
                } => self.warnings.push(format!(
                    "{path} could not be read and was left out: {error}"
                )),
            }
        }
        Ok(items)
    }

    fn folder(&mut self, folder: &Folder, name: &str, children: &[Node]) -> Result<Value> {
        let mut item = Map::new();
        item.insert("name".into(), name.into());
        if !folder.description.is_empty() {
            item.insert("description".into(), folder.description.clone().into());
        }
        item.insert("item".into(), Value::Array(self.items(children)?));
        if let Some(auth) = self.auth(&folder.auth, name) {
            item.insert("auth".into(), auth);
        }
        let headers: Vec<KeyValue> = folder
            .headers
            .iter()
            .filter(|h| h.enabled)
            .cloned()
            .collect();
        let events = events(&folder.scripts, &headers, &Vars::new(), &[]);
        if !events.is_empty() {
            item.insert("event".into(), Value::Array(events));
        }
        if !folder.variables.is_empty() {
            item.insert("variable".into(), variable_list(&folder.variables));
        }
        self.settings_warning(&folder.settings, name);
        Ok(Value::Object(item))
    }

    fn request(&mut self, request: &Request) -> Value {
        let name = request.name.as_str();
        let mut source = Map::new();
        source.insert("method".into(), request.method.to_ascii_uppercase().into());
        let mut header: Vec<Value> = request.headers.iter().map(pair).collect();
        let body = self.body(&request.body, name, &mut header);
        source.insert("header".into(), Value::Array(header));
        source.insert("url".into(), url(request));
        if let Some(body) = body {
            source.insert("body".into(), body);
        }
        if let Some(auth) = self.auth(&request.auth, name) {
            source.insert("auth".into(), auth);
        }
        if !request.description.is_empty() {
            source.insert("description".into(), request.description.clone().into());
        }

        let mut item = Map::new();
        item.insert("name".into(), name.into());
        let (tests, untranslated) = checks_script(&request.checks, &request.captures);
        for line in untranslated {
            self.warnings.push(format!(
                "{name}: the check `{line}` has no Postman equivalent and was left as a comment"
            ));
        }
        let events = events(&request.scripts, &[], &request.variables, &tests);
        if !events.is_empty() {
            item.insert("event".into(), Value::Array(events));
        }
        item.insert("request".into(), Value::Object(source));
        let behavior = behavior(&request.settings);
        if !behavior.is_empty() {
            item.insert("protocolProfileBehavior".into(), Value::Object(behavior));
        }
        if request.settings.timeout_ms.is_some() {
            self.warnings.push(format!(
                "{name}: Postman has no timeout per request; the timeout was left out"
            ));
        }
        Value::Object(item)
    }

    fn body(&mut self, body: &Body, name: &str, headers: &mut Vec<Value>) -> Option<Value> {
        let raw = |content: &str, language: &str| json!({ "mode": "raw", "raw": content, "options": { "raw": { "language": language } } });
        Some(match body {
            Body::None => return None,
            Body::Json { content } => raw(content, "json"),
            Body::Xml { content } => raw(content, "xml"),
            Body::Text {
                content,
                content_type,
            } => {
                // Postman sends raw text as text/plain unless told.
                let has_type = headers.iter().any(|h| {
                    h["key"]
                        .as_str()
                        .is_some_and(|k| k.eq_ignore_ascii_case("content-type"))
                });
                if let Some(content_type) = content_type.as_deref().filter(|_| !has_type) {
                    headers.push(json!({ "key": "Content-Type", "value": content_type }));
                }
                raw(content, "text")
            }
            Body::Form { fields } => json!({
                "mode": "urlencoded",
                "urlencoded": fields.iter().map(pair).collect::<Vec<_>>(),
            }),
            Body::Multipart { fields } => {
                let list: Vec<Value> = fields
                    .iter()
                    .map(|field| {
                        let mut entry = Map::new();
                        entry.insert("key".into(), field.name.clone().into());
                        match &field.file {
                            Some(file) => {
                                entry.insert("type".into(), "file".into());
                                entry.insert("src".into(), file.clone().into());
                            }
                            None => {
                                entry.insert("type".into(), "text".into());
                                entry.insert("value".into(), field.value.clone().into());
                            }
                        }
                        if let Some(content_type) = &field.content_type {
                            entry.insert("contentType".into(), content_type.clone().into());
                        }
                        if !field.enabled {
                            entry.insert("disabled".into(), true.into());
                        }
                        Value::Object(entry)
                    })
                    .collect();
                if fields.iter().any(|f| f.file.is_some()) {
                    self.warnings.push(format!(
                        "{name}: Postman reads form files from its working directory; files are named relative to the workspace"
                    ));
                }
                json!({ "mode": "formdata", "formdata": list })
            }
            Body::Binary { file, content_type } => {
                if let Some(content_type) = content_type {
                    headers.push(json!({ "key": "Content-Type", "value": content_type }));
                }
                json!({ "mode": "file", "file": { "src": file } })
            }
            Body::Graphql {
                query, variables, ..
            } => json!({
                "mode": "graphql",
                "graphql": { "query": query, "variables": variables },
            }),
        })
    }

    fn auth(&mut self, auth: &Auth, name: &str) -> Option<Value> {
        let list = |pairs: &[(&str, &str)]| -> Value {
            Value::Array(
                pairs
                    .iter()
                    .map(|(key, value)| json!({ "key": key, "value": value, "type": "string" }))
                    .collect(),
            )
        };
        Some(match auth {
            Auth::Inherit => return None,
            Auth::None => json!({ "type": "noauth" }),
            Auth::Basic { username, password } => json!({
                "type": "basic",
                "basic": list(&[("username", username), ("password", password)]),
            }),
            Auth::Bearer { token, prefix } if prefix.trim().is_empty() || prefix == "Bearer" => {
                json!({ "type": "bearer", "bearer": list(&[("token", token)]) })
            }
            // Postman's Bearer has no other scheme; send the same header.
            Auth::Bearer { token, prefix } => {
                let value = format!("{} {token}", prefix.trim());
                json!({
                    "type": "apikey",
                    "apikey": list(&[("key", "Authorization"), ("value", &value), ("in", "header")]),
                })
            }
            Auth::ApiKey {
                key,
                value,
                location,
            } => json!({
                "type": "apikey",
                "apikey": list(&[
                    ("key", key),
                    ("value", value),
                    ("in", if *location == ApiKeyLocation::Query { "query" } else { "header" }),
                ]),
            }),
            Auth::Digest { username, password } => json!({
                "type": "digest",
                "digest": list(&[("username", username), ("password", password)]),
            }),
            Auth::AwsSigV4(aws) => {
                if aws.unsigned_payload {
                    self.warnings.push(format!(
                        "{name}: Postman always signs the payload; the unsigned payload setting was left out"
                    ));
                }
                json!({
                    "type": "awsv4",
                    "awsv4": entries(vec![
                        ("accessKey", aws.access_key.as_str().into()),
                        ("secretKey", aws.secret_key.as_str().into()),
                        ("sessionToken", aws.session_token.as_str().into()),
                        ("region", aws.region.as_str().into()),
                        ("service", aws.service.as_str().into()),
                        ("addAuthDataToQuery", (aws.location == ApiKeyLocation::Query).into()),
                    ]),
                })
            }
            Auth::Oauth1(oauth) => {
                self.key_file_warning(&oauth.private_key_file, name);
                if oauth.location == OAuth1Location::Body {
                    self.warnings.push(format!(
                        "{name}: Postman puts OAuth 1.0 parameters in the header or the URL; they now go in the URL"
                    ));
                }
                json!({
                    "type": "oauth1",
                    "oauth1": entries(vec![
                        ("signatureMethod", oauth1_method(oauth.signature_method).into()),
                        ("consumerKey", oauth.consumer_key.as_str().into()),
                        ("consumerSecret", oauth.consumer_secret.as_str().into()),
                        ("token", oauth.token.as_str().into()),
                        ("tokenSecret", oauth.token_secret.as_str().into()),
                        ("privateKey", oauth.private_key.as_str().into()),
                        ("realm", oauth.realm.as_str().into()),
                        ("callback", oauth.callback.as_str().into()),
                        ("verifier", oauth.verifier.as_str().into()),
                        ("version", or(&oauth.version, "1.0").into()),
                        ("timestamp", oauth.timestamp.as_str().into()),
                        ("nonce", oauth.nonce.as_str().into()),
                        ("includeBodyHash", oauth.body_hash.into()),
                        ("addParamsToHeader", (oauth.location == OAuth1Location::Header).into()),
                    ]),
                })
            }
            Auth::Jwt(jwt) => {
                self.key_file_warning(&jwt.private_key_file, name);
                if jwt.issued_at || jwt.expires_in.is_some() {
                    self.warnings.push(format!(
                        "{name}: Postman does not add iat or exp to a JWT; add them to the payload"
                    ));
                }
                let add_to = if jwt.location == ApiKeyLocation::Query {
                    "queryParam"
                } else {
                    "header"
                };
                json!({
                    "type": "jwt",
                    "jwt": entries(vec![
                        ("algorithm", jwt_algorithm(jwt.algorithm).into()),
                        ("secret", jwt.secret.as_str().into()),
                        ("isSecretBase64Encoded", jwt.secret_base64.into()),
                        ("privateKey", jwt.private_key.as_str().into()),
                        ("header", or(&jwt.header, "{}").into()),
                        ("payload", or(&jwt.payload, "{}").into()),
                        ("addTokenTo", add_to.into()),
                        ("headerPrefix", or(&jwt.prefix, "Bearer").into()),
                        ("queryParamKey", or(&jwt.param, "token").into()),
                    ]),
                })
            }
            Auth::Hawk(hawk) => {
                let algorithm = if hawk.algorithm == HawkAlgorithm::Sha1 {
                    "sha1"
                } else {
                    "sha256"
                };
                json!({
                    "type": "hawk",
                    "hawk": entries(vec![
                        ("authId", hawk.id.as_str().into()),
                        ("authKey", hawk.key.as_str().into()),
                        ("algorithm", algorithm.into()),
                        ("extraData", hawk.ext.as_str().into()),
                        ("app", hawk.app.as_str().into()),
                        ("delegation", hawk.dlg.as_str().into()),
                        ("includePayloadHash", hawk.payload_hash.into()),
                    ]),
                })
            }
            Auth::Edgegrid(edgegrid) => json!({
                "type": "edgegrid",
                "edgegrid": entries(vec![
                    ("clientToken", edgegrid.client_token.as_str().into()),
                    ("clientSecret", edgegrid.client_secret.as_str().into()),
                    ("accessToken", edgegrid.access_token.as_str().into()),
                    ("baseURL", edgegrid.base_url.as_str().into()),
                    ("headersToSign", edgegrid.headers_to_sign.join(",").into()),
                    (
                        "maxBody",
                        edgegrid.max_body.map(|n| n.to_string()).unwrap_or_default().into(),
                    ),
                ]),
            }),
            Auth::Asap(asap) => {
                self.key_file_warning(&asap.private_key_file, name);
                json!({
                    "type": "asap",
                    "asap": entries(vec![
                        ("alg", asap.algorithm.map_or("RS256", jwt_algorithm).into()),
                        ("iss", asap.issuer.as_str().into()),
                        ("sub", asap.subject.as_str().into()),
                        ("aud", asap.audience.as_str().into()),
                        ("kid", asap.key_id.as_str().into()),
                        ("privateKey", asap.private_key.as_str().into()),
                        ("claims", or(&asap.claims, "{}").into()),
                        (
                            "exp",
                            asap.expires_in.map(|s| s.to_string()).unwrap_or_default().into(),
                        ),
                    ]),
                })
            }
            Auth::Oauth2(oauth) => {
                let grant = match oauth.grant {
                    OAuth2Grant::ClientCredentials => "client_credentials",
                    OAuth2Grant::Password => "password_credentials",
                    OAuth2Grant::AuthorizationCode if oauth.pkce == Pkce::Off => {
                        "authorization_code"
                    }
                    OAuth2Grant::AuthorizationCode => "authorization_code_with_pkce",
                    OAuth2Grant::Implicit => "implicit",
                };
                let client_auth = if oauth.client_auth == ClientAuth::Body {
                    "body"
                } else {
                    "header"
                };
                let add_to = if oauth.token_in == ApiKeyLocation::Query {
                    "queryParams"
                } else {
                    "header"
                };
                let mut pairs: Vec<(&str, Value)> = vec![
                    ("grant_type", grant.into()),
                    ("accessTokenUrl", oauth.token_url.as_str().into()),
                    ("clientId", oauth.client_id.as_str().into()),
                    ("clientSecret", oauth.client_secret.as_str().into()),
                    ("scope", oauth.scope.as_str().into()),
                    ("client_authentication", client_auth.into()),
                    ("addTokenTo", add_to.into()),
                    ("headerPrefix", or(&oauth.prefix, "Bearer").into()),
                ];
                if oauth.grant.uses_browser() {
                    pairs.push(("authUrl", oauth.auth_url.as_str().into()));
                }
                if !oauth.callback_url.is_empty() {
                    pairs.push(("redirect_uri", oauth.callback_url.as_str().into()));
                }
                match oauth.pkce {
                    Pkce::Off => {}
                    Pkce::S256 => pairs.push(("challengeAlgorithm", "S256".into())),
                    Pkce::Plain => pairs.push(("challengeAlgorithm", "plain".into())),
                }
                if !oauth.audience.is_empty() {
                    pairs.push(("audience", oauth.audience.as_str().into()));
                }
                if !oauth.resource.is_empty() {
                    pairs.push(("resource", oauth.resource.as_str().into()));
                }
                if oauth.grant == OAuth2Grant::Password {
                    pairs.push(("username", oauth.username.as_str().into()));
                    pairs.push(("password", oauth.password.as_str().into()));
                }
                if !oauth.auth_params.is_empty() {
                    pairs.push(("authRequestParams", params(&oauth.auth_params)));
                }
                if !oauth.token_params.is_empty() {
                    pairs.push(("tokenRequestParams", params(&oauth.token_params)));
                }
                if !oauth.audience.is_empty() {
                    self.warnings.push(format!(
                        "{name}: Postman sends the OAuth 2.0 audience only in some versions; check the token request"
                    ));
                }
                json!({ "type": "oauth2", "oauth2": entries(pairs) })
            }
        })
    }

    /// Exports never carry a private key file: the key would end up in a
    /// file people share.
    fn key_file_warning(&mut self, file: &str, name: &str) {
        if !file.is_empty() {
            self.warnings.push(format!(
                "{name}: the private key in {file} was left out; paste it into Postman"
            ));
        }
    }

    fn settings_warning(&mut self, settings: &Settings, name: &str) {
        if !settings.is_empty() {
            self.warnings.push(format!(
                "{name}: settings on a folder or the workspace have no Postman equivalent; set them on the requests"
            ));
        }
    }
}

/// Postman's list of auth settings, with switches as booleans.
fn entries(pairs: Vec<(&str, Value)>) -> Value {
    Value::Array(
        pairs
            .into_iter()
            .map(|(key, value)| {
                let kind = if value.is_boolean() {
                    "boolean"
                } else {
                    "string"
                };
                json!({ "key": key, "value": value, "type": kind })
            })
            .collect(),
    )
}

/// Extra OAuth 2.0 parameters, as Postman lists them.
fn params(items: &[KeyValue]) -> Value {
    Value::Array(
        items
            .iter()
            .map(|item| json!({ "key": item.name, "value": item.value, "enabled": item.enabled }))
            .collect(),
    )
}

/// `value`, or `default` when it is empty: Postman writes defaults out.
fn or<'a>(value: &'a str, default: &'a str) -> &'a str {
    if value.trim().is_empty() {
        default
    } else {
        value
    }
}

fn jwt_algorithm(algorithm: JwtAlgorithm) -> &'static str {
    match algorithm {
        JwtAlgorithm::Hs256 => "HS256",
        JwtAlgorithm::Hs384 => "HS384",
        JwtAlgorithm::Hs512 => "HS512",
        JwtAlgorithm::Rs256 => "RS256",
        JwtAlgorithm::Rs384 => "RS384",
        JwtAlgorithm::Rs512 => "RS512",
        JwtAlgorithm::Ps256 => "PS256",
        JwtAlgorithm::Ps384 => "PS384",
        JwtAlgorithm::Ps512 => "PS512",
        JwtAlgorithm::Es256 => "ES256",
        JwtAlgorithm::Es384 => "ES384",
    }
}

fn oauth1_method(method: OAuth1Method) -> &'static str {
    match method {
        OAuth1Method::HmacSha1 => "HMAC-SHA1",
        OAuth1Method::HmacSha256 => "HMAC-SHA256",
        OAuth1Method::HmacSha512 => "HMAC-SHA512",
        OAuth1Method::Plaintext => "PLAINTEXT",
        OAuth1Method::RsaSha1 => "RSA-SHA1",
        OAuth1Method::RsaSha256 => "RSA-SHA256",
        OAuth1Method::RsaSha512 => "RSA-SHA512",
    }
}

/// A header, query parameter or form field.
fn pair(item: &KeyValue) -> Value {
    let mut entry = Map::new();
    entry.insert("key".into(), item.name.clone().into());
    entry.insert("value".into(), item.value.clone().into());
    if !item.enabled {
        entry.insert("disabled".into(), true.into());
    }
    if !item.description.is_empty() {
        entry.insert("description".into(), item.description.clone().into());
    }
    Value::Object(entry)
}

fn variable_list(vars: &Vars) -> Value {
    Value::Array(
        vars.iter()
            .map(|(key, value)| json!({ "key": key, "value": value, "type": "string" }))
            .collect(),
    )
}

/// Postman's URL object: the text and its parts.
fn url(request: &Request) -> Value {
    let raw = request.url.trim();
    let (before_fragment, _) = raw.split_once('#').unwrap_or((raw, ""));
    let (base, query) = before_fragment
        .split_once('?')
        .unwrap_or((before_fragment, ""));
    let (protocol, rest) = match base.split_once("://") {
        Some((protocol, rest)) => (Some(protocol), rest),
        None => (None, base),
    };
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) => {
            (host, Some(port))
        }
        _ => (authority, None),
    };
    let mut out = Map::new();
    out.insert("raw".into(), raw.into());
    if let Some(protocol) = protocol {
        out.insert("protocol".into(), protocol.into());
    }
    let host: Vec<Value> = if host.starts_with("{{") {
        vec![host.into()]
    } else {
        host.split('.').map(Value::from).collect()
    };
    out.insert("host".into(), Value::Array(host));
    if let Some(port) = port {
        out.insert("port".into(), port.into());
    }
    if !path.is_empty() {
        out.insert(
            "path".into(),
            Value::Array(path.split('/').map(Value::from).collect()),
        );
    }
    let mut params: Vec<Value> = query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (key, value) = p.split_once('=').unwrap_or((p, ""));
            json!({ "key": key, "value": value })
        })
        .collect();
    params.extend(request.disabled_params.iter().map(|p| {
        let mut entry = pair(p);
        entry["disabled"] = true.into();
        entry
    }));
    if !params.is_empty() {
        out.insert("query".into(), Value::Array(params));
    }
    if !request.path_params.is_empty() {
        out.insert(
            "variable".into(),
            Value::Array(
                request
                    .path_params
                    .iter()
                    .map(|(key, value)| json!({ "key": key, "value": value }))
                    .collect(),
            ),
        );
    }
    Value::Object(out)
}

fn behavior(settings: &Settings) -> Map<String, Value> {
    let mut out = Map::new();
    if let Some(follow) = settings.follow_redirects {
        out.insert("followRedirects".into(), follow.into());
    }
    if let Some(max) = settings.max_redirects {
        out.insert("maxRedirects".into(), max.into());
    }
    if let Some(keep) = settings.follow_original_method {
        out.insert("followOriginalHttpMethod".into(), keep.into());
    }
    if let Some(keep) = settings.follow_authorization {
        out.insert("followAuthorizationHeader".into(), keep.into());
    }
    if let Some(verify) = settings.verify_tls {
        out.insert("strictSSL".into(), verify.into());
    }
    out
}

/// Script events, with what Postman has no field for written as script
/// lines: headers and variables before the scripts, checks after.
fn events(
    scripts: &Scripts,
    headers: &[KeyValue],
    variables: &Vars,
    tests: &[String],
) -> Vec<Value> {
    let js = |text: &str| serde_json::to_string(text).unwrap_or_default();
    let mut pre: Vec<String> = Vec::new();
    for header in headers {
        pre.push(format!(
            "pm.request.headers.upsert({{ key: {}, value: {} }});",
            js(&header.name),
            js(&header.value)
        ));
    }
    for (name, value) in variables {
        pre.push(format!("pm.variables.set({}, {});", js(name), js(value)));
    }
    if !scripts.pre_request.trim().is_empty() {
        if !pre.is_empty() {
            pre.push(String::new());
        }
        pre.extend(scripts.pre_request.lines().map(str::to_owned));
    }
    let mut post: Vec<String> = scripts.post_response.lines().map(str::to_owned).collect();
    if !tests.is_empty() {
        if !post.is_empty() {
            post.push(String::new());
        }
        post.extend(tests.iter().cloned());
    }
    let mut events = Vec::new();
    for (listen, lines) in [("prerequest", pre), ("test", post)] {
        if lines.iter().any(|l| !l.trim().is_empty()) {
            events.push(json!({
                "listen": listen,
                "script": { "type": "text/javascript", "exec": lines },
            }));
        }
    }
    events
}

/// Checks and captures as Postman test script lines. Returns the lines
/// and the checks that have no translation, which are kept as comments.
fn checks_script(checks: &[String], captures: &Vars) -> (Vec<String>, Vec<String>) {
    let js = |text: &str| serde_json::to_string(text).unwrap_or_default();
    let mut lines = Vec::new();
    let mut untranslated = Vec::new();
    for line in checks {
        match Check::parse(line).ok().and_then(|check| check_line(&check)) {
            Some(assertion) => lines.push(format!(
                "pm.test({}, () => {{ {assertion}; }});",
                js(line.trim())
            )),
            None => {
                lines.push(format!("// Getman check: {}", line.trim()));
                untranslated.push(line.trim().to_owned());
            }
        }
    }
    for (name, target) in captures {
        match target_expression(target) {
            Some(expression) => lines.push(format!(
                "(pm.environment.name ? pm.environment : pm.collectionVariables).set({}, {expression});",
                js(name)
            )),
            None => {
                lines.push(format!("// Getman capture: {name}: {target}"));
                untranslated.push(format!("{name}: {target}"));
            }
        }
    }
    (lines, untranslated)
}

/// A check as a `pm.expect` assertion.
fn check_line(check: &Check) -> Option<String> {
    let actual = target_expression(&check.target)?;
    let value = &check.value;
    // A value that parses as JSON is compared as JSON, anything else as text.
    let expected = if serde_json::from_str::<Value>(value).is_ok() {
        value.clone()
    } else {
        serde_json::to_string(value).ok()?
    };
    let text = serde_json::to_string(value).ok()?;
    Some(match check.operator.as_str() {
        "==" => format!("pm.expect({actual}).to.eql({expected})"),
        "!=" => format!("pm.expect({actual}).to.not.eql({expected})"),
        "<" => format!("pm.expect({actual}).to.be.below({expected})"),
        "<=" => format!("pm.expect({actual}).to.be.at.most({expected})"),
        ">" => format!("pm.expect({actual}).to.be.above({expected})"),
        ">=" => format!("pm.expect({actual}).to.be.at.least({expected})"),
        "contains" => format!("pm.expect(String({actual})).to.include({text})"),
        "!contains" => format!("pm.expect(String({actual})).to.not.include({text})"),
        "startsWith" => format!("pm.expect(String({actual}).startsWith({text})).to.be.true"),
        "endsWith" => format!("pm.expect(String({actual}).endsWith({text})).to.be.true"),
        "matches" => format!("pm.expect(String({actual})).to.match(new RegExp({text}))"),
        "!matches" => format!("pm.expect(String({actual})).to.not.match(new RegExp({text}))"),
        "exists" => format!("pm.expect({actual}).to.not.be.undefined"),
        "!exists" => format!("pm.expect({actual}).to.be.undefined"),
        "is" => match value.to_ascii_lowercase().as_str() {
            "integer" => format!("pm.expect(Number.isInteger({actual})).to.be.true"),
            "array" => format!("pm.expect({actual}).to.be.an(\"array\")"),
            "null" => format!("pm.expect({actual}).to.be.null"),
            kind @ ("string" | "number" | "boolean" | "object") => {
                format!(
                    "pm.expect({actual}).to.be.a({})",
                    serde_json::to_string(kind).ok()?
                )
            }
            _ => return None,
        },
        _ => return None,
    })
}

/// A check target as a JavaScript expression over `pm`.
fn target_expression(target: &str) -> Option<String> {
    let js = |text: &str| serde_json::to_string(text).unwrap_or_default();
    let (head, rest) = match target.find(['.', '[']) {
        Some(at) => (&target[..at], &target[at..]),
        None => (target, ""),
    };
    let simple = |expression: &str| rest.is_empty().then(|| expression.to_owned());
    match head {
        "status" => simple("pm.response.code"),
        "time" => simple("pm.response.responseTime"),
        "size" => simple("pm.response.responseSize"),
        "url" => simple("pm.request.url.toString()"),
        "body" | "text" => simple("pm.response.text()"),
        "header" | "headers" => {
            let name = rest.strip_prefix('.')?;
            Some(format!("pm.response.headers.get({})", js(name)))
        }
        "cookie" | "cookies" => {
            let name = rest.strip_prefix('.')?;
            Some(format!("pm.cookies.get({})", js(name)))
        }
        "json" => Some(format!("pm.response.json(){}", json_path(rest)?)),
        _ => None,
    }
}

/// A Getman JSON path as JavaScript property access. Negative indexes
/// count from the end, as `.at()` does.
fn json_path(path: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = path.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '.' => {
                let key: String =
                    std::iter::from_fn(|| chars.next_if(|c| *c != '.' && *c != '[')).collect();
                if key.is_empty() {
                    return None;
                }
                let identifier = key.chars().enumerate().all(|(i, c)| {
                    c == '_' || c == '$' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit())
                });
                if identifier {
                    out.push('.');
                    out.push_str(&key);
                } else {
                    out.push_str(&format!("[{}]", serde_json::to_string(&key).ok()?));
                }
            }
            '[' => {
                let inner: String = std::iter::from_fn(|| chars.next_if(|c| *c != ']')).collect();
                chars.next()?;
                let inner = inner.trim();
                if let Ok(index) = inner.parse::<i64>() {
                    if index < 0 {
                        out.push_str(&format!(".at({index})"));
                    } else {
                        out.push_str(&format!("[{index}]"));
                    }
                } else {
                    let key = inner
                        .strip_prefix('"')
                        .and_then(|s| s.strip_suffix('"'))
                        .or_else(|| inner.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))?;
                    out.push_str(&format!("[{}]", serde_json::to_string(key).ok()?));
                }
            }
            _ => return None,
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FormField, OAuth2};
    use crate::workspace::EnvironmentFiles;

    fn workspace() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::init(dir.path(), "Shop").unwrap();
        let mut root = ws.read_folder("").unwrap();
        root.variables
            .insert("baseUrl".into(), "https://shop.example.com".into());
        root.auth = Auth::Bearer {
            token: "{{token}}".into(),
            prefix: String::new(),
        };
        ws.write_folder("", &root).unwrap();
        let orders = ws.create_folder("", "Orders").unwrap();
        let mut folder = ws.read_folder(&orders).unwrap();
        folder.headers.push(KeyValue::new("X-Team", "core"));
        folder.scripts.pre_request = "console.log('orders');".into();
        ws.write_folder(&orders, &folder).unwrap();
        ws.create_request(
            &orders,
            "List orders",
            &Request {
                url: "{{baseUrl}}/orders/:id?page=2".into(),
                path_params: [("id".to_owned(), "7".to_owned())].into_iter().collect(),
                disabled_params: vec![KeyValue {
                    enabled: false,
                    ..KeyValue::new("debug", "1")
                }],
                variables: [("limit".to_owned(), "10".to_owned())]
                    .into_iter()
                    .collect(),
                checks: vec![
                    "status == 200".into(),
                    "json.items[-1].id exists".into(),
                    "json[\"odd key\"] is string".into(),
                    "header.content-type contains json".into(),
                    "time << 3".into(),
                ],
                captures: [("orderId".to_owned(), "json.items[0].id".to_owned())]
                    .into_iter()
                    .collect(),
                settings: Settings {
                    follow_redirects: Some(false),
                    follow_original_method: Some(true),
                    timeout_ms: Some(100),
                    ..Settings::default()
                },
                ..Request::default()
            },
        )
        .unwrap();
        ws.create_request(
            "",
            "Upload",
            &Request {
                method: "POST".into(),
                url: "https://files.example.com:8443/upload".into(),
                body: Body::Multipart {
                    fields: vec![FormField {
                        name: "photo".into(),
                        value: String::new(),
                        file: Some("photos/cat.png".into()),
                        content_type: None,
                        enabled: true,
                    }],
                },
                auth: Auth::Oauth2(OAuth2 {
                    grant: OAuth2Grant::Password,
                    token_url: "https://id.example.com/token".into(),
                    client_id: "shop".into(),
                    username: "ada".into(),
                    ..OAuth2::default()
                }),
                ..Request::default()
            },
        )
        .unwrap();
        (dir, ws)
    }

    #[test]
    fn a_workspace_becomes_a_collection() {
        let (_dir, ws) = workspace();
        let exported = collection(&ws, "").unwrap();
        assert_eq!(exported.file_name, "shop.postman_collection.json");
        let doc: Value = serde_json::from_str(&exported.content).unwrap();
        assert_eq!(doc["info"]["schema"], SCHEMA);
        assert_eq!(doc["auth"]["type"], "bearer");
        assert_eq!(doc["variable"][0]["key"], "baseUrl");

        let orders = &doc["item"][0];
        assert_eq!(orders["name"], "Orders");
        let pre = orders["event"][0]["script"]["exec"].as_array().unwrap();
        assert_eq!(
            pre[0],
            "pm.request.headers.upsert({ key: \"X-Team\", value: \"core\" });"
        );
        assert_eq!(pre.last().unwrap(), "console.log('orders');");

        let list = &orders["item"][0];
        let url = &list["request"]["url"];
        assert_eq!(url["raw"], "{{baseUrl}}/orders/:id?page=2");
        assert_eq!(url["host"][0], "{{baseUrl}}");
        assert_eq!(url["path"][1], ":id");
        assert_eq!(url["query"][0]["key"], "page");
        assert_eq!(url["query"][1]["disabled"], true);
        assert_eq!(url["variable"][0]["value"], "7");
        assert_eq!(list["protocolProfileBehavior"]["followRedirects"], false);
        assert_eq!(
            list["protocolProfileBehavior"]["followOriginalHttpMethod"],
            true
        );
        let events = list["event"].as_array().unwrap();
        assert_eq!(
            events[0]["script"]["exec"][0],
            "pm.variables.set(\"limit\", \"10\");"
        );
        let tests: Vec<&str> = events[1]["script"]["exec"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert_eq!(
            tests[0],
            "pm.test(\"status == 200\", () => { pm.expect(pm.response.code).to.eql(200); });"
        );
        assert!(
            tests[1].contains("pm.response.json().items.at(-1).id).to.not.be.undefined"),
            "{tests:?}"
        );
        assert!(
            tests[2].contains("pm.response.json()[\"odd key\"]).to.be.a(\"string\")"),
            "{tests:?}"
        );
        assert!(
            tests[3].contains("pm.response.headers.get(\"content-type\"))).to.include(\"json\")"),
            "{tests:?}"
        );
        assert_eq!(tests[4], "// Getman check: time << 3");
        assert!(
            tests[5].contains(".set(\"orderId\", pm.response.json().items[0].id);"),
            "{tests:?}"
        );

        let upload = &doc["item"][1];
        assert_eq!(upload["request"]["url"]["port"], "8443");
        assert_eq!(upload["request"]["url"]["host"][1], "example");
        assert_eq!(
            upload["request"]["body"]["formdata"][0]["src"],
            "photos/cat.png"
        );
        assert_eq!(
            upload["request"]["auth"]["oauth2"][0]["value"],
            "password_credentials"
        );

        let warnings = exported.warnings.join("\n");
        assert!(warnings.contains("time << 3"), "{warnings}");
        assert!(warnings.contains("timeout"), "{warnings}");
    }

    #[test]
    fn a_folder_carries_what_it_inherits_and_round_trips() {
        let (_dir, ws) = workspace();
        let exported = collection(&ws, "orders").unwrap();
        let doc: Value = serde_json::from_str(&exported.content).unwrap();
        assert_eq!(doc["info"]["name"], "Orders");
        assert_eq!(doc["variable"][0]["value"], "https://shop.example.com");
        assert_eq!(doc["auth"]["bearer"][0]["value"], "{{token}}");

        // Importing the export gives the same requests back.
        let other = tempfile::tempdir().unwrap();
        let back = Workspace::init(other.path(), "Back").unwrap();
        let report = crate::import::postman::import(&back, "", &exported.content, false).unwrap();
        assert_eq!(report.requests, 1);
        let list = back.read_request("orders/list-orders.yaml").unwrap();
        assert_eq!(list.url, "{{baseUrl}}/orders/:id?page=2");
        assert_eq!(list.disabled_params[0].name, "debug");
        assert_eq!(list.path_params["id"], "7");
        assert!(collection(&ws, "nowhere").is_err());
    }

    #[test]
    fn signed_auth_round_trips_through_postman() {
        use crate::model::{
            Asap, AwsSigV4, EdgeGrid, Hawk, Jwt, OAuth1, OAuth2, OAuth2Grant, Pkce,
        };

        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::init(dir.path(), "Shop").unwrap();
        let folder = ws.create_folder("", "Signed").unwrap();
        let auths = [
            Auth::AwsSigV4(AwsSigV4 {
                access_key: "{{awsKey}}".into(),
                secret_key: "{{awsSecret}}".into(),
                session_token: "{{awsSession}}".into(),
                region: "eu-west-1".into(),
                service: "execute-api".into(),
                location: ApiKeyLocation::Query,
                ..AwsSigV4::default()
            }),
            Auth::Oauth1(OAuth1 {
                signature_method: OAuth1Method::HmacSha256,
                consumer_key: "ck".into(),
                consumer_secret: "{{cs}}".into(),
                token: "t".into(),
                token_secret: "{{ts}}".into(),
                realm: "photos".into(),
                body_hash: true,
                location: OAuth1Location::Query,
                ..OAuth1::default()
            }),
            Auth::Jwt(Jwt {
                algorithm: JwtAlgorithm::Hs384,
                secret: "{{jwtSecret}}".into(),
                secret_base64: true,
                payload: r#"{"sub": "{{user}}"}"#.into(),
                location: ApiKeyLocation::Query,
                param: "access".into(),
                ..Jwt::default()
            }),
            Auth::Hawk(Hawk {
                id: "dh37fgj492je".into(),
                key: "{{hawkKey}}".into(),
                algorithm: HawkAlgorithm::Sha1,
                ext: "some-app-ext-data".into(),
                payload_hash: true,
                ..Hawk::default()
            }),
            Auth::Edgegrid(EdgeGrid {
                client_token: "ct".into(),
                client_secret: "{{edgeSecret}}".into(),
                access_token: "at".into(),
                base_url: "https://akab.example.net".into(),
                headers_to_sign: vec!["X-A".into(), "X-B".into()],
                max_body: Some(131_072),
            }),
            Auth::Asap(Asap {
                issuer: "shop".into(),
                audience: "orders".into(),
                key_id: "shop/key-1".into(),
                private_key: "{{asapKey}}".into(),
                expires_in: Some(300),
                ..Asap::default()
            }),
            Auth::Oauth2(OAuth2 {
                grant: OAuth2Grant::AuthorizationCode,
                auth_url: "https://id.example.com/authorize".into(),
                token_url: "https://id.example.com/token".into(),
                client_id: "shop".into(),
                callback_url: "http://127.0.0.1:8765/callback".into(),
                pkce: Pkce::S256,
                scope: "orders:read".into(),
                auth_params: vec![KeyValue::new("prompt", "consent")],
                token_in: ApiKeyLocation::Query,
                ..OAuth2::default()
            }),
        ];
        for (i, auth) in auths.iter().enumerate() {
            let request = Request {
                url: format!("https://api.example.com/{i}"),
                auth: auth.clone(),
                ..Request::default()
            };
            ws.create_request(&folder, &format!("Request {i}"), &request)
                .unwrap();
        }

        let exported = collection(&ws, &folder).unwrap();
        assert!(exported.warnings.is_empty(), "{:?}", exported.warnings);
        let doc: Value = serde_json::from_str(&exported.content).unwrap();
        assert_eq!(doc["item"][0]["request"]["auth"]["type"], "awsv4");
        assert_eq!(
            doc["item"][0]["request"]["auth"]["awsv4"][5]["value"], true,
            "switches are booleans"
        );

        let other = tempfile::tempdir().unwrap();
        let back = Workspace::init(other.path(), "Back").unwrap();
        crate::import::postman::import(&back, "", &exported.content, false).unwrap();
        for (i, auth) in auths.iter().enumerate() {
            let request = back
                .read_request(&format!("signed/request-{i}.yaml"))
                .unwrap();
            assert_eq!(&request.auth, auth, "request {i}");
        }
    }

    #[test]
    fn private_key_files_and_unsigned_payloads_are_reported() {
        use crate::model::{AwsSigV4, Jwt};

        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::init(dir.path(), "Shop").unwrap();
        let jwt = Request {
            auth: Auth::Jwt(Jwt {
                algorithm: JwtAlgorithm::Rs256,
                private_key_file: "keys/signing.pem".into(),
                expires_in: Some(60),
                ..Jwt::default()
            }),
            ..Request::default()
        };
        let s3 = Request {
            auth: Auth::AwsSigV4(AwsSigV4 {
                service: "s3".into(),
                unsigned_payload: true,
                ..AwsSigV4::default()
            }),
            ..Request::default()
        };
        ws.create_request("", "Jwt", &jwt).unwrap();
        ws.create_request("", "S3", &s3).unwrap();
        let exported = collection(&ws, "").unwrap();
        let warnings = exported.warnings.join("\n");
        assert!(
            warnings.contains("keys/signing.pem was left out"),
            "{warnings}"
        );
        assert!(warnings.contains("iat or exp"), "{warnings}");
        assert!(warnings.contains("unsigned payload"), "{warnings}");
        assert!(!exported.content.contains("signing.pem"));
    }

    #[test]
    fn environments_leave_local_values_out_unless_asked() {
        let (_dir, ws) = workspace();
        let id = ws.create_environment("Staging").unwrap();
        let mut shared = crate::Environment {
            name: "Staging".into(),
            secrets: vec!["token".into()],
            ..crate::Environment::default()
        };
        shared
            .variables
            .insert("baseUrl".into(), "https://staging.example.com".into());
        shared.variables.insert("token".into(), "s3cret".into());
        ws.write_environment(
            &id,
            &EnvironmentFiles {
                shared,
                local: Vars::new(),
            },
        )
        .unwrap();

        let exported = environment(&ws, &id, false).unwrap();
        let doc: Value = serde_json::from_str(&exported.content).unwrap();
        assert_eq!(doc["_postman_variable_scope"], "environment");
        assert_eq!(doc["values"][0]["value"], "https://staging.example.com");
        assert_eq!(doc["values"][1]["type"], "secret");
        assert_eq!(doc["values"][1]["value"], "");
        assert!(!exported.content.contains("s3cret"));
        assert!(
            exported.warnings[0].contains("1 value"),
            "{:?}",
            exported.warnings
        );

        let with = environment(&ws, &id, true).unwrap();
        assert!(with.content.contains("s3cret"));

        let mut state = ws.read_local_state().unwrap();
        state.globals.insert("g".into(), "secret-global".into());
        ws.write_local_state(&state).unwrap();
        let exported = globals(&ws, false).unwrap();
        assert!(!exported.content.contains("secret-global"));
        assert!(exported.content.contains("\"globals\""));
        assert!(
            globals(&ws, true)
                .unwrap()
                .content
                .contains("secret-global")
        );
    }
}
