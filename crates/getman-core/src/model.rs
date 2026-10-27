//! The documents a workspace is made of: requests, folders and environments.
//!
//! Every type here round-trips through YAML on disk and JSON over the local
//! API. Fields that hold their default value are left out when serialized,
//! so the files stay short and diffs stay small.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::lenient;

/// Named string values, in the order they were written.
pub type Vars = IndexMap<String, String>;

/// One request, stored as `<name>.yaml` inside the workspace. Requests are
/// HTTP unless `kind` says otherwise.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Request {
    /// Display name. When missing, the file name is used.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// What the request speaks. Not written for HTTP.
    #[serde(skip_serializing_if = "RequestKind::is_http")]
    pub kind: RequestKind,
    /// HTTP method, such as `GET` or `POST`. Any token is accepted. For
    /// gRPC, the method's full name, such as `shop.Orders/GetOrder`.
    pub method: String,
    /// URL with the enabled query parameters. May contain `{{variables}}`
    /// and `:path` parameters. For gRPC, the server's address.
    #[serde(deserialize_with = "lenient::string")]
    pub url: String,
    /// gRPC settings, for `kind: grpc`.
    #[serde(skip_serializing_if = "Grpc::is_empty")]
    pub grpc: Grpc,
    /// Free-form Markdown shown next to the request.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Values for the `:name` segments of the URL path.
    #[serde(
        skip_serializing_if = "IndexMap::is_empty",
        deserialize_with = "lenient::vars"
    )]
    pub path_params: Vars,
    /// Query parameters kept in the request but switched off, so they are
    /// not part of the URL.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub disabled_params: Vec<KeyValue>,
    /// Request headers.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<KeyValue>,
    /// Request body.
    #[serde(skip_serializing_if = "Body::is_none")]
    pub body: Body,
    /// Authentication. Inherited from the enclosing folder by default.
    #[serde(skip_serializing_if = "Auth::is_inherit")]
    pub auth: Auth,
    /// Variables scoped to this request.
    #[serde(
        skip_serializing_if = "IndexMap::is_empty",
        deserialize_with = "lenient::vars"
    )]
    pub variables: Vars,
    /// Scripts run before sending and after receiving.
    #[serde(skip_serializing_if = "Scripts::is_empty")]
    pub scripts: Scripts,
    /// Declarative assertions such as `status == 200`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<String>,
    /// Values to copy out of the response into variables, such as
    /// `token: json.access_token`.
    #[serde(
        skip_serializing_if = "IndexMap::is_empty",
        deserialize_with = "lenient::vars"
    )]
    pub captures: Vars,
    /// Transport settings that override the folder's.
    #[serde(skip_serializing_if = "Settings::is_empty")]
    pub settings: Settings,
}

impl Default for Request {
    fn default() -> Self {
        Self {
            name: String::new(),
            kind: RequestKind::Http,
            method: "GET".to_owned(),
            url: String::new(),
            grpc: Grpc::default(),
            description: String::new(),
            path_params: Vars::new(),
            disabled_params: Vec::new(),
            headers: Vec::new(),
            body: Body::None,
            auth: Auth::Inherit,
            variables: Vars::new(),
            scripts: Scripts::default(),
            checks: Vec::new(),
            captures: Vars::new(),
            settings: Settings::default(),
        }
    }
}

/// What a request speaks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum RequestKind {
    /// HTTP, the default.
    #[default]
    Http,
    /// A gRPC call over HTTP/2.
    Grpc,
}

impl RequestKind {
    /// Every kind, with the name files use for it.
    pub const ALL: &[(Self, &str)] = &[(Self::Http, "http"), (Self::Grpc, "grpc")];

    /// The name files use for this kind.
    pub fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(kind, _)| *kind == self)
            .map_or("http", |(_, name)| name)
    }

    /// Whether this is [`RequestKind::Http`].
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub fn is_http(&self) -> bool {
        *self == Self::Http
    }
}

impl Serialize for RequestKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.name())
    }
}

impl<'de> Deserialize<'de> for RequestKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        Self::ALL
            .iter()
            .find(|(_, known)| known.eq_ignore_ascii_case(name.trim()))
            .map(|(kind, _)| *kind)
            .ok_or_else(|| {
                let known: Vec<&str> = Self::ALL.iter().map(|(_, name)| *name).collect();
                serde::de::Error::custom(format!(
                    "{name:?} is not a kind of request this version of Getman knows; use {}",
                    known.join(" or ")
                ))
            })
    }
}

