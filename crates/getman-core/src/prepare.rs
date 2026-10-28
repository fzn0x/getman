//! Turning a stored request into one ready to send: inherited settings
//! applied, variables replaced, body encoded.

use std::path::{Component, Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use serde::Serialize;

use crate::error::{Error, Result};
use crate::http::{
    Header, Outgoing, OutgoingBody, Part, Payload, ProxyRoute, Transport, USER_AGENT, find_header,
    set_header,
};
use crate::model::{ApiKeyLocation, Auth, Body, ClientCert, Folder, KeyValue, Request, Settings};
use crate::tls::ClientIdentity;
use crate::vars::VarStack;

/// Characters escaped in a path parameter's value.
const PATH_SEGMENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'/')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}');

/// A script and where it was written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Script {
    /// Where the script comes from, such as `folder "Users"`.
    pub origin: String,
    /// The JavaScript source.
    pub code: String,
}

/// What a request inherits from its folders, combined with its own
/// settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Effective {
    /// Headers of the folders and the request; deeper ones replace outer
    /// ones with the same name.
    pub headers: Vec<KeyValue>,
    /// The nearest authentication that is not `inherit`.
    pub auth: Auth,
    /// Transport settings, each the nearest one set; see [`transport`] for
    /// them with the defaults filled in.
    pub settings: Settings,
    /// Pre-request scripts, outermost first.
    pub pre_request: Vec<Script>,
    /// Post-response scripts, outermost first.
    pub post_response: Vec<Script>,
}

impl Effective {
    /// Combines a request with the settings of its folders, given from the
    /// workspace down (see [`crate::Workspace::folder_chain`]).
    pub fn new(chain: &[Folder], request: &Request) -> Self {
        let mut headers: Vec<KeyValue> = Vec::new();
        let mut settings = Settings::default();
        let mut auth = Auth::None;
        let mut pre_request = Vec::new();
        let mut post_response = Vec::new();

        let levels = chain
            .iter()
            .enumerate()
            .map(|(depth, folder)| {
                let origin = if depth == 0 {
                    "workspace".to_owned()
                } else {
                    format!("folder {:?}", folder.name)
                };
                (
                    origin,
                    &folder.headers,
                    &folder.auth,
                    &folder.settings,
                    &folder.scripts,
                )
            })
            .chain(std::iter::once((
                "request".to_owned(),
                &request.headers,
                &request.auth,
                &request.settings,
                &request.scripts,
            )));
        for (origin, level_headers, level_auth, level_settings, scripts) in levels {
            for header in level_headers.iter().filter(|h| h.enabled) {
                headers.retain(|h| !h.name.eq_ignore_ascii_case(&header.name));
                headers.push(header.clone());
            }
            if !level_auth.is_inherit() {
                auth = level_auth.clone();
            }
            settings = level_settings.clone().or(settings);
            if !scripts.pre_request.trim().is_empty() {
                pre_request.push(Script {
                    origin: origin.clone(),
                    code: scripts.pre_request.clone(),
                });
            }
            if !scripts.post_response.trim().is_empty() {
                post_response.push(Script {
                    origin,
                    code: scripts.post_response.clone(),
                });
            }
        }
        Self {
            headers,
            auth,
            settings,
            pre_request,
            post_response,
        }
    }
}

