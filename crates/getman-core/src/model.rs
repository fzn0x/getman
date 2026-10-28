//! The documents a workspace is made of: requests, folders and environments.
//!
//! Every type here round-trips through YAML on disk and JSON over the local
//! API. Fields that hold their default value are left out when serialized,
//! so the files stay short and diffs stay small.

use indexmap::IndexMap;
use serde::{Deserialize, Deserializer, Serialize};

use crate::lenient;
use crate::realtime::{SocketIoSettings, SseSettings, WebSocketSettings};

/// Named string values, in the order they were written.
pub type Vars = IndexMap<String, String>;

/// One HTTP request, stored as `<name>.yaml` inside the workspace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Request {
    /// Display name. When missing, the file name is used.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// What the file holds: an HTTP request unless it says otherwise.
    #[serde(skip_serializing_if = "RequestKind::is_http")]
    pub kind: RequestKind,
    /// HTTP method, such as `GET` or `POST`. Any token is accepted.
    pub method: String,
    /// URL with the enabled query parameters. May contain `{{variables}}`
    /// and `:path` parameters.
    #[serde(deserialize_with = "lenient::string")]
    pub url: String,
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
    /// Saved responses, kept to look at and to serve from `getman mock`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<Example>,
    /// Subprotocols and saved messages of a WebSocket request.
    #[serde(skip_serializing_if = "WebSocketSettings::is_empty")]
    pub websocket: WebSocketSettings,
    /// Settings of a Server-Sent Events request.
    #[serde(skip_serializing_if = "SseSettings::is_empty")]
    pub sse: SseSettings,
    /// Namespace, events and saved events of a Socket.IO request.
    #[serde(skip_serializing_if = "SocketIoSettings::is_empty")]
    pub socketio: SocketIoSettings,
}

impl Default for Request {
    fn default() -> Self {
        Self {
            name: String::new(),
            kind: RequestKind::Http,
            method: "GET".to_owned(),
            url: String::new(),
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
            examples: Vec::new(),
            websocket: WebSocketSettings::default(),
            sse: SseSettings::default(),
            socketio: SocketIoSettings::default(),
        }
    }
}

/// What a request file holds: one HTTP request and its response, or a
/// connection that stays open and carries messages.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestKind {
    /// An HTTP request. Files without a `kind` hold one.
    #[default]
    Http,
    /// A WebSocket connection.
    Websocket,
    /// A stream of Server-Sent Events.
    Sse,
    /// A Socket.IO connection, over WebSocket.
    Socketio,
}

impl RequestKind {
    /// Every kind, in the order they are offered.
    pub const ALL: [Self; 4] = [Self::Http, Self::Websocket, Self::Sse, Self::Socketio];

    /// The name written in files, such as `websocket`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Websocket => "websocket",
            Self::Sse => "sse",
            Self::Socketio => "socketio",
        }
    }

    /// A name for people, such as `WebSocket`.
    pub fn label(self) -> &'static str {
        match self {
            Self::Http => "HTTP",
            Self::Websocket => "WebSocket",
            Self::Sse => "Server-Sent Events",
            Self::Socketio => "Socket.IO",
        }
    }

    /// Whether this is [`RequestKind::Http`].
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub fn is_http(&self) -> bool {
        *self == Self::Http
    }

    /// Whether requests of this kind stay open and carry messages, so they
    /// are connected to rather than sent once.
    pub fn is_realtime(self) -> bool {
        matches!(self, Self::Websocket | Self::Sse | Self::Socketio)
    }
}

