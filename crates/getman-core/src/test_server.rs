//! A local HTTP server the tests send requests to, over plain HTTP, TLS
//! (optionally asking for a client certificate) or HTTP/3.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use axum::Router;
use axum::extract::{Path, Query, Request};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get, post};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

/// A running test server.
pub(crate) struct TestServer {
    addr: SocketAddr,
    scheme: &'static str,
}

impl TestServer {
    /// The URL of a path on the server.
    pub(crate) fn url(&self, path: &str) -> String {
        format!("{}://{}{path}", self.scheme, self.addr)
    }

    /// The server's port.
    pub(crate) fn port(&self) -> u16 {
        self.addr.port()
    }
}

fn app() -> Router {
    Router::new()
        .route("/echo", any(echo))
        .route("/echo/{*rest}", any(echo))
        .route("/redirect", get(redirect))
        .route("/cookies/set", get(set_cookies))
        .route("/bytes", get(bytes))
        .route("/bytes/{count}", get(many_bytes))
        .route("/zstd", get(zstd_body))
        .route("/alt-svc", get(alt_svc))
        .route("/slow", get(slow))
        .route("/status/{code}", any(status))
        .route("/digest", get(digest))
        .route("/token", post(token))
        .route("/sse", get(sse))
        .route("/sse/forever", get(sse_forever))
        .route("/sse/count", get(sse_count))
}

/// Starts a server on a free port of the loopback interface.
pub(crate) async fn spawn() -> TestServer {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app()).await.unwrap() });
    TestServer {
        addr,
        scheme: "http",
    }
}

/// Certificates for TLS tests, made once per test binary: a CA, a server
/// certificate for 127.0.0.1 and localhost, and a client certificate,
/// both signed by the CA.
pub(crate) struct Pki {
    /// The CA certificate, PEM.
    pub(crate) ca_pem: String,
    /// The client certificate, PEM.
    pub(crate) client_cert_pem: String,
    /// The client's private key, PEM.
    pub(crate) client_key_pem: String,
    /// The client certificate and key as PKCS#12, with passphrase
    /// [`P12_PASSPHRASE`].
    pub(crate) client_p12: Vec<u8>,
    ca_der: CertificateDer<'static>,
    server_cert: CertificateDer<'static>,
    server_key: Vec<u8>,
}

/// Passphrase of [`Pki::client_p12`].
pub(crate) const P12_PASSPHRASE: &str = "s3cret";

pub(crate) fn pki() -> &'static Pki {
    static PKI: OnceLock<Pki> = OnceLock::new();
    PKI.get_or_init(|| {
        use rcgen::{
            BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer,
            KeyPair, KeyUsagePurpose,
        };
        let ca_key = KeyPair::generate().unwrap();
        let mut ca_params = CertificateParams::new(Vec::<String>::new()).unwrap();
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        ca_params
            .distinguished_name
            .push(DnType::CommonName, "Getman Test CA");
        ca_params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
            KeyUsagePurpose::DigitalSignature,
        ];
        let ca = ca_params.self_signed(&ca_key).unwrap();
        let issuer = Issuer::new(ca_params, ca_key);

        let server_key = KeyPair::generate().unwrap();
        let mut server_params =
            CertificateParams::new(vec!["localhost".to_owned(), "127.0.0.1".to_owned()]).unwrap();
        server_params
            .distinguished_name
            .push(DnType::CommonName, "getman test server");
        server_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        let server = server_params.signed_by(&server_key, &issuer).unwrap();

        let client_key = KeyPair::generate().unwrap();
        let mut client_params = CertificateParams::new(Vec::<String>::new()).unwrap();
        client_params
            .distinguished_name
            .push(DnType::CommonName, "getman test client");
        client_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
        let client = client_params.signed_by(&client_key, &issuer).unwrap();

        let chain = p12_keystore::PrivateKeyChain::new(
            "client",
            p12_keystore::PrivateKey::from_der(&client_key.serialize_der()).unwrap(),
            [p12_keystore::Certificate::from_der(client.der()).unwrap()],
        );
        let mut store = p12_keystore::KeyStore::new();
        store.add_entry(
            "client",
            p12_keystore::KeyStoreEntry::PrivateKeyChain(chain),
        );
        let client_p12 = store.writer(P12_PASSPHRASE).write().unwrap();

        Pki {
            ca_pem: ca.pem(),
            client_cert_pem: client.pem(),
            client_key_pem: client_key.serialize_pem(),
            client_p12,
            ca_der: ca.der().clone(),
            server_cert: server.der().clone(),
            server_key: server_key.serialize_der(),
        }
    })
}

