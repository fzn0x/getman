//! The wire under gRPC: one HTTP/2 connection, plaintext or TLS, and the
//! length-prefixed messages gRPC sends over it.

use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use base64::Engine as _;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use http::header::{CONTENT_TYPE, TE, USER_AGENT};
use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use http_body::{Body, Frame};
use http_body_util::BodyExt as _;
use hyper::body::Incoming;
use hyper::client::conn::http2::{self, SendRequest};
use hyper_util::rt::{TokioExecutor, TokioIo};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use rustls_platform_verifier::BuilderVerifierExt as _;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::mpsc;

use super::status::{Code, Status};
use crate::error::{Error, Result};
use crate::http::{Header, Transport};
use crate::tunnel;

/// Largest message Getman sends or reads.
pub(crate) const MAX_MESSAGE_BYTES: usize = 64 * 1024 * 1024;

/// Value of the `user-agent` metadata unless the request sets one.
const USER_AGENT_VALUE: &str = concat!("getman/", env!("CARGO_PKG_VERSION"), " grpc");

/// Where a call goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Target {
    /// Whether to connect with TLS.
    pub(crate) tls: bool,
    /// Host name or IP address, without brackets.
    pub(crate) host: String,
    /// TCP port.
    pub(crate) port: u16,
}

impl Target {
    /// Reads an address such as `localhost:50051`, `[::1]:443` or
    /// `https://api.example.com`. `tls` applies unless the address says
    /// `https://`.
    pub(crate) fn parse(address: &str, tls: bool) -> Result<Self> {
        let raw = address.trim();
        if raw.is_empty() {
            return Err(Error::Invalid(
                "the request has no address; write it as host:port, such as localhost:50051".into(),
            ));
        }
        let (tls, rest) = match raw.split_once("://") {
            Some((scheme, rest)) => match scheme.to_ascii_lowercase().as_str() {
                "https" | "grpcs" => (true, rest),
                "http" | "grpc" => (tls, rest),
                other => {
                    return Err(Error::Invalid(format!(
                        "{other}:// is not a gRPC address; write host:port, or https://host:port for TLS"
                    )));
                }
            },
            None => (tls, raw),
        };
        let rest = rest.trim_end_matches('/');
        if rest.contains(['/', '?', '#']) {
            return Err(Error::Invalid(format!(
                "{raw:?} has a path; a gRPC address is only a host and port, and the method is chosen apart"
            )));
        }
        let url = url::Url::parse(&format!("http://{rest}"))
            .map_err(|e| Error::Invalid(format!("{raw:?} is not a valid address: {e}")))?;
        let host = match url.host() {
            Some(url::Host::Domain(name)) => name.to_owned(),
            Some(url::Host::Ipv4(ip)) => ip.to_string(),
            Some(url::Host::Ipv6(ip)) => ip.to_string(),
            None => return Err(Error::Invalid(format!("{raw:?} has no host"))),
        };
        let port = url.port().unwrap_or(if tls { 443 } else { 80 });
        Ok(Self { tls, host, port })
    }

    /// `host:port`, with brackets around an IPv6 address.
    pub(crate) fn authority(&self) -> String {
        self.destination().authority()
    }

    /// The server, as the tunnel module takes it.
    fn destination(&self) -> tunnel::Destination<'_> {
        tunnel::Destination {
            tls: self.tls,
            host: &self.host,
            port: self.port,
        }
    }

    /// The server's name as HTTP writes it: the port is left out when it
    /// is the scheme's default.
    pub(crate) fn http_authority(&self) -> String {
        let default = if self.tls { 443 } else { 80 };
        if self.port == default {
            if self.host.contains(':') {
                format!("[{}]", self.host)
            } else {
                self.host.clone()
            }
        } else {
            self.authority()
        }
    }

    /// The URL a call to a path on the server is made to, which is also
    /// what a request signature covers.
    pub(crate) fn http_url(&self, path: &str) -> String {
        format!(
            "{}://{}{path}",
            if self.tls { "https" } else { "http" },
            self.http_authority()
        )
    }

    /// The URL of a path on the server, with its port.
    pub(crate) fn url(&self, path: &str) -> String {
        format!(
            "{}://{}{path}",
            if self.tls { "https" } else { "http" },
            self.authority()
        )
    }
}

/// An open HTTP/2 connection to a gRPC server.
#[derive(Debug)]
pub(crate) struct Connection {
    sender: SendRequest<Outbound>,
    target: Target,
    /// What to add when the server closes the connection on a call: with
    /// TLS 1.3 a refused client certificate shows only then.
    closed_hint: Option<&'static str>,
}