impl<'de> Deserialize<'de> for RequestKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = Option::<String>::deserialize(deserializer)?.unwrap_or_default();
        let name = name.trim();
        if name.is_empty() {
            return Ok(Self::Http);
        }
        Self::ALL
            .into_iter()
            .find(|kind| kind.as_str().eq_ignore_ascii_case(name))
            .ok_or_else(|| {
                let known: Vec<&str> = Self::ALL.iter().map(|kind| kind.as_str()).collect();
                serde::de::Error::custom(format!(
                    "{name:?} is not a request kind this version of Getman knows ({}); a newer version may have written it",
                    known.join(", ")
                ))
            })
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
    /// HTTP Digest authentication (MD5, SHA-256, SHA-512-256 and their
    /// -sess forms).
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
    /// AWS Signature Version 4, computed when the request is sent.
    #[serde(rename = "aws_sigv4")]
    AwsSigV4(AwsSigV4),
    /// A JSON Web Token, signed when the request is sent.
    Jwt(Jwt),
    /// Atlassian ASAP: a short-lived JWT signed with a private key.
    Asap(Asap),
    /// OAuth 1.0a, signed when the request is sent.
    Oauth1(OAuth1),
    /// Hawk, signed when the request is sent.
    Hawk(Hawk),
    /// Akamai EdgeGrid, signed when the request is sent.
    Edgegrid(EdgeGrid),
}

impl Auth {
    /// Whether this is [`Auth::Inherit`].
    pub fn is_inherit(&self) -> bool {
        matches!(self, Self::Inherit)
    }
}

/// Where an API key, a token or a signature goes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiKeyLocation {
    /// In a request header.
    #[default]
    Header,
    /// In the query string.
    Query,
}

/// OAuth 2.0 settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OAuth2 {
    /// The grant used to get a token.
    pub grant: OAuth2Grant,
    /// Authorization endpoint, for the grants that sign in with a browser.
    #[serde(
        skip_serializing_if = "String::is_empty",
        deserialize_with = "lenient::string"
    )]
    pub auth_url: String,
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
    /// Where the provider sends the browser back: a loopback `http` URL.
    /// When empty, Getman listens on a free port of 127.0.0.1.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub callback_url: String,
    /// Proof Key for Code Exchange, for the authorization code grant.
    #[serde(skip_serializing_if = "Pkce::is_off")]
    pub pkce: Pkce,
    /// Space-separated scopes.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub scope: String,
    /// Audience, for providers that require one.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub audience: String,
    /// Resource indicator (RFC 8707), for providers that require one.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub resource: String,
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
    /// More parameters for the authorization request.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub auth_params: Vec<KeyValue>,
    /// More parameters for token requests.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub token_params: Vec<KeyValue>,
    /// Scheme written before the token. `Bearer` when empty.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub prefix: String,
    /// Where the token goes: the `Authorization` header, or the query as
    /// `access_token`.
    #[serde(skip_serializing_if = "is_header")]
    pub token_in: ApiKeyLocation,
}

/// OAuth 2.0 grant types.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OAuth2Grant {
    /// Client credentials grant.
    #[default]
    ClientCredentials,
    /// Resource owner password credentials grant.
    Password,
    /// Authorization code grant: the user signs in with a browser.
    AuthorizationCode,
    /// Implicit grant: the browser receives the token directly.
    Implicit,
}

impl OAuth2Grant {
    /// Whether the user signs in with a browser to get a token.
    pub fn uses_browser(self) -> bool {
        matches!(self, Self::AuthorizationCode | Self::Implicit)
    }
}

/// Proof Key for Code Exchange (RFC 7636).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pkce {
    /// No PKCE.
    #[default]
    Off,
    /// A SHA-256 challenge.
    S256,
    /// The verifier itself as the challenge.
    Plain,
}

impl Pkce {
    /// Whether PKCE is off.
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub fn is_off(&self) -> bool {
        *self == Self::Off
    }
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

/// AWS Signature Version 4 settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AwsSigV4 {
    /// Access key ID.
    #[serde(deserialize_with = "lenient::string")]
    pub access_key: String,
    /// Secret access key.
    #[serde(deserialize_with = "lenient::string")]
    pub secret_key: String,
    /// Session token, for temporary credentials.
    #[serde(
        skip_serializing_if = "String::is_empty",
        deserialize_with = "lenient::string"
    )]
    pub session_token: String,
    /// Region, such as `us-east-1`.
    #[serde(deserialize_with = "lenient::string")]
    pub region: String,
    /// Service name, such as `s3` or `execute-api`.
    #[serde(deserialize_with = "lenient::string")]
    pub service: String,
    /// Where the signature goes: headers, or a presigned query string.
    #[serde(rename = "in", skip_serializing_if = "is_header")]
    pub location: ApiKeyLocation,
    /// Seconds a presigned URL stays valid. Default 3600.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<u64>,
    /// Sign `UNSIGNED-PAYLOAD` instead of the body's hash, as S3 allows.
    #[serde(skip_serializing_if = "is_false")]
    pub unsigned_payload: bool,
}

