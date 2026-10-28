use std::time::Duration;

use serde_json::{Value, json};

use super::test_socketio::{self, Transports};
use super::*;
use crate::model::{KeyValue, Proxy, Scripts, Settings};
use crate::test_server;
use crate::workspace::Workspace;

fn setup() -> (tempfile::TempDir, Executor) {
    let dir = tempfile::tempdir().unwrap();
    let workspace = Workspace::init(dir.path(), "Realtime").unwrap();
    (dir, Executor::new(workspace))
}

fn open(executor: &Executor, request: &Request, options: Options) -> Session {
    connect(executor, None, request, &Context::default(), options).unwrap()
}

/// Reads events until one matches, failing after a few seconds.
async fn until(
    session: &mut Session,
    mut done: impl FnMut(&StreamEvent) -> bool,
) -> Vec<StreamEvent> {
    let mut events = Vec::new();
    let read = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(event) = session.next().await {
            let stop = done(&event);
            events.push(event);
            if stop {
                return true;
            }
        }
        false
    })
    .await;
    assert_eq!(read, Ok(true), "the event never came; got {events:#?}");
    events
}

fn received(event: &StreamEvent) -> Option<&Message> {
    match &event.kind {
        EventKind::Message(message) if message.direction == Direction::Received => Some(message),
        _ => None,
    }
}

fn messages(events: &[StreamEvent]) -> Vec<&Message> {
    events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::Message(message) => Some(message),
            _ => None,
        })
        .collect()
}

fn is_closed(event: &StreamEvent) -> bool {
    matches!(event.kind, EventKind::Closed { .. })
}