/// Opens a connection the way `transport` says: through its proxy,
/// trusting its certificate authorities, and presenting its client
/// certificate. `timeout` bounds connecting and each handshake.
pub(crate) async fn connect(
    target: &Target,
    transport: &Transport,
    timeout: Duration,
) -> Result<Connection> {
    let destination = target.destination();
    let proxy = tunnel::route(destination, &transport.proxy).map_err(Error::Invalid)?;
    let failed =
        |reason: String| Error::Http(tunnel::failure(destination, proxy.as_ref(), &reason));
    let mut stream = tunnel::open(
        destination,
        proxy.as_ref(),
        // TLS to the proxy itself; the client certificate is for the
        // server behind it.
        || {
            tls_config(transport, false, Vec::new())
                .map(Arc::new)
                .map_err(|e| e.to_string())
        },
        Some(timeout),
    )
    .await
    .map_err(failed)?;

    if target.tls {
        let config =
            tls_config(transport, true, vec![b"h2".to_vec()]).map_err(|e| failed(e.to_string()))?;
        stream = tunnel::secure(stream, &target.host, Arc::new(config), Some(timeout))
            .await
            .map_err(failed)?;
    }
    let closed_hint = match (target.tls, &transport.client_identity) {
        (false, _) => None,
        (true, None) => {
            Some("if the server asks for a client certificate, set one in the request's settings")
        }
        (true, Some(_)) => Some("the server may have refused the client certificate"),
    };
    let sender = handshake(stream).await.map_err(|reason| {
        failed(match closed_hint {
            Some(hint) => format!("{reason}; {hint}"),
            None => reason,
        })
    })?;
    Ok(Connection {
        sender,
        target: target.clone(),
        closed_hint,
    })
}

async fn handshake<T>(io: T) -> Result<SendRequest<Outbound>, String>
where
    T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (sender, connection) = http2::Builder::new(TokioExecutor::new())
        .adaptive_window(true)
        .handshake(TokioIo::new(io))
        .await
        .map_err(|e| format!("the server does not speak HTTP/2 ({})", describe(&e)))?;
    tokio::spawn(async move {
        // Ends when the connection closes; errors reach the calls on it.
        let _ = connection.await;
    });
    Ok(sender)
}

/// The TLS settings of `transport`: the certificates it trusts besides the
/// system's, or none when checking is off, and with `identity` its client
/// certificate.
fn tls_config(
    transport: &Transport,
    identity: bool,
    alpn: Vec<Vec<u8>>,
) -> Result<rustls::ClientConfig, Error> {
    let tls = |e: rustls::Error| Error::Http(format!("Could not set up TLS: {e}"));
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let builder = rustls::ClientConfig::builder_with_provider(Arc::clone(&provider))
        .with_safe_default_protocol_versions()
        .map_err(tls)?;
    let builder = if !transport.verify_tls {
        builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(AnyCertificate(provider)))
    } else if transport.ca_certs.is_empty() {
        builder
            .with_platform_verifier()
            .map_err(|e| Error::Http(format!("Could not load the system's certificates: {e}")))?
    } else {
        let roots = crate::tls::ca_roots(&transport.ca_certs)?;
        let verifier = rustls_platform_verifier::Verifier::new_with_extra_roots(roots, provider)
            .map_err(|e| Error::Http(format!("Could not load the system's certificates: {e}")))?;
        builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(verifier))
    };
    let mut config = match (&transport.client_identity, identity) {
        (Some(client), true) => {
            let (chain, key) = client.rustls_parts()?;
            builder.with_client_auth_cert(chain, key).map_err(tls)?
        }
        _ => builder.with_no_client_auth(),
    };
    config.alpn_protocols = alpn;
    Ok(config)
}

/// Accepts any certificate, for servers with self-signed ones when
/// checking is switched off. Signatures are still verified, so the server
/// must hold the certificate's key.
#[derive(Debug)]
struct AnyCertificate(Arc<CryptoProvider>);