/// A JSON Web Token signed when the request is sent.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Jwt {
    /// Signature algorithm. Default `HS256`.
    #[serde(skip_serializing_if = "JwtAlgorithm::is_default")]
    pub algorithm: JwtAlgorithm,
    /// Secret for the HMAC algorithms.
    #[serde(
        skip_serializing_if = "String::is_empty",
        deserialize_with = "lenient::string"
    )]
    pub secret: String,
    /// Whether `secret` is base64 text to decode first.
    #[serde(skip_serializing_if = "is_false")]
    pub secret_base64: bool,
    /// PEM private key for the RSA and ECDSA algorithms, usually a
    /// `{{variable}}`.
    #[serde(
        skip_serializing_if = "String::is_empty",
        deserialize_with = "lenient::string"
    )]
    pub private_key: String,
    /// A PEM private key file in the workspace, used instead of
    /// `private_key`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub private_key_file: String,
    /// Extra header fields, as JSON text. `alg` is set from `algorithm`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub header: String,
    /// The claims, as JSON text.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub payload: String,
    /// Add an `iat` claim with the current time.
    #[serde(skip_serializing_if = "is_false")]
    pub issued_at: bool,
    /// Add an `exp` claim this many seconds from now.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<u64>,
    /// Where the token goes: the `Authorization` header or the query.
    #[serde(rename = "in", skip_serializing_if = "is_header")]
    pub location: ApiKeyLocation,
    /// Scheme written before the token in the header. `Bearer` when empty.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub prefix: String,
    /// Query parameter name. `token` when empty.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub param: String,
}

/// A JSON Web Signature algorithm.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum JwtAlgorithm {
    /// HMAC with SHA-256.
    #[default]
    Hs256,
    /// HMAC with SHA-384.
    Hs384,
    /// HMAC with SHA-512.
    Hs512,
    /// RSA PKCS#1 v1.5 with SHA-256.
    Rs256,
    /// RSA PKCS#1 v1.5 with SHA-384.
    Rs384,
    /// RSA PKCS#1 v1.5 with SHA-512.
    Rs512,
    /// RSA-PSS with SHA-256.
    Ps256,
    /// RSA-PSS with SHA-384.
    Ps384,
    /// RSA-PSS with SHA-512.
    Ps512,
    /// ECDSA on P-256 with SHA-256.
    Es256,
    /// ECDSA on P-384 with SHA-384.
    Es384,
}

impl JwtAlgorithm {
    /// Whether this is the default, `HS256`.
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Atlassian ASAP settings: a JWT signed with a private key, sent as a
/// Bearer token.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Asap {
    /// Signature algorithm. Default `RS256`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub algorithm: Option<JwtAlgorithm>,
    /// The `iss` claim: who issues the token.
    #[serde(deserialize_with = "lenient::string")]
    pub issuer: String,
    /// The `sub` claim. Left out when empty.
    #[serde(
        skip_serializing_if = "String::is_empty",
        deserialize_with = "lenient::string"
    )]
    pub subject: String,
    /// The `aud` claim; separate several with commas.
    #[serde(deserialize_with = "lenient::string")]
    pub audience: String,
    /// The `kid` header: the key's ID.
    #[serde(deserialize_with = "lenient::string")]
    pub key_id: String,
    /// PEM private key, usually a `{{variable}}`.
    #[serde(
        skip_serializing_if = "String::is_empty",
        deserialize_with = "lenient::string"
    )]
    pub private_key: String,
    /// A PEM private key file in the workspace, used instead of
    /// `private_key`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub private_key_file: String,
    /// More claims, as JSON text.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub claims: String,
    /// Seconds the token is valid. Default 3600.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<u64>,
}