/// Fills in the defaults of transport settings, replaces variables in the
/// proxy and certificate settings, and reads certificate files from
/// inside the workspace at `root`.
pub fn transport(settings: &Settings, vars: &VarStack, root: &Path) -> Result<Transport> {
    let defaults = Transport::default();
    let proxy = match &settings.proxy {
        None => ProxyRoute::System,
        Some(proxy) if proxy.is_system() => ProxyRoute::System,
        Some(proxy) if proxy.is_off() => ProxyRoute::Off,
        Some(proxy) => ProxyRoute::Url {
            url: proxy_url(
                &strict(vars, proxy.url.trim(), "proxy URL")?,
                &strict(vars, &proxy.username, "proxy user name")?,
                &strict(vars, &proxy.password, "proxy password")?,
            )?,
            bypass: strict(vars, &proxy.bypass, "proxy bypass list")?,
        },
    };
    let client_identity = match &settings.client_cert {
        Some(cert) if !cert.is_empty() => Some(client_identity(cert, vars, root)?),
        _ => None,
    };
    let mut ca_certs = Vec::new();
    for file in settings.ca_certs.iter().flatten() {
        let file = strict(vars, file, "CA certificate path")?;
        if file.trim().is_empty() {
            continue;
        }
        let pem = read_inside(root, &file, "CA certificate")?;
        crate::tls::check_ca_pem(&pem, &file)?;
        ca_certs.push(pem);
    }
    Ok(Transport {
        timeout_ms: settings.timeout_ms.unwrap_or(defaults.timeout_ms),
        follow_redirects: settings
            .follow_redirects
            .unwrap_or(defaults.follow_redirects),
        max_redirects: settings.max_redirects.unwrap_or(defaults.max_redirects),
        verify_tls: settings.verify_tls.unwrap_or(defaults.verify_tls),
        http_version: settings.http_version.unwrap_or(defaults.http_version),
        title_case_headers: settings
            .title_case_headers
            .unwrap_or(defaults.title_case_headers),
        proxy,
        client_identity,
        ca_certs,
        cookies: settings.cookies.unwrap_or(defaults.cookies),
        max_body_bytes: settings
            .max_response_mb
            .map_or(defaults.max_body_bytes, |mb| mb.saturating_mul(1024 * 1024)),
    })
}

/// Checks a proxy URL and puts the credentials in it, where both HTTP and
/// SOCKS proxies find them.
fn proxy_url(raw: &str, username: &str, password: &str) -> Result<String> {
    let mut url = url::Url::parse(raw).map_err(|e| {
        Error::Invalid(format!(
            "the proxy {raw:?} is not a valid URL ({e}); write it as http://host:port"
        ))
    })?;
    if !matches!(url.scheme(), "http" | "https" | "socks5" | "socks5h") {
        return Err(Error::Invalid(format!(
            "the proxy URL starts with {}://; use http://, https://, socks5:// or socks5h://",
            url.scheme()
        )));
    }
    if url.host_str().is_none_or(str::is_empty) {
        return Err(Error::Invalid(format!("the proxy {raw:?} has no host")));
    }
    if !username.is_empty() || !password.is_empty() {
        url.set_username(username)
            .and_then(|()| url.set_password(Some(password)))
            .map_err(|()| Error::Invalid("the proxy URL cannot carry credentials".into()))?;
    }
    Ok(url.to_string())
}

/// Replaces variables in a transport setting. A missing one is an error
/// rather than text sent as written, since these are credentials and
/// file names.
fn strict(vars: &VarStack, text: &str, what: &str) -> Result<String> {
    let out = vars.interpolate(text);
    match out.missing.first() {
        Some(name) => Err(Error::Invalid(format!(
            "the {what} cannot be filled in: {}",
            crate::vars::missing_reason(name)
        ))),
        None => Ok(out.text),
    }
}

fn client_identity(cert: &ClientCert, vars: &VarStack, root: &Path) -> Result<ClientIdentity> {
    let pkcs12 = strict(vars, cert.pkcs12.trim(), "PKCS#12 path")?;
    if !pkcs12.is_empty() {
        let der = read_inside(root, &pkcs12, "client certificate")?;
        let passphrase = strict(vars, &cert.passphrase, "PKCS#12 passphrase")?;
        return ClientIdentity::from_pkcs12(&der, &passphrase, &pkcs12);
    }
    let cert_file = strict(vars, cert.cert.trim(), "client certificate path")?;
    let key_file = strict(vars, cert.key.trim(), "client key path")?;
    if cert_file.is_empty() {
        return Err(Error::Invalid(
            "the client certificate names a key but no cert file".into(),
        ));
    }
    let mut pem = read_inside(root, &cert_file, "client certificate")?;
    let mut name = cert_file.clone();
    if !key_file.is_empty() && key_file != cert_file {
        if !pem.ends_with(b"\n") {
            pem.push(b'\n');
        }
        pem.extend(read_inside(root, &key_file, "client key")?);
        name = format!("{cert_file} and {key_file}");
    }
    ClientIdentity::from_pem(pem, &name)
}

