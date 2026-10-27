//! Requests that stay open and carry messages: WebSocket, Server-Sent
//! Events and Socket.IO.

mod settings;

pub use settings::{
    MessageFormat, SavedEmit, SavedMessage, SocketIoSettings, SseSettings, WebSocketSettings,
};