/// OAuth 1.0a settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OAuth1 {
    /// How the request is signed. Default `HMAC-SHA1`.
    #[serde(skip_serializing_if = "OAuth1Method::is_default")]
    pub signature_method: OAuth1Method,
    /// Consumer key.
    #[serde(deserialize_with = "lenient::string")]
    pub consumer_key: String,
    /// Consumer secret, for the HMAC and PLAINTEXT methods.
    #[serde(
        skip_serializing_if = "String::is_empty",
        deserialize_with = "lenient::string"
    )]
    pub consumer_secret: String,
    /// Access token.
    #[serde(
        skip_serializing_if = "String::is_empty",
        deserialize_with = "lenient::string"
    )]
    pub token: String,
    /// Token secret.
    #[serde(
        skip_serializing_if = "String::is_empty",
        deserialize_with = "lenient::string"
    )]
    pub token_secret: String,
    /// PEM private key for the RSA methods, usually a `{{variable}}`.
    #[serde(
        skip_serializing_if = "String::is_empty",
        deserialize_with = "lenient::string"
    )]
    pub private_key: String,
    /// A PEM private key file in the workspace, used instead of
    /// `private_key`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub private_key_file: String,
    /// `realm`, sent in the header only.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub realm: String,
    /// `oauth_callback`, when asking for a request token.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub callback: String,
    /// `oauth_verifier`, when exchanging a request token.
    #[serde(
        skip_serializing_if = "String::is_empty",
        deserialize_with = "lenient::string"
    )]
    pub verifier: String,
    /// `oauth_version`. `1.0` when empty.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub version: String,
    /// A fixed `oauth_timestamp`, instead of the current time.
    #[serde(
        skip_serializing_if = "String::is_empty",
        deserialize_with = "lenient::string"
    )]
    pub timestamp: String,
    /// A fixed `oauth_nonce`, instead of a random one.
    #[serde(
        skip_serializing_if = "String::is_empty",
        deserialize_with = "lenient::string"
    )]
    pub nonce: String,
    /// Sign the body with `oauth_body_hash`, for bodies that are not forms.
    #[serde(skip_serializing_if = "is_false")]
    pub body_hash: bool,
    /// Where the parameters go.
    #[serde(rename = "in", skip_serializing_if = "OAuth1Location::is_default")]
    pub location: OAuth1Location,
}

/// An OAuth 1.0a signature method.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OAuth1Method {
    /// HMAC with SHA-1.
    #[default]
    #[serde(rename = "HMAC-SHA1")]
    HmacSha1,
    /// HMAC with SHA-256.
    #[serde(rename = "HMAC-SHA256")]
    HmacSha256,
    /// HMAC with SHA-512.
    #[serde(rename = "HMAC-SHA512")]
    HmacSha512,
    /// The secrets themselves, for use over TLS only.
    #[serde(rename = "PLAINTEXT")]
    Plaintext,
    /// RSA with SHA-1.
    #[serde(rename = "RSA-SHA1")]
    RsaSha1,
    /// RSA with SHA-256.
    #[serde(rename = "RSA-SHA256")]
    RsaSha256,
    /// RSA with SHA-512.
    #[serde(rename = "RSA-SHA512")]
    RsaSha512,
}

impl OAuth1Method {
    /// Whether this is the default, `HMAC-SHA1`.
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Where OAuth 1.0a parameters go.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OAuth1Location {
    /// The `Authorization` header.
    #[default]
    Header,
    /// The query string.
    Query,
    /// A URL-encoded form body.
    Body,
}

impl OAuth1Location {
    /// Whether this is the default, the header.
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Hawk settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Hawk {
    /// Key identifier.
    #[serde(deserialize_with = "lenient::string")]
    pub id: String,
    /// The shared key.
    #[serde(deserialize_with = "lenient::string")]
    pub key: String,
    /// MAC algorithm. Default `sha256`.
    #[serde(skip_serializing_if = "HawkAlgorithm::is_default")]
    pub algorithm: HawkAlgorithm,
    /// Application-specific data, sent as `ext`.
    #[serde(
        skip_serializing_if = "String::is_empty",
        deserialize_with = "lenient::string"
    )]
    pub ext: String,
    /// Application ID, for delegated credentials.
    #[serde(
        skip_serializing_if = "String::is_empty",
        deserialize_with = "lenient::string"
    )]
    pub app: String,
    /// Delegated-by application ID.
    #[serde(
        skip_serializing_if = "String::is_empty",
        deserialize_with = "lenient::string"
    )]
    pub dlg: String,
    /// Sign the body too, with a payload `hash`.
    #[serde(skip_serializing_if = "is_false")]
    pub payload_hash: bool,
}

