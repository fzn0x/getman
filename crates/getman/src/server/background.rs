//! The server run in the background of another program, as the desktop app
//! runs it: started with options instead of command-line flags, silent,
//! and able to switch to another workspace without changing its address.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use axum::Router;
use axum::extract::Request;
use axum::response::IntoResponse as _;
use getman_core::Workspace;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tower::ServiceExt as _;

/// How to start a [`Server`].
#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    /// Port to listen on; 0 takes any free port. `None` takes 7878 or the
    /// next free port, as `getman` does.
    pub port: Option<u16>,
    /// Opens the UI in the browser once the server listens.
    pub open_browser: bool,
}

/// A server for the UI, running on the current Tokio runtime until it is
/// stopped or the runtime ends. It prints nothing.
pub struct Server {
    address: SocketAddr,
    token: String,
    current: Arc<RwLock<Router>>,
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
        let current = Arc::new(RwLock::new(super::router(super::state(
            workspace,
            token.clone(),
            port,
        ))));
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
            current,
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

    /// Serves `workspace` from now on, at the same address.
    pub fn open(&self, workspace: Workspace) {
        let router = super::router(super::state(
            workspace,
            self.token.clone(),
            self.address.port(),
        ));
        *self
            .current
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = router;
    }

    /// Stops listening and waits a moment for requests under way.
    pub async fn stop(self) {
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

    #[tokio::test]
    async fn serves_a_workspace_on_any_port_and_switches_to_another() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let server = Server::start(
            Workspace::init(first.path(), "First").unwrap(),
            Options {
                port: Some(0),
                open_browser: false,
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

        let address = server.address();
        server.stop().await;
        assert!(tokio::net::TcpStream::connect(address).await.is_err());
    }
}
