//! The JSON-RPC handling, driven in-process as a client would.

use std::time::Duration;

use getman_core::model::{Environment, Example, Request};
use getman_core::workspace::EnvironmentFiles;
use serde_json::{Value, json};

use super::*;

fn workspace() -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().unwrap();
    let workspace = Workspace::init(dir.path(), "Shop").unwrap();
    (dir, workspace)
}

async fn ask(server: &mut Server, message: Value) -> Value {
    server.handle(message).await.expect("a reply")
}

#[allow(clippy::needless_pass_by_value)]
fn call(id: u64, name: &str, arguments: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": { "name": name, "arguments": arguments },
    })
}

/// The structured result of a tool call that succeeded.
async fn tool(server: &mut Server, name: &str, arguments: Value) -> Value {
    let reply = ask(server, call(1, name, arguments)).await;
    let result = &reply["result"];
    assert_eq!(result["isError"], false, "{reply}");
    let text = result["content"][0]["text"].as_str().unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(text).unwrap(),
        result["structuredContent"]
    );
    result["structuredContent"].clone()
}

fn modern_meta() -> Value {
    json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientCapabilities": {},
        "io.modelcontextprotocol/clientInfo": { "name": "test", "version": "1" },
    })
}

#[tokio::test]
async fn the_handshake_agrees_on_a_version() {
    let (_dir, ws) = workspace();
    let mut server = Server::new(ws);
    for (asked, agreed) in [
        ("2025-06-18", "2025-06-18"),
        ("2025-11-25", "2025-11-25"),
        ("2024-11-05", "2024-11-05"),
        ("1999-01-01", "2025-11-25"),
        ("2026-07-28", "2025-11-25"),
    ] {
        let reply = ask(
            &mut server,
            json!({
                "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": { "protocolVersion": asked, "capabilities": {}, "clientInfo": { "name": "t", "version": "1" } },
            }),
        )
        .await;
        assert_eq!(reply["id"], 1);
        assert_eq!(reply["result"]["protocolVersion"], agreed, "{asked}");
        assert_eq!(reply["result"]["serverInfo"]["name"], "getman");
        assert_eq!(
            reply["result"]["capabilities"]["tools"]["listChanged"],
            false
        );
        assert!(reply["result"].get("resultType").is_none());
    }
    let notification = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
    assert!(server.handle(notification).await.is_none());
    let reply = ask(
        &mut server,
        json!({ "jsonrpc": "2.0", "id": "p", "method": "ping" }),
    )
    .await;
    assert_eq!(reply, json!({ "jsonrpc": "2.0", "id": "p", "result": {} }));
}

