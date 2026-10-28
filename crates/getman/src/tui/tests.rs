//! The terminal UI's keys, followed through to what they start: a request
//! cancelled in flight, a WebSocket and an event stream connected to local
//! servers, a gRPC call steered by keys, and a browser sign-in against a
//! stand-in provider.

use std::net::SocketAddr;
use std::sync::Mutex;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use getman_core::Workspace;
use getman_core::grpc::{CallType, Command, LiveEvent, StreamMessage};
use getman_core::model::RequestKind;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpListener;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use super::app::{App, Focus, Message, Mode, RequestPane};
use super::stream::{Link, Phase, Stream, Tone};

struct Rig {
    app: App,
    tx: UnboundedSender<Message>,
    rx: UnboundedReceiver<Message>,
    dir: tempfile::TempDir,
}

impl Rig {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let workspace = Workspace::init(dir.path(), "Test").unwrap();
        let (tx, rx) = unbounded_channel();
        Self {
            app: App::new(workspace).unwrap(),
            tx,
            rx,
            dir,
        }
    }

    /// Saves a request file and opens it.
    fn open(&mut self, file: &str, yaml: &str) {
        std::fs::write(self.dir.path().join(file), yaml).unwrap();
        self.app.open_request(file);
        assert!(self.app.open.is_some(), "{:?}", self.app.message);
    }

    fn press(&mut self, code: KeyCode) {
        self.app
            .on_key(KeyEvent::new(code, KeyModifiers::NONE), &self.tx);
    }

    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            self.press(KeyCode::Char(c));
        }
    }

    /// The footer's message.
    fn said(&self) -> String {
        self.app
            .message
            .as_ref()
            .map(|(text, _, _)| text.clone())
            .unwrap_or_default()
    }

    fn stream(&self) -> &Stream {
        self.app.stream.as_ref().expect("a connection")
    }

    /// Hears background news until `done` holds.
    async fn until(&mut self, what: &str, done: impl Fn(&App) -> bool) {
        let waited = tokio::time::timeout(Duration::from_secs(20), async {
            while !done(&self.app) {
                let message = self.rx.recv().await.expect("the channel stays open");
                self.app.on_message(message);
            }
        })
        .await;
        assert!(waited.is_ok(), "timed out waiting for {what}");
    }
}

fn text_of(stream: &Stream) -> String {
    stream
        .entries
        .iter()
        .map(|e| format!("{} {}", e.label, e.text))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A port nothing listens on.
fn closed_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// A server that accepts connections and never answers.
async fn silent_server() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((stream, _)) = listener.accept().await {
            held.push(stream);
        }
    });
    addr
}

/// An event stream server: two events, then it stays open.
async fn events_server() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buffer = [0_u8; 2048];
                let _ = stream.read(&mut buffer).await;
                let reply = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n\
                    data: {\"n\":1,\"ok\":true}\n\nevent: tick\ndata: 2\n\n";
                let _ = stream.write_all(reply.as_bytes()).await;
                tokio::time::sleep(Duration::from_secs(60)).await;
            });
        }
    });
    addr
}

/// A WebSocket server that echoes text.
fn echo_server() -> SocketAddr {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { return };
            std::thread::spawn(move || {
                let Ok(mut socket) = tungstenite::accept(stream) else {
                    return;
                };
                while let Ok(message) = socket.read() {
                    if message.is_text() && socket.send(message).is_err() {
                        return;
                    }
                }
            });
        }
    });
    addr
}

#[tokio::test]
async fn esc_cancels_a_request_in_flight() {
    let server = silent_server().await;
    let mut rig = Rig::new();
    rig.open("slow.yaml", &format!("url: http://{server}/\n"));

    rig.press(KeyCode::Esc);
    assert_eq!(rig.said(), "", "Esc with nothing to stop says nothing");
    rig.press(KeyCode::Char('x'));
    assert_eq!(rig.said(), "Nothing is running");

    rig.press(KeyCode::Char('s'));
    assert!(rig.app.sending);
    rig.press(KeyCode::Esc);
    assert_eq!(rig.said(), "Cancelling the request");
    rig.until("the cancelled request", |app| !app.sending).await;
    let execution = rig.app.execution.as_ref().expect("an execution");
    assert!(execution.cancelled);
    assert_eq!(
        execution.error.as_deref(),
        Some(getman_core::execute::CANCELLED)
    );

    // The same key stops it again with the dedicated key.
    rig.press(KeyCode::Char('s'));
    rig.press(KeyCode::Char('x'));
    rig.until("the second cancel", |app| !app.sending).await;
    assert!(rig.app.execution.as_ref().unwrap().cancelled);
}