fn server_tls(pki: &Pki, require_client_cert: bool) -> rustls::ServerConfig {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let builder = rustls::ServerConfig::builder_with_provider(Arc::clone(&provider))
        .with_safe_default_protocol_versions()
        .unwrap();
    let builder = if require_client_cert {
        let mut roots = rustls::RootCertStore::empty();
        roots.add(pki.ca_der.clone()).unwrap();
        let verifier =
            rustls::server::WebPkiClientVerifier::builder_with_provider(Arc::new(roots), provider)
                .build()
                .unwrap();
        builder.with_client_cert_verifier(verifier)
    } else {
        builder.with_no_client_auth()
    };
    builder
        .with_single_cert(
            vec![pki.server_cert.clone()],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(pki.server_key.clone())),
        )
        .unwrap()
}

/// Accepts TLS connections for axum; failed handshakes are skipped.
struct TlsListener {
    tcp: tokio::net::TcpListener,
    acceptor: tokio_rustls::TlsAcceptor,
}

impl axum::serve::Listener for TlsListener {
    type Io = tokio_rustls::server::TlsStream<tokio::net::TcpStream>;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let Ok((stream, addr)) = self.tcp.accept().await else {
                continue;
            };
            if let Ok(tls) = self.acceptor.accept(stream).await {
                return (tls, addr);
            }
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        self.tcp.local_addr()
    }
}

/// Starts the server over TLS with the [`pki`] server certificate,
/// optionally refusing clients without a certificate from the CA.
pub(crate) async fn spawn_tls(require_client_cert: bool) -> TestServer {
    let mut config = server_tls(pki(), require_client_cert);
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    let tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = tcp.local_addr().unwrap();
    let listener = TlsListener {
        tcp,
        acceptor: tokio_rustls::TlsAcceptor::from(Arc::new(config)),
    };
    tokio::spawn(async move { axum::serve(listener, app()).await.unwrap() });
    TestServer {
        addr,
        scheme: "https",
    }
}

/// Starts an HTTP/3 server, over QUIC only, with the [`pki`] server
/// certificate. Every request gets a JSON echo of its method, path and
/// protocol version.
#[cfg(reqwest_unstable)]
pub(crate) fn spawn_h3() -> TestServer {
    let mut config = server_tls(pki(), false);
    config.alpn_protocols = vec![b"h3".to_vec()];
    let quic = quinn::crypto::rustls::QuicServerConfig::try_from(config).unwrap();
    let server = quinn::ServerConfig::with_crypto(Arc::new(quic));
    let endpoint = quinn::Endpoint::server(server, "127.0.0.1:0".parse().unwrap()).unwrap();
    let addr = endpoint.local_addr().unwrap();
    tokio::spawn(async move {
        while let Some(incoming) = endpoint.accept().await {
            tokio::spawn(async move {
                let Ok(connection) = incoming.await else {
                    return;
                };
                let Ok(mut h3) = h3::server::Connection::<_, bytes::Bytes>::new(
                    h3_quinn::Connection::new(connection),
                )
                .await
                else {
                    return;
                };
                while let Ok(Some(resolver)) = h3.accept().await {
                    tokio::spawn(async move {
                        let Ok((request, mut stream)) = resolver.resolve_request().await else {
                            return;
                        };
                        let body = json!({
                            "method": request.method().as_str(),
                            "path": request.uri().path(),
                            "version": format!("{:?}", request.version()),
                        })
                        .to_string();
                        let response = http::Response::builder()
                            .status(200)
                            .header("content-type", "application/json")
                            .body(())
                            .unwrap();
                        if stream.send_response(response).await.is_ok()
                            && stream.send_data(bytes::Bytes::from(body)).await.is_ok()
                        {
                            let _ = stream.finish().await;
                        }
                    });
                }
            });
        }
    });
    TestServer {
        addr,
        scheme: "https",
    }
}

