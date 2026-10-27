//! gRPC requests: unary and streaming calls over HTTP/2, with schemas from
//! `.proto` files in the workspace or from server reflection, and messages
//! written and shown as JSON.

mod json;
mod schema;
mod status;

use prost_reflect::MethodDescriptor;
use serde::Serialize;

pub use json::example;
pub use schema::{MethodInfo, Schema, ServiceInfo, proto_files};
pub use status::{Code, Status, code_name};

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