#[tokio::test]
async fn news_of_a_request_the_user_left_is_ignored() {
    let server = silent_server().await;
    let mut rig = Rig::new();
    rig.open("slow.yaml", &format!("url: http://{server}/\n"));
    rig.press(KeyCode::Char('s'));
    assert!(rig.app.sending);
    rig.open("other.yaml", "url: http://127.0.0.1:1/\n");
    assert!(!rig.app.sending, "opening another request stops the send");
    // The cancelled send reports back; the new request does not show it.
    tokio::time::sleep(Duration::from_millis(200)).await;
    while let Ok(message) = rig.rx.try_recv() {
        rig.app.on_message(message);
    }
    assert!(rig.app.execution.is_none());
}

#[tokio::test]
async fn an_event_stream_is_connected_logged_and_disconnected() {
    let server = events_server().await;
    let mut rig = Rig::new();
    rig.open(
        "events.yaml",
        &format!("kind: sse\nurl: http://{server}/\n"),
    );
    assert_eq!(
        rig.app.open.as_ref().unwrap().request.kind,
        RequestKind::Sse
    );

    rig.press(KeyCode::Char('m'));
    assert_eq!(rig.said(), "Event streams only receive");
    rig.press(KeyCode::Char('s'));
    assert_eq!(rig.app.focus, Focus::Response);
    rig.until("two events", |app| {
        app.stream.as_ref().is_some_and(|s| s.received == 2)
    })
    .await;

    let stream = rig.stream();
    assert_eq!(stream.phase, Phase::Open);
    let received: Vec<_> = stream
        .entries
        .iter()
        .filter(|e| e.tone == Tone::Received)
        .collect();
    assert_eq!(received[0].text, "{\n  \"n\": 1,\n  \"ok\": true\n}");
    assert_eq!(received[1].label, "tick");
    assert_eq!(received[1].text, "2");

    // Running already: the key says so instead of connecting twice.
    rig.press(KeyCode::Char('s'));
    assert!(rig.said().contains("already running"), "{}", rig.said());

    rig.press(KeyCode::Char('x'));
    assert_eq!(rig.said(), "Disconnecting");
    rig.until("the disconnect", |app| {
        app.stream.as_ref().is_some_and(|s| s.phase == Phase::Ended)
    })
    .await;
    assert_eq!(rig.stream().result, Some(("Disconnected".to_owned(), true)));
    assert!(text_of(rig.stream()).contains("Disconnected"));

    // Connect again, then leave: the connection closes with the request.
    rig.press(KeyCode::Char('s'));
    rig.until("the second connection", |app| {
        app.stream.as_ref().is_some_and(|s| s.received == 2)
    })
    .await;
    rig.open("other.yaml", "url: http://127.0.0.1:1/\n");
    assert!(rig.app.stream.is_none());
}