#[tokio::test]
async fn modern_requests_carry_their_version() {
    let (_dir, ws) = workspace();
    let mut server = Server::new(ws);
    let reply = ask(
        &mut server,
        json!({ "jsonrpc": "2.0", "id": 7, "method": "server/discover", "params": { "_meta": modern_meta() } }),
    )
    .await;
    let result = &reply["result"];
    assert_eq!(result["resultType"], "complete");
    assert_eq!(result["supportedVersions"][0], "2026-07-28");
    assert_eq!(
        result["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
        "getman"
    );
    assert!(result["capabilities"]["tools"].is_object());

    let reply = ask(
        &mut server,
        json!({ "jsonrpc": "2.0", "id": 8, "method": "tools/list", "params": { "_meta": modern_meta() } }),
    )
    .await;
    assert_eq!(reply["result"]["resultType"], "complete");
    assert_eq!(reply["result"]["tools"].as_array().unwrap().len(), 7);

    let mut old = modern_meta();
    old["io.modelcontextprotocol/protocolVersion"] = json!("1900-01-01");
    let reply = ask(
        &mut server,
        json!({ "jsonrpc": "2.0", "id": 9, "method": "tools/list", "params": { "_meta": old } }),
    )
    .await;
    assert_eq!(reply["error"]["code"], -32022);
    assert_eq!(reply["error"]["data"]["requested"], "1900-01-01");
    assert_eq!(reply["error"]["data"]["supported"][1], "2025-11-25");

    let mut partial = modern_meta();
    partial
        .as_object_mut()
        .unwrap()
        .remove("io.modelcontextprotocol/clientCapabilities");
    let reply = ask(
        &mut server,
        json!({ "jsonrpc": "2.0", "id": 10, "method": "tools/list", "params": { "_meta": partial } }),
    )
    .await;
    assert_eq!(reply["error"]["code"], -32602);

    // A legacy client probing server/discover gets an error and falls back.
    let reply = ask(
        &mut server,
        json!({ "jsonrpc": "2.0", "id": 11, "method": "server/discover" }),
    )
    .await;
    assert_eq!(reply["error"]["code"], -32602);
}

#[tokio::test]
async fn malformed_messages_get_json_rpc_errors() {
    let (_dir, ws) = workspace();
    let mut server = Server::new(ws);
    let reply: Value = serde_json::from_str(&server.handle_line("{nope").await.unwrap()).unwrap();
    assert_eq!(reply["error"]["code"], -32700);
    assert_eq!(reply["id"], Value::Null);
    assert!(server.handle_line("   ").await.is_none());

    let reply = ask(&mut server, json!({ "id": 1, "method": "ping" })).await;
    assert_eq!(reply["error"]["code"], -32600);
    let reply = ask(&mut server, json!(42)).await;
    assert_eq!(reply["error"]["code"], -32600);
    let reply = ask(
        &mut server,
        json!({ "jsonrpc": "2.0", "id": 2, "method": "resources/list" }),
    )
    .await;
    assert_eq!(reply["error"]["code"], -32601);
    let reply = ask(&mut server, call(3, "rm_rf", json!({}))).await;
    assert_eq!(reply["error"]["code"], -32602);
    let reply = ask(
        &mut server,
        json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "name": "read_request", "arguments": [] } }),
    )
    .await;
    assert_eq!(reply["error"]["code"], -32602);
    // A response from the client needs no answer.
    assert!(
        server
            .handle(json!({ "jsonrpc": "2.0", "id": 5, "result": {} }))
            .await
            .is_none()
    );

    let replies = ask(
        &mut server,
        json!([
            { "jsonrpc": "2.0", "id": 1, "method": "ping" },
            { "jsonrpc": "2.0", "method": "notifications/initialized" },
            { "jsonrpc": "2.0", "id": 2, "method": "ping" }
        ]),
    )
    .await;
    assert_eq!(replies.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn every_tool_has_a_schema() {
    let tools = tools::list();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        [
            "list_requests",
            "read_request",
            "send_request",
            "run_folder",
            "list_environments",
            "save_request",
            "import_curl"
        ]
    );
    for tool in &tools {
        assert_eq!(tool["inputSchema"]["type"], "object", "{tool}");
        assert!(tool["description"].as_str().unwrap().len() > 20);
    }
}

