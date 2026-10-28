//! HTTP/3 over QUIC, on quinn and h3.
//!
//! reqwest can speak HTTP/3 too, but its `http3` feature turns on its
//! `rustls` feature, which builds a second crypto library (aws-lc-rs) next
//! to the ring Getman uses everywhere else. So requests that ask for HTTP/3
//! come here instead, with the same TLS setup as the rest of Getman (see
//! [`crate::tls::client_config`]), the same cookie jar, and the redirect
//! loop of [`crate::http`] on top.

use std::collections::HashMap;
use std::fmt;
use std::io::Write as _;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bytes::{Buf as _, Bytes};
use futures_util::StreamExt as _;
use futures_util::stream::FuturesUnordered;
use http::header::{ACCEPT_ENCODING, CONTENT_ENCODING, CONTENT_LENGTH, COOKIE, SET_COOKIE};
use reqwest::cookie::CookieStore as _;
use reqwest_cookie_store::CookieStoreMutex;
use url::Position;

use crate::error::{Error, Result};
use crate::timing;
use crate::tls::{CertificateInfo, ClientIdentity, TlsDetails};

/// How long opening a QUIC connection may take before Getman gives up on
/// it, whatever the request's own timeout is.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// How long the first address gets before the next one is tried alongside.
const STAGGER: Duration = Duration::from_millis(250);

type Sender = h3::client::SendRequest<h3_quinn::OpenStreams, Bytes>;
type Stream = h3::client::RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>;

/// An open connection to one server.
#[derive(Clone)]
struct Live {
    sender: Sender,
    /// Set when the connection has ended, and a new one is needed.
    closed: Arc<AtomicBool>,
    remote: SocketAddr,
    tls: TlsDetails,
}

/// Sends requests over HTTP/3, keeping a connection per server.
pub(crate) struct Client {
    endpoint: quinn::Endpoint,
    cookies: Option<Arc<CookieStoreMutex>>,
    open: Mutex<HashMap<String, Live>>,
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("http3::Client { .. }")
    }
}

/// A response whose headers have arrived.
pub(crate) struct Reply {
    pub(crate) status: http::StatusCode,
    /// Without the headers that described a compression Getman undid.
    pub(crate) headers: http::HeaderMap,
    pub(crate) remote_addr: String,
    pub(crate) tls: TlsDetails,
    pub(crate) body: Body,
}

impl Client {
    /// A client for this TLS setup. Needs a Tokio runtime.
    pub(crate) fn new(
        verify: bool,
        ca_certs: &[Vec<u8>],
        identity: Option<&ClientIdentity>,
        cookies: Option<Arc<CookieStoreMutex>>,
    ) -> Result<Self> {
        let tls = crate::tls::client_config(verify, ca_certs, identity, &[b"h3"])?;
        let quic = quinn::crypto::rustls::QuicClientConfig::try_from(tls)
            .map_err(|e| Error::Invalid(format!("Could not set up TLS for HTTP/3: {e}")))?;
        let mut endpoint = ["[::]:0", "0.0.0.0:0"]
            .iter()
            .find_map(|address| quinn::Endpoint::client(address.parse().ok()?).ok())
            .ok_or_else(|| Error::Http("Could not open a UDP socket for HTTP/3".into()))?;
        endpoint.set_default_client_config(quinn::ClientConfig::new(Arc::new(quic)));
        Ok(Self {
            endpoint,
            cookies,
            open: Mutex::new(HashMap::new()),
        })
    }

