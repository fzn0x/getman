//! A local gRPC server the tests call, built on tonic: an echo service
//! with every kind of call, errors with details, and optional reflection.

use std::collections::HashMap;
use std::convert::Infallible;
use std::future::Future;
use std::task::{Context, Poll};
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use prost::Message as _;
use protox::file::{ChainFileResolver, File, FileResolver, GoogleFileResolver};
use tokio::sync::mpsc;
use tokio_stream::wrappers::{ReceiverStream, TcpListenerStream};
use tonic::codegen::{BoxFuture, Service, http};
use tonic::metadata::{KeyAndValueRef, MetadataMap};
use tonic::server::{Grpc, NamedService};
use tonic::{Code, Request, Response, Status, Streaming};
use tonic_prost::ProstCodec;

/// The service's schema, as a workspace would hold it.
pub(crate) const PROTO: &str = r#"syntax = "proto3";

package getman.test;

import "google/protobuf/timestamp.proto";

service Echo {
  rpc Unary(EchoRequest) returns (EchoReply);
  rpc ServerStream(EchoRequest) returns (stream EchoReply);
  rpc ClientStream(stream EchoRequest) returns (EchoReply);
  rpc Bidi(stream EchoRequest) returns (stream EchoReply);
  rpc Fail(FailRequest) returns (EchoReply);
  rpc Slow(EchoRequest) returns (EchoReply);
}

message EchoRequest {
  string text = 1;
  int32 times = 2;
  google.protobuf.Timestamp at = 3;
}

message EchoReply {
  string text = 1;
  int32 index = 2;
  map<string, string> metadata = 3;
}

message FailRequest {
  int32 code = 1;
  string message = 2;
}
"#;

/// Where the schema sits in the workspace the tests make.
pub(crate) const PROTO_PATH: &str = "protos/getman/test/echo.proto";

#[derive(Clone, PartialEq, prost::Message)]
pub(crate) struct EchoRequest {
    #[prost(string, tag = "1")]
    pub(crate) text: String,
    #[prost(int32, tag = "2")]
    pub(crate) times: i32,
    #[prost(message, optional, tag = "3")]
    pub(crate) at: Option<prost_types::Timestamp>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub(crate) struct EchoReply {
    #[prost(string, tag = "1")]
    pub(crate) text: String,
    #[prost(int32, tag = "2")]
    pub(crate) index: i32,
    #[prost(map = "string, string", tag = "3")]
    pub(crate) metadata: HashMap<String, String>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub(crate) struct FailRequest {
    #[prost(int32, tag = "1")]
    pub(crate) code: i32,
    #[prost(string, tag = "2")]
    pub(crate) message: String,
}

#[derive(Clone, PartialEq, prost::Message)]
struct RpcStatus {
    #[prost(int32, tag = "1")]
    code: i32,
    #[prost(string, tag = "2")]
    message: String,
    #[prost(message, repeated, tag = "3")]
    details: Vec<prost_types::Any>,
}

#[derive(Clone, PartialEq, prost::Message)]
struct ErrorInfo {
    #[prost(string, tag = "1")]
    reason: String,
    #[prost(string, tag = "2")]
    domain: String,
    #[prost(map = "string, string", tag = "3")]
    metadata: HashMap<String, String>,
}

/// Which reflection service the server offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reflection {
    None,
    V1,
    V1Alpha,
}

/// The schema compiled, with its imports, as reflection serves it.
pub(crate) fn descriptor_set() -> Vec<u8> {
    struct Echo;
    impl FileResolver for Echo {
        fn open_file(&self, name: &str) -> Result<File, protox::Error> {
            if name == "getman/test/echo.proto" {
                File::from_source(name, PROTO)
            } else {
                Err(protox::Error::file_not_found(name))
            }
        }
    }
    let mut resolver = ChainFileResolver::new();
    resolver.add(Echo);
    resolver.add(GoogleFileResolver::new());
    let mut compiler = protox::Compiler::with_file_resolver(resolver);
    compiler.include_imports(true);
    compiler.open_file("getman/test/echo.proto").unwrap();
    compiler.encode_file_descriptor_set()
}

/// A running server.
pub(crate) struct TestServer {
    /// `127.0.0.1:port`.
    pub(crate) address: String,
}

/// Starts a plaintext server on a free port of the loopback interface.
pub(crate) async fn spawn(reflection: Reflection) -> TestServer {
    serve(reflection, None).await
}

/// Starts a TLS server with a self-signed certificate for `localhost`.
pub(crate) async fn spawn_tls() -> TestServer {
    let certified = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).unwrap();
    let identity = tonic::transport::Identity::from_pem(
        certified.cert.pem(),
        certified.signing_key.serialize_pem(),
    );
    serve(
        Reflection::None,
        Some(tonic::transport::ServerTlsConfig::new().identity(identity)),
    )
    .await
}

