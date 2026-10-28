use super::*;
use crate::http::Header;
use crate::model::FormField;
use crate::test_server;

fn script(code: &str) -> Vec<Script> {
    vec![Script {
        origin: "request".into(),
        code: code.into(),
    }]
}

fn input(event: Event) -> ScriptInput {
    let mut environment = Vars::new();
    environment.insert("baseUrl".into(), "https://api.example.com".into());
    environment.insert("token".into(), "abc".into());
    let mut collection = Vars::new();
    collection.insert("baseUrl".into(), "https://collection.example.com".into());
    collection.insert("version".into(), "v1".into());
    ScriptInput {
        event,
        info: ScriptInfo {
            request_name: "Get user".into(),
            request_path: "users/get-user.yaml".into(),
            iteration: 0,
            iteration_count: 1,
        },
        scopes: ScriptScopes {
            environment,
            environment_name: Some("Staging".into()),
            collection,
            ..ScriptScopes::default()
        },
        request: ScriptRequest {
            method: "POST".into(),
            url: "{{baseUrl}}/users?page=1".into(),
            headers: vec![KeyValue::new("Accept", "application/json")],
            body: Some(ScriptBody {
                mode: "raw".into(),
                raw: Some(r#"{"name":"Ada"}"#.into()),
                ..ScriptBody::default()
            }),
        },
        response: (event == Event::PostResponse).then(|| ScriptResponse {
            code: 201,
            status: "Created".into(),
            headers: vec![
                Header::new("Content-Type", "application/json"),
                Header::new("Set-Cookie", "sid=xyz; Path=/; HttpOnly"),
            ],
            body: r#"{"id": 7, "name": "Ada", "tags": ["a", "b"], "nested": {"deep": [1, 2, 3]}}"#
                .into(),
            response_time: 42.0,
            response_size: 70,
        }),
        messages: None,
        cookies: vec![],
    }
}

fn post(code: &str) -> ScriptOutput {
    run(
        &ScriptHost::default(),
        &script(code),
        &input(Event::PostResponse),
    )
}

fn pre(code: &str) -> ScriptOutput {
    run(
        &ScriptHost::default(),
        &script(code),
        &input(Event::PreRequest),
    )
}

fn assert_all_pass(output: &ScriptOutput) {
    assert!(output.error.is_none(), "{:?}", output.error);
    assert!(!output.tests.is_empty());
    for test in &output.tests {
        assert!(test.passed, "{}: {:?}", test.name, test.message);
    }
}

#[test]
fn expectations_pass() {
    let output = post(
        r#"
        gm.test("status and body", () => {
            gm.response.to.have.status(201);
            gm.response.to.have.status("Created");
            gm.response.to.be.ok;
            gm.response.to.be.success;
            gm.response.to.not.be.error;
            gm.response.to.be.json;
            gm.response.to.have.header("Content-Type");
            gm.response.to.have.header("content-type", "application/json");
            gm.response.to.have.jsonBody("nested.deep");
            gm.response.to.have.jsonBody("name", "Ada");
            gm.expect(gm.response).to.have.status(201);
        });
        gm.test("chai style", () => {
            const json = gm.response.json();
            gm.expect(json.id).to.equal(7).and.to.be.a("number").that.is.above(6);
            gm.expect(json).to.have.property("name", "Ada");
            gm.expect(json).to.have.property("tags").that.has.lengthOf(2);
            gm.expect(json).to.have.nested.property("nested.deep[2]", 3);
            gm.expect(json.tags).to.include("b").and.not.include("c");
            gm.expect(json.tags).to.have.members(["b", "a"]);
            gm.expect(json.nested.deep).to.include.members([3]);
            gm.expect(json).to.include({ name: "Ada" });
            gm.expect(json).to.have.all.keys("id", "name", "tags", "nested");
            gm.expect(json).to.include.keys("id");
            gm.expect(json).to.have.any.keys("id", "missing");
            gm.expect(json).to.deep.equal({ id: 7, name: "Ada", tags: ["a", "b"], nested: { deep: [1, 2, 3] } });
            gm.expect(json.name).to.match(/^A/).and.to.be.oneOf(["Ada", "Grace"]);
            gm.expect([]).to.be.empty;
            gm.expect(null).to.not.exist;
            gm.expect(0.1 + 0.2).to.be.closeTo(0.3, 1e-9);
            gm.expect(() => { throw new TypeError("nope"); }).to.throw(TypeError);
            gm.expect(() => { throw new Error("bad thing"); }).to.throw("bad");
            gm.expect(gm.response.responseTime).to.be.below(100);
        });
        gm.test("json schema", () => {
            gm.response.to.have.jsonSchema({
                type: "object",
                required: ["id", "name"],
                properties: { id: { type: "integer", minimum: 1 }, tags: { type: "array", items: { type: "string" } } },
            });
        });
        "#,
    );
    assert_all_pass(&output);
    assert_eq!(output.tests.len(), 3);
}

#[test]
fn failures_explain_themselves() {
    let output = post(
        r#"
        gm.test("wrong status", () => gm.response.to.have.status(200));
        gm.test("wrong value", () => gm.expect(gm.response.json().id).to.equal(8));
        gm.test("negated", () => gm.expect("abc").to.not.include("b"));
        gm.test("custom message", () => gm.expect(1, "the count").to.equal(2));
        gm.test("schema", () => gm.response.to.have.jsonSchema({ required: ["email"] }));
        gm.test("thrown value", () => { throw "plain"; });
        "#,
    );
    let messages: Vec<_> = output
        .tests
        .iter()
        .map(|t| {
            assert!(!t.passed, "{}", t.name);
            t.message.clone().unwrap()
        })
        .collect();
    assert_eq!(messages[0], "expected the status to be 200 but it is 201");
    assert_eq!(messages[1], "expected 7 to equal 8");
    assert_eq!(messages[2], r#"expected "abc" not to include "b""#);
    assert_eq!(messages[3], "the count: expected 1 to equal 2");
    assert!(
        messages[4].contains("$.email is required"),
        "{}",
        messages[4]
    );
    assert_eq!(messages[5], r#""plain""#);
}

#[test]
fn variables_resolve_by_scope_and_report_changes() {
    let output = post(
        r#"
        gm.test("reads", () => {
            gm.expect(gm.environment.get("token")).to.equal("abc");
            gm.expect(gm.environment.name).to.equal("Staging");
            gm.expect(gm.variables.get("baseUrl")).to.equal("https://api.example.com");
            gm.expect(gm.collectionVariables.get("baseUrl")).to.equal("https://collection.example.com");
            gm.expect(gm.variables.replaceIn("{{baseUrl}}/{{version}}")).to.equal("https://api.example.com/v1");
            gm.expect(gm.variables.replaceIn("{{$uuid}}")).to.have.lengthOf(36);
        });
        gm.environment.set("token", gm.response.json().id);
        gm.collectionVariables.set("obj", { a: 1 });
        gm.globals.set("g", true);
        gm.variables.set("local", "x");
        gm.environment.unset("baseUrl");
        gm.test("local wins", () => gm.expect(gm.variables.get("local")).to.equal("x"));
        gm.test("typed until stored", () => gm.expect(gm.environment.get("token")).to.equal(7));
        "#,
    );
    assert_all_pass(&output);
    let changes: Vec<_> = output
        .changes
        .iter()
        .map(|c| (c.scope, c.name.as_str(), c.value.as_deref()))
        .collect();
    assert_eq!(
        changes,
        [
            (ChangeScope::Environment, "token", Some("7")),
            (ChangeScope::Collection, "obj", Some(r#"{"a":1}"#)),
            (ChangeScope::Global, "g", Some("true")),
            (ChangeScope::Local, "local", Some("x")),
            (ChangeScope::Environment, "baseUrl", None),
        ]
    );
}

#[test]
fn clearing_variables_removes_only_local_ones() {
    let mut input = input(Event::PostResponse);
    input.scopes.local.insert("earlier".into(), "1".into());
    let output = run(
        &ScriptHost::default(),
        &script(
            r#"
            gm.variables.set("later", "2");
            gm.variables.clear();
            gm.test("cleared", () => {
                gm.expect(gm.variables.has("earlier")).to.be.false;
                gm.expect(gm.variables.has("later")).to.be.false;
                gm.expect(gm.variables.get("token")).to.equal("abc");
            });
            "#,
        ),
        &input,
    );
    assert_all_pass(&output);
    let changes: Vec<_> = output
        .changes
        .iter()
        .map(|c| (c.scope, c.name.as_str(), c.value.as_deref()))
        .collect();
    assert_eq!(
        changes,
        [
            (ChangeScope::Local, "later", Some("2")),
            (ChangeScope::Local, "earlier", None),
            (ChangeScope::Local, "later", None),
        ]
    );
}

#[test]
fn pre_request_scripts_edit_the_request() {
    let output = pre(r#"
        gm.request.headers.upsert({ key: "accept", value: "text/plain" });
        gm.request.headers.add({ key: "X-Signature", value: "sig" });
        gm.request.addHeader("X-Trace: 1");
        gm.request.url.query.add({ key: "sort", value: "name" });
        gm.request.url.query.remove("page");
        gm.request.method = "put";
        gm.request.body.update({ name: "Grace" });
        console.log("host", gm.request.url.getHost(), { n: 1 });
        console.warn("careful");
        "#);
    assert!(output.error.is_none(), "{:?}", output.error);
    let request = output.request.unwrap();
    assert_eq!(request.method, "PUT");
    assert_eq!(request.url, "{{baseUrl}}/users?sort=name");
    let headers: Vec<_> = request
        .headers
        .iter()
        .map(|h| (h.name.as_str(), h.value.as_str()))
        .collect();
    assert_eq!(
        headers,
        [
            ("accept", "text/plain"),
            ("X-Signature", "sig"),
            ("X-Trace", "1")
        ]
    );
    assert_eq!(request.body.as_deref(), Some(r#"{"name":"Grace"}"#));
    assert_eq!(output.logs[0].message, "host {{baseUrl}} { n: 1 }");
    assert_eq!(output.logs[0].level, "info");
    assert_eq!(output.logs[1].level, "warn");
    assert_eq!(output.logs[1].origin, "request");
}

#[test]
fn errors_name_the_script_and_line_and_stop_later_scripts() {
    let scripts = vec![
        Script {
            origin: "workspace".into(),
            code: "gm.variables.set('ran', 'workspace');".into(),
        },
        Script {
            origin: "folder \"Users\"".into(),
            code: "const a = 1;\n\nundefinedFunction();".into(),
        },
        Script {
            origin: "request".into(),
            code: "gm.variables.set('ran', 'request');".into(),
        },
    ];
    let output = run(&ScriptHost::default(), &scripts, &input(Event::PreRequest));
    let error = output.error.unwrap();
    assert_eq!(error.origin, "folder \"Users\"");
    assert!(
        error.message.starts_with("ReferenceError"),
        "{}",
        error.message
    );
    assert!(error.message.ends_with("(line 3)"), "{}", error.message);
    assert_eq!(output.changes.len(), 1);

    let output = pre("let x = ;");
    assert!(output.error.unwrap().message.starts_with("SyntaxError"));
}

#[test]
fn runaway_scripts_are_stopped() {
    let host = ScriptHost {
        timeout: Duration::from_millis(200),
        ..ScriptHost::default()
    };
    let started = Instant::now();
    let output = run(&host, &script("while (true) {}"), &input(Event::PreRequest));
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(
        output.error.unwrap().message,
        "the script ran longer than 0.2 s and was stopped"
    );

    let output = run(
        &host,
        &script("await new Promise(() => {});"),
        &input(Event::PreRequest),
    );
    assert!(output.error.unwrap().message.contains("never finished"));
}

#[test]
fn async_code_and_timers_run_to_completion() {
    let output = post(
        r#"
        const order = [];
        setTimeout(() => order.push("late"), 1000);
        setTimeout(() => order.push("soon"), 10);
        await new Promise((resolve) => setTimeout(resolve, 500));
        order.push("after await");
        gm.test("async test", async () => {
            await null;
            gm.expect(order).to.eql(["soon", "after await"]);
        });
        gm.test("failing async test", async () => {
            await null;
            throw new Error("later");
        });
        "#,
    );
    assert!(output.error.is_none(), "{:?}", output.error);
    assert!(output.tests[0].passed, "{:?}", output.tests[0].message);
    assert_eq!(output.tests[1].message.as_deref(), Some("later"));
}

#[test]
fn crypto_matches_known_vectors() {
    let output = post(
        r#"
        const CryptoJS = require("crypto-js");
        gm.test("hmac", () => {
            // RFC 4231 test case 2.
            const mac = CryptoJS.HmacSHA256("what do ya want for nothing?", "Jefe");
            gm.expect(mac.toString()).to.equal("5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
            gm.expect(gm.crypto.hmac("sha256", "Jefe", "what do ya want for nothing?", "base64"))
                .to.equal("W9zBRr9gdU5qBCQmCJV1x1oAPwidJzmDnexYuWTsOEM=");
        });
        gm.test("hashes", () => {
            gm.expect(CryptoJS.MD5("abc").toString()).to.equal("900150983cd24fb0d6963f7d28e17f72");
            gm.expect(CryptoJS.SHA1("abc").toString()).to.equal("a9993e364706816aba3e25717850c26c9cd0d89d");
            gm.expect(gm.crypto.hash("SHA-256", "abc")).to.equal("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        });
        gm.test("encodings", () => {
            const words = CryptoJS.enc.Utf8.parse("héllo");
            gm.expect(CryptoJS.enc.Base64.stringify(words)).to.equal("aMOpbGxv");
            gm.expect(CryptoJS.enc.Utf8.stringify(CryptoJS.enc.Base64.parse("aMOpbGxv"))).to.equal("héllo");
            gm.expect(btoa("user:pass")).to.equal("dXNlcjpwYXNz");
            gm.expect(atob("dXNlcjpwYXNz")).to.equal("user:pass");
            gm.expect(gm.crypto.base64Decode(gm.crypto.base64Encode("✓ ok"))).to.equal("✓ ok");
            gm.expect(crypto.randomUUID()).to.match(/^[0-9a-f-]{36}$/);
        });
        gm.test("unknown modules", () => {
            gm.expect(() => require("lodash")).to.throw(/not available/);
        });
        "#,
    );
    assert_all_pass(&output);
}

#[test]
fn older_globals_still_work() {
    let output = post(
        r#"
        tests["status is 201"] = responseCode.code === 201;
        tests["body mentions Grace"] = responseBody.indexOf("Grace") >= 0;
        postman.setEnvironmentVariable("legacy", "yes");
        pm.test("pm alias", () => pm.expect(pm.response.code).to.equal(201));
        "#,
    );
    let results: Vec<_> = output
        .tests
        .iter()
        .map(|t| (t.name.as_str(), t.passed))
        .collect();
    assert_eq!(
        results,
        [
            ("pm alias", true),
            ("status is 201", true),
            ("body mentions Grace", false)
        ]
    );
    assert_eq!(output.changes[0].name, "legacy");
}

#[test]
fn flow_control_is_reported() {
    let output = pre("gm.execution.skipRequest(); gm.execution.setNextRequest(null);");
    assert!(output.skip_request);
    assert_eq!(output.next_request, Some(NextRequest::Stop));
    let output = pre("gm.execution.setNextRequest('Login');");
    assert_eq!(output.next_request, Some(NextRequest::Jump("Login".into())));
    let output = pre("1;");
    assert_eq!(output.next_request, None);
    let output = post("gm.test.skip('later', () => {});");
    assert!(output.tests[0].skipped);
}

#[test]
fn cookies_and_info_are_visible() {
    let output = post(
        r#"
        gm.test("cookies", () => {
            gm.expect(gm.response.cookies.get("sid")).to.equal("xyz");
            gm.expect(gm.response.cookies.has("nope")).to.be.false;
        });
        gm.test("info", () => {
            gm.expect(gm.info.requestName).to.equal("Get user");
            gm.expect(gm.info.eventName).to.equal("test");
            gm.expect(gm.info.iterationCount).to.equal(1);
        });
        "#,
    );
    assert_all_pass(&output);
}

#[tokio::test(flavor = "multi_thread")]
async fn scripts_can_send_requests() {
    let server = test_server::spawn().await;
    let host = ScriptHost {
        http: Some((
            Arc::new(HttpClient::new()),
            tokio::runtime::Handle::current(),
        )),
        ..ScriptHost::default()
    };
    let code = format!(
        r#"
        const res = await gm.sendRequest({{
            url: "{url}",
            method: "POST",
            header: [{{ key: "X-From", value: "script" }}],
            body: {{ mode: "raw", raw: "ping" }},
        }});
        gm.environment.set("echoed", res.json().body);
        gm.sendRequest("{url}", (err, reply) => {{
            gm.test("callback", () => gm.expect(reply.json().headers["x-from"]).to.be.undefined);
        }});
        gm.sendRequest("http://127.0.0.1:9/", (err) => {{
            gm.test("errors reach the callback", () => gm.expect(err.message).to.include("Could not connect"));
        }});
        "#,
        url = server.url("/echo")
    );
    let output =
        tokio::task::spawn_blocking(move || run(&host, &script(&code), &input(Event::PreRequest)))
            .await
            .unwrap();
    assert_all_pass(&output);
    assert_eq!(output.changes[0].value.as_deref(), Some("ping"));
}

fn form_input() -> ScriptInput {
    let mut input = input(Event::PreRequest);
    input.request.body = Some(ScriptBody {
        mode: "urlencoded".into(),
        urlencoded: Some(vec![
            KeyValue::new("grant_type", "password"),
            KeyValue {
                enabled: false,
                ..KeyValue::new("old", "1")
            },
        ]),
        ..ScriptBody::default()
    });
    input
}

#[test]
fn pre_request_scripts_edit_the_fields_of_a_form() {
    let output = run(
        &ScriptHost::default(),
        &script(
            r#"
            const fields = gm.request.body.urlencoded;
            gm.test("reads", () => {
                gm.expect(gm.request.body.mode).to.equal("urlencoded");
                gm.expect(fields.get("grant_type")).to.equal("password");
                gm.expect(fields.has("old")).to.be.false;
                gm.expect(fields.count()).to.equal(2);
                gm.expect(gm.request.body.formdata).to.be.undefined;
            });
            fields.upsert({ key: "grant_type", value: "client_credentials" });
            fields.add({ key: "scope", value: "read write" });
            fields.remove("old");
            "#,
        ),
        &form_input(),
    );
    assert_all_pass(&output);
    let request = output.request.unwrap();
    let fields = request.urlencoded.unwrap();
    let pairs: Vec<_> = fields
        .iter()
        .map(|f| (f.name.as_str(), f.value.as_str(), f.enabled))
        .collect();
    assert_eq!(
        pairs,
        [
            ("grant_type", "client_credentials", true),
            ("scope", "read write", true)
        ]
    );
    assert!(request.formdata.is_none());
}

#[test]
fn pre_request_scripts_edit_multipart_fields_and_files() {
    let mut input = input(Event::PreRequest);
    input.request.body = Some(ScriptBody {
        mode: "formdata".into(),
        formdata: Some(vec![
            FormField {
                name: "photo".into(),
                value: String::new(),
                file: Some("img/cat.png".into()),
                content_type: Some("image/png".into()),
                enabled: true,
            },
            FormField {
                name: "title".into(),
                value: "Cat".into(),
                file: None,
                content_type: None,
                enabled: true,
            },
        ]),
        ..ScriptBody::default()
    });
    let output = run(
        &ScriptHost::default(),
        &script(
            r#"
            const form = gm.request.body.formdata;
            gm.test("reads", () => {
                gm.expect(form.one("photo").type).to.equal("file");
                gm.expect(form.one("photo").src).to.equal("img/cat.png");
                gm.expect(form.get("title")).to.equal("Cat");
            });
            form.upsert({ key: "title", value: "Dog" });
            form.add({ key: "doc", type: "file", src: "docs/a.pdf", contentType: "application/pdf" });
            form.add({ key: "note", value: 5 });
            "#,
        ),
        &input,
    );
    assert_all_pass(&output);
    let fields = output.request.unwrap().formdata.unwrap();
    let rows: Vec<_> = fields
        .iter()
        .map(|f| (f.name.as_str(), f.value.as_str(), f.file.as_deref()))
        .collect();
    assert_eq!(
        rows,
        [
            ("photo", "", Some("img/cat.png")),
            ("title", "Dog", None),
            ("doc", "", Some("docs/a.pdf")),
            ("note", "5", None)
        ]
    );
    assert_eq!(fields[0].content_type.as_deref(), Some("image/png"));
    assert_eq!(fields[2].content_type.as_deref(), Some("application/pdf"));
}

#[test]
fn the_request_has_an_id_and_a_script_can_stop_the_run() {
    let output = pre(
        "gm.test('id', () => gm.expect(pm.request.id).to.equal('users/get-user.yaml')); \
         pm.execution.stopExecution();",
    );
    assert_all_pass(&output);
    assert!(output.stop_run);
    assert!(!output.skip_request);
    assert!(!pre("1;").stop_run);
}

#[test]
fn session_variables_are_a_scope_between_data_and_local() {
    let mut input = input(Event::PostResponse);
    input.scopes.session.insert("token".into(), "s".into());
    input
        .scopes
        .data
        .insert("token".into(), serde_json::json!("d"));
    input
        .scopes
        .data
        .insert("row".into(), serde_json::json!("r"));
    let output = run(
        &ScriptHost::default(),
        &script(
            r#"
            gm.test("session beats data", () => {
                gm.expect(gm.variables.get("token")).to.equal("s");
                gm.expect(gm.session.get("token")).to.equal("s");
                gm.expect(gm.variables.get("row")).to.equal("r");
            });
            gm.session.set("count", 3);
            gm.variables.set("token", "l");
            gm.test("local beats session", () => {
                gm.expect(gm.variables.get("token")).to.equal("l");
                gm.expect(gm.session.get("count")).to.equal(3);
                gm.expect(gm.variables.replaceIn("{{count}}")).to.equal("3");
            });
            gm.session.unset("token");
            "#,
        ),
        &input,
    );
    assert_all_pass(&output);
    let changes: Vec<_> = output
        .changes
        .iter()
        .map(|c| (c.scope, c.name.as_str(), c.value.as_deref()))
        .collect();
    assert_eq!(
        changes,
        [
            (ChangeScope::Session, "count", Some("3")),
            (ChangeScope::Local, "token", Some("l")),
            (ChangeScope::Session, "token", None),
        ]
    );
}

#[test]
fn bruno_scripts_run_before_a_request() {
    let mut input = input(Event::PreRequest);
    input.scopes.globals.insert("g".into(), "global".into());
    input.scopes.folder.insert("f".into(), "folder".into());
    input.scopes.request.insert("r".into(), "request".into());
    input.scopes.local.insert("loc".into(), "local".into());
    let output = run(
        &ScriptHost::default(),
        &script(
            r#"
            test("reads the scopes", () => {
                expect(bru.getEnvName()).to.equal("Staging");
                expect(bru.getEnvVar("token")).to.equal("abc");
                expect(bru.getGlobalEnvVar("g")).to.equal("global");
                expect(bru.getCollectionVar("version")).to.equal("v1");
                expect(bru.getFolderVar("f")).to.equal("folder");
                expect(bru.getRequestVar("r")).to.equal("request");
                expect(bru.getVar("loc")).to.equal("local");
                expect(bru.getVar("nope")).to.be.undefined;
                expect(bru.getProcessEnv("PATH")).to.be.undefined;
                expect(bru.interpolate("{{baseUrl}}/x")).to.equal("https://api.example.com/x");
                expect(bru.interpolate({ a: "{{token}}" })).to.eql({ a: "abc" });
            });
            bru.setVar("n", 1);
            test("runtime variables", () => {
                expect(bru.hasVar("n")).to.be.true;
                expect(bru.getVar("n")).to.equal(1);
            });
            bru.deleteVar("n");
            bru.setEnvVar("e", "1");
            bru.setGlobalEnvVar("g2", "2");
            bru.runner.skipRequest();
            bru.setNextRequest("Login");

            test("request", () => {
                expect(req.getMethod()).to.equal("POST");
                expect(req.getName()).to.equal("Get user");
                expect(req.getHeader("accept")).to.equal("application/json");
                expect(req.getHeaders()).to.eql({ Accept: "application/json" });
                expect(req.getBody()).to.eql({ name: "Ada" });
                expect(typeof res).to.equal("undefined");
            });
            req.setUrl("https://example.com/other");
            req.setMethod("put");
            req.setHeader("X-A", "1");
            req.setHeader("accept", "text/plain");
            req.deleteHeader("X-A");
            req.setBody({ name: "Grace" });
            "#,
        ),
        &input,
    );
    assert_all_pass(&output);
    assert!(output.skip_request);
    assert_eq!(output.next_request, Some(NextRequest::Jump("Login".into())));
    let changes: Vec<_> = output
        .changes
        .iter()
        .map(|c| (c.scope, c.name.as_str(), c.value.as_deref()))
        .collect();
    assert_eq!(
        changes,
        [
            (ChangeScope::Session, "n", Some("1")),
            (ChangeScope::Session, "n", None),
            (ChangeScope::Environment, "e", Some("1")),
            (ChangeScope::Global, "g2", Some("2")),
        ]
    );
    let request = output.request.unwrap();
    assert_eq!(request.method, "PUT");
    assert_eq!(request.url, "https://example.com/other");
    assert_eq!(request.headers.len(), 1);
    assert_eq!(request.headers[0].value, "text/plain");
    assert_eq!(request.body.as_deref(), Some(r#"{"name":"Grace"}"#));

    let output = pre("bru.runner.stopExecution();");
    assert!(output.stop_run);
}

#[test]
fn bruno_scripts_read_the_response() {
    let output = post(
        r#"
        test("res", () => {
            expect(res.status).to.equal(201);
            expect(res.statusText).to.equal("Created");
            expect(res.body.name).to.equal("Ada");
            expect(res("nested.deep[1]")).to.equal(2);
            expect(res("tags[-1]")).to.equal("b");
            expect(res("$.id")).to.equal(7);
            expect(res("missing.path")).to.be.undefined;
            expect(res.headers["content-type"]).to.equal("application/json");
            expect(res.getHeader("Content-Type")).to.equal("application/json");
            expect(Object.keys(res.getHeaders())).to.include("set-cookie");
            expect(res.getStatus()).to.equal(201);
            expect(res.getBody().id).to.equal(7);
            expect(res.getResponseTime()).to.equal(42);
            expect(res.responseTime).to.equal(42);
            expect(res.getSize().body).to.equal(70);
            expect(res.getSize().total).to.be.above(70);
        });
        test("async", async () => {
            await bru.sleep(1);
            expect(req.getUrl()).to.equal("{{baseUrl}}/users?page=1");
        });
        "#,
    );
    assert_all_pass(&output);
    assert_eq!(output.tests.len(), 2);
}

#[test]
fn bru_sleep_stops_at_the_time_limit() {
    let host = ScriptHost {
        timeout: Duration::from_millis(150),
        ..ScriptHost::default()
    };
    let started = Instant::now();
    let output = run(
        &host,
        &script("await bru.sleep(60000);"),
        &input(Event::PreRequest),
    );
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(
        output.error.unwrap().message.contains("ran longer than"),
        "the script was stopped"
    );
    let started = Instant::now();
    let output = pre("await bru.sleep(30);");
    assert!(output.error.is_none(), "{:?}", output.error);
    assert!(started.elapsed() >= Duration::from_millis(30));
}

#[tokio::test(flavor = "multi_thread")]
async fn bruno_scripts_can_send_requests() {
    let server = test_server::spawn().await;
    let host = ScriptHost {
        http: Some((
            Arc::new(HttpClient::new()),
            tokio::runtime::Handle::current(),
        )),
        ..ScriptHost::default()
    };
    let code = format!(
        r#"
        const res = await bru.sendRequest({{
            url: "{url}",
            method: "POST",
            headers: {{ "X-From": "bru" }},
            data: {{ a: 1 }},
        }});
        test("axios-style response", () => {{
            expect(res.status).to.equal(200);
            expect(res.data.headers["x-from"]).to.equal("bru");
            expect(res.data.body).to.equal('{{"a":1}}');
            expect(res.data.path).to.equal("/echo");
        }});
        "#,
        url = server.url("/echo")
    );
    let output =
        tokio::task::spawn_blocking(move || run(&host, &script(&code), &input(Event::PreRequest)))
            .await
            .unwrap();
    assert_all_pass(&output);
}

fn workspace_with_modules() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let write = |path: &str, text: &str| {
        let file = dir.path().join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    };
    write(
        "lib/helpers.js",
        "const sign = require('./sign.js');\n\
         globalThis.loads = (globalThis.loads || 0) + 1;\n\
         module.exports = { shout: (s) => sign(s.toUpperCase()), file: __filename, dir: __dirname };",
    );
    write("lib/sign.js", "module.exports = (s) => s + '!';");
    write("lib/broken.js", "module.exports = ;");
    write("lib/throws.js", "throw new Error('boom');");
    write(
        "lib/a.js",
        "exports.name = 'a'; exports.b = require('./b.js');",
    );
    write(
        "lib/b.js",
        "exports.name = 'b'; exports.a = require('./a.js');",
    );
    write("lib/data.json", "{}");
    write("secret.txt", "nope");
    dir
}

fn with_root(dir: &tempfile::TempDir) -> ScriptHost {
    ScriptHost {
        root: Some(dir.path().to_path_buf()),
        ..ScriptHost::default()
    }
}

#[test]
fn scripts_require_modules_from_the_workspace() {
    let dir = workspace_with_modules();
    let output = run(
        &with_root(&dir),
        &script(
            r#"
            const helpers = require("./lib/helpers.js");
            gm.test("works", () => {
                gm.expect(helpers.shout("hi")).to.equal("HI!");
                gm.expect(helpers.file).to.equal("lib/helpers.js");
                gm.expect(helpers.dir).to.equal("lib");
            });
            gm.test("cached", () => {
                gm.expect(require("./lib/helpers")).to.equal(helpers);
                gm.expect(require("./lib/../lib/helpers.js")).to.equal(helpers);
                gm.expect(globalThis.loads).to.equal(1);
            });
            gm.test("cycles get the partial exports", () => {
                const a = require("./lib/a.js");
                gm.expect(a.b.name).to.equal("b");
                gm.expect(a.b.a.name).to.equal("a");
            });
            gm.test("built-ins still win", () => {
                gm.expect(require("uuid").v4()).to.have.lengthOf(36);
            });
            "#,
        ),
        &input(Event::PreRequest),
    );
    assert_all_pass(&output);
}

#[test]
fn modules_stay_inside_the_workspace_and_explain_failures() {
    let dir = workspace_with_modules();
    let output = run(
        &with_root(&dir),
        &script(
            r#"
            const failure = (name) => {
                try { require(name); } catch (error) { return error.message; }
                return "no error";
            };
            gm.test("escapes", () => {
                gm.expect(failure("../evil.js")).to.include("outside the workspace");
                gm.expect(failure("./lib/../../evil.js")).to.include("outside the workspace");
                gm.expect(failure("/etc/evil.js")).to.include("relative path");
                gm.expect(failure("./lib\\..\\..\\evil.js")).to.include("use /");
            });
            gm.test("only js", () => {
                gm.expect(failure("./lib/data.json")).to.include("only .js files");
                gm.expect(failure("./secret.txt")).to.include("only .js files");
            });
            gm.test("missing and broken", () => {
                gm.expect(failure("./lib/nope.js")).to.include('require("./lib/nope.js")');
                gm.expect(failure("./lib/broken.js")).to.include("lib/broken.js");
                gm.expect(failure("./lib/throws.js")).to.equal("lib/throws.js: boom");
                gm.expect(failure("./lib/throws.js")).to.equal("lib/throws.js: boom");
            });
            gm.test("other names", () => {
                gm.expect(failure("lodash")).to.include("relative path");
            });
            "#,
        ),
        &input(Event::PreRequest),
    );
    assert_all_pass(&output);

    std::fs::write(
        dir.path().join("lib/big.js"),
        format!("//{}", "x".repeat(300 * 1024)),
    )
    .unwrap();
    let output = run(
        &with_root(&dir),
        &script("require('./lib/big.js');"),
        &input(Event::PreRequest),
    );
    assert!(
        output
            .error
            .unwrap()
            .message
            .contains("larger than 256 KiB"),
        "too big"
    );

    let output = pre("require('./lib/sign.js');");
    assert!(
        output
            .error
            .unwrap()
            .message
            .contains("cannot be required here")
    );
}

#[cfg(unix)]
#[test]
fn a_link_out_of_the_workspace_is_not_followed() {
    let dir = workspace_with_modules();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("evil.js"), "module.exports = 1;").unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("link")).unwrap();
    let output = run(
        &with_root(&dir),
        &script("require('./link/evil.js');"),
        &input(Event::PreRequest),
    );
    assert!(output.error.unwrap().message.contains("links outside"));
}
