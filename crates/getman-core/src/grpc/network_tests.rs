//! What reaches a gRPC server beyond the message: client certificates,
//! certificate authorities, proxies, and signed credentials.

use std::net::SocketAddr;

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD as BASE64, URL_SAFE_NO_PAD};
use prost::Message as _;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use super::test_server::{self, EchoRequest, Reflection};
use super::tests::{call, received, run, setup};
use super::transport::frame;
use crate::execute::Execution;
use crate::http::{Header, Outgoing, OutgoingBody, Payload, find_header};
use crate::model::{
    ApiKeyLocation, Asap, Auth, AwsSigV4, ClientCert, EdgeGrid, Hawk, Jwt, JwtAlgorithm, OAuth1,
    Proxy, Request,
};

// ----- certificates -----

/// Writes the test CA and client certificate into the workspace at `dir`.
fn write_certificates(dir: &std::path::Path) {
    let pki = crate::test_server::pki();
    std::fs::create_dir_all(dir.join("certs")).unwrap();
    std::fs::write(dir.join("certs/ca.pem"), &pki.ca_pem).unwrap();
    std::fs::write(dir.join("certs/me.pem"), &pki.client_cert_pem).unwrap();
    std::fs::write(dir.join("certs/me.key"), &pki.client_key_pem).unwrap();
    std::fs::write(dir.join("certs/me.p12"), &pki.client_p12).unwrap();
}

fn tls_call(server: &test_server::TestServer) -> Request {
    let port = server.address.rsplit(':').next().unwrap();
    let mut request = call(
        &format!("localhost:{port}"),
        "Unary",
        r#"{"text": "secure"}"#,
    );
    request.grpc.tls = true;
    request
}