impl ServerCertVerifier for AnyCertificate {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

impl Connection {
    /// Starts a call to `path`, such as `/shop.Orders/GetOrder`, sending
    /// the metadata and, when given, the deadline.
    pub(crate) async fn start(
        &mut self,
        path: &str,
        metadata: &[Header],
        deadline: Option<Duration>,
    ) -> Result<Call> {
        self.sender.ready().await.map_err(|e| {
            Error::Http(format!(
                "The connection to {} closed: {}",
                self.target.authority(),
                describe(&e)
            ))
        })?;
        let (sender, receiver) = mpsc::unbounded_channel();
        let mut request = http::Request::builder()
            .method(Method::POST)
            .uri(self.target.http_url(path))
            .body(Outbound { receiver })
            .map_err(|e| Error::Invalid(format!("{path:?} is not a valid method path: {e}")))?;
        let headers = request.headers_mut();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/grpc"));
        headers.insert(TE, HeaderValue::from_static("trailers"));
        headers.insert(USER_AGENT, HeaderValue::from_static(USER_AGENT_VALUE));
        if let Some(deadline) = deadline {
            headers.insert(
                HeaderName::from_static("grpc-timeout"),
                HeaderValue::from_str(&timeout_value(deadline)).expect("ASCII timeout"),
            );
        }
        for header in metadata {
            let name = HeaderName::from_bytes(header.name.as_bytes()).map_err(|_| {
                Error::Invalid(format!("{:?} is not a valid metadata name", header.name))
            })?;
            let value = HeaderValue::from_str(&header.value).map_err(|_| {
                Error::Invalid(format!(
                    "the value of metadata {} has characters metadata cannot carry",
                    header.name
                ))
            })?;
            if name == USER_AGENT {
                headers.insert(name, value);
            } else {
                headers.append(name, value);
            }
        }
        let response = self.sender.send_request(request);
        Ok(Call {
            closed_hint: self.closed_hint,
            outbound: Some(sender),
            stage: Stage::Waiting(Box::pin(response)),
            head: None,
            buffer: BytesMut::new(),
            trailers: None,
        })
    }
}

/// The `grpc-timeout` header for a deadline: at most eight digits and a
/// unit.
fn timeout_value(deadline: Duration) -> String {
    let millis = deadline.as_millis().max(1);
    if millis < 100_000_000 {
        return format!("{millis}m");
    }
    let seconds = deadline.as_secs();
    if seconds < 100_000_000 {
        format!("{seconds}S")
    } else {
        format!("{}H", (seconds / 3600).min(99_999_999))
    }
}

/// The request body: messages as the call sends them, ending when the
/// call ends its side of the stream.
#[derive(Debug)]
struct Outbound {
    receiver: mpsc::UnboundedReceiver<Bytes>,
}

impl Body for Outbound {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Infallible>>> {
        self.receiver
            .poll_recv(cx)
            .map(|chunk| chunk.map(|data| Ok(Frame::data(data))))
    }
}

type ResponseFuture = Pin<Box<dyn Future<Output = hyper::Result<http::Response<Incoming>>> + Send>>;

enum Stage {
    Waiting(ResponseFuture),
    Reading(Incoming),
    Done,
}

/// What came back before the messages.
#[derive(Debug, Clone)]
pub(crate) struct Head {
    /// HTTP status of the response.
    pub(crate) status: StatusCode,
    /// Response headers.
    pub(crate) headers: HeaderMap,
}

/// A call in progress.
pub(crate) struct Call {
    closed_hint: Option<&'static str>,
    outbound: Option<mpsc::UnboundedSender<Bytes>>,
    stage: Stage,
    head: Option<Head>,
    buffer: BytesMut,
    trailers: Option<HeaderMap>,
}

impl std::fmt::Debug for Call {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Call")
            .field("head", &self.head)
            .field("trailers", &self.trailers)
            .finish_non_exhaustive()
    }
}

/// A message as it goes on the wire: not compressed, and led by its
/// length.
pub(crate) fn frame(message: &[u8]) -> Bytes {
    let mut frame = BytesMut::with_capacity(message.len() + 5);
    frame.put_u8(0);
    frame.put_u32(u32::try_from(message.len()).expect("checked size"));
    frame.extend_from_slice(message);
    frame.freeze()
}

impl Call {
    /// Sends one encoded message.
    pub(crate) fn send(&self, message: &[u8]) -> Result<()> {
        let Some(outbound) = &self.outbound else {
            return Err(Error::Invalid(
                "the stream was ended, so no more messages can be sent".into(),
            ));
        };
        if message.len() > MAX_MESSAGE_BYTES {
            return Err(Error::Invalid(format!(
                "the message is {} bytes; Getman sends at most {MAX_MESSAGE_BYTES}",
                message.len()
            )));
        }
        outbound
            .send(frame(message))
            .map_err(|_| Error::Http("the server closed the stream".into()))
    }

    /// Ends the client's side of the stream: no more messages follow.
    pub(crate) fn end(&mut self) {
        self.outbound = None;
    }

    /// Whether the client's side of the stream is still open.
    pub(crate) fn sending(&self) -> bool {
        self.outbound.is_some()
    }