fn text(content: &str) -> Outbound {
    Outbound::Message(SavedMessage {
        content: content.into(),
        ..SavedMessage::default()
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn websocket_sends_receives_and_closes() {
    let addr = test_servers::websocket().await;
    let (_dir, executor) = setup();
    let request = Request {
        kind: RequestKind::Websocket,
        url: format!("ws://{addr}/chat?room={{{{room}}}}"),
        headers: vec![KeyValue::new("X-Test", "yes")],
        auth: Auth::Bearer {
            token: "t0k".into(),
            prefix: String::new(),
        },
        variables: [("room".to_owned(), "7".to_owned())].into_iter().collect(),
        websocket: WebSocketSettings {
            subprotocols: vec!["chat.v1".into(), "chat.v2".into()],
            messages: Vec::new(),
        },
        ..Request::default()
    };
    let mut session = open(&executor, &request, Options::default());
    let events = until(&mut session, |e| received(e).is_some()).await;
    let EventKind::Connecting { url, .. } = &events[0].kind else {
        panic!("{events:#?}")
    };
    assert_eq!(url, &format!("ws://{addr}/chat?room=7"));
    let EventKind::Open {
        status, protocol, ..
    } = &events[1].kind
    else {
        panic!("{events:#?}")
    };
    assert_eq!(*status, 101);
    assert_eq!(protocol.as_deref(), Some("chat.v2"));
    let hello: Value = serde_json::from_str(&received(&events[2]).unwrap().data).unwrap();
    assert_eq!(hello["authorization"], "Bearer t0k");
    assert_eq!(hello["query"], "room=7");
    assert_eq!(hello["x_test"], "yes");
    assert_eq!(hello["protocol"], "chat.v1, chat.v2");

    let control = session.control();
    control
        .send(Outbound::Message(SavedMessage {
            name: "Hello".into(),
            format: MessageFormat::Json,
            content: r#"{"room": "{{room}}"}"#.into(),
        }))
        .unwrap();
    control
        .send(Outbound::Message(SavedMessage {
            name: String::new(),
            format: MessageFormat::Binary,
            content: "AAEC".into(),
        }))
        .unwrap();
    control
        .send(Outbound::Message(SavedMessage {
            format: MessageFormat::Json,
            content: "{oops".into(),
            ..SavedMessage::default()
        }))
        .unwrap();
    let (mut echoes, mut warned) = (0, false);
    let events = until(&mut session, |e| {
        echoes += usize::from(received(e).is_some());
        warned |=
            matches!(&e.kind, EventKind::Warning { message } if message.contains("not valid JSON"));
        echoes == 2 && warned
    })
    .await;
    let mut seen: Vec<_> = messages(&events)
        .iter()
        .map(|m| (m.direction, m.data.as_str(), m.binary, m.name.as_deref()))
        .collect();
    seen.sort_unstable_by_key(|m| (m.1, m.0 == Direction::Received));
    assert_eq!(
        seen,
        [
            (Direction::Sent, "AAEC", true, None),
            (Direction::Received, "AAEC", true, None),
            (Direction::Sent, r#"{"room": "7"}"#, false, Some("Hello")),
            (Direction::Received, r#"{"room": "7"}"#, false, None),
        ]
    );

    control.close();
    let events = until(&mut session, is_closed).await;
    assert_eq!(
        events.last().unwrap().kind,
        EventKind::Closed {
            code: Some(1000),
            reason: String::new(),
            by_client: true
        }
    );
    assert!(session.next().await.is_none());
    assert!(control.send(text("late")).is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn websocket_answers_pings_and_reports_the_servers_close() {
    let addr = test_servers::websocket().await;
    let (_dir, executor) = setup();
    let request = Request {
        kind: RequestKind::Websocket,
        url: format!("http://{addr}/"),
        ..Request::default()
    };
    let mut session = open(&executor, &request, Options::default());
    until(&mut session, |e| received(e).is_some()).await;
    session.control().send(text("ping")).unwrap();
    until(&mut session, |e| {
        received(e).is_some_and(|m| m.data == "pong: are you there")
    })
    .await;
    session.control().send(text("close")).unwrap();
    let events = until(&mut session, is_closed).await;
    assert_eq!(
        events.last().unwrap().kind,
        EventKind::Closed {
            code: Some(4001),
            reason: "bye".into(),
            by_client: false
        }
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn websocket_failures_explain_themselves() {
    let addr = test_servers::websocket().await;
    let (_dir, executor) = setup();
    let mut request = Request {
        kind: RequestKind::Websocket,
        url: format!("ws://{addr}/refuse"),
        ..Request::default()
    };
    let mut session = open(&executor, &request, Options::default());
    let events = until(&mut session, is_closed).await;
    let EventKind::Error { message } = &events[events.len() - 2].kind else {
        panic!("{events:#?}")
    };
    assert_eq!(
        message,
        "The server answered 403 Forbidden instead of opening a WebSocket: go away"
    );

    // A server that accepts the connection but never answers.
    let silent = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    request.url = format!("ws://{}/", silent.local_addr().unwrap());
    request.settings.timeout_ms = Some(200);
    let mut session = open(&executor, &request, Options::default());
    let events = until(&mut session, is_closed).await;
    assert!(
        events.iter().any(|e| matches!(&e.kind,
            EventKind::Error { message } if message.starts_with("The connection timed out after 200 ms"))),
        "{events:#?}"
    );

    request.url = "ftp://example.com".into();
    assert!(
        connect(
            &executor,
            None,
            &request,
            &Context::default(),
            Options::default()
        )
        .is_err()
    );
    request.url = format!("ws://{addr}/");
    request.auth = Auth::Digest {
        username: "a".into(),
        password: "b".into(),
    };
    let error = connect(
        &executor,
        None,
        &request,
        &Context::default(),
        Options::default(),
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.starts_with("Digest authentication cannot open"),
        "{error}"
    );
    request.kind = RequestKind::Http;
    assert!(
        connect(
            &executor,
            None,
            &request,
            &Context::default(),
            Options::default()
        )
        .is_err()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_lossy_session_skips_what_it_cannot_show() {
    let addr = test_servers::websocket().await;
    let (_dir, executor) = setup();
    let request = Request {
        kind: RequestKind::Websocket,
        url: format!("ws://{addr}/"),
        ..Request::default()
    };
    let mut session = open(&executor, &request, Options { lossy: true });
    until(&mut session, |e| received(e).is_some()).await;
    session.control().send(text("flood")).unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    session.control().send(text("done?")).unwrap();
    let events = until(&mut session, |e| {
        received(e).is_some_and(|m| m.data == "done")
    })
    .await;
    let floods = messages(&events)
        .iter()
        .filter(|m| m.data.starts_with("flood "))
        .count();
    assert!(floods < 2000, "{floods}");
    assert!(
        events.iter().any(|e| matches!(&e.kind,
            EventKind::Warning { message } if message.starts_with("Skipped "))),
        "{events:#?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn event_streams_arrive_event_by_event() {
    let server = test_server::spawn().await;
    let (_dir, executor) = setup();
    let request = Request {
        kind: RequestKind::Sse,
        url: server.url("/sse"),
        auth: Auth::Bearer {
            token: "abc".into(),
            prefix: String::new(),
        },
        ..Request::default()
    };
    let mut session = open(&executor, &request, Options::default());
    let events = until(&mut session, is_closed).await;
    let EventKind::Connecting { headers, .. } = &events[0].kind else {
        panic!("{events:#?}")
    };
    assert_eq!(
        crate::http::find_header(headers, "accept"),
        Some("text/event-stream")
    );
    assert!(matches!(
        events[1].kind,
        EventKind::Open { status: 200, .. }
    ));
    let summary: Vec<_> = messages(&events)
        .iter()
        .map(|m| {
            (
                m.event.as_deref().unwrap_or_default(),
                m.data.as_str(),
                m.id.as_deref(),
                m.retry,
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("hello", "Bearer abc", None, None),
            ("message", r#"{"price": 42}"#, Some("1"), Some(2500)),
            ("message", "two\nlines", Some("1"), None),
        ]
    );
    assert_eq!(
        events.last().unwrap().kind,
        EventKind::Closed {
            code: None,
            reason: "The server ended the stream".into(),
            by_client: false
        }
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn event_streams_reconnect_with_the_last_id_and_stop() {
    let server = test_server::spawn().await;
    let (_dir, executor) = setup();
    let request = Request {
        kind: RequestKind::Sse,
        url: server.url("/sse/count"),
        sse: SseSettings { reconnect: true },
        ..Request::default()
    };
    let mut session = open(&executor, &request, Options::default());
    let mut count = 0;
    let events = until(&mut session, |e| {
        count += usize::from(received(e).is_some());
        count == 3
    })
    .await;
    let data: Vec<_> = messages(&events).iter().map(|m| m.data.as_str()).collect();
    assert_eq!(data, ["1", "2", "3"]);
    assert!(
        events.iter().any(|e| matches!(&e.kind,
            EventKind::Note { message } if message.ends_with("with Last-Event-ID 1"))),
        "{events:#?}"
    );
    session.control().close();
    let events = until(&mut session, is_closed).await;
    assert!(matches!(
        events.last().unwrap().kind,
        EventKind::Closed {
            by_client: true,
            ..
        }
    ));

    let request = Request {
        kind: RequestKind::Sse,
        url: server.url("/sse/forever"),
        ..Request::default()
    };
    let mut session = open(&executor, &request, Options::default());
    let mut count = 0;
    until(&mut session, |e| {
        count += usize::from(received(e).is_some());
        count == 3
    })
    .await;
    session.control().close();
    until(&mut session, is_closed).await;

    let request = Request {
        kind: RequestKind::Sse,
        url: server.url("/status/404"),
        ..Request::default()
    };
    let mut session = open(&executor, &request, Options::default());
    let events = until(&mut session, is_closed).await;
    assert!(
        events.iter().any(|e| matches!(&e.kind,
            EventKind::Error { message } if message.starts_with("The server answered 404 Not Found instead of an event stream"))),
        "{events:#?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn event_streams_answer_digest_challenges() {
    let server = test_server::spawn().await;
    let (_dir, executor) = setup();
    let request = Request {
        kind: RequestKind::Sse,
        url: server.url("/digest"),
        auth: Auth::Digest {
            username: "ada".into(),
            password: "lovelace".into(),
        },
        ..Request::default()
    };
    let mut session = open(&executor, &request, Options::default());
    let events = until(&mut session, is_closed).await;
    assert!(
        events
            .iter()
            .any(|e| matches!(e.kind, EventKind::Open { status: 200, .. })),
        "{events:#?}"
    );
}

/// Joins a namespace of the Socket.IO test server over `transport`, and
/// goes through events, acknowledgements, binary attachments and the
/// server's disconnect. Returns the events up to the first one received,
/// for the test to see how the connection was made.
async fn exercise_socket_io(
    addr: std::net::SocketAddr,
    transport: SocketIoTransport,
) -> Vec<StreamEvent> {
    let (_dir, executor) = setup();
    let request = Request {
        kind: RequestKind::Socketio,
        url: format!("http://{addr}"),
        variables: [("token".to_owned(), "secret".to_owned())]
            .into_iter()
            .collect(),
        socketio: SocketIoSettings {
            namespace: "/chat".into(),
            auth: r#"{"token": "{{token}}"}"#.into(),
            events: vec!["welcome".into(), "echo".into(), "file".into()],
            transport,
            ..SocketIoSettings::default()
        },
        ..Request::default()
    };
    let mut session = open(&executor, &request, Options::default());
    let events = until(&mut session, |e| received(e).is_some()).await;
    let joined = events.clone();
    let welcome = received(events.last().unwrap()).unwrap();
    assert_eq!(welcome.event.as_deref(), Some("welcome"));
    assert_eq!(welcome.data, r#"[{"hi":true}]"#);

    let control = session.control();
    control
        .send(Outbound::Emit(SavedEmit {
            name: "Echo".into(),
            event: "echo".into(),
            args: r#"["hi", 2]"#.into(),
            ack: true,
        }))
        .unwrap();
    let mut count = 0;
    let events = until(&mut session, |e| {
        count += usize::from(received(e).is_some());
        count == 2
    })
    .await;
    let summary: Vec<_> = messages(&events)
        .iter()
        .map(|m| (m.direction, m.event.as_deref(), m.data.as_str(), m.ack))
        .collect();
    assert_eq!(
        summary,
        [
            (Direction::Sent, Some("echo"), r#"["hi",2]"#, Some(0)),
            (Direction::Received, None, r#"["hi",2]"#, Some(0)),
            (Direction::Received, Some("echo"), r#"["hi",2]"#, None),
        ]
    );

    control
        .send(Outbound::Emit(SavedEmit {
            event: "binary".into(),
            ..SavedEmit::default()
        }))
        .unwrap();
    let events = until(&mut session, |e| received(e).is_some()).await;
    let file = received(events.last().unwrap()).unwrap();
    assert_eq!(file.event.as_deref(), Some("file"));
    assert_eq!(
        serde_json::from_str::<Value>(&file.data).unwrap(),
        json!([{"base64": "AQID"}])
    );

    control
        .send(Outbound::Emit(SavedEmit {
            event: "disconnect".into(),
            ..SavedEmit::default()
        }))
        .unwrap();
    until(
        &mut session,
        |e| matches!(&e.kind, EventKind::Warning { message } if message.contains("reserved")),
    )
    .await;

    // The server pings every 100 ms; answered pings keep the socket open.
    tokio::time::sleep(Duration::from_millis(600)).await;
    control
        .send(Outbound::Emit(SavedEmit {
            event: "kick".into(),
            ..SavedEmit::default()
        }))
        .unwrap();
    let events = until(&mut session, is_closed).await;
    assert!(
        !events
            .iter()
            .any(|e| matches!(e.kind, EventKind::Error { .. })),
        "{events:#?}"
    );
    assert_eq!(
        events.last().unwrap().kind,
        EventKind::Closed {
            code: None,
            reason: "The server disconnected the socket from /chat".into(),
            by_client: false
        }
    );
    joined
}

fn notes(events: &[StreamEvent]) -> Vec<&str> {
    events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::Note { message } => Some(message.as_str()),
            _ => None,
        })
        .collect()
}

fn opened_with(events: &[StreamEvent]) -> u16 {
    events
        .iter()
        .find_map(|e| match &e.kind {
            EventKind::Open { status, .. } => Some(*status),
            _ => None,
        })
        .expect("the connection never opened")
}

fn upgraded(event: &StreamEvent) -> bool {
    matches!(&event.kind, EventKind::Note { message } if message.starts_with("Upgraded"))
}

#[tokio::test(flavor = "multi_thread")]
async fn socket_io_over_websocket_only() {
    let server = test_socketio::server(Transports::Websocket).await;
    let events = exercise_socket_io(server.addr, SocketIoTransport::Websocket).await;
    assert_eq!(opened_with(&events), 101);
    assert_eq!(
        notes(&events),
        ["Joined namespace /chat as socket socket-1"]
    );
    // Long polling was never tried.
    assert_eq!(server.count("GET", "polling"), 0);
    assert_eq!(server.count("GET", "websocket"), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn socket_io_falls_back_to_websocket_when_polling_is_refused() {
    let server = test_socketio::server(Transports::Websocket).await;
    let events = exercise_socket_io(server.addr, SocketIoTransport::Auto).await;
    assert_eq!(opened_with(&events), 101);
    assert_eq!(
        notes(&events),
        [
            "Joined namespace /chat as socket socket-1",
            "The server does not accept long polling, so Getman connected over WebSocket"
        ]
    );
    assert_eq!(server.count("GET", "polling"), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn socket_io_works_over_long_polling_alone() {
    let server = test_socketio::server(Transports::Polling).await;
    let events = exercise_socket_io(server.addr, SocketIoTransport::Auto).await;
    assert_eq!(opened_with(&events), 200);
    assert_eq!(
        notes(&events),
        ["Joined namespace /chat as socket socket-1 over long polling"]
    );
    let seen = server.seen.lock().unwrap().clone();
    assert_eq!(server.count("GET", "websocket"), 0, "{seen:?}");
    assert!(server.count("POST", "polling") >= 1, "{seen:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn socket_io_upgrades_from_long_polling_to_websocket() {
    let server = test_socketio::server(Transports::Upgrading).await;
    let addr = server.addr;
    let (_dir, executor) = setup();
    let request = Request {
        kind: RequestKind::Socketio,
        url: format!("http://{addr}"),
        socketio: SocketIoSettings {
            auth: r#"{"token": "secret"}"#.into(),
            events: vec!["welcome".into(), "echo".into()],
            ..SocketIoSettings::default()
        },
        ..Request::default()
    };
    let mut session = open(&executor, &request, Options::default());
    let events = until(&mut session, upgraded).await;
    assert_eq!(opened_with(&events), 200);
    assert_eq!(
        notes(&events),
        [
            "Joined namespace / as socket socket-1 over long polling",
            "Upgraded from long polling to WebSocket"
        ]
    );
    assert_eq!(server.count("GET", "websocket"), 1);

    // Events go both ways on the WebSocket now, and the pings go on.
    session
        .control()
        .send(Outbound::Emit(SavedEmit {
            event: "echo".into(),
            args: r#"["after"]"#.into(),
            ..SavedEmit::default()
        }))
        .unwrap();
    until(&mut session, |e| {
        received(e).is_some_and(|m| m.event.as_deref() == Some("echo") && m.data == r#"["after"]"#)
    })
    .await;
    tokio::time::sleep(Duration::from_millis(1200)).await;
    session.control().close();
    let events = until(&mut session, is_closed).await;
    assert!(
        !events
            .iter()
            .any(|e| matches!(e.kind, EventKind::Error { .. })),
        "{events:#?}"
    );
    // The poll that was open when the upgrade began has ended: nothing is
    // asked of the long-polling endpoint any more.
    let polls = server.count("GET", "polling");
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(server.count("GET", "polling"), polls);

    // The same server, asked to stay on long polling.
    let events = exercise_socket_io(addr, SocketIoTransport::Polling).await;
    assert_eq!(
        notes(&events),
        ["Joined namespace /chat as socket socket-1 over long polling"]
    );
    // And asked for WebSocket only.
    let events = exercise_socket_io(addr, SocketIoTransport::Websocket).await;
    assert_eq!(opened_with(&events), 101);
}

#[tokio::test(flavor = "multi_thread")]
async fn socket_io_stays_on_long_polling_when_the_upgrade_fails() {
    let server = test_socketio::server(Transports::BrokenUpgrade).await;
    let events = exercise_socket_io(server.addr, SocketIoTransport::Auto).await;
    assert_eq!(opened_with(&events), 200);
    let warned = events.iter().any(|e| {
        matches!(&e.kind,
            EventKind::Warning { message } if message.starts_with("Could not upgrade to WebSocket") && message.ends_with("staying on long polling"))
    });
    assert!(warned, "{events:#?}");
    assert_eq!(server.count("GET", "websocket"), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn socket_io_sends_and_stores_cookies_unless_told_not_to() {
    let server = test_socketio::server(Transports::Upgrading).await;
    let (_dir, executor) = setup();
    let request = Request {
        kind: RequestKind::Socketio,
        url: format!("http://{}", server.addr),
        socketio: SocketIoSettings {
            auth: r#"{"token": "secret"}"#.into(),
            ..SocketIoSettings::default()
        },
        ..Request::default()
    };
    let mut session = open(&executor, &request, Options::default());
    until(&mut session, upgraded).await;
    session.control().close();
    until(&mut session, is_closed).await;

    // The cookie the handshake set came back on every later request, the
    // WebSocket upgrade's included, and is in the jar.
    let seen = server.seen.lock().unwrap().clone();
    assert_eq!(seen[0], "GET polling cookie=", "{seen:?}");
    assert!(
        seen[1..]
            .iter()
            .all(|line| line.ends_with("cookie=io=engine-1")),
        "{seen:?}"
    );
    assert!(
        seen.iter().any(|l| l.starts_with("GET websocket")),
        "{seen:?}"
    );
    assert_eq!(executor.http.cookies()[0].name, "io");

    // With `cookies: false` it is neither sent nor kept.
    executor.http.clear_cookies();
    server.seen.lock().unwrap().clear();
    let mut request = request;
    request.settings.cookies = Some(false);
    let mut session = open(&executor, &request, Options::default());
    until(&mut session, upgraded).await;
    session.control().close();
    until(&mut session, is_closed).await;
    let seen = server.seen.lock().unwrap().clone();
    assert!(
        seen.iter().all(|line| line.ends_with("cookie=")),
        "{seen:?}"
    );
    assert_eq!(executor.http.cookies(), []);
}

#[tokio::test(flavor = "multi_thread")]
async fn socket_io_reports_a_refused_join() {
    let addr = test_socketio::websocket_only().await;
    let (_dir, executor) = setup();
    let request = Request {
        kind: RequestKind::Socketio,
        url: format!("ws://{addr}/"),
        socketio: SocketIoSettings {
            auth: r#"{"token": "wrong"}"#.into(),
            ..SocketIoSettings::default()
        },
        ..Request::default()
    };
    let mut session = open(&executor, &request, Options::default());
    let events = until(&mut session, is_closed).await;
    assert!(
        events.iter().any(|e| matches!(&e.kind,
            EventKind::Error { message } if message == "The server refused to let Getman join /: bad token")),
        "{events:#?}"
    );

    let request = Request {
        socketio: SocketIoSettings {
            auth: "not json".into(),
            ..SocketIoSettings::default()
        },
        ..request
    };
    let mut session = open(&executor, &request, Options::default());
    let events = until(&mut session, is_closed).await;
    assert!(
        events.iter().any(|e| matches!(&e.kind,
            EventKind::Error { message } if message.starts_with("The Socket.IO auth payload must be a JSON object"))),
        "{events:#?}"
    );
}

/// A WebSocket request to the echo server that goes through `proxy`.
fn through(addr: std::net::SocketAddr, host: &str, proxy: Option<Proxy>) -> Request {
    Request {
        kind: RequestKind::Websocket,
        url: format!("ws://{host}:{}/chat", addr.port()),
        settings: Settings {
            proxy,
            ..Settings::default()
        },
        ..Request::default()
    }
}

fn proxy(url: String, bypass: &str) -> Proxy {
    Proxy {
        url,
        bypass: bypass.into(),
        ..Proxy::default()
    }
}

fn theme_cookie() -> crate::http::CookieInfo {
    crate::http::CookieInfo {
        domain: "127.0.0.1".into(),
        path: "/".into(),
        name: "theme".into(),
        value: "dark".into(),
        expires: None,
        secure: false,
        http_only: false,
        host_only: true,
    }
}

/// The server's greeting: what its handshake carried.
async fn greeting(session: &mut Session) -> Value {
    let events = until(session, |e| received(e).is_some()).await;
    serde_json::from_str(&received(events.last().unwrap()).unwrap().data).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn websocket_goes_through_an_http_proxy_with_credentials() {
    let server = test_servers::websocket().await;
    let proxy_server = test_proxies::connect_proxy(Some(("ada", "p@ss"))).await;
    let (_dir, executor) = setup();

    // The credentials in the proxy URL are percent-encoded, as in HTTP.
    let request = through(
        server,
        "127.0.0.1",
        Some(proxy(
            format!("http://ada:p%40ss@{}", proxy_server.addr),
            "",
        )),
    );
    let mut session = open(&executor, &request, Options::default());
    assert_eq!(greeting(&mut session).await["path"], "/chat");
    session.control().send(text("through")).unwrap();
    until(&mut session, |e| {
        received(e).is_some_and(|m| m.data == "through")
    })
    .await;
    session.control().close();
    until(&mut session, is_closed).await;
    let seen = proxy_server.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1, "{seen:?}");
    // ada:p@ss
    assert_eq!(
        seen[0],
        format!(
            "CONNECT 127.0.0.1:{} HTTP/1.1 | Basic YWRhOnBAc3M=",
            server.port()
        )
    );

    // Without them the proxy refuses, and the message says why.
    let request = through(
        server,
        "127.0.0.1",
        Some(proxy(format!("http://{}", proxy_server.addr), "")),
    );
    let mut session = open(&executor, &request, Options::default());
    let events = until(&mut session, is_closed).await;
    assert!(
        events.iter().any(|e| matches!(
            &e.kind,
            EventKind::Error { message } if message.contains("407") && message.contains("user name and password")
        )),
        "{events:#?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn websocket_goes_through_socks5_and_socks5h() {
    let server = test_servers::websocket().await;
    let proxy_server = test_proxies::socks5_proxy(Some(("bob", "hunter2")), server).await;
    let (_dir, executor) = setup();

    // socks5h leaves the name to the proxy, which knows where it leads.
    let request = through(
        server,
        "chat.invalid",
        Some(proxy(
            format!("socks5h://bob:hunter2@{}", proxy_server.addr),
            "",
        )),
    );
    let mut session = open(&executor, &request, Options::default());
    assert_eq!(greeting(&mut session).await["path"], "/chat");
    assert_eq!(
        proxy_server.seen.lock().unwrap().clone(),
        [format!("name chat.invalid:{}", server.port())]
    );

    // socks5 resolves the name first, so the proxy is given an address.
    let request = through(
        server,
        "localhost",
        Some(proxy(
            format!("socks5://bob:hunter2@{}", proxy_server.addr),
            "",
        )),
    );
    let mut session = open(&executor, &request, Options::default());
    assert_eq!(greeting(&mut session).await["path"], "/chat");
    let seen = proxy_server.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 2, "{seen:?}");
    assert!(seen[1].starts_with("address 127.0.0.1:"), "{seen:?}");

    // Wrong credentials are refused.
    let request = through(
        server,
        "127.0.0.1",
        Some(proxy(
            format!("socks5://bob:wrong@{}", proxy_server.addr),
            "",
        )),
    );
    let mut session = open(&executor, &request, Options::default());
    let events = until(&mut session, is_closed).await;
    assert!(
        events.iter().any(|e| matches!(
            &e.kind,
            EventKind::Error { message } if message.contains("refused the user name")
        )),
        "{events:#?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn websocket_skips_the_proxy_when_off_or_bypassed() {
    let server = test_servers::websocket().await;
    // A proxy that would fail every request.
    let proxy_server = test_proxies::connect_proxy(Some(("nobody", "nothing"))).await;
    let (_dir, executor) = setup();
    let url = format!("http://{}", proxy_server.addr);

    for settings in [
        Some(Proxy {
            url: "off".into(),
            ..Proxy::default()
        }),
        Some(proxy(url.clone(), "127.0.0.1, .internal")),
    ] {
        let request = through(server, "127.0.0.1", settings);
        let mut session = open(&executor, &request, Options::default());
        assert_eq!(greeting(&mut session).await["path"], "/chat");
    }
    assert_eq!(
        proxy_server.seen.lock().unwrap().clone(),
        Vec::<String>::new()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn websocket_sends_and_stores_cookies_unless_told_not_to() {
    let server = test_servers::websocket().await;
    let (_dir, executor) = setup();
    let mut request = through(server, "127.0.0.1", None);
    request.url = format!("ws://127.0.0.1:{}/session", server.port());

    // The handshake sets a cookie, which the jar keeps.
    let mut session = open(&executor, &request, Options::default());
    assert_eq!(greeting(&mut session).await["cookie"], "");
    let cookies = executor.http.cookies();
    assert_eq!(cookies.len(), 1, "{cookies:?}");
    assert_eq!(
        (cookies[0].name.as_str(), cookies[0].value.as_str()),
        ("session", "abc123")
    );

    // The next connection sends it, as does a cookie added by hand.
    let mut session = open(&executor, &request, Options::default());
    assert_eq!(greeting(&mut session).await["cookie"], "session=abc123");
    executor.http.set_cookie(&theme_cookie()).unwrap();
    let mut session = open(&executor, &request, Options::default());
    let cookie = greeting(&mut session).await["cookie"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        cookie.contains("session=abc123") && cookie.contains("theme=dark"),
        "{cookie}"
    );

    // A Cookie header of the request itself wins over the jar.
    request.headers.push(KeyValue::new("Cookie", "mine=1"));
    let mut session = open(&executor, &request, Options::default());
    assert_eq!(greeting(&mut session).await["cookie"], "mine=1");
    request.headers.clear();

    // `cookies: false` neither sends nor stores.
    executor.http.clear_cookies();
    request.settings.cookies = Some(false);
    let mut session = open(&executor, &request, Options::default());
    assert_eq!(greeting(&mut session).await["cookie"], "");
    assert_eq!(executor.http.cookies(), []);
    executor.http.set_cookie(&theme_cookie()).unwrap();
    let mut session = open(&executor, &request, Options::default());
    assert_eq!(greeting(&mut session).await["cookie"], "");
}

fn passed(results: &[crate::checks::TestResult]) -> bool {
    results.iter().all(|r| r.passed)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_websocket_request_runs_with_scripts_checks_and_captures() {
    let addr = test_servers::websocket().await;
    let (_dir, executor) = setup();
    let request = Request {
        kind: RequestKind::Websocket,
        url: format!("ws://{addr}/{{{{room}}}}"),
        scripts: Scripts {
            pre_request: "gm.variables.set('room', 'lobby');\n\
                gm.request.headers.add({ key: 'X-Test', value: 'from script' });"
                .into(),
            post_response: r"
                gm.test('the handshake is the response', () => {
                  gm.expect(gm.response.code).to.equal(101);
                  gm.expect(gm.response.headers.get('upgrade').toLowerCase()).to.equal('websocket');
                });
                gm.test('the messages arrived', () => {
                  gm.expect(gm.messages.length).to.equal(3);
                  gm.expect(gm.messages.json(0).x_test).to.equal('from script');
                  gm.expect(gm.messages[1]).to.include({ direction: 'received', data: 'one' });
                  gm.expect(gm.messages[1].time).to.be.a('number');
                });
                gm.test('data that is not JSON says so', () => {
                  gm.expect(() => gm.messages.json(1)).to.throw('message 1 is not JSON');
                });
                gm.collectionVariables.set('last', gm.messages[2].data);
            "
            .into(),
        },
        checks: vec![
            "status == 101".into(),
            "messages.length == 3".into(),
            "messages[0].json.path == /lobby".into(),
            "messages[0].json.x_test == from script".into(),
            "messages[1].data == one".into(),
            "messages[-1].data == two".into(),
            "messages[1].direction == received".into(),
            "messages[5] !exists".into(),
        ],
        captures: [("echoed".to_owned(), "messages[2].data".to_owned())]
            .into_iter()
            .collect(),
        websocket: WebSocketSettings {
            subprotocols: Vec::new(),
            messages: vec![
                SavedMessage {
                    content: "one".into(),
                    ..SavedMessage::default()
                },
                SavedMessage {
                    content: "two".into(),
                    ..SavedMessage::default()
                },
            ],
        },
        until: Until {
            messages: Some(3),
            time_ms: None,
        },
        ..Request::default()
    };
    let mut context = Context::default();
    let execution = executor.execute(None, &request, &mut context).await;
    assert_eq!(execution.error, None, "{execution:#?}");
    assert!(!execution.skipped);
    assert_eq!(execution.script_errors, []);
    assert!(
        passed(&execution.checks) && execution.checks.len() == 8,
        "{:#?}",
        execution.checks
    );
    assert!(
        passed(&execution.tests) && execution.tests.len() == 3,
        "{:#?}",
        execution.tests
    );
    let response = execution.response.as_ref().unwrap();
    assert_eq!(
        (response.status, response.status_text.as_str()),
        (101, "Switching Protocols")
    );
    assert_eq!(
        execution.request.as_ref().unwrap().url,
        format!("ws://{addr}/lobby")
    );
    assert_eq!(
        context.overlay.collection.get("last"),
        Some(&Some("two".to_owned()))
    );
    assert_eq!(
        context.overlay.collection.get("echoed"),
        Some(&Some("two".to_owned()))
    );

    // The whole conversation is kept for the report, sent messages too.
    let call = execution.realtime.as_ref().unwrap();
    let sent = call
        .messages
        .iter()
        .filter(|m| m.direction == Direction::Sent)
        .count();
    assert_eq!(sent, 2);
    assert_eq!(call.messages.len(), 5);
    assert!(call.closed.as_ref().is_some_and(|c| c.by_client));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failing_check_or_script_on_a_connection_fails_the_request() {
    let addr = test_servers::websocket().await;
    let (_dir, executor) = setup();
    let mut request = Request {
        kind: RequestKind::Websocket,
        url: format!("ws://{addr}/"),
        checks: vec!["messages.length > 5".into()],
        until: Until {
            messages: Some(1),
            time_ms: None,
        },
        ..Request::default()
    };
    let execution = executor
        .execute(None, &request, &mut Context::default())
        .await;
    assert!(execution.failed());
    let message = execution.checks[0].message.as_deref().unwrap();
    assert!(message.contains("messages.length is 1"), "{message}");

    // A script that throws before connecting stops it.
    request.scripts.pre_request = "throw new Error('no');".into();
    let execution = executor
        .execute(None, &request, &mut Context::default())
        .await;
    assert!(execution.failed());
    assert_eq!(execution.response, None);
    assert_eq!(execution.realtime, None);

    // And one that skips the request.
    request.scripts.pre_request = "gm.execution.skipRequest();".into();
    let execution = executor
        .execute(None, &request, &mut Context::default())
        .await;
    assert!(execution.skipped && !execution.failed());

    // A server that cannot be reached is an error, without scripts or checks.
    request.scripts.pre_request.clear();
    request.url = "ws://127.0.0.1:1/".into();
    let execution = executor
        .execute(None, &request, &mut Context::default())
        .await;
    assert!(execution.error.is_some());
    assert_eq!(execution.checks, []);
}

#[tokio::test(flavor = "multi_thread")]
async fn connections_keep_session_variables_and_obey_the_scripts_switch() {
    let addr = test_servers::websocket().await;
    let (_dir, executor) = setup();
    let request = Request {
        kind: RequestKind::Websocket,
        url: format!("ws://{addr}/"),
        scripts: Scripts {
            pre_request: "gm.session.set('seen', gm.session.get('seen') + 'a');".into(),
            post_response: "gm.session.set('seen', gm.session.get('seen') + 'b');".into(),
        },
        until: Until {
            messages: Some(1),
            time_ms: None,
        },
        ..Request::default()
    };

    // Session variables carry from one connection to the next.
    executor.clear_session();
    let mut context = Context::default();
    context
        .overlay
        .session
        .insert("seen".into(), Some(String::new()));
    let execution = executor.execute(None, &request, &mut context).await;
    assert!(!execution.failed(), "{execution:#?}");
    let execution = executor
        .execute(None, &request, &mut Context::default())
        .await;
    assert!(!execution.failed(), "{execution:#?}");
    assert_eq!(
        executor.session().get("seen").map(String::as_str),
        Some("abab")
    );

    // With scripts off, neither script runs and the result says so once.
    let context = &mut Context {
        no_scripts: true,
        ..Context::default()
    };
    let execution = executor.execute(None, &request, context).await;
    assert!(!execution.failed(), "{execution:#?}");
    assert!(execution.realtime.is_some());
    assert_eq!(
        executor.session().get("seen").map(String::as_str),
        Some("abab")
    );
    let notes: Vec<_> = execution
        .warnings
        .iter()
        .filter(|w| w.starts_with("Scripts are turned off"))
        .collect();
    assert_eq!(notes.len(), 1, "{:?}", execution.warnings);
    assert!(notes[0].contains("2 scripts"), "{}", notes[0]);
}

#[tokio::test(flavor = "multi_thread")]
async fn event_streams_and_socket_io_run_too() {
    let server = test_server::spawn().await;
    let (_dir, executor) = setup();
    let events = Request {
        kind: RequestKind::Sse,
        url: server.url("/sse"),
        auth: Auth::Bearer {
            token: "abc".into(),
            prefix: String::new(),
        },
        checks: vec![
            "status == 200".into(),
            "messages.length == 3".into(),
            "messages[0].event == hello".into(),
            "messages[0].data == Bearer abc".into(),
            "messages[1].json.price == 42".into(),
            "messages[1].id == 1".into(),
        ],
        ..Request::default()
    };
    let execution = executor
        .execute(None, &events, &mut Context::default())
        .await;
    assert!(
        !execution.failed() && execution.checks.len() == 6,
        "{execution:#?}"
    );

    // Without a limit of its own, a stream that never ends is left after
    // five seconds or ten messages, whichever comes first: ten here.
    let forever = Request {
        url: server.url("/sse/forever"),
        checks: vec!["messages.length == 10".into()],
        ..Request::default()
    };
    let forever = Request {
        kind: RequestKind::Sse,
        ..forever
    };
    let execution = executor
        .execute(None, &forever, &mut Context::default())
        .await;
    assert!(!execution.failed(), "{execution:#?}");

    // A time limit ends it, however many messages came.
    let timed = Request {
        until: Until {
            messages: None,
            time_ms: Some(300),
        },
        checks: vec!["messages.length > 3".into()],
        ..forever
    };
    let started = std::time::Instant::now();
    let execution = executor
        .execute(None, &timed, &mut Context::default())
        .await;
    assert!(!execution.failed(), "{execution:#?}");
    assert!(started.elapsed() < Duration::from_secs(3));

    let socket = test_socketio::server(Transports::Upgrading).await;
    let request = Request {
        kind: RequestKind::Socketio,
        url: format!("http://{}", socket.addr),
        socketio: SocketIoSettings {
            auth: r#"{"token": "secret"}"#.into(),
            events: vec!["welcome".into(), "echo".into()],
            messages: vec![SavedEmit {
                event: "echo".into(),
                args: r#"["hi", 2]"#.into(),
                ..SavedEmit::default()
            }],
            ..SocketIoSettings::default()
        },
        until: Until {
            messages: Some(2),
            time_ms: None,
        },
        checks: vec![
            "status == 200".into(),
            "messages[0].event == welcome".into(),
            "messages[0].json[0].hi == true".into(),
            "messages[1].event == echo".into(),
            "messages[1].json == [\"hi\",2]".into(),
        ],
        scripts: Scripts {
            pre_request: String::new(),
            post_response:
                "gm.test('events', () => gm.expect(gm.messages.json(1)[1]).to.equal(2));".into(),
        },
        ..Request::default()
    };
    let execution = executor
        .execute(None, &request, &mut Context::default())
        .await;
    assert!(!execution.failed(), "{execution:#?}");
    assert_eq!(execution.tests.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_runner_connects_to_realtime_requests_instead_of_skipping_them() {
    let addr = test_servers::websocket().await;
    let dir = tempfile::tempdir().unwrap();
    let workspace = Workspace::init(dir.path(), "Run").unwrap();
    let executor = Executor::new(workspace);
    executor
        .workspace
        .create_request(
            "",
            "Chat",
            &Request {
                kind: RequestKind::Websocket,
                url: format!("ws://{addr}/"),
                checks: vec!["status == 101".into(), "messages.length == 2".into()],
                websocket: WebSocketSettings {
                    subprotocols: Vec::new(),
                    messages: vec![SavedMessage {
                        content: "hi".into(),
                        ..SavedMessage::default()
                    }],
                },
                until: Until {
                    messages: Some(2),
                    time_ms: None,
                },
                ..Request::default()
            },
        )
        .unwrap();
    let report = crate::runner::run(
        &executor,
        &crate::runner::RunOptions::default(),
        &std::sync::atomic::AtomicBool::new(false),
        |_| {},
    )
    .await
    .unwrap();
    let summary = report.summary;
    assert_eq!((summary.requests, summary.skipped_requests), (1, 0));
    assert_eq!(
        (summary.checks_passed, summary.checks_failed),
        (2, 0),
        "{summary:?}"
    );
    assert!(summary.passed());
}

#[test]
fn until_has_defaults_that_never_wait_for_ever() {
    let none = Until::default().limits();
    assert_eq!(
        (none.messages, none.time),
        (Some(10), Some(Duration::from_secs(5)))
    );
    let count = Until {
        messages: Some(3),
        time_ms: None,
    }
    .limits();
    assert_eq!(
        (count.messages, count.time),
        (Some(3), Some(Duration::from_secs(60)))
    );
    let time = Until {
        messages: None,
        time_ms: Some(1500),
    }
    .limits();
    assert_eq!(
        (time.messages, time.time),
        (None, Some(Duration::from_millis(1500)))
    );
    let request: Request =
        serde_yaml_ng::from_str("kind: sse\nurl: x\nuntil:\n  messages: 4\n").unwrap();
    assert_eq!(request.until.messages, Some(4));
    let written = serde_yaml_ng::to_string(&request).unwrap();
    assert!(
        written.contains("until:") && !written.contains("time_ms"),
        "{written}"
    );
    assert!(
        !serde_yaml_ng::to_string(&Request::default())
            .unwrap()
            .contains("until")
    );
}

#[test]
fn events_serialize_for_the_ui() {
    let mut message = Message::new(Direction::Received, vec![0, 255], true);
    message.event = Some("x".into());
    let event = StreamEvent {
        at: 5,
        kind: EventKind::Message(message),
    };
    assert_eq!(
        serde_json::to_value(&event).unwrap(),
        json!({
            "at": 5,
            "type": "message",
            "direction": "received",
            "data": "AP8=",
            "binary": true,
            "size": 2,
            "event": "x",
        })
    );
    let long = Message::text(Direction::Sent, "é".repeat(MAX_SHOWN_BYTES));
    assert!(long.truncated);
    assert_eq!(long.size, 2 * MAX_SHOWN_BYTES as u64);
    assert_eq!(long.data.len(), MAX_SHOWN_BYTES);

    let outbound: Outbound =
        serde_json::from_value(json!({"emit": {"event": "a", "args": "[1]", "ack": true}}))
            .unwrap();
    assert!(matches!(
        outbound,
        Outbound::Emit(SavedEmit { ack: true, .. })
    ));
}