#[tokio::test(flavor = "multi_thread")]
async fn extra_authorities_and_client_certificates_reach_a_tls_server() {
    let server = test_server::spawn_secure(true).await;
    let (dir, executor) = setup();
    write_certificates(dir.path());

    // The system does not know the test CA.
    let mut request = tls_call(&server);
    let error = run(&executor, &request).await.error.unwrap();
    assert!(error.starts_with("Could not connect"), "{error}");

    // Trusting the CA is not enough: the server asks for a certificate.
    request.settings.ca_certs = Some(vec!["certs/ca.pem".into()]);
    let error = run(&executor, &request).await.error.unwrap();
    assert!(error.contains("client certificate"), "{error}");

    request.settings.client_cert = Some(ClientCert {
        cert: "certs/me.pem".into(),
        key: "certs/me.key".into(),
        ..ClientCert::default()
    });
    let execution = run(&executor, &request).await;
    assert!(execution.error.is_none(), "{execution:#?}");
    assert_eq!(received(&execution)[0]["text"], "secure");

    request.settings.client_cert = Some(ClientCert {
        pkcs12: "certs/me.p12".into(),
        passphrase: crate::test_server::P12_PASSPHRASE.into(),
        ..ClientCert::default()
    });
    let execution = run(&executor, &request).await;
    assert!(execution.error.is_none(), "{execution:#?}");

    // A wrong passphrase is named before anything is sent.
    request.settings.client_cert = Some(ClientCert {
        pkcs12: "certs/me.p12".into(),
        passphrase: "wrong".into(),
        ..ClientCert::default()
    });
    let error = run(&executor, &request).await.error.unwrap();
    assert!(error.contains("passphrase"), "{error}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_server_without_client_certificates_works_with_just_the_authority() {
    let server = test_server::spawn_secure(false).await;
    let (dir, executor) = setup();
    write_certificates(dir.path());
    let mut request = tls_call(&server);
    request.settings.ca_certs = Some(vec!["certs/ca.pem".into()]);
    let execution = run(&executor, &request).await;
    assert!(execution.error.is_none(), "{execution:#?}");
    assert!(execution.grpc.unwrap().tls);
}

// ----- proxies -----

/// A proxy for the tests: it records what the client asked for and always
/// connects on to `upstream`, so the names a test uses need not exist.
struct Fixture {
    address: SocketAddr,
    asked: UnboundedReceiver<String>,
}

async fn relay(mut client: TcpStream, upstream: SocketAddr) {
    if let Ok(mut server) = TcpStream::connect(upstream).await {
        let _ = tokio::io::copy_bidirectional(&mut client, &mut server).await;
    }
}

/// An HTTP proxy that answers `CONNECT` when the credentials are
/// `Basic <expected>` (any, when `None`), and tells what it was asked.
async fn connect_proxy(upstream: SocketAddr, expected: Option<String>) -> Fixture {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (sender, asked) = unbounded_channel();
    tokio::spawn(async move {
        while let Ok((mut client, _)) = listener.accept().await {
            let (sender, expected) = (sender.clone(), expected.clone());
            tokio::spawn(async move {
                let mut head = Vec::new();
                let mut byte = [0_u8; 1];
                while !head.ends_with(b"\r\n\r\n") {
                    if client.read_exact(&mut byte).await.is_err() {
                        return;
                    }
                    head.push(byte[0]);
                }
                let head = String::from_utf8_lossy(&head).into_owned();
                let _ = sender.send(head.clone());
                let authorized = expected.is_none_or(|expected| {
                    head.contains(&format!("Proxy-Authorization: Basic {expected}\r\n"))
                });
                if !authorized {
                    let _ = client
                        .write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\n\r\n")
                        .await;
                    return;
                }
                let _ = client
                    .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                    .await;
                relay(client, upstream).await;
            });
        }
    });
    Fixture { address, asked }
}

/// A SOCKS5 proxy that wants this user name and password, if any, and
/// tells what address type and host it was asked to connect to.
async fn socks_proxy(upstream: SocketAddr, login: Option<(&'static str, &'static str)>) -> Fixture {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (sender, asked) = unbounded_channel();
    tokio::spawn(async move {
        while let Ok((mut client, _)) = listener.accept().await {
            let sender = sender.clone();
            tokio::spawn(async move {
                let _ = socks_session(&mut client, &sender, login).await;
                relay(client, upstream).await;
            });
        }
    });
    Fixture { address, asked }
}

async fn socks_session(
    client: &mut TcpStream,
    sender: &tokio::sync::mpsc::UnboundedSender<String>,
    login: Option<(&str, &str)>,
) -> std::io::Result<()> {
    let mut hello = [0_u8; 2];
    client.read_exact(&mut hello).await?;
    let mut methods = vec![0_u8; usize::from(hello[1])];
    client.read_exact(&mut methods).await?;
    if let Some((user, password)) = login {
        client.write_all(&[5, 2]).await?;
        let mut version_and_len = [0_u8; 2];
        client.read_exact(&mut version_and_len).await?;
        let mut name = vec![0_u8; usize::from(version_and_len[1])];
        client.read_exact(&mut name).await?;
        let mut len = [0_u8; 1];
        client.read_exact(&mut len).await?;
        let mut secret = vec![0_u8; usize::from(len[0])];
        client.read_exact(&mut secret).await?;
        if name != user.as_bytes() || secret != password.as_bytes() {
            let _ = sender.send("login refused".into());
            client.write_all(&[1, 1]).await?;
            return Err(std::io::ErrorKind::PermissionDenied.into());
        }
        client.write_all(&[1, 0]).await?;
    } else {
        client.write_all(&[5, 0]).await?;
    }
    let mut head = [0_u8; 4];
    client.read_exact(&mut head).await?;
    let asked = match head[3] {
        1 => {
            let mut ip = [0_u8; 4];
            client.read_exact(&mut ip).await?;
            format!("ip {}", std::net::Ipv4Addr::from(ip))
        }
        4 => {
            let mut ip = [0_u8; 16];
            client.read_exact(&mut ip).await?;
            format!("ip {}", std::net::Ipv6Addr::from(ip))
        }
        _ => {
            let mut len = [0_u8; 1];
            client.read_exact(&mut len).await?;
            let mut name = vec![0_u8; usize::from(len[0])];
            client.read_exact(&mut name).await?;
            format!("name {}", String::from_utf8_lossy(&name))
        }
    };
    let mut port = [0_u8; 2];
    client.read_exact(&mut port).await?;
    let _ = sender.send(format!("{asked}:{}", u16::from_be_bytes(port)));
    client.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]).await
}

