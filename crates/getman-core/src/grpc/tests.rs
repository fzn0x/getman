//! gRPC calls end to end, against the tonic server in `test_server`.

use std::sync::atomic::AtomicBool;

use serde_json::json;
use tokio::sync::{mpsc, oneshot};

use super::test_server::{self, PROTO, PROTO_PATH, Reflection};
use super::{CallType, Command, Direction, Live, LiveEvent, grpcurl, load_schema};
use crate::execute::{Context, Execution, Executor};
use crate::model::{Auth, Grpc, KeyValue, Request, RequestKind, Scripts, Settings};
use crate::runner::{self, RunEvent, RunOptions};
use crate::workspace::Workspace;

pub(super) fn setup() -> (tempfile::TempDir, Executor) {
    let dir = tempfile::tempdir().unwrap();
    let workspace = Workspace::init(dir.path(), "gRPC").unwrap();
    let proto = dir.path().join(PROTO_PATH);
    std::fs::create_dir_all(proto.parent().unwrap()).unwrap();
    std::fs::write(proto, PROTO).unwrap();
    (dir, Executor::new(workspace))
}

pub(super) fn call(address: &str, method: &str, message: &str) -> Request {
    Request {
        name: method.into(),
        kind: RequestKind::Grpc,
        method: format!("getman.test.Echo/{method}"),
        url: address.into(),
        grpc: Grpc {
            proto: PROTO_PATH.into(),
            tls: false,
            message: message.into(),
        },
        ..Request::default()
    }
}

pub(super) async fn run(executor: &Executor, request: &Request) -> Execution {
    executor
        .execute(None, request, &mut Context::default())
        .await
}

