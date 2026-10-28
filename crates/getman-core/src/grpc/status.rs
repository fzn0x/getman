//! gRPC status codes, and the rich error details servers may attach in
//! `grpc-status-details-bin`.

use std::sync::OnceLock;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use prost_reflect::{DescriptorPool, DynamicMessage, Value as Field};
use protox::file::{ChainFileResolver, File, FileResolver, GoogleFileResolver};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::json::to_json;

/// The status codes gRPC defines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Code {
    /// Success.
    Ok = 0,
    /// The call was cancelled, usually by the caller.
    Cancelled = 1,
    /// An error without a better code.
    Unknown = 2,
    /// The caller sent something invalid.
    InvalidArgument = 3,
    /// The deadline passed before the call finished.
    DeadlineExceeded = 4,
    /// Something asked for was not found.
    NotFound = 5,
    /// Something to be created already exists.
    AlreadyExists = 6,
    /// The caller may not do this.
    PermissionDenied = 7,
    /// A quota or limit ran out.
    ResourceExhausted = 8,
    /// The system is not in a state for this.
    FailedPrecondition = 9,
    /// The call was aborted, often by a conflict.
    Aborted = 10,
    /// A value was outside its range.
    OutOfRange = 11,
    /// The server does not implement this method.
    Unimplemented = 12,
    /// The server broke an invariant.
    Internal = 13,
    /// The service cannot be reached now.
    Unavailable = 14,
    /// Data was lost or corrupted.
    DataLoss = 15,
    /// The caller is not authenticated.
    Unauthenticated = 16,
}

const NAMES: [&str; 17] = [
    "OK",
    "CANCELLED",
    "UNKNOWN",
    "INVALID_ARGUMENT",
    "DEADLINE_EXCEEDED",
    "NOT_FOUND",
    "ALREADY_EXISTS",
    "PERMISSION_DENIED",
    "RESOURCE_EXHAUSTED",
    "FAILED_PRECONDITION",
    "ABORTED",
    "OUT_OF_RANGE",
    "UNIMPLEMENTED",
    "INTERNAL",
    "UNAVAILABLE",
    "DATA_LOSS",
    "UNAUTHENTICATED",
];

/// The name of a status code, such as `NOT_FOUND`.
pub fn code_name(code: u32) -> String {
    usize::try_from(code)
        .ok()
        .and_then(|i| NAMES.get(i))
        .map_or_else(|| format!("CODE_{code}"), |name| (*name).to_owned())
}

/// How a call ended.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Status {
    /// The status code; 0 is success.
    pub code: u32,
    /// The code's name, such as `NOT_FOUND`.
    pub name: String,
    /// The server's message, if it sent one.
    pub message: String,
    /// `grpc-status-details-bin` as a `google.rpc.Status`, with details of
    /// known types as JSON.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
    /// The raw details, until they are decoded.
    #[serde(skip)]
    pub(crate) details_bytes: Option<Vec<u8>>,
}

impl Status {
    /// A status made on this side, such as for a passed deadline.
    pub(crate) fn new(code: Code, message: impl Into<String>) -> Self {
        Self::from_code(code as u32, message.into())
    }

    pub(crate) fn from_code(code: u32, message: String) -> Self {
        Self {
            code,
            name: code_name(code),
            message,
            details: None,
            details_bytes: None,
        }
    }

    /// Whether the call succeeded.
    pub fn is_ok(&self) -> bool {
        self.code == 0
    }

    /// Decodes the details, looking up their types in the call's schema
    /// first and then among the standard error details.
    pub(crate) fn decode_details(&mut self, schema: Option<&DescriptorPool>) {
        if let Some(bytes) = self.details_bytes.take() {
            self.details = Some(details_json(&bytes, schema));
        }
    }
}

/// `google.rpc.Status` and its standard details, compiled once from the
/// sources in `rpc/`.
fn rpc_pool() -> &'static DescriptorPool {
    static POOL: OnceLock<DescriptorPool> = OnceLock::new();
    POOL.get_or_init(|| {
        let mut resolver = ChainFileResolver::new();
        resolver.add(Embedded);
        resolver.add(GoogleFileResolver::new());
        let mut compiler = protox::Compiler::with_file_resolver(resolver);
        compiler.include_imports(true);
        compiler
            .open_files(["google/rpc/status.proto", "google/rpc/error_details.proto"])
            .expect("the embedded error protos compile");
        compiler.descriptor_pool()
    })
}

struct Embedded;

impl FileResolver for Embedded {
    fn open_file(&self, name: &str) -> Result<File, protox::Error> {
        let source = match name {
            "google/rpc/status.proto" => include_str!("rpc/status.proto"),
            "google/rpc/error_details.proto" => include_str!("rpc/error_details.proto"),
            _ => return Err(protox::Error::file_not_found(name)),
        };
        File::from_source(name, source)
    }
}