#[tokio::test]
async fn a_websocket_sends_saved_and_typed_messages() {
    let server = echo_server();
    let mut rig = Rig::new();
    rig.open(
        "chat.yaml",
        &format!(
            "kind: websocket\nurl: ws://{server}/\nwebsocket:\n  messages:\n    - name: Hello\n      type: json\n      content: '{{\"type\": \"hello\"}}'\n    - name: Plain\n      content: hi there\n"
        ),
    );

    // Nothing to send to yet.
    rig.press(KeyCode::Char('m'));
    assert_eq!(rig.said(), "Not connected: s connects first");
    assert_eq!(rig.app.mode, Mode::Normal);

    rig.press(KeyCode::Char('s'));
    rig.until("the connection", |app| {
        app.stream.as_ref().is_some_and(|s| s.phase == Phase::Open)
    })
    .await;
    assert_eq!(
        rig.stream().connected.as_deref().map(|c| &c[..3]),
        Some("101")
    );

    // A saved message, picked from the list.
    rig.press(KeyCode::Char('m'));
    assert_eq!(rig.app.mode, Mode::Pick);
    rig.press(KeyCode::Down);
    rig.press(KeyCode::Down);
    assert_eq!(rig.app.pick, 1, "the choice stays in the list");
    rig.press(KeyCode::Enter);
    assert_eq!(rig.app.mode, Mode::Normal);
    rig.until("the echo of the saved message", |app| {
        app.stream.as_ref().is_some_and(|s| s.received == 1)
    })
    .await;
    let log = text_of(rig.stream());
    assert!(log.contains("Plain hi there"), "{log}");
    assert_eq!(rig.stream().sent, 1);

    // The first one by its number, and its JSON pretty-printed back.
    rig.press(KeyCode::Char('m'));
    rig.press(KeyCode::Char('1'));
    rig.until("the echo of the JSON message", |app| {
        app.stream.as_ref().is_some_and(|s| s.received == 2)
    })
    .await;
    assert!(
        text_of(rig.stream()).contains("{\n  \"type\": \"hello\"\n}"),
        "{}",
        text_of(rig.stream())
    );

    // A message typed on the spot.
    rig.press(KeyCode::Char('t'));
    assert_eq!(rig.app.mode, Mode::Compose);
    rig.type_text("x{\"b\":2}");
    rig.press(KeyCode::Backspace);
    rig.type_text("]");
    rig.press(KeyCode::Enter);
    rig.until("the echo of the typed message", |app| {
        app.stream.as_ref().is_some_and(|s| s.received == 3)
    })
    .await;
    assert!(
        rig.stream()
            .entries
            .back()
            .unwrap()
            .text
            .contains("x{\"b\":2]"),
        "{}",
        text_of(rig.stream())
    );
    // Esc leaves the box without sending.
    rig.press(KeyCode::Char('t'));
    rig.type_text("never sent");
    rig.press(KeyCode::Esc);
    assert_eq!(rig.app.mode, Mode::Normal);
    assert_eq!(rig.stream().sent, 3);

    rig.press(KeyCode::Esc);
    rig.until("the disconnect", |app| {
        app.stream.as_ref().is_some_and(|s| s.phase == Phase::Ended)
    })
    .await;
    rig.press(KeyCode::Char('t'));
    assert_eq!(rig.said(), "Not connected: s connects first");
}

#[tokio::test]
async fn a_connection_that_fails_says_why() {
    let mut rig = Rig::new();
    rig.open(
        "down.yaml",
        &format!("kind: websocket\nurl: ws://127.0.0.1:{}/\n", closed_port()),
    );
    rig.press(KeyCode::Char('s'));
    rig.until("the failure", |app| {
        app.stream.as_ref().is_some_and(|s| s.phase == Phase::Ended)
    })
    .await;
    let stream = rig.stream();
    assert!(stream.entries.iter().any(|e| e.tone == Tone::Error));
    assert_eq!(stream.result.as_ref().map(|r| r.1), Some(false));
}

#[tokio::test]
async fn a_request_that_cannot_connect_says_so_in_the_footer() {
    let mut rig = Rig::new();
    rig.open("bad.yaml", "kind: websocket\nurl: ''\n");
    rig.press(KeyCode::Char('s'));
    assert!(rig.app.stream.is_none());
    assert!(!rig.said().is_empty());
}

/// A gRPC request, and a call to it that the test steers: the channel
/// stands where the call would listen for commands.
fn steered_call(rig: &mut Rig, message: &str) -> UnboundedReceiver<Command> {
    rig.open(
        "watch.yaml",
        &format!(
            "kind: grpc\nmethod: shop.Orders/Watch\nurl: localhost:50051\ngrpc:\n  proto: protos/shop.proto\n  message: |\n    {message}\n"
        ),
    );
    let (commands, receiver) = unbounded_channel();
    rig.app.stream = Some(Stream::new(7, RequestKind::Grpc, Link::Grpc(commands)));
    receiver
}

fn started(rig: &mut Rig, call_type: CallType) {
    rig.app.on_message(Message::Grpc(
        7,
        LiveEvent::Started {
            method: "shop.Orders/Watch".into(),
            call_type,
            address: "localhost:50051".into(),
        },
    ));
}

