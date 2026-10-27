//! gRPC requests: unary and streaming calls over HTTP/2, with schemas from
//! `.proto` files in the workspace or from server reflection, and messages
//! written and shown as JSON.
//!
//! A gRPC request is a [`Request`](crate::model::Request) with
//! `kind: grpc`. Its `url` is the server's address, its `method` the
//! method's full name (`shop.Orders/GetOrder`), its headers and
//! authentication become metadata, and its `grpc` block names the schema
//! and holds the message.
//!
//! [`Executor::execute`](crate::execute::Executor::execute) runs one like
//! an HTTP request, so scripts, checks and
//! captures work the same: they see the call as a response whose status is
//! the gRPC status code, whose headers are the response headers followed
//! by the trailers, and whose body is the response message as JSON (an
//! array of messages when the server streams).

mod execute;
mod generate;
mod json;
mod metadata;
mod reflection;
mod schema;
mod status;
mod transport;

#[cfg(test)]
mod test_server;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use prost_reflect::MethodDescriptor;
use serde::Serialize;
use serde_json::Value;

pub(crate) use execute::execute;
pub use execute::{Command, Live, LiveEvent, grpcurl, load_schema};
pub use generate::generate_requests;
pub use json::example;
pub use schema::{MethodInfo, Schema, ServiceInfo, proto_files};
pub use status::{Code, Status, code_name};

use crate::http::Header;

/// Which sides of a call stream.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CallType {
    /// One message each way.
    #[default]
    Unary,
    /// One message in, a stream out.
    ServerStreaming,
    /// A stream in, one message out.
    ClientStreaming,
    /// Streams both ways.
    BidiStreaming,
}

impl CallType {
    /// The call type of a method.
    pub fn of(method: &MethodDescriptor) -> Self {
        match (method.is_client_streaming(), method.is_server_streaming()) {
            (false, false) => Self::Unary,
            (false, true) => Self::ServerStreaming,
            (true, false) => Self::ClientStreaming,
            (true, true) => Self::BidiStreaming,
        }
    }

    /// Whether the client sends a stream.
    pub fn client_streams(self) -> bool {
        matches!(self, Self::ClientStreaming | Self::BidiStreaming)
    }

    /// Whether the server answers with a stream.
    pub fn server_streams(self) -> bool {
        matches!(self, Self::ServerStreaming | Self::BidiStreaming)
    }
}

/// Which way a message went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// From Getman to the server.
    Sent,
    /// From the server.
    Received,
}

/// One message of a call.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StreamMessage {
    /// Which way it went.
    pub direction: Direction,
    /// The message as JSON.
    pub message: Value,
    /// Milliseconds from the start of the call.
    pub time_ms: f64,
    /// Size of the encoded message in bytes.
    pub size: u64,
    /// Why the message could not be shown as JSON, if it could not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Everything about a gRPC call that does not fit an HTTP response.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct GrpcCall {
    /// The method's full name, such as `shop.Orders/GetOrder`.
    pub method: String,
    /// Which sides stream.
    pub call_type: CallType,
    /// The server's address, `host:port`.
    pub address: String,
    /// Whether the call used TLS.
    pub tls: bool,
    /// How the call ended; `None` if it never got that far.
    pub status: Option<Status>,
    /// Response headers.
    pub headers: Vec<Header>,
    /// Response trailers.
    pub trailers: Vec<Header>,
    /// Messages sent and received, in order.
    pub messages: Vec<StreamMessage>,
    /// Messages left out of `messages` because there were too many.
    #[serde(skip_serializing_if = "is_zero")]
    pub dropped_messages: usize,
    /// Milliseconds from the start of the call until it ended.
    pub time_ms: f64,
    /// Milliseconds until the response headers arrived.
    pub headers_ms: Option<f64>,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// Schemas read through reflection, by server, until refreshed.
fn reflected() -> &'static Mutex<HashMap<String, Schema>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Schema>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}