/// `grpc-status-details-bin` as JSON: the code, the message and each
/// detail with its `@type`. Details of unknown types keep their bytes as
/// Base64.
pub(crate) fn details_json(bytes: &[u8], schema: Option<&DescriptorPool>) -> Value {
    let pool = rpc_pool();
    let descriptor = pool
        .get_message_by_name("google.rpc.Status")
        .expect("google.rpc.Status is embedded");
    let status = match DynamicMessage::decode(descriptor, bytes) {
        Ok(status) => status,
        Err(e) => {
            return json!({
                "error": format!("the details are not a google.rpc.Status: {e}"),
                "base64": BASE64.encode(bytes),
            });
        }
    };
    let code = status
        .get_field_by_name("code")
        .and_then(|v| v.as_i32())
        .unwrap_or_default();
    let message = status
        .get_field_by_name("message")
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    let details: Vec<Value> = status
        .get_field_by_name("details")
        .and_then(|v| v.as_list().map(<[Field]>::to_vec))
        .unwrap_or_default()
        .iter()
        .filter_map(Field::as_message)
        .map(|any| detail_json(any, schema, pool))
        .collect();
    json!({ "code": code, "message": message, "details": details })
}

fn detail_json(
    any: &DynamicMessage,
    schema: Option<&DescriptorPool>,
    pool: &DescriptorPool,
) -> Value {
    let type_url = any
        .get_field_by_name("type_url")
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    let value = any
        .get_field_by_name("value")
        .and_then(|v| v.as_bytes().cloned())
        .unwrap_or_default();
    let name = type_url.rsplit('/').next().unwrap_or_default();
    let descriptor = schema
        .and_then(|s| s.get_message_by_name(name))
        .or_else(|| pool.get_message_by_name(name));
    let decoded = descriptor
        .and_then(|d| DynamicMessage::decode(d, value.as_ref()).ok())
        .and_then(|message| to_json(&message).ok());
    let mut out = serde_json::Map::new();
    out.insert("@type".into(), Value::String(type_url));
    match decoded {
        Some(Value::Object(fields)) => out.extend(fields),
        Some(other) => {
            out.insert("value".into(), other);
        }
        None => {
            out.insert("base64".into(), Value::String(BASE64.encode(&value)));
        }
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message as _;

    #[test]
    fn codes_have_names() {
        assert_eq!(code_name(0), "OK");
        assert_eq!(code_name(5), "NOT_FOUND");
        assert_eq!(code_name(16), "UNAUTHENTICATED");
        assert_eq!(code_name(99), "CODE_99");
    }

    #[test]
    fn known_details_decode_to_json() {
        let pool = rpc_pool();
        let mut info =
            DynamicMessage::new(pool.get_message_by_name("google.rpc.ErrorInfo").unwrap());
        info.set_field_by_name("reason", Field::String("STOCK".into()));
        info.set_field_by_name("domain", Field::String("shop.example".into()));
        let any_desc = pool.get_message_by_name("google.protobuf.Any").unwrap();
        let mut known = DynamicMessage::new(any_desc.clone());
        known.set_field_by_name(
            "type_url",
            Field::String("type.googleapis.com/google.rpc.ErrorInfo".into()),
        );
        known.set_field_by_name("value", Field::Bytes(info.encode_to_vec().into()));
        let mut unknown = DynamicMessage::new(any_desc);
        unknown.set_field_by_name(
            "type_url",
            Field::String("type.googleapis.com/acme.Secret".into()),
        );
        unknown.set_field_by_name("value", Field::Bytes(vec![1, 2].into()));
        let mut status =
            DynamicMessage::new(pool.get_message_by_name("google.rpc.Status").unwrap());
        status.set_field_by_name("code", Field::I32(9));
        status.set_field_by_name("message", Field::String("out of stock".into()));
        status.set_field_by_name(
            "details",
            Field::List(vec![Field::Message(known), Field::Message(unknown)]),
        );

        let json = details_json(&status.encode_to_vec(), None);
        assert_eq!(json["code"], 9);
        assert_eq!(json["message"], "out of stock");
        assert_eq!(
            json["details"][0]["@type"],
            "type.googleapis.com/google.rpc.ErrorInfo"
        );
        assert_eq!(json["details"][0]["reason"], "STOCK");
        assert_eq!(json["details"][1]["base64"], "AQI=");

        let broken = details_json(b"\xff\xff", None);
        assert!(
            broken["error"]
                .as_str()
                .unwrap()
                .contains("google.rpc.Status")
        );
    }
}