/// How a gRPC request finds its schema, how it connects, and what it
/// sends.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Grpc {
    /// A `.proto` file or a folder of them, relative to the workspace
    /// root. When empty, the schema comes from the server's reflection
    /// service.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub proto: String,
    /// Connect with TLS. Addresses that start with `https://` always do.
    #[serde(skip_serializing_if = "is_false")]
    pub tls: bool,
    /// The message to send, as JSON. May contain `{{variables}}`. Calls
    /// that stream from the client send every message written here, one
    /// after another.
    #[serde(
        skip_serializing_if = "String::is_empty",
        deserialize_with = "lenient::string"
    )]
    pub message: String,
}

impl Grpc {
    /// Whether nothing is set.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// A name and value pair, as used for headers, query parameters and form
/// fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyValue {
    /// Name of the header, parameter or field.
    pub name: String,
    /// Its value. May contain `{{variables}}`.
    #[serde(default, deserialize_with = "lenient::string")]
    pub value: String,
    /// Whether the pair is sent.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub enabled: bool,
    /// Optional note for people reading the request.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

impl KeyValue {
    /// An enabled pair without a description.
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            enabled: true,
            description: String::new(),
        }
    }
}

/// A field of a `multipart/form-data` body: text, or a file read when the
/// request is sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormField {
    /// Field name.
    pub name: String,
    /// Text value, used when `file` is not set.
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "lenient::string"
    )]
    pub value: String,
    /// Path of a file to upload, relative to the workspace root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Content type of the part. Guessed from the file name when missing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// Whether the field is sent.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub enabled: bool,
}

/// The body of a request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Body {
    /// No body.
    #[default]
    None,
    /// JSON text, sent as `application/json`.
    Json {
        /// The JSON text. May contain `{{variables}}`.
        #[serde(default)]
        content: String,
    },
    /// XML text, sent as `application/xml`.
    Xml {
        /// The XML text.
        #[serde(default)]
        content: String,
    },
    /// Any other text, sent as `text/plain` unless a type is given.
    Text {
        /// The text.
        #[serde(default)]
        content: String,
        /// Content type to send instead of `text/plain`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        content_type: Option<String>,
    },
    /// URL-encoded form, sent as `application/x-www-form-urlencoded`.
    Form {
        /// The form fields.
        #[serde(default)]
        fields: Vec<KeyValue>,
    },
    /// Multipart form with text fields and files.
    Multipart {
        /// The form fields.
        #[serde(default)]
        fields: Vec<FormField>,
    },
    /// The raw bytes of a file.
    Binary {
        /// Path of the file, relative to the workspace root.
        #[serde(default)]
        file: String,
        /// Content type to send. Guessed from the file name when missing.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        content_type: Option<String>,
    },
    /// A GraphQL operation, sent as JSON.
    Graphql {
        /// The query or mutation.
        #[serde(default)]
        query: String,
        /// Variables as JSON text.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        variables: String,
    },
}

impl Body {
    /// Whether this is [`Body::None`].
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }
}

/// How a request authenticates.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Auth {
    /// Use the enclosing folder's authentication.
    #[default]
    Inherit,
    /// Send no credentials, even when a folder sets some.
    None,
    /// HTTP Basic authentication.
    Basic {
        /// User name.
        #[serde(default, deserialize_with = "lenient::string")]
        username: String,
        /// Password.
        #[serde(default, deserialize_with = "lenient::string")]
        password: String,
    },
    /// A token in the `Authorization` header.
    Bearer {
        /// The token.
        #[serde(default, deserialize_with = "lenient::string")]
        token: String,
        /// Scheme written before the token. `Bearer` when empty.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        prefix: String,
    },
    /// A key sent in a header or the query string.
    ApiKey {
        /// Header or parameter name.
        #[serde(default)]
        key: String,
        /// The key's value.
        #[serde(default, deserialize_with = "lenient::string")]
        value: String,
        /// Where to send it.
        #[serde(default, rename = "in")]
        location: ApiKeyLocation,
    },
    /// HTTP Digest authentication (MD5, MD5-sess, SHA-256).
    Digest {
        /// User name.
        #[serde(default, deserialize_with = "lenient::string")]
        username: String,
        /// Password.
        #[serde(default, deserialize_with = "lenient::string")]
        password: String,
    },
    /// An OAuth 2.0 access token, fetched and cached automatically.
    Oauth2(OAuth2),
}

impl Auth {
    /// Whether this is [`Auth::Inherit`].
    pub fn is_inherit(&self) -> bool {
        matches!(self, Self::Inherit)
    }
}

/// Where an API key goes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiKeyLocation {
    /// In a request header.
    #[default]
    Header,
    /// In the query string.
    Query,
}