    /// Sends one request and returns once the response headers arrive.
    pub(crate) async fn send(
        &self,
        method: &http::Method,
        url: &url::Url,
        headers: &http::HeaderMap,
        body: Option<&Bytes>,
    ) -> Result<Reply> {
        if url.scheme() != "https" {
            return Err(Error::Invalid(
                "HTTP/3 needs an https:// URL; it always runs over TLS".into(),
            ));
        }
        let request = self.request(method, url, headers, body)?;
        let key = format!(
            "{}:{}",
            url.host_str().unwrap_or_default(),
            url.port_or_known_default().unwrap_or(443)
        );
        let mut reused = true;
        let (stream, live) = loop {
            let live = self.live(&key);
            let (live, fresh) = match live {
                Some(live) => (live, false),
                None => (self.connect(url).await?, true),
            };
            if fresh {
                self.open
                    .lock()
                    .expect("connection list lock")
                    .insert(key.clone(), live.clone());
            }
            match live.sender.clone().send_request(request.clone()).await {
                Ok(stream) => break (stream, live),
                // A connection the server closed while idle: try a new one.
                Err(_) if reused && !fresh => {
                    reused = false;
                    self.open.lock().expect("connection list lock").remove(&key);
                }
                Err(e) => {
                    self.open.lock().expect("connection list lock").remove(&key);
                    return Err(Error::Http(format!("The request failed: {e}")));
                }
            }
        };
        let mut stream = stream;
        let failed = |e: h3::error::StreamError| Error::Http(format!("The request failed: {e}"));
        if let Some(body) = body.filter(|body| !body.is_empty()) {
            stream.send_data(body.clone()).await.map_err(failed)?;
        }
        stream.finish().await.map_err(failed)?;
        let response = stream.recv_response().await.map_err(failed)?;
        let (parts, ()) = response.into_parts();
        if let Some(jar) = &self.cookies {
            jar.set_cookies(&mut parts.headers.get_all(SET_COOKIE).iter(), url);
        }
        let mut headers = parts.headers;
        let decoder = Decoder::from_headers(&mut headers)?;
        Ok(Reply {
            status: parts.status,
            headers,
            remote_addr: live.remote.to_string(),
            tls: live.tls,
            body: Body { stream, decoder },
        })
    }

    /// The request as HTTP/3 sends it: no connection-specific headers,
    /// and the cookies of the jar, if the request did not set its own.
    fn request(
        &self,
        method: &http::Method,
        url: &url::Url,
        headers: &http::HeaderMap,
        body: Option<&Bytes>,
    ) -> Result<http::Request<()>> {
        let authority = match headers.get(http::header::HOST).map(|h| h.to_str()) {
            Some(Ok(host)) if !host.trim().is_empty() => host.trim(),
            _ => &url[Position::BeforeHost..Position::AfterPort],
        };
        let target = format!(
            "https://{authority}{}",
            &url[Position::BeforePath..Position::AfterQuery]
        );
        let uri: http::Uri = target
            .parse()
            .map_err(|e| Error::Invalid(format!("{target:?} is not a valid URL: {e}")))?;
        let mut request = http::Request::builder()
            .method(method.clone())
            .uri(uri)
            .body(())
            .map_err(|e| Error::Http(format!("The request is invalid: {e}")))?;
        let map = request.headers_mut();
        for (name, value) in headers {
            if !matches!(
                name.as_str(),
                "host"
                    | "connection"
                    | "keep-alive"
                    | "proxy-connection"
                    | "transfer-encoding"
                    | "upgrade"
            ) {
                map.append(name.clone(), value.clone());
            }
        }
        if !map.contains_key(ACCEPT_ENCODING) {
            map.insert(
                ACCEPT_ENCODING,
                http::HeaderValue::from_static("zstd, br, gzip, deflate"),
            );
        }
        if !map.contains_key(COOKIE)
            && let Some(jar) = &self.cookies
            && let Some(cookie) = jar.cookies(url)
        {
            map.insert(COOKIE, cookie);
        }
        let length = body.map_or(0, Bytes::len);
        if length > 0 || matches!(method.as_str(), "POST" | "PUT" | "PATCH") {
            map.entry(CONTENT_LENGTH).or_insert_with(|| length.into());
        }
        Ok(request)
    }

    /// The open connection to this server, if there still is one.
    fn live(&self, key: &str) -> Option<Live> {
        let mut open = self.open.lock().expect("connection list lock");
        match open.get(key) {
            Some(live) if !live.closed.load(Ordering::Relaxed) => Some(live.clone()),
            Some(_) => {
                open.remove(key);
                None
            }
            None => None,
        }
    }