/// Reads a file named in the settings, inside the workspace only.
fn read_inside(root: &Path, rel: &str, what: &str) -> Result<Vec<u8>> {
    let path = resolve_file(root, rel).map_err(|e| match e {
        Error::NotFound(_) => Error::NotFound(format!("{what} file {rel}")),
        other => other,
    })?;
    std::fs::read(&path).map_err(|e| Error::io(&path, e))
}

/// A request ready to send, and the variables it referred to that have
/// no value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prepared {
    /// The request.
    pub outgoing: Outgoing,
    /// Referenced variables that are not defined.
    pub missing: Vec<String>,
}

/// Collects interpolations and the variables they missed.
struct Renderer<'a> {
    vars: &'a VarStack,
    missing: Vec<String>,
}

impl Renderer<'_> {
    fn render(&mut self, text: &str) -> String {
        let out = self.vars.interpolate(text);
        for name in out.missing {
            if !self.missing.contains(&name) {
                self.missing.push(name);
            }
        }
        out.text
    }
}

/// Builds the request to send. `headers` are the effective headers (see
/// [`Effective`]), and files named by the body are looked up under `root`.
pub fn prepare(
    request: &Request,
    headers: &[KeyValue],
    auth: &Auth,
    vars: &VarStack,
    root: &Path,
) -> Result<Prepared> {
    let mut r = Renderer {
        vars,
        missing: Vec::new(),
    };

    let url = build_url(request, &mut r)?;
    let mut outgoing = Outgoing {
        method: request.method.trim().to_ascii_uppercase(),
        url,
        headers: headers
            .iter()
            .filter(|h| h.enabled && !h.name.trim().is_empty())
            .map(|h| Header::new(r.render(h.name.trim()), r.render(&h.value)))
            .collect(),
        body: OutgoingBody::None,
    };

    let (body, content_type) = build_body(&request.body, &mut r, root)?;
    outgoing.body = body;
    if let Some(content_type) = content_type
        && find_header(&outgoing.headers, "content-type").is_none()
    {
        outgoing
            .headers
            .push(Header::new("Content-Type", content_type));
    }

    apply_auth(auth, &mut r, &mut outgoing)?;

    if find_header(&outgoing.headers, "user-agent").is_none() {
        outgoing.headers.push(Header::new("User-Agent", USER_AGENT));
    }
    if find_header(&outgoing.headers, "accept").is_none() {
        outgoing.headers.push(Header::new("Accept", "*/*"));
    }
    // Signatures cover the finished request, so they come last.
    crate::auth::sign(auth, &mut |text| r.render(text), &mut outgoing, root)?;

    Ok(Prepared {
        outgoing,
        missing: r.missing,
    })
}