/// Starts a TLS server whose certificate the test CA signed for
/// `localhost` and `127.0.0.1`, which refuses clients without a
/// certificate from the CA when `require_client_cert` is set.
pub(crate) async fn spawn_secure(require_client_cert: bool) -> TestServer {
    let pki = crate::test_server::pki();
    let mut config = tonic::transport::ServerTlsConfig::new().identity(
        tonic::transport::Identity::from_pem(&pki.server_cert_pem, &pki.server_key_pem),
    );
    if require_client_cert {
        config = config.client_ca_root(tonic::transport::Certificate::from_pem(&pki.ca_pem));
    }
    serve(Reflection::None, Some(config)).await
}

async fn serve(
    reflection: Reflection,
    tls: Option<tonic::transport::ServerTlsConfig>,
) -> TestServer {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let mut builder = tonic::transport::Server::builder();
    if let Some(config) = tls {
        // tonic builds the server's TLS config with the process default,
        // which rustls cannot pick while two providers are compiled in.
        // Fails only if a provider is installed already, which is fine.
        let _ = rustls::crypto::ring::default_provider().install_default();
        builder = builder.tls_config(config).unwrap();
    }
    let descriptors: &'static [u8] = Box::leak(descriptor_set().into_boxed_slice());
    let reflection_builder = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(descriptors);
    let router = match reflection {
        Reflection::None => builder.add_service(EchoService),
        Reflection::V1 => builder
            .add_service(EchoService)
            .add_service(reflection_builder.build_v1().unwrap()),
        Reflection::V1Alpha => builder
            .add_service(EchoService)
            .add_service(reflection_builder.build_v1alpha().unwrap()),
    };
    tokio::spawn(router.serve_with_incoming(TcpListenerStream::new(listener)));
    TestServer { address }
}

/// Adapts an async function to the service traits tonic expects.
#[derive(Clone)]
struct Handler<F>(F);

impl<F, R, T, Fut> Service<Request<R>> for Handler<F>
where
    F: FnMut(Request<R>) -> Fut,
    Fut: Future<Output = Result<Response<T>, Status>>,
{
    type Response = Response<T>;
    type Error = Status;
    type Future = Fut;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Status>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request<R>) -> Fut {
        (self.0)(request)
    }
}

#[derive(Clone)]
struct EchoService;

impl NamedService for EchoService {
    const NAME: &'static str = "getman.test.Echo";
}

impl Service<http::Request<tonic::body::Body>> for EchoService {
    type Response = http::Response<tonic::body::Body>;
    type Error = Infallible;
    type Future = BoxFuture<Self::Response, Infallible>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Infallible>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: http::Request<tonic::body::Body>) -> Self::Future {
        Box::pin(async move {
            let echo = || Grpc::new(ProstCodec::<EchoReply, EchoRequest>::default());
            let response = match request.uri().path() {
                "/getman.test.Echo/Unary" => echo().unary(Handler(unary), request).await,
                "/getman.test.Echo/ServerStream" => {
                    echo()
                        .server_streaming(Handler(server_stream), request)
                        .await
                }
                "/getman.test.Echo/ClientStream" => {
                    echo()
                        .client_streaming(Handler(client_stream), request)
                        .await
                }
                "/getman.test.Echo/Bidi" => echo().streaming(Handler(bidi), request).await,
                "/getman.test.Echo/Slow" => echo().unary(Handler(slow), request).await,
                "/getman.test.Echo/Fail" => {
                    Grpc::new(ProstCodec::<EchoReply, FailRequest>::default())
                        .unary(Handler(fail), request)
                        .await
                }
                other => Status::unimplemented(format!("{other} is not here")).into_http(),
            };
            Ok(response)
        })
    }
}