    /// Opens a connection to the server the URL names.
    async fn connect(&self, url: &url::Url) -> Result<Live> {
        let port = url.port_or_known_default().unwrap_or(443);
        let (addresses, name) = resolve(url, port).await?;
        let connection = tokio::time::timeout(CONNECT_TIMEOUT, self.race(addresses, &name))
            .await
            .map_err(|_| Error::Http("Could not connect: the QUIC handshake timed out".into()))??;
        timing::record_connect();

        let tls = TlsDetails {
            // QUIC always runs TLS 1.3.
            version: Some("TLSv1.3".into()),
            certificate: connection
                .peer_identity()
                .and_then(|identity| {
                    identity
                        .downcast::<Vec<rustls::pki_types::CertificateDer<'static>>>()
                        .ok()
                })
                .and_then(|certs| {
                    certs
                        .first()
                        .and_then(|cert| CertificateInfo::from_der(cert))
                }),
        };
        // The socket is dual-stack, which shows IPv4 as an IPv6 address.
        let peer = connection.remote_address();
        let remote = SocketAddr::new(peer.ip().to_canonical(), peer.port());
        let (mut driver, sender) = h3::client::new(h3_quinn::Connection::new(connection))
            .await
            .map_err(|e| Error::Http(format!("Could not connect: {e}")))?;
        let closed = Arc::new(AtomicBool::new(false));
        let ended = Arc::clone(&closed);
        tokio::spawn(async move {
            // Ends when the connection does, whoever closed it.
            let _ = std::future::poll_fn(|cx| driver.poll_close(cx)).await;
            ended.store(true, Ordering::Relaxed);
        });
        Ok(Live {
            sender,
            closed,
            remote,
            tls,
        })
    }

    /// Tries the addresses one after the other, starting the next one
    /// when the last has had a moment, and keeps the first connection that
    /// succeeds.
    async fn race(&self, addresses: Vec<SocketAddr>, name: &str) -> Result<quinn::Connection> {
        let attempt = |address: SocketAddr| {
            let connecting = self.endpoint.connect(address, name);
            async move {
                connecting
                    .map_err(|e| e.to_string())?
                    .await
                    .map_err(|e| e.to_string())
            }
        };
        let mut rest = addresses.into_iter();
        let mut pending = FuturesUnordered::new();
        let mut last = String::from("there is no address to connect to");
        loop {
            if pending.is_empty() {
                match rest.next() {
                    Some(address) => pending.push(attempt(address)),
                    None => break,
                }
            }
            tokio::select! {
                Some(result) = pending.next() => match result {
                    Ok(connection) => return Ok(connection),
                    Err(e) => last = e,
                },
                () = tokio::time::sleep(STAGGER), if rest.len() > 0 => {
                    if let Some(address) = rest.next() {
                        pending.push(attempt(address));
                    }
                }
            }
        }
        Err(Error::Http(format!("Could not connect: {last}")))
    }
}

/// The addresses of the server a URL names, and the name to check its
/// certificate for. A lookup counts toward the request's DNS time.
async fn resolve(url: &url::Url, port: u16) -> Result<(Vec<SocketAddr>, String)> {
    match url.host() {
        Some(url::Host::Ipv4(ip)) => Ok((vec![SocketAddr::new(ip.into(), port)], ip.to_string())),
        Some(url::Host::Ipv6(ip)) => Ok((vec![SocketAddr::new(ip.into(), port)], ip.to_string())),
        Some(url::Host::Domain(name)) => {
            let started = Instant::now();
            let found = tokio::net::lookup_host((name, port)).await;
            timing::record_dns(started.elapsed());
            let addresses: Vec<SocketAddr> = found
                .map_err(|e| {
                    Error::Http(format!("Could not connect: could not look up {name}: {e}"))
                })?
                .collect();
            if addresses.is_empty() {
                return Err(Error::Http(format!(
                    "Could not connect: {name} has no address"
                )));
            }
            Ok((addresses, name.to_owned()))
        }
        None => Err(Error::Invalid("the URL has no host".into())),
    }
}

/// The body of a response, decompressed as it is read.
pub(crate) struct Body {
    stream: Stream,
    decoder: Option<Decoder>,
}

impl Body {
    /// The next piece of the body, `None` at its end.
    pub(crate) async fn chunk(&mut self) -> Result<Option<Bytes>> {
        let unreadable =
            |e: &dyn fmt::Display| Error::Http(format!("The response body could not be read: {e}"));
        loop {
            let received = self
                .stream
                .recv_data()
                .await
                .map_err(|e| unreadable(&e))?
                .map(|mut data| data.copy_to_bytes(data.remaining()));
            let Some(data) = received else {
                // The end: whatever the decoders still hold comes out now.
                return match self.decoder.take() {
                    Some(decoder) => {
                        let rest = decoder.finish().map_err(|e| unreadable(&e))?;
                        Ok((!rest.is_empty()).then(|| Bytes::from(rest)))
                    }
                    None => Ok(None),
                };
            };
            let Some(undoing) = &mut self.decoder else {
                return Ok(Some(data));
            };
            let plain = undoing.feed(&data).map_err(|e| unreadable(&e))?;
            if !plain.is_empty() {
                return Ok(Some(Bytes::from(plain)));
            }
        }
    }
}

