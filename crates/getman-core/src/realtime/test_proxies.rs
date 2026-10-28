//! Local proxies the realtime tests connect through: an HTTP one that
//! tunnels `CONNECT`, and a SOCKS5 one.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpStream;

use super::test_servers::listen;

/// A proxy, and what it was asked to reach.
pub(super) struct TestProxy {
    pub addr: SocketAddr,
    /// One line per request it understood.
    pub seen: Arc<Mutex<Vec<String>>>,
}

async fn read_head(stream: &mut TcpStream) -> String {
    let mut head = Vec::new();
    let mut byte = [0_u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte).await.unwrap_or(0) == 0 {
            break;
        }
        head.push(byte[0]);
    }
    String::from_utf8_lossy(&head).into_owned()
}

/// An HTTP proxy that tunnels `CONNECT` requests to the server they name.
/// With `credentials`, it answers 407 to anyone who does not give them as
/// Basic credentials. It notes `CONNECT target | Proxy-Authorization`.
pub(super) async fn connect_proxy(credentials: Option<(&str, &str)>) -> TestProxy {
    let wanted = credentials.map(|(user, password)| {
        format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(format!("{user}:{password}"))
        )
    });
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&seen);
    let addr = listen(move |mut stream| {
        let wanted = wanted.clone();
        let log = Arc::clone(&log);
        async move {
            let head = read_head(&mut stream).await;
            let mut lines = head.lines();
            let first = lines.next().unwrap_or_default().to_owned();
            let given = lines
                .find_map(|l| l.strip_prefix("Proxy-Authorization: "))
                .unwrap_or_default()
                .to_owned();
            log.lock().unwrap().push(format!("{first} | {given}"));
            if wanted.as_ref().is_some_and(|wanted| *wanted != given) {
                let _ = stream
                    .write_all(
                        b"HTTP/1.1 407 Proxy Authentication Required\r\nContent-Length: 0\r\n\r\n",
                    )
                    .await;
                return;
            }
            let Some(target) = first
                .strip_prefix("CONNECT ")
                .and_then(|rest| rest.split(' ').next())
            else {
                let _ = stream
                    .write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n")
                    .await;
                return;
            };
            let Ok(mut server) = TcpStream::connect(target).await else {
                let _ = stream
                    .write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n")
                    .await;
                return;
            };
            let _ = stream
                .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
                .await;
            let _ = tokio::io::copy_bidirectional(&mut stream, &mut server).await;
        }
    })
    .await;
    TestProxy { addr, seen }
}

/// A SOCKS5 proxy. A name it is asked to reach is taken to be `server`, as
/// a proxy that resolves names would find it; an address is reached as it
/// is. With `credentials` it wants a user name and password. It notes
/// `name host:port` or `address ip:port` for each request.
pub(super) async fn socks5_proxy(
    credentials: Option<(&str, &str)>,
    server: SocketAddr,
) -> TestProxy {
    let wanted = credentials.map(|(user, password)| (user.to_owned(), password.to_owned()));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&seen);
    let addr = listen(move |mut stream| {
        let wanted = wanted.clone();
        let log = Arc::clone(&log);
        async move {
            let mut head = [0_u8; 2];
            if stream.read_exact(&mut head).await.is_err() {
                return;
            }
            let mut methods = vec![0_u8; usize::from(head[1])];
            if stream.read_exact(&mut methods).await.is_err() {
                return;
            }
            if let Some((user, password)) = &wanted {
                if !methods.contains(&2) {
                    let _ = stream.write_all(&[5, 0xFF]).await;
                    return;
                }
                let _ = stream.write_all(&[5, 2]).await;
                let mut version = [0_u8; 2];
                let _ = stream.read_exact(&mut version).await;
                let mut name = vec![0_u8; usize::from(version[1])];
                let _ = stream.read_exact(&mut name).await;
                let mut length = [0_u8; 1];
                let _ = stream.read_exact(&mut length).await;
                let mut pass = vec![0_u8; usize::from(length[0])];
                let _ = stream.read_exact(&mut pass).await;
                let ok = name == user.as_bytes() && pass == password.as_bytes();
                let _ = stream.write_all(&[1, u8::from(!ok)]).await;
                if !ok {
                    return;
                }
            } else {
                let _ = stream.write_all(&[5, 0]).await;
            }
            let mut request = [0_u8; 4];
            if stream.read_exact(&mut request).await.is_err() {
                return;
            }
            let target = match request[3] {
                1 => {
                    let mut octets = [0_u8; 4];
                    let _ = stream.read_exact(&mut octets).await;
                    let mut port = [0_u8; 2];
                    let _ = stream.read_exact(&mut port).await;
                    let target = format!(
                        "{}:{}",
                        std::net::Ipv4Addr::from(octets),
                        u16::from_be_bytes(port)
                    );
                    log.lock().unwrap().push(format!("address {target}"));
                    target
                }
                3 => {
                    let mut length = [0_u8; 1];
                    let _ = stream.read_exact(&mut length).await;
                    let mut name = vec![0_u8; usize::from(length[0])];
                    let _ = stream.read_exact(&mut name).await;
                    let mut port = [0_u8; 2];
                    let _ = stream.read_exact(&mut port).await;
                    log.lock().unwrap().push(format!(
                        "name {}:{}",
                        String::from_utf8_lossy(&name),
                        u16::from_be_bytes(port)
                    ));
                    server.to_string()
                }
                _ => return,
            };
            let Ok(mut upstream) = TcpStream::connect(&target).await else {
                let _ = stream.write_all(&[5, 5, 0, 1, 0, 0, 0, 0, 0, 0]).await;
                return;
            };
            let _ = stream.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]).await;
            let _ = tokio::io::copy_bidirectional(&mut stream, &mut upstream).await;
        }
    })
    .await;
    TestProxy { addr, seen }
}