    /// The response's status and headers, once they arrived.
    pub(crate) fn head(&self) -> Option<&Head> {
        self.head.as_ref()
    }

    /// Waits for the response headers. Safe to cancel and call again.
    pub(crate) async fn wait_head(&mut self) -> Result<&Head> {
        if let Stage::Waiting(response) = &mut self.stage {
            let response = response.await.map_err(|e| {
                let reason = describe(&e);
                let hint = self
                    .closed_hint
                    .filter(|_| broke_off(e.is_canceled() || e.is_closed(), &reason));
                Error::Http(match hint {
                    Some(hint) => format!("{reason}; {hint}"),
                    None => reason,
                })
            })?;
            let (parts, body) = response.into_parts();
            self.head = Some(Head {
                status: parts.status,
                headers: parts.headers,
            });
            self.stage = if is_grpc(self.head.as_ref().expect("just set")) {
                Stage::Reading(body)
            } else {
                Stage::Done
            };
        }
        self.head
            .as_ref()
            .ok_or_else(|| Error::Http("the call ended before the server answered".into()))
    }

    /// The next message from the server, or `None` once the server is
    /// done. Safe to cancel and call again.
    pub(crate) async fn next(&mut self) -> Result<Option<Bytes>> {
        self.wait_head().await?;
        loop {
            if let Some(message) = self.take_message()? {
                return Ok(Some(message));
            }
            let Stage::Reading(body) = &mut self.stage else {
                return Ok(None);
            };
            match body.frame().await {
                None => {
                    self.stage = Stage::Done;
                    if !self.buffer.is_empty() {
                        return Err(Error::Http(
                            "the server ended the stream in the middle of a message".into(),
                        ));
                    }
                    return Ok(None);
                }
                Some(Err(e)) => {
                    self.stage = Stage::Done;
                    return Err(Error::Http(describe(&e)));
                }
                Some(Ok(frame)) => match frame.into_data() {
                    Ok(data) => self.buffer.extend_from_slice(&data),
                    Err(frame) => {
                        if let Ok(trailers) = frame.into_trailers() {
                            self.trailers = Some(trailers);
                        }
                    }
                },
            }
        }
    }

    fn take_message(&mut self) -> Result<Option<Bytes>> {
        if self.buffer.len() < 5 {
            return Ok(None);
        }
        let compressed = self.buffer[0];
        let length = u32::from_be_bytes([
            self.buffer[1],
            self.buffer[2],
            self.buffer[3],
            self.buffer[4],
        ]) as usize;
        if compressed != 0 {
            let encoding = self
                .head
                .as_ref()
                .and_then(|h| h.headers.get("grpc-encoding"))
                .and_then(|v| v.to_str().ok())
                .unwrap_or("an unknown encoding");
            return Err(Error::Http(format!(
                "the server compressed a message with {encoding}, which Getman did not ask for"
            )));
        }
        if length > MAX_MESSAGE_BYTES {
            return Err(Error::Http(format!(
                "the server sent a message of {length} bytes; Getman reads at most {MAX_MESSAGE_BYTES}"
            )));
        }
        if self.buffer.len() < length + 5 {
            return Ok(None);
        }
        self.buffer.advance(5);
        Ok(Some(self.buffer.split_to(length).freeze()))
    }

    /// The trailers, once the server sent them.
    pub(crate) fn trailers(&self) -> Option<&HeaderMap> {
        self.trailers.as_ref()
    }

    /// The call's status, from the trailers, or from the headers of a
    /// response without a body, or from the HTTP status when the server
    /// did not answer as gRPC.
    pub(crate) fn status(&self) -> Status {
        let Some(head) = &self.head else {
            return Status::new(Code::Unknown, "the server did not answer");
        };
        let source = self
            .trailers
            .as_ref()
            .filter(|t| t.contains_key("grpc-status"))
            .or_else(|| {
                head.headers
                    .contains_key("grpc-status")
                    .then_some(&head.headers)
            });
        if let Some(source) = source {
            return status_from(source);
        }
        if !is_grpc(head) {
            let content_type = head
                .headers
                .get(CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("none");
            return Status::new(
                http_code(head.status),
                format!(
                    "the server answered HTTP {} with content type {content_type}, not gRPC",
                    head.status.as_u16()
                ),
            );
        }
        Status::new(
            Code::Unknown,
            "the server ended the call without a grpc-status",
        )
    }
}

fn is_grpc(head: &Head) -> bool {
    let content_type = head
        .headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    head.status == StatusCode::OK
        && (content_type == "application/grpc" || content_type.starts_with("application/grpc+"))
}

/// Reads `grpc-status`, `grpc-message` and `grpc-status-details-bin`.
fn status_from(headers: &HeaderMap) -> Status {
    let text = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    let code = text("grpc-status")
        .and_then(|v| v.trim().parse::<u32>().ok())
        .unwrap_or(Code::Unknown as u32);
    let message = text("grpc-message")
        .map(|m| {
            percent_encoding::percent_decode_str(m)
                .decode_utf8_lossy()
                .into_owned()
        })
        .unwrap_or_default();
    let details = text("grpc-status-details-bin").and_then(|v| decode_binary(v).ok());
    Status {
        details_bytes: details,
        ..Status::from_code(code, message)
    }
}

/// Decodes a `-bin` metadata value, which may be sent with or without
/// padding.
pub(crate) fn decode_binary(value: &str) -> Result<Vec<u8>, base64::DecodeError> {
    let trimmed = value.trim().trim_end_matches('=');
    base64::engine::general_purpose::STANDARD_NO_PAD
        .decode(trimmed)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(trimmed))
}