fn build_url(request: &Request, r: &mut Renderer<'_>) -> Result<String> {
    let raw = r.render(request.url.trim());
    if raw.is_empty() {
        return Err(Error::Invalid("the request has no URL".into()));
    }
    let with_scheme = if raw.contains("://") {
        raw
    } else {
        format!("http://{raw}")
    };
    let mut url = url::Url::parse(&with_scheme)
        .map_err(|e| Error::Invalid(format!("{with_scheme:?} is not a valid URL: {e}")))?;

    if !request.path_params.is_empty() && !url.cannot_be_a_base() {
        let segments: Vec<String> = url
            .path_segments()
            .map(|segments| {
                segments
                    .map(|segment| {
                        let value = segment
                            .strip_prefix(':')
                            .and_then(|name| request.path_params.get(name));
                        match value {
                            Some(value) => {
                                utf8_percent_encode(&r.render(value), PATH_SEGMENT).to_string()
                            }
                            None => segment.to_owned(),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        url.set_path(&format!("/{}", segments.join("/")));
    }
    Ok(url.to_string())
}

fn build_body(
    body: &Body,
    r: &mut Renderer<'_>,
    root: &Path,
) -> Result<(OutgoingBody, Option<String>)> {
    let bytes = |text: String| OutgoingBody::Bytes {
        data: Payload(text.into_bytes()),
    };
    Ok(match body {
        Body::None => (OutgoingBody::None, None),
        Body::Json { content } => (bytes(r.render(content)), Some("application/json".into())),
        Body::Xml { content } => (bytes(r.render(content)), Some("application/xml".into())),
        Body::Text {
            content,
            content_type,
        } => (
            bytes(r.render(content)),
            Some(
                content_type
                    .as_deref()
                    .map_or_else(|| "text/plain".to_owned(), |t| r.render(t)),
            ),
        ),
        Body::Form { fields } => {
            let mut form = url::form_urlencoded::Serializer::new(String::new());
            for field in fields.iter().filter(|f| f.enabled) {
                form.append_pair(&r.render(&field.name), &r.render(&field.value));
            }
            (
                bytes(form.finish()),
                Some("application/x-www-form-urlencoded".into()),
            )
        }
        Body::Multipart { fields } => {
            let mut parts = Vec::new();
            for field in fields.iter().filter(|f| f.enabled) {
                let file = match &field.file {
                    Some(file) => Some(resolve_file(root, &r.render(file))?),
                    None => None,
                };
                parts.push(Part {
                    name: r.render(&field.name),
                    value: if file.is_some() {
                        String::new()
                    } else {
                        r.render(&field.value)
                    },
                    file,
                    content_type: field.content_type.as_deref().map(|t| r.render(t)),
                });
            }
            // reqwest writes the Content-Type with the boundary.
            (OutgoingBody::Multipart { parts }, None)
        }
        Body::Binary { file, content_type } => {
            let path = resolve_file(root, &r.render(file))?;
            let content_type = content_type.as_deref().map_or_else(
                || {
                    mime_guess::from_path(&path)
                        .first_or_octet_stream()
                        .essence_str()
                        .to_owned()
                },
                |t| r.render(t),
            );
            (OutgoingBody::File { path }, Some(content_type))
        }
        Body::Graphql { query, variables } => {
            let mut payload = serde_json::Map::new();
            payload.insert("query".into(), r.render(query).into());
            let variables = r.render(variables);
            if !variables.trim().is_empty() {
                let parsed: serde_json::Value = serde_json::from_str(&variables).map_err(|e| {
                    Error::Invalid(format!("the GraphQL variables are not valid JSON: {e}"))
                })?;
                payload.insert("variables".into(), parsed);
            }
            (
                bytes(serde_json::Value::Object(payload).to_string()),
                Some("application/json".into()),
            )
        }
    })
}

/// Adds credentials that need no round trip: Basic, Bearer and API keys.
/// Digest and OAuth 2.0 are handled when sending, and signatures once the
/// request is complete (see [`crate::auth`]).
fn apply_auth(auth: &Auth, r: &mut Renderer<'_>, outgoing: &mut Outgoing) -> Result<()> {
    match auth {
        Auth::Basic { username, password } => {
            let credentials = format!("{}:{}", r.render(username), r.render(password));
            set_header(
                &mut outgoing.headers,
                "Authorization",
                format!("Basic {}", BASE64.encode(credentials)),
            );
        }
        Auth::Bearer { token, prefix } => {
            let prefix = if prefix.trim().is_empty() {
                "Bearer".to_owned()
            } else {
                r.render(prefix.trim())
            };
            set_header(
                &mut outgoing.headers,
                "Authorization",
                format!("{prefix} {}", r.render(token)),
            );
        }
        Auth::ApiKey {
            key,
            value,
            location,
        } => {
            let (key, value) = (r.render(key), r.render(value));
            if key.is_empty() {
                return Err(Error::Invalid("the API key has no name".into()));
            }
            match location {
                ApiKeyLocation::Header => set_header(&mut outgoing.headers, &key, value),
                ApiKeyLocation::Query => {
                    let mut url = url::Url::parse(&outgoing.url)
                        .map_err(|e| Error::Invalid(e.to_string()))?;
                    url.query_pairs_mut().append_pair(&key, &value);
                    outgoing.url = url.to_string();
                }
            }
        }
        // Handled when sending, or signed once the request is complete.
        _ => {}
    }
    Ok(())
}

/// Resolves a file a request reads, relative to the workspace root.
/// Files outside the workspace are refused, so a collection from someone
/// else cannot upload your private files.
pub fn resolve_file(root: &Path, rel: &str) -> Result<PathBuf> {
    let rel_path = Path::new(rel.trim());
    if rel.trim().is_empty() {
        return Err(Error::Invalid("no file is selected".into()));
    }
    let escapes = rel_path
        .components()
        .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir));
    if escapes {
        return Err(Error::Invalid(format!(
            "{rel:?} is outside the workspace; files must be inside it"
        )));
    }
    let path = root.join(rel_path);
    let canonical = path
        .canonicalize()
        .map_err(|_| Error::NotFound(format!("file {rel}")))?;
    let canonical_root = root.canonicalize().map_err(|e| Error::io(root, e))?;
    if !canonical.starts_with(&canonical_root) {
        return Err(Error::Invalid(format!(
            "{rel:?} links outside the workspace; files must be inside it"
        )));
    }
    if !canonical.is_file() {
        return Err(Error::NotFound(format!("file {rel}")));
    }
    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FormField, HttpVersion, Proxy, Scripts};
    use crate::vars::Scope;

    fn vars(pairs: &[(&str, &str)]) -> VarStack {
        let mut stack = VarStack::new();
        stack.push(
            Scope::Environment,
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
        );
        stack
    }

    fn prepare_simple(request: &Request, stack: &VarStack) -> Prepared {
        let effective = Effective::new(&[Folder::default()], request);
        prepare(
            request,
            &effective.headers,
            &effective.auth,
            stack,
            Path::new("."),
        )
        .unwrap()
    }

    #[test]
    fn deeper_levels_override_outer_ones() {
        let workspace = Folder {
            headers: vec![
                KeyValue::new("X-Team", "core"),
                KeyValue::new("Accept", "text/html"),
            ],
            auth: Auth::Bearer {
                token: "outer".into(),
                prefix: String::new(),
            },
            settings: Settings {
                timeout_ms: Some(1000),
                verify_tls: Some(false),
                ..Settings::default()
            },
            scripts: Scripts {
                pre_request: "a()".into(),
                post_response: String::new(),
            },
            ..Folder::default()
        };
        let folder = Folder {
            name: "Users".into(),
            headers: vec![KeyValue::new("accept", "application/json")],
            scripts: Scripts {
                pre_request: "b()".into(),
                post_response: "c()".into(),
            },
            ..Folder::default()
        };
        let request = Request {
            settings: Settings {
                timeout_ms: Some(5),
                ..Settings::default()
            },
            ..Request::default()
        };
        let effective = Effective::new(&[workspace, folder], &request);
        let names: Vec<_> = effective
            .headers
            .iter()
            .map(|h| (h.name.as_str(), h.value.as_str()))
            .collect();
        assert_eq!(names, [("X-Team", "core"), ("accept", "application/json")]);
        assert!(matches!(&effective.auth, Auth::Bearer { token, .. } if token == "outer"));
        assert_eq!(effective.settings.timeout_ms, Some(5));
        assert_eq!(effective.settings.verify_tls, Some(false));
        let origins: Vec<_> = effective
            .pre_request
            .iter()
            .map(|s| s.origin.as_str())
            .collect();
        assert_eq!(origins, ["workspace", "folder \"Users\""]);
        assert_eq!(effective.post_response.len(), 1);
    }

    #[test]
    fn auth_none_stops_inheritance() {
        let workspace = Folder {
            auth: Auth::Basic {
                username: "a".into(),
                password: "b".into(),
            },
            ..Folder::default()
        };
        let request = Request {
            auth: Auth::None,
            ..Request::default()
        };
        assert_eq!(Effective::new(&[workspace], &request).auth, Auth::None);
    }

    #[test]
    fn urls_get_variables_a_scheme_and_path_parameters() {
        let request = Request {
            url: "{{host}}/users/:id/posts/:missing?q=a b".into(),
            path_params: [("id".to_owned(), "{{userId}}".to_owned())]
                .into_iter()
                .collect(),
            ..Request::default()
        };
        let prepared = prepare_simple(
            &request,
            &vars(&[("host", "example.com"), ("userId", "7/8")]),
        );
        assert_eq!(
            prepared.outgoing.url,
            "http://example.com/users/7%2F8/posts/:missing?q=a%20b"
        );
        assert_eq!(prepared.outgoing.method, "GET");
    }

    #[test]
    fn missing_variables_are_reported_once() {
        let request = Request {
            url: "https://example.com/{{a}}/{{a}}".into(),
            headers: vec![KeyValue::new("X-B", "{{b}}")],
            ..Request::default()
        };
        let prepared = prepare_simple(&request, &VarStack::new());
        assert_eq!(prepared.missing, ["a", "b"]);
    }

    #[test]
    fn bodies_set_a_content_type_unless_one_is_given() {
        let mut request = Request {
            url: "https://example.com".into(),
            body: Body::Json {
                content: r#"{"id": {{id}}}"#.into(),
            },
            ..Request::default()
        };
        let prepared = prepare_simple(&request, &vars(&[("id", "42")]));
        assert_eq!(
            find_header(&prepared.outgoing.headers, "content-type"),
            Some("application/json")
        );
        assert_eq!(
            prepared.outgoing.body,
            OutgoingBody::Bytes {
                data: Payload(br#"{"id": 42}"#.to_vec())
            }
        );
        assert_eq!(
            find_header(&prepared.outgoing.headers, "user-agent"),
            Some(USER_AGENT)
        );

        request
            .headers
            .push(KeyValue::new("Content-Type", "application/vnd.api+json"));
        let prepared = prepare_simple(&request, &VarStack::new());
        let types: Vec<_> = prepared
            .outgoing
            .headers
            .iter()
            .filter(|h| h.name.eq_ignore_ascii_case("content-type"))
            .collect();
        assert_eq!(types.len(), 1);
        assert_eq!(types[0].value, "application/vnd.api+json");
    }

    #[test]
    fn forms_are_url_encoded_without_disabled_fields() {
        let request = Request {
            url: "https://example.com".into(),
            body: Body::Form {
                fields: vec![
                    KeyValue::new("name", "Ada Lovelace"),
                    KeyValue::new("lang", "{{lang}}"),
                    KeyValue {
                        enabled: false,
                        ..KeyValue::new("debug", "1")
                    },
                ],
            },
            ..Request::default()
        };
        let prepared = prepare_simple(&request, &vars(&[("lang", "en&fr")]));
        assert_eq!(
            prepared.outgoing.body,
            OutgoingBody::Bytes {
                data: Payload(b"name=Ada+Lovelace&lang=en%26fr".to_vec())
            }
        );
    }

    #[test]
    fn graphql_variables_must_be_json() {
        let mut request = Request {
            url: "https://example.com/graphql".into(),
            body: Body::Graphql {
                query: "query { me { id } }".into(),
                variables: r#"{"n": {{n}}}"#.into(),
            },
            ..Request::default()
        };
        let prepared = prepare_simple(&request, &vars(&[("n", "3")]));
        let OutgoingBody::Bytes { data } = prepared.outgoing.body else {
            panic!()
        };
        let json: serde_json::Value = serde_json::from_slice(&data.0).unwrap();
        assert_eq!(json["variables"]["n"], 3);

        request.body = Body::Graphql {
            query: "{ a }".into(),
            variables: "{oops".into(),
        };
        let effective = Effective::new(&[], &request);
        let error = prepare(
            &request,
            &effective.headers,
            &effective.auth,
            &VarStack::new(),
            Path::new("."),
        )
        .unwrap_err();
        assert!(error.to_string().contains("GraphQL"), "{error}");
    }

    #[test]
    fn static_auth_is_applied() {
        let stack = vars(&[("user", "ada"), ("key", "k3y")]);
        let mut request = Request {
            url: "https://example.com/x?a=1".into(),
            auth: Auth::Basic {
                username: "{{user}}".into(),
                password: "pw".into(),
            },
            ..Request::default()
        };
        let prepared = prepare_simple(&request, &stack);
        assert_eq!(
            find_header(&prepared.outgoing.headers, "authorization"),
            Some("Basic YWRhOnB3")
        );

        request.auth = Auth::ApiKey {
            key: "api_key".into(),
            value: "{{key}}".into(),
            location: ApiKeyLocation::Query,
        };
        let prepared = prepare_simple(&request, &stack);
        assert_eq!(
            prepared.outgoing.url,
            "https://example.com/x?a=1&api_key=k3y"
        );

        request.auth = Auth::Bearer {
            token: "t".into(),
            prefix: "Token".into(),
        };
        let prepared = prepare_simple(&request, &stack);
        assert_eq!(
            find_header(&prepared.outgoing.headers, "authorization"),
            Some("Token t")
        );
    }

    #[test]
    fn signatures_are_inherited_rendered_and_added_last() {
        use crate::model::{AwsSigV4, Hawk};

        let workspace = Folder {
            auth: Auth::AwsSigV4(AwsSigV4 {
                access_key: "{{awsKey}}".into(),
                secret_key: "{{awsSecret}}".into(),
                region: "{{region}}".into(),
                service: "execute-api".into(),
                ..AwsSigV4::default()
            }),
            ..Folder::default()
        };
        let request = Request {
            url: "https://api.example.com/items".into(),
            ..Request::default()
        };
        let effective = Effective::new(&[workspace], &request);
        let prepared = prepare(
            &request,
            &effective.headers,
            &effective.auth,
            &vars(&[("awsKey", "AKID"), ("awsSecret", "s")]),
            Path::new("."),
        )
        .unwrap();
        let authorization = find_header(&prepared.outgoing.headers, "authorization").unwrap();
        assert!(
            authorization.starts_with("AWS4-HMAC-SHA256 Credential=AKID/"),
            "{authorization}"
        );
        // The default Accept header is there before signing, so it is signed.
        assert!(authorization.contains("SignedHeaders=accept;host;x-amz-date"));
        assert_eq!(prepared.missing, ["region"]);

        let request = Request {
            url: "https://api.example.com/items".into(),
            auth: Auth::Hawk(Hawk {
                id: "id".into(),
                key: "{{hawkKey}}".into(),
                ..Hawk::default()
            }),
            ..Request::default()
        };
        let prepared = prepare_simple(&request, &vars(&[("hawkKey", "k")]));
        assert!(
            find_header(&prepared.outgoing.headers, "authorization")
                .unwrap()
                .starts_with(r#"Hawk id="id", ts=""#)
        );
    }

    #[test]
    fn files_must_be_inside_the_workspace() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("fixtures")).unwrap();
        std::fs::write(dir.path().join("fixtures/a.png"), b"png").unwrap();
        let file = resolve_file(dir.path(), "fixtures/a.png").unwrap();
        assert!(file.ends_with("a.png"));
        assert!(resolve_file(dir.path(), "../outside.txt").is_err());
        assert!(resolve_file(dir.path(), "/etc/passwd").is_err());
        assert!(matches!(
            resolve_file(dir.path(), "fixtures/missing.png"),
            Err(Error::NotFound(_))
        ));

        let request = Request {
            url: "https://example.com".into(),
            body: Body::Multipart {
                fields: vec![
                    FormField {
                        name: "avatar".into(),
                        value: String::new(),
                        file: Some("fixtures/a.png".into()),
                        content_type: None,
                        enabled: true,
                    },
                    FormField {
                        name: "name".into(),
                        value: "Ada".into(),
                        file: None,
                        content_type: None,
                        enabled: true,
                    },
                ],
            },
            ..Request::default()
        };
        let effective = Effective::new(&[], &request);
        let prepared = prepare(
            &request,
            &effective.headers,
            &effective.auth,
            &VarStack::new(),
            dir.path(),
        )
        .unwrap();
        let OutgoingBody::Multipart { parts } = prepared.outgoing.body else {
            panic!()
        };
        assert_eq!(parts.len(), 2);
        assert!(parts[0].file.is_some());
        assert!(find_header(&prepared.outgoing.headers, "content-type").is_none());
    }

    #[test]
    fn transport_fills_defaults_and_inherits() {
        let workspace = Folder {
            settings: Settings {
                http_version: Some(HttpVersion::Http2),
                proxy: Some(Proxy {
                    url: "http://proxy.example:3128".into(),
                    ..Proxy::default()
                }),
                ..Settings::default()
            },
            ..Folder::default()
        };
        let request = Request {
            settings: Settings {
                proxy: Some(Proxy {
                    url: "off".into(),
                    ..Proxy::default()
                }),
                max_response_mb: Some(2),
                ..Settings::default()
            },
            ..Request::default()
        };
        let effective = Effective::new(&[workspace], &request);
        let transport = transport(&effective.settings, &VarStack::new(), Path::new(".")).unwrap();
        assert_eq!(transport.http_version, HttpVersion::Http2);
        assert_eq!(transport.proxy, ProxyRoute::Off);
        assert_eq!(transport.max_body_bytes, 2 * 1024 * 1024);
        assert!(transport.cookies && transport.title_case_headers);
        assert_eq!(transport.timeout_ms, 30_000);
    }

    #[test]
    fn proxy_credentials_come_from_variables() {
        let settings = Settings {
            proxy: Some(Proxy {
                url: "socks5h://{{proxyHost}}:1080".into(),
                username: "{{proxyUser}}".into(),
                password: "{{proxyPass}}".into(),
                bypass: "localhost, .internal".into(),
            }),
            ..Settings::default()
        };
        let stack = vars(&[
            ("proxyHost", "10.0.0.5"),
            ("proxyUser", "ada"),
            ("proxyPass", "p@ss word"),
        ]);
        let transport = transport(&settings, &stack, Path::new(".")).unwrap();
        let ProxyRoute::Url { url, bypass } = &transport.proxy else {
            panic!("{:?}", transport.proxy)
        };
        assert_eq!(url, "socks5h://ada:p%40ss%20word@10.0.0.5:1080");
        assert_eq!(bypass, "localhost, .internal");
        assert!(
            !format!("{:?}", transport.proxy).contains("p%40ss"),
            "Debug hides the password"
        );

        let error = transport_error(&settings, &vars(&[("proxyHost", "h")]));
        assert!(error.contains("{{proxyUser}} is not defined"), "{error}");
        let mut wrong = settings.clone();
        wrong.proxy.as_mut().unwrap().url = "ftp://proxy:21".into();
        let error = transport_error(&wrong, &stack);
        assert!(error.contains("socks5://"), "{error}");
    }

    fn transport_error(settings: &Settings, stack: &VarStack) -> String {
        transport(settings, stack, Path::new("."))
            .unwrap_err()
            .to_string()
    }

    #[test]
    fn certificates_are_read_from_inside_the_workspace() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("certs")).unwrap();
        let pki = crate::test_server::pki();
        std::fs::write(dir.path().join("certs/ca.pem"), &pki.ca_pem).unwrap();
        std::fs::write(dir.path().join("certs/me.pem"), &pki.client_cert_pem).unwrap();
        std::fs::write(dir.path().join("certs/me.key"), &pki.client_key_pem).unwrap();
        std::fs::write(dir.path().join("certs/me.p12"), &pki.client_p12).unwrap();
        std::fs::write(dir.path().join("certs/junk.pem"), "not a certificate").unwrap();

        let pem = Settings {
            ca_certs: Some(vec!["certs/ca.pem".into()]),
            client_cert: Some(ClientCert {
                cert: "certs/me.pem".into(),
                key: "certs/me.key".into(),
                ..ClientCert::default()
            }),
            ..Settings::default()
        };
        let transport = transport(&pem, &VarStack::new(), dir.path()).unwrap();
        assert_eq!(transport.ca_certs.len(), 1);
        assert!(transport.client_identity.is_some());

        let pkcs12 = Settings {
            client_cert: Some(ClientCert {
                pkcs12: "certs/me.p12".into(),
                passphrase: "{{p12Pass}}".into(),
                ..ClientCert::default()
            }),
            ..Settings::default()
        };
        let stack = vars(&[("p12Pass", crate::test_server::P12_PASSPHRASE)]);
        assert!(
            super::transport(&pkcs12, &stack, dir.path())
                .unwrap()
                .client_identity
                .is_some()
        );

        let outside = Settings {
            ca_certs: Some(vec!["../ca.pem".into()]),
            ..Settings::default()
        };
        let error = super::transport(&outside, &VarStack::new(), dir.path()).unwrap_err();
        assert!(
            error.to_string().contains("outside the workspace"),
            "{error}"
        );
        let junk = Settings {
            ca_certs: Some(vec!["certs/junk.pem".into()]),
            ..Settings::default()
        };
        let error = super::transport(&junk, &VarStack::new(), dir.path()).unwrap_err();
        assert!(error.to_string().contains("junk.pem"), "{error}");

        // An empty client certificate stops the inherited one.
        let none = Settings {
            client_cert: Some(ClientCert::default()),
            ..Settings::default()
        }
        .or(pem);
        let transport = super::transport(&none, &VarStack::new(), dir.path()).unwrap();
        assert!(transport.client_identity.is_none());
    }
}
