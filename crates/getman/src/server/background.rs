//! The server run in the background of another program, as the desktop app
//! runs it: started with options instead of command-line flags, silent,
//! and able to switch to another workspace without changing its address.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use axum::Router;
use axum::extract::Request;
use axum::response::IntoResponse as _;
use getman_core::Workspace;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tower::ServiceExt as _;

use super::AppState;

/// How to start a [`Server`].
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Port to listen on; 0 takes any free port. `None` takes 7878 or the
    /// next free port, as `getman` does.
    pub port: Option<u16>,
    /// Opens the UI in the browser once the server listens.
    pub open_browser: bool,
    /// Routes of the program running the server, served under `/api/app/`
    /// behind the same guards and token as Getman's own API.
    pub app_routes: Option<Router>,
}

/// A server for the UI, running on the current Tokio runtime until it is
/// stopped or the runtime ends. It prints nothing.
pub struct Server {
    address: SocketAddr,
    token: String,
    app_routes: Option<Router>,
    current: Arc<RwLock<Router>>,
    /// The state of the router in `current`, to end what it has under way
    /// when it is replaced or the server stops.
    state: Mutex<Arc<AppState>>,
    stop: oneshot::Sender<()>,
    task: JoinHandle<()>,
}

impl Server {
    /// Starts serving `workspace` on 127.0.0.1, with the token the browser
    /// UI uses too, so a page signed in to one is signed in to both.
    pub async fn start(workspace: Workspace, options: Options) -> Result<Self, String> {
        let token = super::token::load_or_create()?;
        let listener = super::bind(options.port).await?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let state = super::state(workspace, token.clone(), port);
        let current = Arc::new(RwLock::new(super::router_with(
            Arc::clone(&state),
            options.app_routes.as_ref(),
        )));
        // Every request goes to the router of the workspace open at the
        // time, so switching workspaces keeps the address, and with it the
        // page's own storage, and lets requests under way finish.
        let shared = Arc::clone(&current);
        let app = Router::new().fallback(move |request: Request| {
            let router = shared
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
            async move { router.oneshot(request).await.into_response() }
        });
        let (stop, stopped) = oneshot::channel();
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = stopped.await;
                })
                .await;
        });
        let server = Self {
            address: SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
            token,
            app_routes: options.app_routes,
            current,
            state: Mutex::new(state),
            stop,
            task,
        };
        if options.open_browser {
            let _ = open::that_detached(server.url_with_token());
        }
        Ok(server)
    }

    /// Where the server listens.
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// The token API calls must carry.
    pub fn token(&self) -> &str {
        &self.token
    }

    /// The UI's address, signed in: `http://127.0.0.1:PORT/#token=TOKEN`.
    pub fn url_with_token(&self) -> String {
        format!(
            "http://127.0.0.1:{}/#token={}",
            self.address.port(),
            self.token
        )
    }

    /// Serves `workspace` from now on, at the same address. Live
    /// connections, gRPC calls and runs of the workspace served until now
    /// are ended, since nothing could reach them any more.
    pub fn open(&self, workspace: Workspace) {
        let state = super::state(workspace, self.token.clone(), self.address.port());
        let router = super::router_with(Arc::clone(&state), self.app_routes.as_ref());
        *self
            .current
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = router;
        let old = std::mem::replace(
            &mut *self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            state,
        );
        old.shutdown();
    }

    /// Stops listening and waits a moment for requests under way. Live
    /// connections and gRPC calls are ended first: the server waits for
    /// open responses, and theirs never end by themselves.
    pub async fn stop(self) {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .shutdown();
        let _ = self.stop.send(());
        let mut task = self.task;
        if tokio::time::timeout(Duration::from_secs(3), &mut task)
            .await
            .is_err()
        {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    async fn get(server: &Server, path: &str, token: &str) -> String {
        let mut stream = tokio::net::TcpStream::connect(server.address())
            .await
            .unwrap();
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nx-getman-token: {token}\r\nConnection: close\r\n\r\n",
            server.address().port()
        );
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        response
    }

    /// A server sending an event every 20 ms, and telling when its client
    /// has gone.
    async fn ticking() -> (String, Arc<AtomicBool>) {
        struct Gone(Arc<AtomicBool>);
        impl Drop for Gone {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let gone = Arc::new(AtomicBool::new(false));
        let watched = Arc::clone(&gone);
        let app = Router::new().route(
            "/ticks",
            axum::routing::get(move || {
                let guard = Gone(Arc::clone(&watched));
                async move {
                    let ticks =
                        futures_util::stream::unfold((0_u32, guard), |(n, guard)| async move {
                            tokio::time::sleep(Duration::from_millis(20)).await;
                            Some((
                                Ok::<_, std::convert::Infallible>(format!("data: tick {n}\n\n")),
                                (n + 1, guard),
                            ))
                        });
                    axum::response::Response::builder()
                        .header("content-type", "text/event-stream")
                        .body(axum::body::Body::from_stream(ticks))
                        .unwrap()
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://{addr}/ticks"), gone)
    }

    /// Posts JSON and reads the response until the server ends it, keeping
    /// what has arrived so far where the caller can see it.
    fn post(server: &Server, path: &str, body: &str) -> (Arc<Mutex<String>>, JoinHandle<()>) {
        let address = server.address();
        let request = format!(
            "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nx-getman-token: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            address.port(),
            server.token(),
            body.len()
        );
        let seen = Arc::new(Mutex::new(String::new()));
        let shared = Arc::clone(&seen);
        let task = tokio::spawn(async move {
            let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
            stream.write_all(request.as_bytes()).await.unwrap();
            let mut buffer = [0_u8; 4096];
            loop {
                match stream.read(&mut buffer).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => shared
                        .lock()
                        .unwrap()
                        .push_str(&String::from_utf8_lossy(&buffer[..n])),
                }
            }
        });
        (seen, task)
    }

    async fn until(seen: &Arc<Mutex<String>>, text: &str) {
        let found = tokio::time::timeout(Duration::from_secs(10), async {
            while !seen.lock().unwrap().contains(text) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        assert!(
            found.is_ok(),
            "never saw {text:?} in {:?}",
            seen.lock().unwrap()
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn switching_workspace_ends_live_connections_and_calls() {
        let (url, gone) = ticking().await;
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let server = Server::start(
            Workspace::init(first.path(), "First").unwrap(),
            Options {
                port: Some(0),
                open_browser: false,
                app_routes: None,
            },
        )
        .await
        .unwrap();

        // A page with its event stream open and a connection receiving.
        let (stream, stream_task) = post(&server, "/api/realtime/events", "{}");
        until(&stream, "x-page-id: ").await;
        let page = {
            let text = stream.lock().unwrap();
            let start = text.find("x-page-id: ").unwrap() + "x-page-id: ".len();
            let end = text[start..].find("\r\n").unwrap() + start;
            text[start..end].to_owned()
        };
        let body = serde_json::json!({ "page": page, "request": { "kind": "sse", "url": url } });
        let (_, connect_task) = post(&server, "/api/realtime/connect", &body.to_string());
        connect_task.await.unwrap();
        until(&stream, "\"type\":\"message\"").await;
        assert!(!gone.load(Ordering::SeqCst));

        // A gRPC call under way.
        let old = Arc::clone(&server.state.lock().unwrap());
        let (commands, mut cancelled) = tokio::sync::mpsc::unbounded_channel();
        old.grpc.insert("call".into(), commands);

        server.open(Workspace::init(second.path(), "Second").unwrap());

        let ended = tokio::time::timeout(Duration::from_secs(10), stream_task).await;
        assert!(ended.is_ok(), "the old page's stream kept going");
        let closed = tokio::time::timeout(Duration::from_secs(10), async {
            while !gone.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        assert!(closed.is_ok(), "the old connection stayed open");
        assert!(matches!(
            cancelled.try_recv(),
            Ok(getman_core::grpc::Command::Cancel)
        ));

        // The new workspace starts clean, and stopping ends what is left.
        let (stream, stream_task) = post(&server, "/api/realtime/events", "{}");
        until(&stream, "x-page-id: ").await;
        server.stop().await;
        let ended = tokio::time::timeout(Duration::from_secs(10), stream_task).await;
        assert!(ended.is_ok(), "stopping left a page's stream open");
    }

    #[tokio::test]
    async fn serves_a_workspace_on_any_port_and_switches_to_another() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let app_routes = Router::new().route("/hello", axum::routing::get(|| async { "hi" }));
        let server = Server::start(
            Workspace::init(first.path(), "First").unwrap(),
            Options {
                port: Some(0),
                open_browser: false,
                app_routes: Some(app_routes),
            },
        )
        .await
        .unwrap();
        assert!(server.address().ip().is_loopback());
        assert_ne!(server.address().port(), 0);
        assert!(server.url_with_token().ends_with(server.token()));

        let token = server.token().to_owned();
        let response = get(&server, "/api/workspace", &token).await;
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert!(response.contains("\"First\""));
        assert!(
            get(&server, "/api/workspace", "wrong")
                .await
                .starts_with("HTTP/1.1 401")
        );

        server.open(Workspace::init(second.path(), "Second").unwrap());
        let response = get(&server, "/api/workspace", &token).await;
        assert!(response.contains("\"Second\""), "{response}");

        // The program's own routes keep the guard and the token.
        let response = get(&server, "/api/app/hello", &token).await;
        assert!(response.ends_with("hi"), "{response}");
        assert!(
            get(&server, "/api/app/hello", "wrong")
                .await
                .starts_with("HTTP/1.1 401")
        );

        let address = server.address();
        server.stop().await;
        assert!(tokio::net::TcpStream::connect(address).await.is_err());
    }
}