#[tokio::test]
async fn a_client_stream_sends_the_saved_message_typed_ones_and_ends() {
    let mut rig = Rig::new();
    let mut commands = steered_call(&mut rig, "{ \"sku\": \"BOOK-42\" }");

    rig.press(KeyCode::Char('m'));
    assert_eq!(
        rig.said(),
        "The call is still starting; try again in a moment"
    );

    started(&mut rig, CallType::ClientStreaming);
    assert_eq!(rig.stream().phase, Phase::Open);

    rig.press(KeyCode::Char('m'));
    let Command::Send { text, reply } = commands.try_recv().expect("a command") else {
        panic!("expected a message");
    };
    assert!(text.contains("BOOK-42"), "{text}");
    // The call refuses it; the log says why.
    reply.send(Err("no field named sku".into())).unwrap();
    rig.until("the refusal", |app| {
        app.stream.as_ref().is_some_and(|s| {
            s.entries
                .iter()
                .any(|e| e.tone == Tone::Error && e.text == "no field named sku")
        })
    })
    .await;

    rig.press(KeyCode::Char('t'));
    assert_eq!(rig.app.mode, Mode::Compose);
    rig.type_text("{\"sku\": \"PEN-7\"}");
    rig.press(KeyCode::Enter);
    let Command::Send { text, reply } = commands.try_recv().expect("a command") else {
        panic!("expected a message");
    };
    assert_eq!(text, "{\"sku\": \"PEN-7\"}");
    reply.send(Ok(1)).unwrap();

    rig.press(KeyCode::Char('f'));
    assert!(matches!(commands.try_recv(), Ok(Command::End)));
    assert!(text_of(rig.stream()).contains("ended your side"));

    rig.press(KeyCode::Esc);
    assert_eq!(rig.said(), "Cancelling the call");
    assert!(matches!(commands.try_recv(), Ok(Command::Cancel)));
    assert!(rig.stream().stopping);
}

#[tokio::test]
async fn calls_that_send_one_message_take_no_more() {
    let mut rig = Rig::new();
    let mut commands = steered_call(&mut rig, "{}");
    started(&mut rig, CallType::ServerStreaming);
    for key in ['m', 't', 'f'] {
        rig.press(KeyCode::Char(key));
        assert_eq!(
            rig.said(),
            "A server stream call sends its message when it starts",
            "{key}"
        );
        assert_eq!(rig.app.mode, Mode::Normal);
    }
    assert!(commands.try_recv().is_err());

    // Replies show as they arrive, with their direction.
    rig.app.on_message(Message::Grpc(
        7,
        LiveEvent::Message {
            message: StreamMessage {
                direction: getman_core::grpc::Direction::Received,
                message: serde_json::json!({"state": "PACKED"}),
                time_ms: 4.0,
                size: 12,
                error: None,
            },
        },
    ));
    assert_eq!(rig.stream().received, 1);
    assert!(text_of(rig.stream()).contains("\"state\": \"PACKED\""));
}

#[tokio::test]
async fn a_bidirectional_stream_without_a_saved_message_says_so() {
    let mut rig = Rig::new();
    let _commands = steered_call(&mut rig, "");
    started(&mut rig, CallType::BidiStreaming);
    rig.press(KeyCode::Char('m'));
    assert_eq!(rig.said(), "The request has no saved message; t types one");
}

#[tokio::test]
async fn a_grpc_call_that_cannot_connect_reports_its_failure() {
    let mut rig = Rig::new();
    rig.open(
        "get.yaml",
        &format!(
            "kind: grpc\nmethod: shop.Orders/Get\nurl: 127.0.0.1:{}\n",
            closed_port()
        ),
    );
    rig.press(KeyCode::Char('s'));
    assert_eq!(rig.stream().kind, RequestKind::Grpc);
    rig.until("the call to end", |app| {
        app.stream.as_ref().is_some_and(|s| s.phase == Phase::Ended)
    })
    .await;
    assert!(rig.app.execution.as_ref().unwrap().error.is_some());
    assert_eq!(rig.stream().result.as_ref().map(|r| r.1), Some(false));
    assert!(rig.stream().entries.iter().any(|e| e.tone == Tone::Error));
}

#[tokio::test]
async fn a_grpc_call_that_is_still_connecting_can_be_cancelled() {
    let server = silent_server().await;
    let mut rig = Rig::new();
    rig.open(
        "get.yaml",
        &format!("kind: grpc\nmethod: shop.Orders/Get\nurl: {server}\n"),
    );
    rig.press(KeyCode::Char('s'));
    rig.press(KeyCode::Char('x'));
    assert!(rig.stream().stopping);
    rig.until("the call to be dropped", |app| {
        app.stream.as_ref().is_some_and(|s| s.phase == Phase::Ended)
    })
    .await;
    assert_eq!(rig.stream().result, Some(("Cancelled".to_owned(), false)));
}