pub(super) fn received(execution: &Execution) -> Vec<serde_json::Value> {
    execution
        .grpc
        .as_ref()
        .unwrap()
        .messages
        .iter()
        .filter(|m| m.direction == Direction::Received)
        .map(|m| m.message.clone())
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_unary_call_sends_metadata_and_feeds_checks_scripts_and_captures() {
    let server = test_server::spawn(Reflection::None).await;
    let (_dir, executor) = setup();
    let request = Request {
        headers: vec![
            KeyValue::new("X-Trace", "{{trace}}"),
            KeyValue::new("trace-bin", "AQID"),
        ],
        auth: Auth::Bearer {
            token: "t0k3n".into(),
            prefix: String::new(),
        },
        variables: [
            ("who".to_owned(), "ada".to_owned()),
            ("trace".to_owned(), "abc".to_owned()),
        ]
        .into_iter()
        .collect(),
        checks: vec![
            "status == 0".into(),
            "json.text == ada".into(),
            "json.index == 42".into(),
            "header.x-served-by == getman-test".into(),
            "header.grpc-status == 0".into(),
        ],
        captures: [("echoed".to_owned(), "json.text".to_owned())]
            .into_iter()
            .collect(),
        scripts: Scripts {
            pre_request: String::new(),
            post_response:
                "gm.test('sees the message', () => gm.expect(gm.response.json().text).to.equal('ada'));"
                    .into(),
        },
        ..call(
            &server.address,
            "Unary",
            r#"{"text": "{{who}}", "at": "1970-01-01T00:00:42Z"}"#,
        )
    };
    let execution = run(&executor, &request).await;
    assert!(!execution.failed(), "{execution:#?}");
    assert_eq!(execution.checks.len(), 5);
    assert_eq!(execution.tests.len(), 1);
    assert_eq!(execution.changes[0].value.as_deref(), Some("ada"));

    let grpc = execution.grpc.as_ref().unwrap();
    assert_eq!(grpc.call_type, CallType::Unary);
    assert_eq!(grpc.status.as_ref().unwrap().name, "OK");
    assert!(grpc.headers.iter().any(|h| h.name == "x-served-by"));
    assert!(
        grpc.trailers
            .iter()
            .any(|h| h.name == "grpc-status" && h.value == "0")
    );
    assert_eq!(grpc.messages[0].direction, Direction::Sent);
    assert_eq!(grpc.messages[0].message["text"], "ada");
    let reply = &received(&execution)[0];
    assert_eq!(reply["metadata"]["x-trace"], "abc");
    assert_eq!(reply["metadata"]["trace-bin"], "AQID");
    assert_eq!(reply["metadata"]["authorization"], "Bearer t0k3n");

    let sent = execution.request.as_ref().unwrap();
    assert_eq!(sent.method, "GRPC");
    assert!(
        sent.url.ends_with("/getman.test.Echo/Unary"),
        "{}",
        sent.url
    );
    let response = execution.response.as_ref().unwrap();
    assert_eq!((response.status, response.status_text.as_str()), (0, "OK"));
}

#[tokio::test(flavor = "multi_thread")]
async fn pre_request_scripts_edit_the_message_and_metadata() {
    let server = test_server::spawn(Reflection::None).await;
    let (_dir, executor) = setup();
    let request = Request {
        scripts: Scripts {
            pre_request: r"
                gm.request.body.update({ text: JSON.parse(gm.request.body.raw).text + ' edited' });
                gm.request.headers.add({ key: 'x-from-script', value: 'yes' });
            "
            .into(),
            post_response: String::new(),
        },
        ..call(&server.address, "Unary", r#"{"text": "message"}"#)
    };
    let execution = run(&executor, &request).await;
    assert!(!execution.failed(), "{execution:#?}");
    let reply = &received(&execution)[0];
    assert_eq!(reply["text"], "message edited");
    assert_eq!(reply["metadata"]["x-from-script"], "yes");
}

#[tokio::test(flavor = "multi_thread")]
async fn server_streams_arrive_as_a_list() {
    let server = test_server::spawn(Reflection::None).await;
    let (_dir, executor) = setup();
    let request = Request {
        checks: vec!["json.length == 3".into(), "json[2].index == 2".into()],
        ..call(
            &server.address,
            "ServerStream",
            r#"{"text": "hi", "times": 3}"#,
        )
    };
    let execution = run(&executor, &request).await;
    assert!(!execution.failed(), "{execution:#?}");
    let replies = received(&execution);
    assert_eq!(replies.len(), 3);
    let grpc = execution.grpc.as_ref().unwrap();
    assert_eq!(grpc.call_type, CallType::ServerStreaming);
    assert!(
        grpc.messages
            .windows(2)
            .all(|w| w[0].time_ms <= w[1].time_ms)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn client_and_bidi_streams_send_every_saved_message() {
    let server = test_server::spawn(Reflection::None).await;
    let (_dir, executor) = setup();
    let messages = "{\"text\": \"a\"}\n{\"text\": \"b\"}\n{\"text\": \"c\"}";
    let execution = run(&executor, &call(&server.address, "ClientStream", messages)).await;
    assert!(execution.error.is_none(), "{execution:#?}");
    assert_eq!(
        received(&execution),
        [json!({"text": "a b c", "index": 3, "metadata": {}})]
    );

    let execution = run(&executor, &call(&server.address, "Bidi", messages)).await;
    assert!(execution.error.is_none(), "{execution:#?}");
    let texts: Vec<_> = received(&execution)
        .iter()
        .map(|m| m["text"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(texts, ["A", "B", "C"]);
    let body = execution.response.as_ref().unwrap().json().unwrap();
    assert_eq!(body.as_array().unwrap().len(), 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn errors_carry_their_details_and_metadata() {
    let server = test_server::spawn(Reflection::None).await;
    let (_dir, executor) = setup();
    let request = Request {
        checks: vec!["status == 9".into(), "header.x-failed == yes".into()],
        ..call(
            &server.address,
            "Fail",
            r#"{"code": 9, "message": "out of stock"}"#,
        )
    };
    let execution = run(&executor, &request).await;
    assert!(!execution.failed(), "{execution:#?}");
    let status = execution.grpc.as_ref().unwrap().status.clone().unwrap();
    assert_eq!(status.code, 9);
    assert_eq!(status.name, "FAILED_PRECONDITION");
    assert_eq!(status.message, "out of stock");
    let details = status.details.unwrap();
    assert_eq!(details["details"][0]["reason"], "ASKED_TO_FAIL");
    assert_eq!(details["details"][0]["metadata"]["code"], "9");

    let execution = run(
        &executor,
        &call(
            &server.address,
            "ServerStream",
            r#"{"text": "x", "times": -1}"#,
        ),
    )
    .await;
    let grpc = execution.grpc.as_ref().unwrap();
    assert_eq!(received(&execution).len(), 1);
    assert_eq!(grpc.status.as_ref().unwrap().code, 9);
    assert!(grpc.trailers.iter().any(|h| h.name == "x-why"));
}

#[tokio::test(flavor = "multi_thread")]
async fn reflection_finds_the_schema_in_v1_and_v1alpha() {
    for reflection in [Reflection::V1, Reflection::V1Alpha] {
        let server = test_server::spawn(reflection).await;
        let (_dir, executor) = setup();
        let mut request = call(&server.address, "Unary", r#"{"text": "reflected"}"#);
        request.grpc.proto.clear();
        let schema = load_schema(&executor, None, &request, &Context::default(), true)
            .await
            .unwrap();
        let echo = schema
            .services()
            .into_iter()
            .find(|s| s.name == "getman.test.Echo")
            .unwrap();
        assert_eq!(echo.methods.len(), 6);
        let execution = run(&executor, &request).await;
        assert!(execution.error.is_none(), "{reflection:?}: {execution:#?}");
        assert_eq!(received(&execution)[0]["text"], "reflected");
    }

    let server = test_server::spawn(Reflection::None).await;
    let (_dir, executor) = setup();
    let mut request = call(&server.address, "Unary", "{}");
    request.grpc.proto.clear();
    let execution = run(&executor, &request).await;
    let error = execution.error.unwrap();
    assert!(error.contains("does not offer reflection"), "{error}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_deadline_ends_slow_calls() {
    let server = test_server::spawn(Reflection::None).await;
    let (_dir, executor) = setup();
    let request = Request {
        settings: Settings {
            timeout_ms: Some(200),
            ..Settings::default()
        },
        ..call(&server.address, "Slow", "{}")
    };
    let execution = run(&executor, &request).await;
    let grpc = execution.grpc.as_ref().unwrap();
    let code = grpc.status.as_ref().unwrap().code;
    assert!(matches!(code, 1 | 4), "{grpc:#?}");
    assert!(grpc.time_ms < 2000.0, "{}", grpc.time_ms);
}

#[tokio::test(flavor = "multi_thread")]
async fn live_calls_send_one_message_at_a_time() {
    let server = test_server::spawn(Reflection::None).await;
    let (_dir, executor) = setup();
    let (commands, receiver) = mpsc::unbounded_channel();
    let (events, mut seen) = mpsc::unbounded_channel();
    let request = call(&server.address, "Bidi", r#"{"text": "not sent"}"#);
    let task = tokio::spawn(async move {
        executor
            .execute_live(
                None,
                &request,
                &mut Context::default(),
                Live {
                    commands: receiver,
                    events,
                },
            )
            .await
    });

    let send = |text: &str| {
        let (reply, answer) = oneshot::channel();
        commands
            .send(Command::Send {
                text: text.into(),
                reply,
            })
            .unwrap();
        answer
    };
    assert!(matches!(
        seen.recv().await,
        Some(LiveEvent::Started {
            call_type: CallType::BidiStreaming,
            ..
        })
    ));
    assert_eq!(send(r#"{"text": "one"}"#).await.unwrap(), Ok(1));
    let error = send("{oops").await.unwrap().unwrap_err();
    assert!(error.contains("not valid JSON"), "{error}");
    assert_eq!(
        send(r#"{"text": "two"} {"text": "three"}"#).await.unwrap(),
        Ok(2)
    );
    commands.send(Command::End).unwrap();

    let execution = task.await.unwrap();
    assert!(execution.error.is_none(), "{execution:#?}");
    let texts: Vec<_> = received(&execution)
        .iter()
        .map(|m| m["text"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(texts, ["ONE", "TWO", "THREE"]);
    let mut kinds = Vec::new();
    while let Ok(event) = seen.try_recv() {
        kinds.push(match event {
            LiveEvent::Started { .. } => "started",
            LiveEvent::Headers { .. } => "headers",
            LiveEvent::Message { .. } => "message",
        });
    }
    assert!(kinds.contains(&"headers"), "{kinds:?}");
    assert_eq!(kinds.iter().filter(|k| **k == "message").count(), 6);
}

#[tokio::test(flavor = "multi_thread")]
async fn live_calls_can_be_cancelled() {
    let server = test_server::spawn(Reflection::None).await;
    let (_dir, executor) = setup();
    let (commands, receiver) = mpsc::unbounded_channel();
    let (events, mut seen) = mpsc::unbounded_channel();
    let request = call(&server.address, "Bidi", "");
    let task = tokio::spawn(async move {
        executor
            .execute_live(
                None,
                &request,
                &mut Context::default(),
                Live {
                    commands: receiver,
                    events,
                },
            )
            .await
    });
    seen.recv().await.unwrap();
    commands.send(Command::Cancel).unwrap();
    let execution = task.await.unwrap();
    let status = execution.grpc.unwrap().status.unwrap();
    assert_eq!(status.name, "CANCELLED");
}

#[tokio::test(flavor = "multi_thread")]
async fn tls_works_with_checking_switched_off() {
    let server = test_server::spawn_tls().await;
    let (_dir, executor) = setup();
    let port = server.address.rsplit(':').next().unwrap();
    let mut request = call(
        &format!("localhost:{port}"),
        "Unary",
        r#"{"text": "secure"}"#,
    );
    request.grpc.tls = true;

    let execution = run(&executor, &request).await;
    let error = execution.error.unwrap();
    assert!(error.starts_with("Could not connect"), "{error}");
    assert!(
        error.contains("certificate") || error.contains("Certificate"),
        "{error}"
    );

    request.settings.verify_tls = Some(false);
    let execution = run(&executor, &request).await;
    assert!(execution.error.is_none(), "{execution:#?}");
    assert_eq!(received(&execution)[0]["text"], "secure");
    assert!(execution.grpc.unwrap().tls);
}

#[tokio::test(flavor = "multi_thread")]
async fn mistakes_explain_themselves() {
    let server = test_server::spawn(Reflection::None).await;
    let (_dir, executor) = setup();
    let error = |execution: Execution| execution.error.unwrap();

    let message = error(run(&executor, &call(&server.address, "Nope", "{}")).await);
    assert!(message.contains("has no method Nope"), "{message}");
    let message = error(run(&executor, &call(&server.address, "Unary", r#"{"txt": 1}"#)).await);
    assert!(
        message.contains("does not fit getman.test.EchoRequest"),
        "{message}"
    );
    let message = error(run(&executor, &call(&server.address, "Unary", "{} {}")).await);
    assert!(message.contains("takes one message"), "{message}");
    let message = error(run(&executor, &call("127.0.0.1:9", "Unary", "{}")).await);
    assert!(message.starts_with("Could not connect"), "{message}");
    let request = Request {
        auth: Auth::Digest {
            username: "a".into(),
            password: "b".into(),
        },
        ..call(&server.address, "Unary", "{}")
    };
    assert!(error(run(&executor, &request).await).contains("Digest"));
}

#[tokio::test(flavor = "multi_thread")]
async fn runs_mix_grpc_and_http_requests() {
    let server = test_server::spawn(Reflection::None).await;
    let (_dir, executor) = setup();
    let ws = &executor.workspace;
    ws.create_request(
        "",
        "Hello",
        &Request {
            checks: vec!["status == 0".into()],
            ..call(&server.address, "Unary", r#"{"text": "run"}"#)
        },
    )
    .unwrap();
    let mut events = Vec::new();
    let report = runner::run(
        &executor,
        &RunOptions::default(),
        &AtomicBool::new(false),
        |e| {
            events.push(e);
        },
    )
    .await
    .unwrap();
    assert!(report.summary.passed(), "{:#?}", report.summary);
    assert_eq!(report.summary.checks_passed, 1);
    let Some(RunEvent::Start { requests, .. }) = events.first() else {
        panic!("no start event");
    };
    assert_eq!(requests[0].method, "GRPC");
}

#[test]
fn requests_print_as_grpcurl() {
    let (_dir, executor) = setup();
    let request = Request {
        headers: vec![KeyValue::new("x-trace", "a b")],
        settings: Settings {
            timeout_ms: Some(2500),
            ..Settings::default()
        },
        ..call("localhost:50051", "Unary", r#"{"text": "hi"}"#)
    };
    let (code, missing) = grpcurl(&executor, None, &request, &Context::default()).unwrap();
    assert!(missing.is_empty());
    assert_eq!(
        code,
        "grpcurl \\\n  -plaintext \\\n  -import-path protos/getman/test \\\n  -import-path . \\\n  -proto echo.proto \\\n  -H 'x-trace: a b' \\\n  -d '{\"text\": \"hi\"}' \\\n  -max-time 2.5 \\\n  localhost:50051 getman.test.Echo/Unary"
    );
}
