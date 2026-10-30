//! Requests sent from the UI, kept in the workspace's git-ignored state
//! directory so they survive restarts.
//!
//! The list lives in `history.json`; each entry's response, with its body
//! cut to [`MAX_BODY`], lives beside it in `history/<id>.json` so the list
//! stays small.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use getman_core::Workspace;
use getman_core::execute::Execution;
use getman_core::http::Response;
use getman_core::model::Request;
use serde::{Deserialize, Serialize};

const LIMIT: usize = 200;
/// The most of a response body history keeps.
pub(crate) const MAX_BODY: usize = 1024 * 1024;

/// A request sent from the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Entry {
    pub(crate) id: String,
    /// Milliseconds since the Unix epoch.
    pub(crate) at: i64,
    pub(crate) path: Option<String>,
    pub(crate) environment: Option<String>,
    pub(crate) name: String,
    pub(crate) method: String,
    pub(crate) url: String,
    pub(crate) status: Option<u16>,
    pub(crate) time_ms: Option<f64>,
    /// Size of the response body, in bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) size: Option<u64>,
    pub(crate) error: Option<String>,
    /// The request as it was in the editor.
    pub(crate) request: Request,
}

/// What came back for an entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Stored {
    pub(crate) response: Response,
    /// Whether history kept only the first [`MAX_BODY`] bytes of the body.
    #[serde(default)]
    pub(crate) body_cut: bool,
}

pub(crate) struct History {
    workspace: Workspace,
    file: PathBuf,
    responses: PathBuf,
    entries: Mutex<Vec<Entry>>,
}

impl History {
    pub(crate) fn open(workspace: &Workspace) -> Self {
        let file = workspace.state_dir().join("history.json");
        let entries = fs::read_to_string(&file)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self {
            workspace: workspace.clone(),
            responses: workspace.state_dir().join("history"),
            file,
            entries: Mutex::new(entries),
        }
    }

    pub(crate) fn list(&self) -> Vec<Entry> {
        self.entries.lock().expect("history lock").clone()
    }

    /// An entry and the response kept with it, if any.
    pub(crate) fn read(&self, id: &str) -> Option<(Entry, Option<Stored>)> {
        let entry = self
            .entries
            .lock()
            .expect("history lock")
            .iter()
            .find(|e| e.id == id)
            .cloned()?;
        let stored = self
            .response_file(id)
            .and_then(|file| fs::read_to_string(file).ok())
            .and_then(|text| serde_json::from_str(&text).ok());
        Some((entry, stored))
    }

    pub(crate) fn add(
        &self,
        request: &Request,
        environment: Option<String>,
        execution: &Execution,
    ) {
        let entry = Entry {
            id: getman_core::dynamic::uuid_v4(),
            at: getman_core::time::now_millis(),
            path: execution.path.clone(),
            environment,
            name: request.name.clone(),
            method: if request.kind.is_http() {
                request.method.clone()
            } else {
                request.kind.as_str().to_ascii_uppercase()
            },
            url: request.url.clone(),
            status: execution.response.as_ref().map(|r| r.status),
            time_ms: execution.response.as_ref().map(|r| r.time_ms),
            size: execution.response.as_ref().map(|r| r.size.body),
            error: execution.error.clone(),
            request: request.clone(),
        };
        if let Some(response) = &execution.response {
            self.keep_response(&entry.id, response);
        }
        let mut entries = self.entries.lock().expect("history lock");
        entries.insert(0, entry);
        let keep = LIMIT.min(entries.len());
        for dropped in entries.split_off(keep) {
            if let Some(file) = self.response_file(&dropped.id) {
                let _ = fs::remove_file(file);
            }
        }
        self.save(&entries);
    }

    pub(crate) fn clear(&self) {
        let mut entries = self.entries.lock().expect("history lock");
        entries.clear();
        let _ = fs::remove_dir_all(&self.responses);
        self.save(&entries);
    }

    fn keep_response(&self, id: &str, response: &Response) {
        let Some(file) = self.response_file(id) else {
            return;
        };
        if self.workspace.ensure_state_dir().is_err()
            || fs::create_dir_all(&self.responses).is_err()
        {
            return;
        }
        let stored = cut(response);
        if let Ok(text) = serde_json::to_string(&stored) {
            let _ = fs::write(file, text);
        }
    }

    fn response_file(&self, id: &str) -> Option<PathBuf> {
        let safe = !id.is_empty() && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
        safe.then(|| self.responses.join(format!("{id}.json")))
    }

    fn save(&self, entries: &[Entry]) {
        if self.workspace.ensure_state_dir().is_err() {
            return;
        }
        if let Ok(text) = serde_json::to_string(entries) {
            let _ = fs::write(&self.file, text);
        }
    }
}

/// The response with its body cut to [`MAX_BODY`], at a character
/// boundary when the body is text.
fn cut(response: &Response) -> Stored {
    let body = &response.body.0;
    if body.len() <= MAX_BODY {
        return Stored {
            response: response.clone(),
            body_cut: false,
        };
    }
    let mut end = MAX_BODY;
    if std::str::from_utf8(body).is_ok() {
        while end > 0 && body[end] & 0xC0 == 0x80 {
            end -= 1;
        }
    }
    let mut kept = response.clone();
    kept.body.0.truncate(end);
    Stored {
        response: kept,
        body_cut: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use getman_core::http::{Payload, Size};

    fn response(body: Vec<u8>) -> Response {
        Response {
            status: 200,
            status_text: "OK".into(),
            http_version: "HTTP/1.1".into(),
            url: "http://example.com/".into(),
            headers: Vec::new(),
            size: Size {
                headers: 10,
                body: body.len() as u64,
            },
            body: Payload(body),
            truncated: false,
            time_ms: 12.0,
            headers_ms: 3.0,
            remote_addr: None,
            timings: None,
            tls: None,
            hops: Vec::new(),
            http3_available: false,
            stream_open: false,
            added_headers: Vec::new(),
        }
    }

    fn execution(response: Response) -> Execution {
        Execution {
            path: Some("ping.yaml".into()),
            name: "Ping".into(),
            response: Some(response),
            ..Execution::default()
        }
    }

    #[test]
    fn entries_keep_their_response_across_restarts() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = Workspace::init(dir.path(), "Test").unwrap();
        let history = History::open(&workspace);
        let request = Request {
            name: "Ping".into(),
            ..Request::default()
        };
        history.add(&request, None, &execution(response(b"pong".to_vec())));
        let id = history.list()[0].id.clone();

        let reopened = History::open(&workspace);
        let (entry, stored) = reopened.read(&id).unwrap();
        assert_eq!(entry.size, Some(4));
        let stored = stored.unwrap();
        assert_eq!(stored.response.body.0, b"pong");
        assert!(!stored.body_cut);
        assert!(reopened.read("../history").is_none());

        reopened.clear();
        assert!(reopened.list().is_empty());
        assert!(!dir.path().join(".getman/history").exists());
    }

    #[test]
    fn big_bodies_are_cut_at_a_character() {
        let mut body = vec![b'a'; MAX_BODY - 1];
        body.extend("é and more".as_bytes());
        let stored = cut(&response(body));
        assert!(stored.body_cut);
        assert_eq!(stored.response.body.0.len(), MAX_BODY - 1);
        assert!(std::str::from_utf8(&stored.response.body.0).is_ok());
        assert_eq!(stored.response.size.body, MAX_BODY as u64 + 10);
    }
}