#[tokio::test]
async fn the_grpc_message_is_edited_as_the_body() {
    let mut rig = Rig::new();
    rig.open(
        "get.yaml",
        "kind: grpc\nmethod: shop.Orders/Get\nurl: localhost:1\ngrpc:\n  message: '{\"id\": 1}'\n",
    );
    rig.app.request_pane = RequestPane::Body;
    let (text, extension) =
        super::app::editable(&rig.app.open.as_ref().unwrap().request, RequestPane::Body);
    assert_eq!((text.as_str(), extension), ("{\"id\": 1}", "json"));
    let mut request = rig.app.open.as_ref().unwrap().request.clone();
    super::app::apply(&mut request, RequestPane::Body, "{\"id\": 2}").unwrap();
    assert_eq!(request.grpc.message, "{\"id\": 2}");
    assert_eq!(request.body, getman_core::model::Body::None);
}

#[tokio::test]
async fn the_method_key_still_changes_the_method_of_an_http_request() {
    let mut rig = Rig::new();
    rig.open("get.yaml", "url: http://127.0.0.1:1/\n");
    rig.press(KeyCode::Char('m'));
    assert_eq!(rig.app.open.as_ref().unwrap().request.method, "POST");
    assert_eq!(rig.app.mode, Mode::Normal);
    // These keys are for connections only.
    for key in ['t', 'f'] {
        rig.press(KeyCode::Char(key));
        assert_eq!(rig.app.mode, Mode::Normal);
        assert!(rig.app.stream.is_none());
    }
}

// ---- Signing in with a browser -----------------------------------------------

static OPENED: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Stands in for the browser: remembers the address it was sent to.
fn record_browser(url: &str) -> bool {
    OPENED.lock().unwrap().push(url.to_owned());
    true
}

fn opened_with(client: &str) -> Option<String> {
    OPENED
        .lock()
        .unwrap()
        .iter()
        .find(|url| url.contains(&format!("client_id={client}")))
        .cloned()
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(byte) = u8::from_str_radix(&text[i + 1..i + 3], 16)
        {
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn query_param(url: &str, name: &str) -> String {
    let query = url.split_once('?').map_or("", |(_, q)| q);
    query
        .split('&')
        .find_map(|pair| pair.strip_prefix(&format!("{name}=")))
        .map(decode)
        .unwrap_or_default()
}

/// A provider's token endpoint: every request gets a token.
async fn token_server() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut seen = Vec::new();
                let mut buffer = [0_u8; 4096];
                loop {
                    let Ok(n) = stream.read(&mut buffer).await else {
                        return;
                    };
                    seen.extend_from_slice(&buffer[..n]);
                    let text = String::from_utf8_lossy(&seen).into_owned();
                    let Some((head, body)) = text.split_once("\r\n\r\n") else {
                        continue;
                    };
                    let length = head
                        .lines()
                        .find_map(|l| {
                            l.to_lowercase()
                                .strip_prefix("content-length:")
                                .map(str::to_owned)
                        })
                        .and_then(|v| v.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    if body.len() >= length {
                        break;
                    }
                }
                let body = r#"{"access_token":"tok-1","token_type":"Bearer","expires_in":1800}"#;
                let reply = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(reply.as_bytes()).await;
            });
        }
    });
    addr
}

fn oauth_request(client: &str, provider: SocketAddr) -> String {
    format!(
        "url: http://127.0.0.1:1/\nauth:\n  type: oauth2\n  grant: authorization_code\n  auth_url: http://{provider}/authorize\n  token_url: http://{provider}/token\n  client_id: {client}\n"
    )
}