/// Undoes the compressions a `Content-Encoding` header lists.
struct Decoder {
    /// In the order they are undone.
    stages: Vec<Stage>,
}

enum Stage {
    Gzip(flate2::write::GzDecoder<Vec<u8>>),
    Deflate(Deflate),
    Brotli(Box<brotli::DecompressorWriter<Vec<u8>>>),
    Zstd(Box<zstd::stream::write::Decoder<'static, Vec<u8>>>),
}

/// HTTP says `deflate` and means zlib, but some servers send the bare
/// deflate stream. The first two bytes tell which.
enum Deflate {
    Undecided(Vec<u8>),
    Zlib(flate2::write::ZlibDecoder<Vec<u8>>),
    Raw(flate2::write::DeflateDecoder<Vec<u8>>),
}

impl Decoder {
    /// A decoder for the response's encodings, with the headers that
    /// described them removed, as reqwest does for the other versions. A
    /// response with an encoding Getman does not know is left as it is.
    fn from_headers(headers: &mut http::HeaderMap) -> Result<Option<Self>> {
        let listed: Vec<String> = headers
            .get_all(CONTENT_ENCODING)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .flat_map(|value| value.split(','))
            .map(|name| name.trim().to_ascii_lowercase())
            .filter(|name| !name.is_empty() && name != "identity")
            .collect();
        if listed.is_empty() {
            return Ok(None);
        }
        let mut stages = Vec::new();
        for name in listed.iter().rev() {
            let io = |e: std::io::Error| Error::Http(format!("Could not decode the response: {e}"));
            stages.push(match name.as_str() {
                "gzip" | "x-gzip" => Stage::Gzip(flate2::write::GzDecoder::new(Vec::new())),
                "deflate" => Stage::Deflate(Deflate::Undecided(Vec::new())),
                "br" => Stage::Brotli(Box::new(brotli::DecompressorWriter::new(Vec::new(), 4096))),
                "zstd" => Stage::Zstd(Box::new(
                    zstd::stream::write::Decoder::new(Vec::new()).map_err(io)?,
                )),
                _ => return Ok(None),
            });
        }
        headers.remove(CONTENT_ENCODING);
        headers.remove(CONTENT_LENGTH);
        Ok(Some(Self { stages }))
    }

    fn feed(&mut self, chunk: &[u8]) -> std::io::Result<Vec<u8>> {
        let mut data = chunk.to_vec();
        for stage in &mut self.stages {
            data = stage.feed(&data)?;
        }
        Ok(data)
    }

    /// Ends the streams, passing what they still hold along.
    fn finish(self) -> std::io::Result<Vec<u8>> {
        let mut data = Vec::new();
        for mut stage in self.stages {
            if !data.is_empty() {
                data = stage.feed(&data)?;
            }
            data.extend(stage.finish()?);
        }
        Ok(data)
    }
}

impl Stage {
    fn feed(&mut self, data: &[u8]) -> std::io::Result<Vec<u8>> {
        match self {
            Self::Gzip(decoder) => {
                decoder.write_all(data)?;
                Ok(std::mem::take(decoder.get_mut()))
            }
            Self::Deflate(deflate) => deflate.feed(data),
            Self::Brotli(decoder) => {
                decoder.write_all(data)?;
                Ok(std::mem::take(decoder.get_mut()))
            }
            Self::Zstd(decoder) => {
                decoder.write_all(data)?;
                decoder.flush()?;
                Ok(std::mem::take(decoder.get_mut()))
            }
        }
    }

    fn finish(self) -> std::io::Result<Vec<u8>> {
        let ended_early = || {
            std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "the compressed data ends early",
            )
        };
        match self {
            Self::Gzip(decoder) => decoder.finish(),
            Self::Deflate(deflate) => deflate.finish(),
            Self::Brotli(decoder) => (*decoder).into_inner().map_err(|_| ended_early()),
            Self::Zstd(mut decoder) => {
                decoder.flush()?;
                Ok(std::mem::take(decoder.get_mut()))
            }
        }
    }
}

