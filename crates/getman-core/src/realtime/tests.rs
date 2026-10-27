use std::time::Duration;

use serde_json::{Value, json};

use super::*;
use crate::model::KeyValue;
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

#[tokio::test(flavor = "multi_thread")]
async fn socket_io_joins_emits_and_listens() {
    let addr = test_servers::socketio().await;
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
            ..SocketIoSettings::default()
        },
        ..Request::default()
    };
    let mut session = open(&executor, &request, Options::default());
    let events = until(&mut session, |e| received(e).is_some()).await;
    assert!(matches!(
        events[1].kind,
        EventKind::Open { status: 101, .. }
    ));
    assert_eq!(
        events[2].kind,
        EventKind::Note {
            message: "Joined namespace /chat as socket socket-1".into()
        }
    );
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
}

#[tokio::test(flavor = "multi_thread")]
async fn socket_io_reports_a_refused_join() {
    let addr = test_servers::socketio().await;
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