/// The metadata the server saw, binary values as Base64.
fn seen(metadata: &MetadataMap) -> HashMap<String, String> {
    metadata
        .iter()
        .map(|entry| match entry {
            KeyAndValueRef::Ascii(key, value) => (
                key.to_string(),
                value.to_str().unwrap_or_default().to_owned(),
            ),
            KeyAndValueRef::Binary(key, value) => (
                key.to_string(),
                BASE64.encode(value.to_bytes().unwrap_or_default()),
            ),
        })
        .collect()
}

async fn unary(request: Request<EchoRequest>) -> Result<Response<EchoReply>, Status> {
    let metadata = seen(request.metadata());
    let message = request.into_inner();
    let mut response = Response::new(EchoReply {
        text: message.text,
        index: message
            .at
            .map_or(0, |at| i32::try_from(at.seconds).unwrap_or(-1)),
        metadata,
    });
    response
        .metadata_mut()
        .insert("x-served-by", "getman-test".parse().unwrap());
    Ok(response)
}

async fn slow(request: Request<EchoRequest>) -> Result<Response<EchoReply>, Status> {
    tokio::time::sleep(Duration::from_secs(5)).await;
    unary(request).await
}

type Replies = ReceiverStream<Result<EchoReply, Status>>;

/// Replies `times` times; a negative count replies once, then fails with
/// metadata in the trailers.
async fn server_stream(request: Request<EchoRequest>) -> Result<Response<Replies>, Status> {
    let message = request.into_inner();
    let (sender, receiver) = mpsc::channel(16);
    tokio::spawn(async move {
        for index in 0..message.times.max(1) {
            let reply = EchoReply {
                text: message.text.clone(),
                index,
                metadata: HashMap::new(),
            };
            if sender.send(Ok(reply)).await.is_err() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        if message.times < 0 {
            let mut metadata = MetadataMap::new();
            metadata.insert("x-why", "stream-failed".parse().unwrap());
            let _ = sender
                .send(Err(Status::with_metadata(
                    Code::FailedPrecondition,
                    "the stream failed",
                    metadata,
                )))
                .await;
        }
    });
    Ok(Response::new(ReceiverStream::new(receiver)))
}

/// Joins the texts it receives.
async fn client_stream(
    request: Request<Streaming<EchoRequest>>,
) -> Result<Response<EchoReply>, Status> {
    let mut stream = request.into_inner();
    let mut texts = Vec::new();
    while let Some(message) = stream.message().await? {
        texts.push(message.text);
    }
    Ok(Response::new(EchoReply {
        index: i32::try_from(texts.len()).unwrap_or_default(),
        text: texts.join(" "),
        metadata: HashMap::new(),
    }))
}

/// Answers each message with its text in capitals.
async fn bidi(request: Request<Streaming<EchoRequest>>) -> Result<Response<Replies>, Status> {
    let mut stream = request.into_inner();
    let (sender, receiver) = mpsc::channel(16);
    tokio::spawn(async move {
        let mut index = 0;
        while let Ok(Some(message)) = stream.message().await {
            let reply = EchoReply {
                text: message.text.to_uppercase(),
                index,
                metadata: HashMap::new(),
            };
            if sender.send(Ok(reply)).await.is_err() {
                return;
            }
            index += 1;
        }
    });
    Ok(Response::new(ReceiverStream::new(receiver)))
}

/// Fails with the code and message asked for, an `ErrorInfo` detail and
/// metadata.
async fn fail(request: Request<FailRequest>) -> Result<Response<EchoReply>, Status> {
    let asked = request.into_inner();
    let info = ErrorInfo {
        reason: "ASKED_TO_FAIL".into(),
        domain: "getman.test".into(),
        metadata: [("code".to_owned(), asked.code.to_string())]
            .into_iter()
            .collect(),
    };
    let details = RpcStatus {
        code: asked.code,
        message: asked.message.clone(),
        details: vec![prost_types::Any {
            type_url: "type.googleapis.com/google.rpc.ErrorInfo".into(),
            value: info.encode_to_vec(),
        }],
    };
    let mut metadata = MetadataMap::new();
    metadata.insert("x-failed", "yes".parse().unwrap());
    Err(Status::with_details_and_metadata(
        Code::from_i32(asked.code),
        asked.message,
        details.encode_to_vec().into(),
        metadata,
    ))
}
