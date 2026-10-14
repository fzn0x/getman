//! Requests sent from the UI, kept in the workspace's git-ignored state
//! directory so they survive restarts.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use getman_core::Workspace;
use getman_core::execute::Execution;
use getman_core::model::Request;
use serde::{Deserialize, Serialize};

const LIMIT: usize = 200;

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
    pub(crate) error: Option<String>,
    /// The request as it was in the editor.
    pub(crate) request: Request,
}

pub(crate) struct History {
    file: PathBuf,
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
            file,
            entries: Mutex::new(entries),
        }
    }

    pub(crate) fn list(&self) -> Vec<Entry> {
        self.entries.lock().expect("history lock").clone()
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
            method: request.method.clone(),
            url: request.url.clone(),
            status: execution.response.as_ref().map(|r| r.status),
            time_ms: execution.response.as_ref().map(|r| r.time_ms),
            error: execution.error.clone(),
            request: request.clone(),
        };
        let mut entries = self.entries.lock().expect("history lock");
        entries.insert(0, entry);
        entries.truncate(LIMIT);
        self.save(&entries);
    }

    pub(crate) fn clear(&self) {
        let mut entries = self.entries.lock().expect("history lock");
        entries.clear();
        self.save(&entries);
    }

    fn save(&self, entries: &[Entry]) {
        if let Some(dir) = self.file.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string(entries) {
            let _ = fs::write(&self.file, text);
        }
    }
}