/// Starts a bare TCP server that answers every connection with `reply`
/// and hands over the bytes of each request's head, exactly as they
/// arrived.
pub(crate) async fn spawn_raw(
    reply: &'static str,
) -> (SocketAddr, tokio::sync::mpsc::UnboundedReceiver<String>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let sender = sender.clone();
            tokio::spawn(async move {
                let mut head = Vec::new();
                let mut buffer = [0_u8; 4096];
                while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                    match stream.read(&mut buffer).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => head.extend_from_slice(&buffer[..n]),
                    }
                }
                let _ = sender.send(String::from_utf8_lossy(&head).into_owned());
                let _ = stream.write_all(reply.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
    });
    (addr, receiver)
}

/// Replies with what it received, as JSON.
async fn echo(request: Request) -> axum::Json<Value> {
    let (parts, body) = request.into_parts();
    let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
    let mut headers = BTreeMap::new();
    for (name, value) in &parts.headers {
        headers
            .entry(name.to_string())
            .and_modify(|v: &mut String| {
                v.push_str(", ");
                v.push_str(value.to_str().unwrap_or_default());
            })
            .or_insert_with(|| value.to_str().unwrap_or_default().to_owned());
    }
    axum::Json(json!({
        "method": parts.method.as_str(),
        "path": parts.uri.path(),
        "query": parts.uri.query().unwrap_or_default(),
        "headers": headers,
        "body": String::from_utf8_lossy(&bytes),
        "version": format!("{:?}", parts.version),
    }))
}

async fn many_bytes(Path(count): Path<usize>) -> Vec<u8> {
    vec![b'x'; count]
}

/// `zstd works` compressed with zstd.
async fn zstd_body() -> Response {
    Response::builder()
        .header(header::CONTENT_ENCODING, "zstd")
        .header(header::CONTENT_TYPE, "text/plain")
        .body(axum::body::Body::from(
            zstd::encode_all(&b"zstd works"[..], 3).unwrap(),
        ))
        .unwrap()
}