/// The gRPC code for an HTTP status, from gRPC's own mapping.
fn http_code(status: StatusCode) -> Code {
    match status.as_u16() {
        400 => Code::Internal,
        401 => Code::Unauthenticated,
        403 => Code::PermissionDenied,
        404 => Code::Unimplemented,
        429 | 502..=504 => Code::Unavailable,
        _ => Code::Unknown,
    }
}

/// Headers as name and value pairs; binary values stay Base64.
pub(crate) fn header_list(headers: &HeaderMap) -> Vec<Header> {
    headers
        .iter()
        .map(|(name, value)| {
            Header::new(
                name.as_str(),
                String::from_utf8_lossy(value.as_bytes()).into_owned(),
            )
        })
        .collect()
}

/// Explains a connection error in one line.
/// Whether the connection broke off under a call, as it does when a TLS
/// 1.3 server refuses the client's certificate after the handshake.
/// Depending on the system and the timing, the client sees the connection
/// close, the alert the server sent, or a broken pipe or a reset while it
/// is still writing.
fn broke_off(closed: bool, reason: &str) -> bool {
    let reason = reason.to_ascii_lowercase();
    closed
        || ["received fatal alert", "broken pipe", "connection reset"]
            .iter()
            .any(|sign| reason.contains(sign))
}

fn describe(error: &hyper::Error) -> String {
    let mut message = error.to_string();
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        let text = cause.to_string();
        if !text.is_empty() && !message.contains(&text) {
            message.push_str(": ");
            message.push_str(&text);
        }
        source = cause.source();
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refused_client_certificate_is_recognised_however_it_shows() {
        assert!(broke_off(
            true,
            "connection closed before message completed"
        ));
        for reason in [
            "connection error: received fatal alert: CertificateRequired",
            "connection error: stream closed because of a broken pipe",
            "connection error: Connection reset by peer (os error 54)",
        ] {
            assert!(broke_off(false, reason), "{reason}");
        }
        assert!(!broke_off(false, "invalid HTTP status-code parsed"));
    }

    #[test]
    fn addresses_take_a_scheme_a_host_and_a_port() {
        let plain = Target::parse("localhost:50051", false).unwrap();
        assert_eq!(
            (plain.tls, plain.host.as_str(), plain.port),
            (false, "localhost", 50051)
        );
        let tls = Target::parse("https://api.example.com/", false).unwrap();
        assert_eq!((tls.tls, tls.port), (true, 443));
        assert_eq!(tls.url("/a.B/C"), "https://api.example.com:443/a.B/C");
        let v6 = Target::parse("[::1]:9000", true).unwrap();
        assert_eq!(v6.authority(), "[::1]:9000");
        assert!(v6.tls);
        assert!(Target::parse("", false).is_err());
        assert!(Target::parse("localhost:1/shop.Orders/Get", false).is_err());
        assert!(Target::parse("ftp://host:1", false).is_err());
    }

    #[test]
    fn deadlines_fit_in_eight_digits() {
        assert_eq!(timeout_value(Duration::from_millis(1500)), "1500m");
        assert_eq!(timeout_value(Duration::from_secs(200_000)), "200000S");
        assert_eq!(timeout_value(Duration::ZERO), "1m");
    }

    #[test]
    fn binary_values_decode_with_or_without_padding() {
        assert_eq!(decode_binary("AQI=").unwrap(), [1, 2]);
        assert_eq!(decode_binary("AQI").unwrap(), [1, 2]);
        assert_eq!(decode_binary("_-8").unwrap(), [255, 239]);
        assert!(decode_binary("not base64!").is_err());
    }
}