/// A Hawk MAC algorithm.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HawkAlgorithm {
    /// HMAC with SHA-256.
    #[default]
    Sha256,
    /// HMAC with SHA-1.
    Sha1,
}

impl HawkAlgorithm {
    /// Whether this is the default, SHA-256.
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Akamai EdgeGrid settings, as an `.edgerc` section holds them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EdgeGrid {
    /// Client token.
    #[serde(deserialize_with = "lenient::string")]
    pub client_token: String,
    /// Client secret.
    #[serde(deserialize_with = "lenient::string")]
    pub client_secret: String,
    /// Access token.
    #[serde(deserialize_with = "lenient::string")]
    pub access_token: String,
    /// The API host, such as `https://akab-xxx.luna.akamaiapis.net`. When
    /// set, requests go there, keeping their path and query.
    #[serde(
        skip_serializing_if = "String::is_empty",
        deserialize_with = "lenient::string"
    )]
    pub base_url: String,
    /// Headers included in the signature, in order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub headers_to_sign: Vec<String>,
    /// Most body bytes a POST signature covers. Default 131072.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_body: Option<u64>,
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
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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
    /// HTTP version to speak. Default `auto`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_version: Option<HttpVersion>,
    /// Send HTTP/1.1 header names in Title-Case, such as `Content-Type`.
    /// Default true. HTTP/2 and HTTP/3 always send them in lowercase.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_case_headers: Option<bool>,
    /// Proxy to send requests through. Default: the system's proxy
    /// settings and the `HTTPS_PROXY`, `HTTP_PROXY` and `NO_PROXY`
    /// environment variables.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy: Option<Proxy>,
    /// Certificate to present when a server asks for one. An empty one
    /// stops a folder's from being used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_cert: Option<ClientCert>,
    /// Extra certificate authorities to trust: PEM files, relative to the
    /// workspace root, trusted besides the system's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ca_certs: Option<Vec<String>>,
    /// Send and store cookies with the cookie jar. Default true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cookies: Option<bool>,
    /// Largest response body to read, in MiB; the rest is cut off. Default
    /// 100; 0 reads everything.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_response_mb: Option<u64>,
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
            http_version: self.http_version.or(parent.http_version),
            title_case_headers: self.title_case_headers.or(parent.title_case_headers),
            proxy: self.proxy.or(parent.proxy),
            client_cert: self.client_cert.or(parent.client_cert),
            ca_certs: self.ca_certs.or(parent.ca_certs),
            cookies: self.cookies.or(parent.cookies),
            max_response_mb: self.max_response_mb.or(parent.max_response_mb),
        }
    }
}

/// The HTTP version a request is sent with.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HttpVersion {
    /// HTTP/2 when the server offers it over TLS, HTTP/1.1 otherwise.
    #[default]
    Auto,
    /// HTTP/1.1 only.
    Http1,
    /// HTTP/2 only; over plain `http://` without asking first (prior
    /// knowledge).
    Http2,
    /// HTTP/3, over QUIC. Needs an `https://` URL.
    Http3,
}

/// A proxy, written as just its URL (`proxy: off`) when that is all it
/// has, and as a map otherwise.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Proxy {
    /// `system` (the system's and the environment's proxies), `off` (no
    /// proxy), or the proxy's URL: `http://`, `https://`, `socks5://`, or
    /// `socks5h://` to let the proxy look up host names. May contain
    /// `{{variables}}`.
    pub url: String,
    /// User name for the proxy. May contain `{{variables}}`.
    pub username: String,
    /// Password for the proxy. Use a `{{variable}}` kept in a git-ignored
    /// `*.local.yaml` file.
    pub password: String,
    /// Hosts reached without the proxy, separated by commas, as in
    /// `NO_PROXY`: `localhost, .internal.example.com, 10.0.0.0/8`.
    pub bypass: String,
}