#[tokio::test]
async fn the_auth_pane_signs_in_with_the_browser_and_shows_the_expiry() {
    let provider = token_server().await;
    let mut rig = Rig::new();
    rig.app.opener = record_browser;
    rig.open("orders.yaml", &oauth_request("tui-signin", provider));

    rig.press(KeyCode::Char('4'));
    assert_eq!(rig.app.request_pane, RequestPane::Auth);
    let oauth = rig.app.oauth.as_ref().expect("OAuth 2.0 settings");
    assert!(oauth.browser);
    assert!(oauth.token.is_none());

    rig.press(KeyCode::Char('b'));
    assert!(rig.app.sign_in.is_some());
    rig.until("the browser to open", |app| {
        app.sign_in
            .as_ref()
            .is_some_and(|s| s.authorization.is_some())
    })
    .await;
    let url = opened_with("tui-signin").expect("the browser was sent to the provider");
    assert!(
        url.starts_with(&format!("http://{provider}/authorize?")),
        "{url}"
    );
    assert!(
        rig.said().contains("Waiting for you to sign in"),
        "{}",
        rig.said()
    );

    // Pressing b again does not start a second sign-in.
    rig.press(KeyCode::Char('b'));
    assert!(rig.said().contains("Already waiting"), "{}", rig.said());

    // The provider sends the browser back to Getman's callback.
    let redirect = query_param(&url, "redirect_uri");
    let state = query_param(&url, "state");
    let address = redirect
        .strip_prefix("http://")
        .and_then(|rest| rest.split('/').next())
        .expect("a loopback redirect");
    let callback = format!(
        "GET /callback?code=abc&state={state} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
    );
    let mut browser = tokio::net::TcpStream::connect(address).await.unwrap();
    browser.write_all(callback.as_bytes()).await.unwrap();
    let mut page = String::new();
    let _ = browser.read_to_string(&mut page).await;

    rig.until("the token", |app| app.sign_in.is_none()).await;
    let token = rig
        .app
        .oauth
        .as_ref()
        .and_then(|o| o.token.as_ref())
        .expect("a token");
    assert_eq!(token.access_token, "tok-1");
    // A 30-minute token; a slow machine may take a minute off it.
    let said = rig.said();
    assert!(
        ["29 min", "28 min"]
            .iter()
            .any(|left| said.starts_with(&format!("Signed in · expires in {left}"))),
        "{said}"
    );
    // It is kept: another look at the pane finds it.
    rig.app.refresh_credentials();
    assert!(rig.app.oauth.as_ref().unwrap().token.is_some());
}

#[tokio::test]
async fn a_sign_in_can_be_cancelled() {
    let provider = token_server().await;
    let mut rig = Rig::new();
    rig.app.opener = record_browser;
    rig.open("orders.yaml", &oauth_request("tui-cancel", provider));
    rig.press(KeyCode::Char('4'));
    rig.press(KeyCode::Char('b'));
    rig.until("the browser to open", |app| {
        app.sign_in
            .as_ref()
            .is_some_and(|s| s.authorization.is_some())
    })
    .await;
    let id = rig
        .app
        .sign_in
        .as_ref()
        .and_then(|s| s.authorization.as_ref())
        .map(|a| a.id.clone())
        .unwrap();

    rig.press(KeyCode::Esc);
    assert!(rig.app.sign_in.is_none());
    assert_eq!(rig.said(), "Sign-in cancelled");
    // The sign-in is gone from the core too, and the callback port is free.
    let waited = rig
        .app
        .executor
        .authenticator
        .wait_for_token(&rig.app.executor.http, &id, Duration::from_secs(1))
        .await;
    assert!(waited.is_err());
    // Late news from the cancelled sign-in is ignored.
    while let Ok(message) = rig.rx.try_recv() {
        rig.app.on_message(message);
    }
    assert!(rig.app.oauth.as_ref().unwrap().token.is_none());
}

#[tokio::test]
async fn signing_in_needs_oauth_with_a_browser() {
    let mut rig = Rig::new();
    rig.app.opener = record_browser;
    rig.open("plain.yaml", "url: http://127.0.0.1:1/\n");
    rig.press(KeyCode::Char('4'));
    assert!(rig.app.oauth.is_none());
    rig.press(KeyCode::Char('b'));
    assert!(
        rig.said().contains("does not use OAuth 2.0"),
        "{}",
        rig.said()
    );

    rig.open(
        "client.yaml",
        "url: http://127.0.0.1:1/\nauth:\n  type: oauth2\n  grant: client_credentials\n  token_url: http://127.0.0.1:1/token\n  client_id: svc\n",
    );
    rig.press(KeyCode::Char('4'));
    assert!(!rig.app.oauth.as_ref().unwrap().browser);
    rig.press(KeyCode::Char('b'));
    assert!(rig.said().contains("needs no browser"), "{}", rig.said());
    assert!(rig.app.sign_in.is_none());

    // Only the auth pane signs in.
    rig.press(KeyCode::Char('1'));
    rig.press(KeyCode::Char('b'));
    assert!(rig.app.sign_in.is_none());
}