#[tokio::test]
async fn requests_are_listed_read_written_and_imported() {
    let (_dir, ws) = workspace();
    let users = ws.create_folder("", "Users").unwrap();
    ws.create_request(
        &users,
        "Get user",
        &Request {
            url: "{{baseUrl}}/users/:id".into(),
            examples: vec![Example {
                name: "Found".into(),
                body: "{}".into(),
                ..Example::default()
            }],
            ..Request::default()
        },
    )
    .unwrap();
    let mut server = Server::new(ws.clone());

    let listed = tool(&mut server, "list_requests", json!({})).await;
    assert_eq!(listed["workspace"], "Shop");
    assert_eq!(listed["tree"][0]["path"], "users");
    assert_eq!(listed["tree"][0]["children"][0]["method"], "GET");
    let only = tool(&mut server, "list_requests", json!({ "folder": "users/" })).await;
    assert_eq!(only["tree"][0]["path"], "users/get-user.yaml");

    let read = tool(
        &mut server,
        "read_request",
        json!({ "path": "users\\get-user.yaml" }),
    )
    .await;
    assert_eq!(read["request"]["url"], "{{baseUrl}}/users/:id");
    assert!(read["request"].get("examples").is_none());
    assert_eq!(read["examples"][0]["name"], "Found");

    // Updating keeps the examples and the name.
    let saved = tool(
        &mut server,
        "save_request",
        json!({ "path": "users/get-user.yaml", "request": { "method": "GET", "url": "{{baseUrl}}/users/:id", "checks": ["status == 200"] } }),
    )
    .await;
    assert_eq!(saved["created"], false);
    let stored = ws.read_request("users/get-user.yaml").unwrap();
    assert_eq!(stored.checks, ["status == 200"]);
    assert_eq!(stored.examples.len(), 1);
    assert_eq!(stored.name, "Get user");

    let created = tool(
        &mut server,
        "save_request",
        json!({ "folder": "users", "name": "Delete user", "request": { "method": "DELETE", "url": "{{baseUrl}}/users/:id" } }),
    )
    .await;
    assert_eq!(created["path"], "users/delete-user.yaml");

    let reply = ask(
        &mut server,
        call(2, "save_request", json!({ "request": { "method": 5 } })),
    )
    .await;
    assert_eq!(reply["result"]["isError"], true);
    let reply = ask(
        &mut server,
        call(3, "read_request", json!({ "path": "../getman.yaml" })),
    )
    .await;
    assert_eq!(reply["result"]["isError"], true);

    let imported = tool(
        &mut server,
        "import_curl",
        json!({ "command": "curl -X POST https://api.example.com/orders -d '{\"sku\":1}' -H 'Content-Type: application/json'", "name": "Order" }),
    )
    .await;
    assert_eq!(imported["path"], "order.yaml");
    assert_eq!(imported["request"]["method"], "POST");
    assert_eq!(imported["request"]["body"]["type"], "json");
}