async fn alt_svc() -> impl IntoResponse {
    ([("alt-svc", r#"h3=":443"; ma=86400, h2=":443""#)], "hello")
}

async fn redirect() -> impl IntoResponse {
    (StatusCode::FOUND, [(header::LOCATION, "/echo")])
}

async fn set_cookies(Query(cookies): Query<BTreeMap<String, String>>) -> Response {
    let mut response = Response::builder().status(StatusCode::OK);
    for (name, value) in cookies {
        response = response.header(header::SET_COOKIE, format!("{name}={value}; Path=/"));
    }
    response.body(axum::body::Body::from("set")).unwrap()
}

async fn bytes() -> Vec<u8> {
    vec![0, 159, 146, 150]
}

async fn slow() -> &'static str {
    tokio::time::sleep(Duration::from_secs(5)).await;
    "late"
}

async fn status(Path(code): Path<u16>) -> StatusCode {
    StatusCode::from_u16(code).unwrap_or(StatusCode::BAD_REQUEST)
}

/// Requires Digest authentication as `ada` with password `lovelace`.
async fn digest(headers: HeaderMap) -> Response {
    const REALM: &str = "getman-test";
    const NONCE: &str = "dcd98b7102dd2f0e8b11d0f600bfb0c093";
    let challenge = || {
        (
            StatusCode::UNAUTHORIZED,
            [(
                header::WWW_AUTHENTICATE,
                format!(r#"Digest realm="{REALM}", qop="auth", nonce="{NONCE}", opaque="5ccc069c403ebaf9f0171e9517f40e41""#),
            )],
        )
            .into_response()
    };
    let Some(authorization) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Digest "))
    else {
        return challenge();
    };
    let field = |name: &str| -> String {
        authorization
            .split(", ")
            .find_map(|pair| pair.strip_prefix(&format!("{name}=")))
            .unwrap_or_default()
            .trim_matches('"')
            .to_owned()
    };
    let md5 = |text: String| {
        use md5::Digest as _;
        hex::encode(md5::Md5::digest(text))
    };
    let ha1 = md5(format!("ada:{REALM}:lovelace"));
    let ha2 = md5(format!("GET:{}", field("uri")));
    let expected = md5(format!(
        "{ha1}:{NONCE}:{}:{}:auth:{ha2}",
        field("nc"),
        field("cnonce")
    ));
    if field("response") == expected && field("username") == "ada" {
        (StatusCode::OK, "welcome").into_response()
    } else {
        challenge()
    }
}

/// An OAuth 2.0 token endpoint accepting client `app` with secret
/// `s3cret`, and the public client `public` without one. The authorization
/// code `good-code` is valid; its token names the PKCE challenge of the
/// verifier sent. A `lifetime` parameter sets `expires_in`.
async fn token(headers: HeaderMap, body: String) -> Response {
    use base64::Engine as _;
    use sha2::Digest as _;

    let form: BTreeMap<String, String> = url::form_urlencoded::parse(body.as_bytes())
        .into_owned()
        .collect();
    let field = |name: &str| form.get(name).map(String::as_str);
    let basic = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == "Basic YXBwOnMzY3JldA==");
    let in_body = field("client_id") == Some("app") && field("client_secret") == Some("s3cret");
    let public = field("client_id") == Some("public") && field("client_secret").is_none();
    if !(basic || in_body || public) {
        return (
            StatusCode::UNAUTHORIZED,
            axum::Json(json!({"error": "invalid_client"})),
        )
            .into_response();
    }
    let grant = field("grant_type").unwrap_or_default();
    let scope = field("scope").unwrap_or_default();
    let lifetime: u64 = field("lifetime")
        .and_then(|l| l.parse().ok())
        .unwrap_or(3600);
    match grant {
        "authorization_code" => {
            if field("code") != Some("good-code") || field("redirect_uri").is_none() {
                return (
                    StatusCode::BAD_REQUEST,
                    axum::Json(json!({"error": "invalid_grant"})),
                )
                    .into_response();
            }
            let challenge = field("code_verifier").map_or_else(
                || "none".to_owned(),
                |verifier| {
                    base64::engine::general_purpose::URL_SAFE_NO_PAD
                        .encode(sha2::Sha256::digest(verifier.as_bytes()))
                },
            );
            axum::Json(json!({
                "access_token": format!("code-token-{challenge}"),
                "token_type": "Bearer",
                "expires_in": lifetime,
                "refresh_token": "refresh-1",
            }))
            .into_response()
        }
        "refresh_token" => {
            let Some(refresh) = field("refresh_token").filter(|r| r.starts_with("refresh-")) else {
                return (
                    StatusCode::BAD_REQUEST,
                    axum::Json(json!({"error": "invalid_grant"})),
                )
                    .into_response();
            };
            axum::Json(json!({
                "access_token": format!("refreshed-with-{refresh}"),
                "token_type": "Bearer",
                "expires_in": 3600,
            }))
            .into_response()
        }
        _ => axum::Json(json!({
            "access_token": format!("token-for-{grant}-{scope}"),
            "token_type": "Bearer",
            "expires_in": lifetime,
            "refresh_token": if grant == "password" { Some("refresh-pw") } else { None },
        }))
        .into_response(),
    }
}

fn event_stream(body: axum::body::Body) -> Response {
    Response::builder()
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .body(body)
        .unwrap()
}

/// A few events in the `text/event-stream` format, then the end. Echoes
/// the `Authorization` header in the first event.
async fn sse(headers: HeaderMap) -> Response {
    let authorization = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("none");
    event_stream(axum::body::Body::from(format!(
        ": warming up\n\nevent: hello\ndata: {authorization}\n\nid: 1\nretry: 2500\ndata: {{\"price\": 42}}\n\ndata: two\ndata: lines\n\n"
    )))
}

/// An event every 20 ms, forever.
async fn sse_forever() -> Response {
    let ticks = futures_util::stream::unfold(0_u64, |n| async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let chunk = format!("id: {n}\ndata: tick {n}\n\n");
        Some((Ok::<_, std::convert::Infallible>(chunk), n + 1))
    });
    event_stream(axum::body::Body::from_stream(ticks))
}

/// One event numbered one more than `Last-Event-ID`, then the end, asking
/// to reconnect after 50 ms.
async fn sse_count(headers: HeaderMap) -> Response {
    let last: u64 = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let next = last + 1;
    event_stream(axum::body::Body::from(format!(
        "retry: 50\nid: {next}\ndata: {next}\n\n"
    )))
}