fn through(request: &mut Request, url: &str, username: &str, password: &str, bypass: &str) {
    request.settings.proxy = Some(Proxy {
        url: url.into(),
        username: username.into(),
        password: password.into(),
        bypass: bypass.into(),
    });
}

#[tokio::test(flavor = "multi_thread")]
async fn a_connect_proxy_carries_a_call_with_credentials() {
    let server = test_server::spawn(Reflection::None).await;
    let upstream: SocketAddr = server.address.parse().unwrap();
    let credentials = BASE64.encode("ada:p@ss word");
    let mut proxy = connect_proxy(upstream, Some(credentials.clone())).await;
    let (_dir, executor) = setup();

    // The server's name does not exist; only the proxy gets us there.
    let mut request = call("api.example.invalid:50051", "Unary", r#"{"text": "hi"}"#);
    through(
        &mut request,
        &format!("http://{}", proxy.address),
        "ada",
        "p@ss word",
        "",
    );
    let execution = run(&executor, &request).await;
    assert!(execution.error.is_none(), "{execution:#?}");
    assert_eq!(received(&execution)[0]["text"], "hi");
    let head = proxy.asked.recv().await.unwrap();
    assert!(
        head.starts_with("CONNECT api.example.invalid:50051 HTTP/1.1\r\n"),
        "{head}"
    );
    assert!(
        head.contains(&format!("Proxy-Authorization: Basic {credentials}\r\n")),
        "{head}"
    );

    // Wrong credentials are named.
    through(
        &mut request,
        &format!("http://{}", proxy.address),
        "ada",
        "nope",
        "",
    );
    let error = run(&executor, &request).await.error.unwrap();
    assert!(error.contains("through the proxy"), "{error}");
    assert!(error.contains("407"), "{error}");
    assert!(error.contains("user name and password"), "{error}");
    proxy.asked.recv().await.unwrap();

    // The bypass list and `off` go straight to the server.
    let mut direct = call(&server.address, "Unary", r#"{"text": "direct"}"#);
    through(
        &mut direct,
        &format!("http://{}", proxy.address),
        "",
        "",
        "127.0.0.1",
    );
    let execution = run(&executor, &direct).await;
    assert!(execution.error.is_none(), "{execution:#?}");
    through(&mut direct, "off", "", "", "");
    let execution = run(&executor, &direct).await;
    assert!(execution.error.is_none(), "{execution:#?}");
    assert!(proxy.asked.try_recv().is_err(), "nothing else was proxied");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_connect_proxy_carries_tls_calls() {
    let server = test_server::spawn_secure(true).await;
    let upstream: SocketAddr = server.address.parse().unwrap();
    let mut proxy = connect_proxy(upstream, None).await;
    let (dir, executor) = setup();
    write_certificates(dir.path());

    let mut request = tls_call(&server);
    request.settings.ca_certs = Some(vec!["certs/ca.pem".into()]);
    request.settings.client_cert = Some(ClientCert {
        cert: "certs/me.pem".into(),
        key: "certs/me.key".into(),
        ..ClientCert::default()
    });
    through(
        &mut request,
        &format!("http://{}", proxy.address),
        "",
        "",
        "",
    );
    let execution = run(&executor, &request).await;
    assert!(execution.error.is_none(), "{execution:#?}");
    let port = server.address.rsplit(':').next().unwrap();
    let head = proxy.asked.recv().await.unwrap();
    assert!(
        head.starts_with(&format!("CONNECT localhost:{port} HTTP/1.1\r\n")),
        "{head}"
    );
    assert!(!head.contains("Proxy-Authorization"), "{head}");
}

#[tokio::test(flavor = "multi_thread")]
async fn socks5_proxies_take_credentials_and_look_up_names_when_asked() {
    let server = test_server::spawn(Reflection::None).await;
    let upstream: SocketAddr = server.address.parse().unwrap();
    let mut proxy = socks_proxy(upstream, Some(("ada", "p@ss"))).await;
    let (_dir, executor) = setup();

    // socks5h hands the proxy the name.
    let mut request = call("api.example.invalid:50051", "Unary", r#"{"text": "hi"}"#);
    through(
        &mut request,
        &format!("socks5h://{}", proxy.address),
        "ada",
        "p@ss",
        "",
    );
    let execution = run(&executor, &request).await;
    assert!(execution.error.is_none(), "{execution:#?}");
    assert_eq!(received(&execution)[0]["text"], "hi");
    assert_eq!(
        proxy.asked.recv().await.unwrap(),
        "name api.example.invalid:50051"
    );

    // socks5 looks the name up first and hands the proxy an address.
    let mut local = call("localhost:50051", "Unary", r#"{"text": "hi"}"#);
    through(
        &mut local,
        &format!("socks5://{}", proxy.address),
        "ada",
        "p@ss",
        "",
    );
    let execution = run(&executor, &local).await;
    assert!(execution.error.is_none(), "{execution:#?}");
    let asked = proxy.asked.recv().await.unwrap();
    assert!(
        asked.starts_with("ip 127.0.0.1:") || asked.starts_with("ip ::1:"),
        "{asked}"
    );
    assert!(asked.ends_with(":50051"), "{asked}");

    // A wrong password, and none at all.
    through(
        &mut request,
        &format!("socks5h://{}", proxy.address),
        "ada",
        "nope",
        "",
    );
    let error = run(&executor, &request).await.error.unwrap();
    assert!(
        error.contains("rejected the user name or password"),
        "{error}"
    );
    assert_eq!(proxy.asked.recv().await.unwrap(), "login refused");
    through(
        &mut request,
        &format!("socks5h://{}", proxy.address),
        "",
        "",
        "",
    );
    let error = run(&executor, &request).await.error.unwrap();
    assert!(error.contains("wants a user name and password"), "{error}");
}

// ----- signed credentials -----

fn unary_request(server: &test_server::TestServer, auth: Auth) -> Request {
    Request {
        auth,
        ..call(&server.address, "Unary", r#"{"text": "hello"}"#)
    }
}

fn sent_headers(execution: &Execution) -> &[Header] {
    &execution.request.as_ref().unwrap().headers
}

/// The metadata the server saw.
fn seen(execution: &Execution) -> serde_json::Value {
    received(execution)[0]["metadata"].clone()
}

/// The call as the HTTP request the core signs: a POST with the
/// length-prefixed `EchoRequest` as its body, or none for a stream.
fn equivalent(server: &test_server::TestServer, method: &str, text: Option<&str>) -> Outgoing {
    Outgoing {
        method: "POST".into(),
        url: format!("http://{}/getman.test.Echo/{method}", server.address),
        headers: vec![Header::new("content-type", "application/grpc")],
        body: text.map_or(OutgoingBody::None, |text| {
            let message = EchoRequest {
                text: text.to_owned(),
                ..EchoRequest::default()
            };
            OutgoingBody::Bytes {
                data: Payload(frame(&message.encode_to_vec()).to_vec()),
            }
        }),
    }
}

/// What the core makes of `outgoing`: its headers once signed.
fn core_signs(auth: &Auth, mut outgoing: Outgoing, root: &std::path::Path) -> Vec<Header> {
    crate::auth::sign(auth, &mut |text| text.to_owned(), &mut outgoing, root).unwrap();
    outgoing.headers
}

fn value<'a>(headers: &'a [Header], name: &str) -> &'a str {
    find_header(headers, name).unwrap_or_default()
}

fn aws() -> Auth {
    Auth::AwsSigV4(AwsSigV4 {
        access_key: "AKIDEXAMPLE".into(),
        secret_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".into(),
        region: "us-east-1".into(),
        service: "execute-api".into(),
        ..AwsSigV4::default()
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn aws_signatures_match_what_the_core_signs_for_the_equivalent_post() {
    let server = test_server::spawn(Reflection::None).await;
    let (dir, executor) = setup();
    let auth = aws();

    // Unary: the signature covers the length-prefixed message. The
    // signature carries the second, so a call that straddles one tries
    // again.
    for attempt in 0..3 {
        let execution = run(&executor, &unary_request(&server, auth.clone())).await;
        assert!(execution.error.is_none(), "{execution:#?}");
        let metadata = seen(&execution);
        let expected = core_signs(
            &auth,
            equivalent(&server, "Unary", Some("hello")),
            dir.path(),
        );
        if metadata["authorization"] == value(&expected, "authorization")
            && metadata["x-amz-date"] == value(&expected, "x-amz-date")
        {
            assert!(
                value(&expected, "authorization")
                    .contains("SignedHeaders=content-type;host;x-amz-date"),
                "{expected:?}"
            );
            break;
        }
        assert!(attempt < 2, "{metadata} against {expected:?}");
    }

    // A different message gives a different signature.
    let mut other = unary_request(&server, auth.clone());
    other.grpc.message = r#"{"text": "other"}"#.into();
    let execution = run(&executor, &other).await;
    let expected = core_signs(
        &auth,
        equivalent(&server, "Unary", Some("hello")),
        dir.path(),
    );
    assert_ne!(
        seen(&execution)["authorization"],
        value(&expected, "authorization")
    );

    // Streams from the client sign UNSIGNED-PAYLOAD.
    let mut stream = call(
        &server.address,
        "ClientStream",
        r#"{"text": "a"} {"text": "b"}"#,
    );
    stream.auth = auth.clone();
    for attempt in 0..3 {
        let execution = run(&executor, &stream).await;
        assert!(execution.error.is_none(), "{execution:#?}");
        let Auth::AwsSigV4(settings) = &auth else {
            unreachable!()
        };
        let unsigned = Auth::AwsSigV4(AwsSigV4 {
            unsigned_payload: true,
            ..settings.clone()
        });
        let expected = core_signs(
            &unsigned,
            equivalent(&server, "ClientStream", None),
            dir.path(),
        );
        let sent = sent_headers(&execution);
        assert_eq!(value(sent, "x-amz-content-sha256"), "UNSIGNED-PAYLOAD");
        if value(sent, "authorization") == value(&expected, "authorization") {
            break;
        }
        assert!(attempt < 2, "{sent:?} against {expected:?}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn oauth1_signatures_match_and_the_body_hash_waits_for_a_known_body() {
    let server = test_server::spawn(Reflection::None).await;
    let (dir, executor) = setup();
    let auth = Auth::Oauth1(OAuth1 {
        consumer_key: "key".into(),
        consumer_secret: "secret".into(),
        token: "token".into(),
        token_secret: "tokensecret".into(),
        nonce: "fixednonce".into(),
        timestamp: "1700000000".into(),
        body_hash: true,
        ..OAuth1::default()
    });

    let execution = run(&executor, &unary_request(&server, auth.clone())).await;
    assert!(execution.error.is_none(), "{execution:#?}");
    let metadata = seen(&execution);
    let expected = core_signs(
        &auth,
        equivalent(&server, "Unary", Some("hello")),
        dir.path(),
    );
    assert_eq!(metadata["authorization"], value(&expected, "authorization"));
    assert!(
        value(&expected, "authorization").contains("oauth_body_hash="),
        "{expected:?}"
    );

    // No body hash for a stream; the signature is the one without it.
    let Auth::Oauth1(settings) = &auth else {
        unreachable!()
    };
    let without = Auth::Oauth1(OAuth1 {
        body_hash: false,
        ..settings.clone()
    });
    let mut bidi = call(&server.address, "Bidi", r#"{"text": "a"}"#);
    bidi.auth = auth;
    let execution = run(&executor, &bidi).await;
    assert!(execution.error.is_none(), "{execution:#?}");
    let expected = core_signs(&without, equivalent(&server, "Bidi", None), dir.path());
    let sent = sent_headers(&execution);
    assert_eq!(
        value(sent, "authorization"),
        value(&expected, "authorization")
    );
    assert!(!value(sent, "authorization").contains("oauth_body_hash"));
}

/// The value of `name="..."` in a Hawk or EdgeGrid style header.
fn field<'a>(header: &'a str, name: &str) -> &'a str {
    let start = header.find(&format!("{name}=")).unwrap() + name.len() + 1;
    let rest = header[start..].trim_start_matches('"');
    rest.split(['"', ',', ';']).next().unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn hawk_signatures_match_with_and_without_the_payload_hash() {
    let server = test_server::spawn(Reflection::None).await;
    let (_dir, executor) = setup();
    let settings = Hawk {
        id: "dh37fgj492je".into(),
        key: "werxhqb98rpaxn39848xrunpaw3489ruxnpa98w4rxn".into(),
        payload_hash: true,
        ..Hawk::default()
    };

    let execution = run(
        &executor,
        &unary_request(&server, Auth::Hawk(settings.clone())),
    )
    .await;
    assert!(execution.error.is_none(), "{execution:#?}");
    let header = seen(&execution)["authorization"]
        .as_str()
        .unwrap()
        .to_owned();
    let expected = crate::auth::hawk_authorization(
        &settings,
        &equivalent(&server, "Unary", Some("hello")),
        field(&header, "ts").parse().unwrap(),
        field(&header, "nonce"),
    )
    .unwrap();
    assert_eq!(header, expected);
    assert!(header.contains("hash="), "{header}");

    // A server-streaming call still has its one message; a client stream
    // does not, so the hash is left out.
    let mut stream = call(&server.address, "ClientStream", r#"{"text": "a"}"#);
    stream.auth = Auth::Hawk(settings.clone());
    let execution = run(&executor, &stream).await;
    assert!(execution.error.is_none(), "{execution:#?}");
    let header = value(sent_headers(&execution), "authorization").to_owned();
    let unhashed = Hawk {
        payload_hash: false,
        ..settings
    };
    let expected = crate::auth::hawk_authorization(
        &unhashed,
        &equivalent(&server, "ClientStream", None),
        field(&header, "ts").parse().unwrap(),
        field(&header, "nonce"),
    )
    .unwrap();
    assert_eq!(header, expected);
    assert!(!header.contains("hash="), "{header}");
}

#[tokio::test(flavor = "multi_thread")]
async fn edgegrid_signs_the_message_and_refuses_streams_from_the_client() {
    let server = test_server::spawn(Reflection::None).await;
    let (_dir, executor) = setup();
    let settings = EdgeGrid {
        client_token: "akab-client".into(),
        client_secret: "secret".into(),
        access_token: "akab-access".into(),
        // The call goes to the address it was made to, not here.
        base_url: "https://akab-host.luna.akamaiapis.net".into(),
        ..EdgeGrid::default()
    };
    let execution = run(
        &executor,
        &unary_request(&server, Auth::Edgegrid(settings.clone())),
    )
    .await;
    assert!(execution.error.is_none(), "{execution:#?}");
    let header = seen(&execution)["authorization"]
        .as_str()
        .unwrap()
        .to_owned();
    let timestamp = field(&header, "timestamp");
    let expected = crate::auth::edgegrid_authorization(
        &settings,
        &equivalent(&server, "Unary", Some("hello")),
        timestamp,
        field(&header, "nonce"),
    )
    .unwrap();
    assert_eq!(header, expected);

    let mut bidi = call(&server.address, "Bidi", r#"{"text": "a"}"#);
    bidi.auth = Auth::Edgegrid(settings);
    let error = run(&executor, &bidi).await.error.unwrap();
    assert!(error.contains("cannot sign a call that streams"), "{error}");
}

#[tokio::test(flavor = "multi_thread")]
async fn jwt_and_asap_tokens_go_where_the_settings_say() {
    let server = test_server::spawn(Reflection::None).await;
    let (dir, executor) = setup();
    let jwt = Jwt {
        secret: "shh".into(),
        payload: r#"{"sub": "ada"}"#.into(),
        ..Jwt::default()
    };

    let execution = run(&executor, &unary_request(&server, Auth::Jwt(jwt.clone()))).await;
    assert!(execution.error.is_none(), "{execution:#?}");
    let expected = core_signs(
        &Auth::Jwt(jwt.clone()),
        equivalent(&server, "Unary", Some("hello")),
        dir.path(),
    );
    assert_eq!(
        seen(&execution)["authorization"],
        value(&expected, "authorization")
    );
    assert!(value(&expected, "authorization").starts_with("Bearer ey"));

    // Another scheme word, and the query form, which becomes metadata of
    // the parameter's name.
    let prefixed = Jwt {
        prefix: "JWT".into(),
        ..jwt.clone()
    };
    let execution = run(&executor, &unary_request(&server, Auth::Jwt(prefixed))).await;
    assert!(
        seen(&execution)["authorization"]
            .as_str()
            .unwrap()
            .starts_with("JWT ey")
    );
    let query = Jwt {
        location: ApiKeyLocation::Query,
        param: "X-Token".into(),
        ..jwt
    };
    let execution = run(&executor, &unary_request(&server, Auth::Jwt(query))).await;
    let metadata = seen(&execution);
    assert!(metadata["x-token"].as_str().unwrap().starts_with("ey"));
    assert!(metadata.get("authorization").is_none());

    // ASAP signs with a private key file in the workspace.
    let key = rcgen::KeyPair::generate().unwrap();
    std::fs::write(dir.path().join("asap.pem"), key.serialize_pem()).unwrap();
    let asap = Asap {
        algorithm: Some(JwtAlgorithm::Es256),
        issuer: "billing".into(),
        audience: "ledger".into(),
        key_id: "billing/key1".into(),
        private_key_file: "asap.pem".into(),
        ..Asap::default()
    };
    // Also from a client stream, since the token needs no body.
    let mut stream = call(&server.address, "ClientStream", r#"{"text": "a"}"#);
    stream.auth = Auth::Asap(asap.clone());
    let execution = run(&executor, &stream).await;
    assert!(execution.error.is_none(), "{execution:#?}");
    let header = value(sent_headers(&execution), "authorization").to_owned();
    let token = header.strip_prefix("Bearer ").unwrap();
    let part = |n: usize| -> serde_json::Value {
        serde_json::from_slice(
            &URL_SAFE_NO_PAD
                .decode(token.split('.').nth(n).unwrap())
                .unwrap(),
        )
        .unwrap()
    };
    assert_eq!(part(0)["kid"], "billing/key1");
    assert_eq!(part(1)["iss"], "billing");
    let execution = run(&executor, &unary_request(&server, Auth::Asap(asap))).await;
    assert!(
        seen(&execution)["authorization"]
            .as_str()
            .unwrap()
            .starts_with("Bearer ey")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn signatures_that_do_not_fit_a_call_are_explained() {
    let server = test_server::spawn(Reflection::None).await;
    let (_dir, executor) = setup();
    let presigned = Auth::AwsSigV4(AwsSigV4 {
        location: ApiKeyLocation::Query,
        ..match aws() {
            Auth::AwsSigV4(settings) => settings,
            _ => unreachable!(),
        }
    });
    let error = run(&executor, &unary_request(&server, presigned))
        .await
        .error
        .unwrap();
    assert!(error.contains("only in the header"), "{error}");
    let form = Auth::Oauth1(OAuth1 {
        consumer_key: "key".into(),
        location: crate::model::OAuth1Location::Body,
        ..OAuth1::default()
    });
    let error = run(&executor, &unary_request(&server, form))
        .await
        .error
        .unwrap();
    assert!(error.contains("only in the header"), "{error}");
}

#[tokio::test(flavor = "multi_thread")]
async fn reflection_is_signed_without_a_body() {
    let server = test_server::spawn(Reflection::V1).await;
    let (_dir, executor) = setup();
    let mut request = unary_request(&server, aws());
    request.grpc.proto.clear();
    let context = crate::execute::Context::default();
    let schema = super::load_schema(&executor, None, &request, &context, true)
        .await
        .unwrap();
    assert!(
        schema
            .services()
            .iter()
            .any(|s| s.name == "getman.test.Echo")
    );
    let execution = run(&executor, &request).await;
    assert!(execution.error.is_none(), "{execution:#?}");
    assert!(
        value(sent_headers(&execution), "authorization").starts_with("AWS4-HMAC-SHA256 "),
        "{:?}",
        sent_headers(&execution)
    );

    // EdgeGrid signs the body of a POST, which reflection does not have.
    request.auth = Auth::Edgegrid(EdgeGrid {
        client_token: "t".into(),
        client_secret: "s".into(),
        access_token: "a".into(),
        ..EdgeGrid::default()
    });
    let error = super::load_schema(&executor, None, &request, &context, true)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("EdgeGrid"), "{error}");
}