#[tokio::test]
async fn secrets_never_reach_the_results() {
    let (_dir, ws) = workspace();
    // A mock server stands in for the API, answering with the token.
    ws.create_request(
        "",
        "Me",
        &Request {
            url: "{{baseUrl}}/me?key={{token}}".into(),
            headers: vec![getman_core::KeyValue::new(
                "Authorization",
                "Bearer {{token}}",
            )],
            checks: vec!["status == 200".into(), "json.user == ada".into()],
            examples: vec![Example {
                name: "Me".into(),
                body: r#"{"user": "ada", "token": "s3cret-token"}"#.into(),
                ..Example::default()
            }],
            ..Request::default()
        },
    )
    .unwrap();
    let listener = crate::mock::bind(Some(0)).await.unwrap();
    let mock =
        crate::mock::start(ws.clone(), "", listener, Duration::ZERO, Box::new(|_| {})).unwrap();
    let id = ws.create_environment("Dev").unwrap();
    ws.write_environment(
        &id,
        &EnvironmentFiles {
            shared: Environment {
                variables: [("baseUrl".to_owned(), mock.url())].into_iter().collect(),
                ..Environment::default()
            },
            local: [("token".to_owned(), "s3cret-token".to_owned())]
                .into_iter()
                .collect(),
        },
    )
    .unwrap();
    let mut server = Server::new(ws.clone());

    let sent = tool(
        &mut server,
        "send_request",
        json!({ "path": "me.yaml", "environment": "Dev", "max_body_chars": 30 }),
    )
    .await;
    assert_eq!(sent["status"], 200);
    assert_eq!(sent["passed"], true, "{sent}");
    assert_eq!(sent["checks"].as_array().unwrap().len(), 2);
    assert_eq!(sent["body_truncated"], true);
    assert_eq!(sent["body_chars"], 40);
    let text = sent.to_string();
    assert!(!text.contains("s3cret-token"), "{text}");
    assert!(text.contains("key=[secret]"), "{text}");

    let full = tool(
        &mut server,
        "send_request",
        json!({ "path": "me.yaml", "environment": "dev" }),
    )
    .await;
    assert_eq!(full["body"], r#"{"user": "ada", "token": "[secret]"}"#);

    let environments = tool(&mut server, "list_environments", json!({})).await;
    let dev = &environments["environments"][0];
    assert_eq!(dev["id"], "dev");
    assert_eq!(dev["variables"]["token"], "[secret]");
    assert_eq!(dev["variables"]["baseUrl"], mock.url());

    let run = tool(
        &mut server,
        "run_folder",
        json!({ "environment": "dev", "iterations": 2 }),
    )
    .await;
    assert_eq!(run["passed"], true, "{run}");
    assert_eq!(run["summary"]["requests"], 2);
    assert_eq!(run["requests"][1]["iteration"], 2);
    assert!(!run.to_string().contains("s3cret-token"));

    let reply = ask(
        &mut server,
        call(
            9,
            "send_request",
            json!({ "path": "me.yaml", "environment": "nope" }),
        ),
    )
    .await;
    assert_eq!(reply["result"]["isError"], true);
    mock.stop();
}

#[tokio::test]
async fn secrets_of_any_length_are_masked_in_their_encodings_too() {
    use base64::Engine as _;
    let (_dir, ws) = workspace();
    let password = "p@ss word";
    // What an API might echo back: the secret in other forms, inside longer
    // text, and a two-character secret inside a longer word.
    let echoed = json!({
        "basic": base64::engine::general_purpose::STANDARD.encode(format!("ada:{password}")),
        "url": "https://x.test/?pw=p%40ss%20word&ok=1",
        "hex": hex::encode(password),
        "inside": "my password is p@ss word, sure",
        "short": "xyz-in-a-longer-word",
        "number": 12,
    });
    ws.create_request(
        "",
        "Echo",
        &Request {
            url: "{{baseUrl}}/echo".into(),
            examples: vec![Example {
                name: "Echo".into(),
                body: echoed.to_string(),
                ..Example::default()
            }],
            ..Request::default()
        },
    )
    .unwrap();
    let listener = crate::mock::bind(Some(0)).await.unwrap();
    let mock =
        crate::mock::start(ws.clone(), "", listener, Duration::ZERO, Box::new(|_| {})).unwrap();
    let id = ws.create_environment("Dev").unwrap();
    ws.write_environment(
        &id,
        &EnvironmentFiles {
            shared: Environment {
                variables: [("baseUrl".to_owned(), mock.url())].into_iter().collect(),
                ..Environment::default()
            },
            local: [
                ("password".to_owned(), password.to_owned()),
                ("tiny".to_owned(), "xyz".to_owned()),
            ]
            .into_iter()
            .collect(),
        },
    )
    .unwrap();
    let mut server = Server::new(ws.clone());

    let sent = tool(
        &mut server,
        "send_request",
        json!({ "path": "echo.yaml", "environment": "dev" }),
    )
    .await;
    assert_eq!(sent["status"], 200, "{sent}");
    let body: Value = serde_json::from_str(sent["body"].as_str().unwrap()).unwrap();
    // The encoded "ada:" in front is not secret, so it stays.
    let basic = body["basic"].as_str().unwrap();
    assert!(
        basic.starts_with("YWRh") && basic.ends_with("[secret]"),
        "{basic}"
    );
    assert_eq!(body["url"], "https://x.test/?pw=[secret]&ok=1");
    assert_eq!(body["hex"], "[secret]");
    assert_eq!(body["inside"], "my password is [secret], sure");
    assert_eq!(body["short"], "[secret]-in-a-longer-word");
    assert_eq!(body["number"], 12);
    let everything = sent.to_string();
    for leaked in ["p@ss", "p%40ss", "xyz-in"] {
        assert!(!everything.contains(leaked), "{leaked}: {everything}");
    }

    // The same holds for errors.
    ws.write_local_state(&getman_core::workspace::LocalState {
        variables: [("v".to_owned(), "no-such-folder".to_owned())]
            .into_iter()
            .collect(),
        ..getman_core::workspace::LocalState::default()
    })
    .unwrap();
    let reply = ask(
        &mut server,
        call(2, "list_requests", json!({ "folder": "no-such-folder" })),
    )
    .await;
    assert_eq!(reply["result"]["isError"], true);
    assert_eq!(
        reply["result"]["content"][0]["text"],
        "there is no folder [secret]"
    );
    mock.stop();
}