/// OAuth 2.0 settings for grants that need no browser.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OAuth2 {
    /// The grant used to get a token.
    pub grant: OAuth2Grant,
    /// Token endpoint.
    #[serde(deserialize_with = "lenient::string")]
    pub token_url: String,
    /// Client identifier.
    #[serde(deserialize_with = "lenient::string")]
    pub client_id: String,
    /// Client secret.
    #[serde(
        skip_serializing_if = "String::is_empty",
        deserialize_with = "lenient::string"
    )]
    pub client_secret: String,
    /// Space-separated scopes.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub scope: String,
    /// Audience, for providers that require one.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub audience: String,
    /// Resource owner's user name, for the password grant.
    #[serde(
        skip_serializing_if = "String::is_empty",
        deserialize_with = "lenient::string"
    )]
    pub username: String,
    /// Resource owner's password, for the password grant.
    #[serde(
        skip_serializing_if = "String::is_empty",
        deserialize_with = "lenient::string"
    )]
    pub password: String,
    /// How the client authenticates to the token endpoint.
    pub client_auth: ClientAuth,
}

/// OAuth 2.0 grant types Getman can complete on its own.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OAuth2Grant {
    /// Client credentials grant.
    #[default]
    ClientCredentials,
    /// Resource owner password credentials grant.
    Password,
}

/// How an OAuth 2.0 client sends its credentials to the token endpoint.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientAuth {
    /// In a Basic `Authorization` header.
    #[default]
    Basic,
    /// As `client_id` and `client_secret` form fields.
    Body,
}

/// JavaScript run around a request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Scripts {
    /// Runs before the request is sent and may change it.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub pre_request: String,
    /// Runs after the response arrives; usually holds tests.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub post_response: String,
}

impl Scripts {
    /// Whether both scripts are empty.
    pub fn is_empty(&self) -> bool {
        self.pre_request.trim().is_empty() && self.post_response.trim().is_empty()
    }
}

/// Transport settings. Unset values fall back to the enclosing folder, then
/// to the defaults.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Give up after this many milliseconds. Default 30 000; 0 waits forever.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    /// Follow redirects. Default true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub follow_redirects: Option<bool>,
    /// Most redirects to follow. Default 10.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_redirects: Option<u32>,
    /// Check TLS certificates. Default true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verify_tls: Option<bool>,
}

impl Settings {
    /// Whether no setting is set.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// These settings, with unset values taken from `parent`.
    #[must_use]
    pub fn or(self, parent: Self) -> Self {
        Self {
            timeout_ms: self.timeout_ms.or(parent.timeout_ms),
            follow_redirects: self.follow_redirects.or(parent.follow_redirects),
            max_redirects: self.max_redirects.or(parent.max_redirects),
            verify_tls: self.verify_tls.or(parent.verify_tls),
        }
    }
}

/// Settings shared by everything in a folder, stored as `_folder.yaml`.
/// The workspace root uses the same shape in `getman.yaml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Folder {
    /// Display name. When missing, the directory name is used.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Free-form Markdown.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Variables for every request in the folder.
    #[serde(
        skip_serializing_if = "IndexMap::is_empty",
        deserialize_with = "lenient::vars"
    )]
    pub variables: Vars,
    /// Headers added to every request in the folder, unless the request sets
    /// the same header.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<KeyValue>,
    /// Authentication inherited by the folder's requests.
    #[serde(skip_serializing_if = "Auth::is_inherit")]
    pub auth: Auth,
    /// Scripts run around every request in the folder, outermost first.
    #[serde(skip_serializing_if = "Scripts::is_empty")]
    pub scripts: Scripts,
    /// Transport settings inherited by the folder's requests.
    #[serde(skip_serializing_if = "Settings::is_empty")]
    pub settings: Settings,
    /// Order of the folder's items, by file or directory name without
    /// extension. Items not listed follow in alphabetical order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub order: Vec<String>,
}

/// A set of variables to switch between, such as `local` or `production`,
/// stored in `environments/<name>.yaml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Environment {
    /// Display name. When missing, the file name is used.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// The environment's variables.
    #[serde(
        skip_serializing_if = "IndexMap::is_empty",
        deserialize_with = "lenient::vars"
    )]
    pub variables: Vars,
    /// Names of variables whose values are kept out of the shared file and
    /// stored in the git-ignored `<name>.local.yaml` beside it.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub secrets: Vec<String>,
}