impl Serialize for Proxy {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap as _;
        if self.username.is_empty() && self.password.is_empty() && self.bypass.is_empty() {
            return serializer.serialize_str(&self.url);
        }
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("url", &self.url)?;
        for (key, value) in [
            ("username", &self.username),
            ("password", &self.password),
            ("bypass", &self.bypass),
        ] {
            if !value.is_empty() {
                map.serialize_entry(key, value)?;
            }
        }
        map.end()
    }
}

impl Proxy {
    /// Whether this means "use no proxy".
    pub fn is_off(&self) -> bool {
        self.url.trim().eq_ignore_ascii_case("off")
    }

    /// Whether this means "use the system's proxies", the default.
    pub fn is_system(&self) -> bool {
        let url = self.url.trim();
        url.is_empty() || url.eq_ignore_ascii_case("system")
    }
}

impl<'de> Deserialize<'de> for Proxy {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Default, Deserialize)]
        #[serde(default)]
        struct Full {
            #[serde(deserialize_with = "lenient::string")]
            url: String,
            #[serde(deserialize_with = "lenient::string")]
            username: String,
            #[serde(deserialize_with = "lenient::string")]
            password: String,
            #[serde(deserialize_with = "lenient::string")]
            bypass: String,
        }

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            // `proxy: false` reads as `off`, for YAML 1.1 habits.
            Switch(bool),
            Url(String),
            Full(Full),
        }

        Ok(match Repr::deserialize(deserializer)? {
            Repr::Switch(on) => Self {
                url: if on { "system" } else { "off" }.to_owned(),
                ..Self::default()
            },
            Repr::Url(url) => Self {
                url,
                ..Self::default()
            },
            Repr::Full(full) => Self {
                url: full.url,
                username: full.username,
                password: full.password,
                bypass: full.bypass,
            },
        })
    }
}

/// A TLS client certificate, from PEM files or a PKCS#12 bundle. Paths are
/// relative to the workspace root and may contain `{{variables}}`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClientCert {
    /// PEM file with the certificate, and any intermediates after it.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub cert: String,
    /// PEM file with the unencrypted private key. May be the same file as
    /// `cert`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub key: String,
    /// PKCS#12 file (`.p12` or `.pfx`) with the certificate and key, used
    /// instead of `cert` and `key`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub pkcs12: String,
    /// Passphrase of the PKCS#12 file. Use a `{{variable}}` kept in a
    /// git-ignored `*.local.yaml` file.
    #[serde(
        skip_serializing_if = "String::is_empty",
        deserialize_with = "lenient::string"
    )]
    pub passphrase: String,
}

impl ClientCert {
    /// Whether no certificate is named, which means "present none".
    pub fn is_empty(&self) -> bool {
        self.cert.trim().is_empty() && self.key.trim().is_empty() && self.pkcs12.trim().is_empty()
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

/// A saved response: what a request answered once, kept with the request
/// to look at later and to serve from `getman mock`.
///
/// Short text bodies are stored inline in `body`. Bodies longer than
/// [`crate::examples::INLINE_LIMIT`] bytes, or that are not text, are
/// stored in a file named by `body_file`, inside the `<request>.examples`
/// directory beside the request file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Example {
    /// Name, unique within the request.
    pub name: String,
    /// Status code.
    pub status: u16,
    /// Response headers.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<KeyValue>,
    /// The body, when it is short text.
    #[serde(
        skip_serializing_if = "String::is_empty",
        deserialize_with = "lenient::string"
    )]
    pub body: String,
    /// File holding the body, in the `<request>.examples` directory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_file: Option<String>,
    /// The request that got this response, when it is known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<ExampleRequest>,
}

impl Default for Example {
    fn default() -> Self {
        Self {
            name: String::new(),
            status: 200,
            headers: Vec::new(),
            body: String::new(),
            body_file: None,
            request: None,
        }
    }
}