impl Deflate {
    fn feed(&mut self, data: &[u8]) -> std::io::Result<Vec<u8>> {
        if let Self::Undecided(held) = self {
            held.extend_from_slice(data);
            if held.len() < 2 {
                return Ok(Vec::new());
            }
            let held = std::mem::take(held);
            let zlib =
                held[0] & 0x0f == 8 && (u16::from(held[0]) << 8 | u16::from(held[1])) % 31 == 0;
            *self = if zlib {
                Self::Zlib(flate2::write::ZlibDecoder::new(Vec::new()))
            } else {
                Self::Raw(flate2::write::DeflateDecoder::new(Vec::new()))
            };
            return self.feed(&held);
        }
        match self {
            Self::Zlib(decoder) => {
                decoder.write_all(data)?;
                Ok(std::mem::take(decoder.get_mut()))
            }
            Self::Raw(decoder) => {
                decoder.write_all(data)?;
                Ok(std::mem::take(decoder.get_mut()))
            }
            Self::Undecided(_) => unreachable!("decided above"),
        }
    }

    fn finish(self) -> std::io::Result<Vec<u8>> {
        match self {
            Self::Undecided(held) if held.is_empty() => Ok(Vec::new()),
            Self::Undecided(_) => Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "the compressed data ends early",
            )),
            Self::Zlib(decoder) => decoder.finish(),
            Self::Raw(decoder) => decoder.finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gzip(data: &[u8]) -> Vec<u8> {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }

    fn decode(encodings: &str, data: &[u8], piece: usize) -> Vec<u8> {
        let mut headers = http::HeaderMap::new();
        headers.insert(CONTENT_ENCODING, encodings.parse().unwrap());
        headers.insert(CONTENT_LENGTH, "1".parse().unwrap());
        let mut decoder = Decoder::from_headers(&mut headers).unwrap().unwrap();
        assert!(headers.is_empty(), "the encoding headers are removed");
        let mut out = Vec::new();
        for chunk in data.chunks(piece) {
            out.extend(decoder.feed(chunk).unwrap());
        }
        out.extend(decoder.finish().unwrap());
        out
    }

    #[test]
    fn every_encoding_is_undone_however_the_bytes_arrive() {
        let text = "hello over http3, ".repeat(200);
        let text = text.as_bytes();
        let zlib = {
            let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
            e.write_all(text).unwrap();
            e.finish().unwrap()
        };
        let raw = {
            let mut e = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::fast());
            e.write_all(text).unwrap();
            e.finish().unwrap()
        };
        let brotli = {
            let mut out = Vec::new();
            {
                let mut e = brotli::CompressorWriter::new(&mut out, 4096, 5, 20);
                e.write_all(text).unwrap();
            }
            out
        };
        let zstd = zstd::encode_all(text, 1).unwrap();
        for piece in [1, 7, 1000] {
            assert_eq!(decode("gzip", &gzip(text), piece), text);
            assert_eq!(decode("deflate", &zlib, piece), text, "zlib");
            assert_eq!(decode("deflate", &raw, piece), text, "raw deflate");
            assert_eq!(decode("br", &brotli, piece), text);
            assert_eq!(decode("zstd", &zstd, piece), text);
        }
        // Listed in the order they were applied: gzip first, then brotli.
        let mut layered = Vec::new();
        {
            let mut e = brotli::CompressorWriter::new(&mut layered, 4096, 5, 20);
            e.write_all(&gzip(text)).unwrap();
        }
        assert_eq!(decode("gzip, br", &layered, 100), text);
    }

    #[test]
    fn unknown_encodings_and_truncated_data_are_not_hidden() {
        let mut headers = http::HeaderMap::new();
        headers.insert(CONTENT_ENCODING, "compress".parse().unwrap());
        assert!(Decoder::from_headers(&mut headers).unwrap().is_none());
        assert!(
            headers.contains_key(CONTENT_ENCODING),
            "left for the reader"
        );

        let mut headers = http::HeaderMap::new();
        headers.insert(CONTENT_ENCODING, "identity".parse().unwrap());
        assert!(Decoder::from_headers(&mut headers).unwrap().is_none());

        let packed = gzip(b"some text that is long enough to matter");
        let mut headers = http::HeaderMap::new();
        headers.insert(CONTENT_ENCODING, "gzip".parse().unwrap());
        let mut decoder = Decoder::from_headers(&mut headers).unwrap().unwrap();
        decoder.feed(&packed[..packed.len() - 6]).unwrap();
        assert!(decoder.finish().is_err());
    }
}