fn yes() -> bool {
    true
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_true(value: &bool) -> bool {
    *value
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(value: &bool) -> bool {
    !*value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_minimal_request_reads_with_defaults() {
        let request: Request = serde_yaml_ng::from_str("url: https://example.com").unwrap();
        assert_eq!(request.method, "GET");
        assert_eq!(request.url, "https://example.com");
        assert!(request.auth.is_inherit());
        assert!(request.body.is_none());
    }

    #[test]
    fn a_request_round_trips_through_yaml() {
        let request = Request {
            name: "Create user".into(),
            method: "POST".into(),
            url: "{{baseUrl}}/users/:id?verbose=true".into(),
            path_params: [("id".to_owned(), "7".to_owned())].into_iter().collect(),
            disabled_params: vec![KeyValue {
                enabled: false,
                ..KeyValue::new("debug", "1")
            }],
            headers: vec![KeyValue::new("Accept", "application/json")],
            body: Body::Json {
                content: "{\n  \"name\": \"Ada\"\n}\n".into(),
            },
            auth: Auth::ApiKey {
                key: "X-Key".into(),
                value: "{{key}}".into(),
                location: ApiKeyLocation::Query,
            },
            checks: vec!["status == 201".into()],
            settings: Settings {
                timeout_ms: Some(500),
                ..Settings::default()
            },
            ..Request::default()
        };
        let yaml = serde_yaml_ng::to_string(&request).unwrap();
        assert!(yaml.contains("type: api_key"), "{yaml}");
        assert!(yaml.contains("in: query"), "{yaml}");
        assert!(!yaml.contains("scripts"), "{yaml}");
        let back: Request = serde_yaml_ng::from_str(&yaml).unwrap();
        assert_eq!(back, request);
    }

    #[test]
    fn scalars_of_any_type_read_as_strings() {
        let env: Environment = serde_yaml_ng::from_str(
            "variables:\n  port: 3000\n  debug: true\n  ratio: 1.5\n  empty:\n",
        )
        .unwrap();
        assert_eq!(env.variables["port"], "3000");
        assert_eq!(env.variables["debug"], "true");
        assert_eq!(env.variables["ratio"], "1.5");
        assert_eq!(env.variables["empty"], "");
    }

    #[test]
    fn oauth2_settings_sit_beside_the_type() {
        let auth: Auth = serde_yaml_ng::from_str(
            "type: oauth2\ngrant: password\ntoken_url: https://id.example.com/token\nclient_id: cli\nusername: ada\n",
        )
        .unwrap();
        let Auth::Oauth2(oauth) = auth else {
            panic!("expected oauth2, got {auth:?}");
        };
        assert_eq!(oauth.grant, OAuth2Grant::Password);
        assert_eq!(oauth.username, "ada");
        assert_eq!(oauth.client_auth, ClientAuth::Basic);
    }

    #[test]
    fn http_requests_do_not_write_their_kind() {
        let yaml = serde_yaml_ng::to_string(&Request {
            url: "https://example.com".into(),
            ..Request::default()
        })
        .unwrap();
        assert!(!yaml.contains("kind"), "{yaml}");
        assert!(!yaml.contains("grpc"), "{yaml}");
    }

    #[test]
    fn a_grpc_request_round_trips_through_yaml() {
        let request = Request {
            name: "Say hello".into(),
            kind: RequestKind::Grpc,
            method: "greet.Greeter/SayHello".into(),
            url: "localhost:50051".into(),
            grpc: Grpc {
                proto: "protos/greet.proto".into(),
                tls: false,
                message: "{ \"name\": \"{{who}}\" }\n".into(),
            },
            headers: vec![KeyValue::new("x-trace", "abc")],
            checks: vec!["status == 0".into()],
            ..Request::default()
        };
        let yaml = serde_yaml_ng::to_string(&request).unwrap();
        assert!(yaml.starts_with("name: Say hello\nkind: grpc\n"), "{yaml}");
        assert!(!yaml.contains("tls"), "{yaml}");
        let back: Request = serde_yaml_ng::from_str(&yaml).unwrap();
        assert_eq!(back, request);
    }

    #[test]
    fn an_unknown_kind_is_an_error_not_http() {
        let error = serde_yaml_ng::from_str::<Request>("kind: carrier-pigeon\nurl: coop:1\n")
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("\"carrier-pigeon\" is not a kind of request"),
            "{error}"
        );
        assert!(error.contains("http or grpc"), "{error}");
        let request: Request = serde_yaml_ng::from_str("kind: GRPC\nurl: a:1\n").unwrap();
        assert_eq!(request.kind, RequestKind::Grpc);
    }

    #[test]
    fn settings_fall_back_to_the_parent() {
        let child = Settings {
            timeout_ms: Some(10),
            ..Settings::default()
        };
        let parent = Settings {
            timeout_ms: Some(99),
            verify_tls: Some(false),
            ..Settings::default()
        };
        let merged = child.or(parent);
        assert_eq!(merged.timeout_ms, Some(10));
        assert_eq!(merged.verify_tls, Some(false));
        assert_eq!(merged.follow_redirects, None);
    }
}
