//! What WebSocket, Server-Sent Events and Socket.IO requests save beside
//! the URL, headers, authentication and settings they share with HTTP
//! requests. Like the rest of a request file, fields at their defaults are
//! not written.

use serde::{Deserialize, Serialize};

use crate::lenient;

/// WebSocket settings, under `websocket:` in a request file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WebSocketSettings {
    /// Subprotocols to offer in `Sec-WebSocket-Protocol`, preferred first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub subprotocols: Vec<String>,
    /// Messages saved with the request, ready to send.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<SavedMessage>,
}

impl WebSocketSettings {
    /// Whether nothing is set.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// A WebSocket message saved with a request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedMessage {
    /// The name it is picked by.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// How the content is sent.
    #[serde(
        default,
        rename = "type",
        skip_serializing_if = "MessageFormat::is_text"
    )]
    pub format: MessageFormat,
    /// The message. May contain `{{variables}}`. Binary messages are
    /// written in Base64.
    #[serde(default, deserialize_with = "lenient::string")]
    pub content: String,
}

/// How a WebSocket message is sent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageFormat {
    /// A text frame, as written.
    #[default]
    Text,
    /// A text frame that must hold valid JSON.
    Json,
    /// A binary frame of the bytes the Base64 content decodes to.
    Binary,
}

impl MessageFormat {
    /// Whether this is [`MessageFormat::Text`].
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub fn is_text(&self) -> bool {
        *self == Self::Text
    }
}

/// Server-Sent Events settings, under `sse:` in a request file.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SseSettings {
    /// Connect again when the stream ends, sending the id of the last
    /// event in `Last-Event-ID`, as browsers do.
    #[serde(skip_serializing_if = "is_false")]
    pub reconnect: bool,
}

impl SseSettings {
    /// Whether nothing is set.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Socket.IO settings, under `socketio:` in a request file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SocketIoSettings {
    /// Namespace to join, such as `/chat`. When empty, the URL's path is the
    /// namespace, as in the Socket.IO client, and `/` without one.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub namespace: String,
    /// Path the server answers on. Default `/socket.io/`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub path: String,
    /// A JSON object sent when joining the namespace, which the server reads
    /// as `socket.handshake.auth`. May contain `{{variables}}`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub auth: String,
    /// How to reach the server: long polling first and an upgrade to
    /// WebSocket if the server offers it (the default, as in the Socket.IO
    /// client), or only one of them.
    #[serde(skip_serializing_if = "SocketIoTransport::is_auto")]
    pub transport: SocketIoTransport,
    /// Events to show. Every event when empty.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<String>,
    /// Events saved with the request, ready to emit.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<SavedEmit>,
}

impl SocketIoSettings {
    /// Whether nothing is set.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// The transports a Socket.IO connection may use.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SocketIoTransport {
    /// Start with HTTP long polling and upgrade to WebSocket if the
    /// server offers it. If the server turns long polling down, connect
    /// over WebSocket.
    #[default]
    Auto,
    /// WebSocket only, for servers that allow nothing else.
    Websocket,
    /// HTTP long polling only, never upgrading.
    Polling,
}

impl SocketIoTransport {
    /// Whether this is [`SocketIoTransport::Auto`].
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub fn is_auto(&self) -> bool {
        *self == Self::Auto
    }
}

/// A Socket.IO event saved with a request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedEmit {
    /// The name it is picked by.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// The event to emit.
    #[serde(default)]
    pub event: String,
    /// Its arguments: a JSON array of them, or a single JSON value. May
    /// contain `{{variables}}`.
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "lenient::string"
    )]
    pub args: String,
    /// Ask the server to acknowledge the event.
    #[serde(default, skip_serializing_if = "is_false")]
    pub ack: bool,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(value: &bool) -> bool {
    !*value
}

#[cfg(test)]
mod tests {
    use crate::model::{Request, RequestKind};

    #[test]
    fn a_websocket_request_reads_and_writes_its_messages() {
        let yaml = "\
kind: websocket
url: wss://example.com/chat
websocket:
  subprotocols: [chat.v2]
  messages:
    - name: Hello
      type: json
      content: '{\"type\": \"hello\"}'
    - name: Plain
      content: hi
";
        let request: Request = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(request.kind, RequestKind::Websocket);
        assert_eq!(request.websocket.subprotocols, ["chat.v2"]);
        assert_eq!(
            request.websocket.messages[0].format,
            super::MessageFormat::Json
        );
        let written = serde_yaml_ng::to_string(&request).unwrap();
        assert!(!written.contains("type: text"), "{written}");
        assert!(!written.contains("socketio"), "{written}");
        let back: Request = serde_yaml_ng::from_str(&written).unwrap();
        assert_eq!(back, request);
    }

    #[test]
    fn socket_io_and_sse_settings_stay_small() {
        let yaml = "\
kind: socketio
url: http://localhost:3000
socketio:
  namespace: /orders
  transport: websocket
  auth: '{\"token\": \"{{token}}\"}'
  events: [order:created]
  messages:
    - event: subscribe
      args: '[\"eu\"]'
      ack: true
sse:
  reconnect: true
";
        let request: Request = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(request.socketio.namespace, "/orders");
        assert_eq!(
            request.socketio.transport,
            super::SocketIoTransport::Websocket
        );
        assert!(request.socketio.messages[0].ack);
        assert!(request.sse.reconnect);
        let written = serde_yaml_ng::to_string(&request).unwrap();
        assert!(!written.contains("name:"), "{written}");
        assert!(!written.contains("path:"), "{written}");
        assert!(written.contains("transport: websocket"), "{written}");
        assert!(!written.contains("kind: websocket"), "{written}");
        assert_eq!(
            serde_yaml_ng::from_str::<Request>(&written).unwrap(),
            request
        );
    }
}