/// The request an example's response answered, as it was sent.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExampleRequest {
    /// HTTP method.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub method: String,
    /// URL. `getman mock` matches incoming requests against it.
    #[serde(
        skip_serializing_if = "String::is_empty",
        deserialize_with = "lenient::string"
    )]
    pub url: String,
    /// Request headers.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<KeyValue>,
    /// The body, when it was text.
    #[serde(
        skip_serializing_if = "String::is_empty",
        deserialize_with = "lenient::string"
    )]
    pub body: String,
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

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_header(location: &ApiKeyLocation) -> bool {
    *location == ApiKeyLocation::Header
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
    fn examples_stay_short_in_yaml() {
        let request = Request {
            url: "{{baseUrl}}/users/:id".into(),
            examples: vec![Example {
                name: "Found".into(),
                headers: vec![KeyValue::new("Content-Type", "application/json")],
                body: "{\"id\": 7}\n".into(),
                ..Example::default()
            }],
            ..Request::default()
        };
        let yaml = serde_yaml_ng::to_string(&request).unwrap();
        assert!(
            yaml.contains("examples:\n- name: Found\n  status: 200\n"),
            "{yaml}"
        );
        assert!(!yaml.contains("body_file"), "{yaml}");
        assert!(!yaml.contains("request:"), "{yaml}");
        let back: Request = serde_yaml_ng::from_str(&yaml).unwrap();
        assert_eq!(back, request);

        let read: Request =
            serde_yaml_ng::from_str("url: x\nexamples:\n  - name: Gone\n    status: 404\n")
                .unwrap();
        assert_eq!(read.examples[0].status, 404);
        assert_eq!(read.examples[0].body, "");
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
    fn aws_signatures_write_only_what_is_set() {
        let auth = Auth::AwsSigV4(AwsSigV4 {
            access_key: "{{awsKey}}".into(),
            secret_key: "{{awsSecret}}".into(),
            region: "eu-west-1".into(),
            service: "execute-api".into(),
            ..AwsSigV4::default()
        });
        let yaml = serde_yaml_ng::to_string(&auth).unwrap();
        assert_eq!(
            yaml,
            "type: aws_sigv4\naccess_key: '{{awsKey}}'\nsecret_key: '{{awsSecret}}'\nregion: eu-west-1\nservice: execute-api\n"
        );
        let presigned: Auth = serde_yaml_ng::from_str(
            "type: aws_sigv4\nservice: s3\nin: query\nunsigned_payload: true\n",
        )
        .unwrap();
        let Auth::AwsSigV4(settings) = presigned else {
            panic!("expected aws_sigv4, got {presigned:?}");
        };
        assert_eq!(settings.location, ApiKeyLocation::Query);
        assert!(settings.unsigned_payload);
    }

    #[test]
    fn jwt_algorithms_are_written_as_named() {
        let auth: Auth = serde_yaml_ng::from_str(
            "type: jwt\nalgorithm: ES384\nprivate_key_file: keys/signing.pem\npayload: '{\"sub\": \"{{user}}\"}'\nexpires_in: 300\n",
        )
        .unwrap();
        let Auth::Jwt(jwt) = &auth else {
            panic!("expected jwt, got {auth:?}");
        };
        assert_eq!(jwt.algorithm, JwtAlgorithm::Es384);
        assert_eq!(jwt.expires_in, Some(300));
        let yaml = serde_yaml_ng::to_string(&auth).unwrap();
        assert!(yaml.contains("algorithm: ES384"), "{yaml}");
        assert!(!yaml.contains("issued_at"), "{yaml}");
        let default = serde_yaml_ng::to_string(&Auth::Jwt(Jwt::default())).unwrap();
        assert_eq!(default, "type: jwt\n");
    }

    #[test]
    fn browser_grants_keep_their_settings_beside_the_type() {
        let yaml = "type: oauth2\ngrant: authorization_code\nauth_url: https://id.example.com/authorize\ntoken_url: https://id.example.com/token\nclient_id: cli\npkce: s256\nauth_params:\n- name: prompt\n  value: consent\ntoken_in: query\n";
        let auth: Auth = serde_yaml_ng::from_str(yaml).unwrap();
        let Auth::Oauth2(oauth) = &auth else {
            panic!("expected oauth2, got {auth:?}");
        };
        assert_eq!(oauth.grant, OAuth2Grant::AuthorizationCode);
        assert!(oauth.grant.uses_browser());
        assert_eq!(oauth.pkce, Pkce::S256);
        assert_eq!(oauth.auth_params[0].name, "prompt");
        assert_eq!(oauth.token_in, ApiKeyLocation::Query);
        let written = serde_yaml_ng::to_string(&auth).unwrap();
        assert!(written.contains("pkce: s256"), "{written}");
        assert!(!written.contains("prefix"), "{written}");
        assert!(!written.contains("callback_url"), "{written}");
        assert_eq!(serde_yaml_ng::from_str::<Auth>(&written).unwrap(), auth);
    }

    #[test]
    fn the_kind_is_written_only_when_it_is_not_http() {
        let request: Request = serde_yaml_ng::from_str("url: https://example.com").unwrap();
        assert_eq!(request.kind, RequestKind::Http);
        let yaml = serde_yaml_ng::to_string(&request).unwrap();
        assert!(!yaml.contains("kind"), "{yaml}");

        let request: Request =
            serde_yaml_ng::from_str("kind: websocket\nurl: wss://example.com/chat").unwrap();
        assert_eq!(request.kind, RequestKind::Websocket);
        assert!(request.kind.is_realtime());
        let yaml = serde_yaml_ng::to_string(&request).unwrap();
        assert!(yaml.contains("kind: websocket"), "{yaml}");

        let request: Request = serde_yaml_ng::from_str("kind:\nurl: https://example.com").unwrap();
        assert_eq!(request.kind, RequestKind::Http);
    }

    #[test]
    fn an_unknown_kind_is_an_error_not_http() {
        let error = serde_yaml_ng::from_str::<Request>("kind: carrier-pigeon\nurl: x")
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("\"carrier-pigeon\" is not a request kind"),
            "{error}"
        );
        assert!(error.contains("http, websocket, sse, socketio"), "{error}");
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

    #[test]
    fn transport_settings_read_and_write_compactly() {
        let settings: Settings = serde_yaml_ng::from_str(
            "http_version: http3\nproxy: off\ncookies: false\nmax_response_mb: 5\nca_certs: [certs/ca.pem]\nclient_cert:\n  pkcs12: certs/me.p12\n  passphrase: '{{p12Pass}}'\n",
        )
        .unwrap();
        assert_eq!(settings.http_version, Some(HttpVersion::Http3));
        assert!(settings.proxy.as_ref().unwrap().is_off());
        assert_eq!(settings.cookies, Some(false));
        assert_eq!(settings.max_response_mb, Some(5));
        assert_eq!(
            settings.ca_certs.as_deref(),
            Some(&["certs/ca.pem".to_owned()][..])
        );
        assert_eq!(
            settings.client_cert.as_ref().unwrap().pkcs12,
            "certs/me.p12"
        );

        let yaml = serde_yaml_ng::to_string(&settings).unwrap();
        assert!(yaml.contains("proxy: off\n"), "{yaml}");
        assert!(
            !yaml.contains(" cert:"),
            "empty fields are left out: {yaml}"
        );
        let back: Settings = serde_yaml_ng::from_str(&yaml).unwrap();
        assert_eq!(back, settings);
    }

    #[test]
    fn a_proxy_is_a_url_or_a_map() {
        let proxy: Proxy = serde_yaml_ng::from_str(
            "url: socks5h://127.0.0.1:1080\nusername: '{{user}}'\npassword: '{{pass}}'\nbypass: localhost\n",
        )
        .unwrap();
        assert_eq!(proxy.url, "socks5h://127.0.0.1:1080");
        assert_eq!(proxy.password, "{{pass}}");
        let yaml = serde_yaml_ng::to_string(&proxy).unwrap();
        assert!(yaml.contains("bypass: localhost"), "{yaml}");
        assert_eq!(serde_yaml_ng::from_str::<Proxy>(&yaml).unwrap(), proxy);

        let short: Proxy = serde_json::from_str(r#""http://proxy:3128""#).unwrap();
        assert_eq!(short.url, "http://proxy:3128");
        assert_eq!(serde_json::to_value(&short).unwrap(), "http://proxy:3128");
        let off: Proxy = serde_yaml_ng::from_str("false").unwrap();
        assert!(off.is_off());
        assert!(Proxy::default().is_system());
    }
}
