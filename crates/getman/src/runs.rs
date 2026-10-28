//! Runs kept in the workspace's git-ignored `.getman/runs`, so the Runner
//! can list past runs, reopen one and download its report in any format.
//!
//! Each run is one JSON file. Bodies, headers and variable values are left
//! out: the results, timings and checks are what a past run is for, and
//! leaving the rest out keeps the files small and tokens off the disk.

use std::fs;
use std::path::PathBuf;

use getman_core::Workspace;
use getman_core::execute::Execution;
use getman_core::http::{OutgoingBody, Payload};
use getman_core::runner::{RunEvent, RunItem, RunSummary};
use serde::{Deserialize, Serialize};

use crate::report::{About, Recorded};

/// How many runs are kept; older ones are deleted.
pub(crate) const LIMIT: usize = 20;

/// A finished run as kept on disk.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct StoredRun {
    pub(crate) id: String,
    pub(crate) about: About,
    pub(crate) iterations: usize,
    /// The requests planned for one iteration.
    pub(crate) items: Vec<RunItem>,
    pub(crate) results: Vec<RunResult>,
    pub(crate) summary: Option<RunSummary>,
}

/// One request of one iteration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct RunResult {
    pub(crate) iteration: usize,
    /// Position in `items`.
    pub(crate) index: usize,
    pub(crate) execution: Execution,
}

/// A stored run without its results, for lists.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Listing {
    pub(crate) id: String,
    pub(crate) about: About,
    pub(crate) iterations: usize,
    pub(crate) requests: usize,
    pub(crate) summary: Option<RunSummary>,
}

impl StoredRun {
    pub(crate) fn new(id: String, about: About) -> Self {
        Self {
            id,
            about,
            ..Self::default()
        }
    }

    /// Takes in an event of the run, as it happens.
    pub(crate) fn record(&mut self, event: &RunEvent) {
        match event {
            RunEvent::Start {
                requests,
                iterations,
            } => {
                self.items.clone_from(requests);
                self.iterations = *iterations;
            }
            RunEvent::Request {
                iteration,
                index,
                execution,
            } => self.results.push(RunResult {
                iteration: *iteration,
                index: *index,
                execution: slim(execution),
            }),
            RunEvent::Finish { summary } => self.summary = Some(summary.clone()),
            RunEvent::Iteration { .. } => {}
        }
    }

    /// The run in the shape the reporters take.
    pub(crate) fn recorded(&self) -> Recorded {
        Recorded {
            iterations: self.iterations,
            requests: self
                .results
                .iter()
                .map(|result| (result.iteration, result.execution.clone()))
                .collect(),
            summary: self.summary.clone(),
            items: self.items.clone(),
            about: self.about.clone(),
        }
    }

    fn listing(&self) -> Listing {
        Listing {
            id: self.id.clone(),
            about: self.about.clone(),
            iterations: self.iterations,
            requests: self.results.len(),
            summary: self.summary.clone(),
        }
    }
}

/// An execution without bodies, headers or variable values.
fn slim(execution: &Execution) -> Execution {
    let mut execution = execution.clone();
    if let Some(request) = &mut execution.request {
        request.headers.clear();
        request.body = OutgoingBody::None;
    }
    if let Some(response) = &mut execution.response {
        response.headers.clear();
        response.body = Payload::default();
    }
    execution.changes.clear();
    execution
}

fn dir(workspace: &Workspace) -> PathBuf {
    workspace.state_dir().join("runs")
}

/// Ids are made by Getman; anything else could name a file elsewhere.
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn file(workspace: &Workspace, id: &str) -> Result<PathBuf, String> {
    if valid_id(id) {
        Ok(dir(workspace).join(format!("{id}.json")))
    } else {
        Err(format!("{id:?} is not a run id"))
    }
}

/// Keeps a run, and deletes the oldest beyond the newest [`LIMIT`].
pub(crate) fn save(workspace: &Workspace, run: &StoredRun) -> Result<(), String> {
    let path = file(workspace, &run.id)?;
    let folder = dir(workspace);
    workspace.ensure_state_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let text = serde_json::to_string(run).map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    for old in stored(workspace).into_iter().skip(LIMIT) {
        let _ = fs::remove_file(dir(workspace).join(format!("{}.json", old.id)));
    }
    Ok(())
}

/// Every kept run, newest first.
fn stored(workspace: &Workspace) -> Vec<StoredRun> {
    let Ok(entries) = fs::read_dir(dir(workspace)) else {
        return Vec::new();
    };
    let mut runs: Vec<StoredRun> = entries
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
        .filter_map(|entry| {
            let text = fs::read_to_string(entry.path()).ok()?;
            let run: StoredRun = serde_json::from_str(&text).ok()?;
            valid_id(&run.id).then_some(run)
        })
        .collect();
    runs.sort_by(|a, b| {
        b.about
            .started
            .cmp(&a.about.started)
            .then_with(|| b.id.cmp(&a.id))
    });
    runs
}

/// The kept runs, newest first, without their results.
pub(crate) fn list(workspace: &Workspace) -> Vec<Listing> {
    stored(workspace).iter().map(StoredRun::listing).collect()
}

/// A kept run.
pub(crate) fn read(workspace: &Workspace, id: &str) -> Result<StoredRun, String> {
    let path = file(workspace, id)?;
    let text = fs::read_to_string(&path).map_err(|_| format!("there is no run {id}"))?;
    serde_json::from_str(&text).map_err(|e| format!("the run {id} cannot be read: {e}"))
}

/// Forgets a kept run.
pub(crate) fn delete(workspace: &Workspace, id: &str) -> Result<(), String> {
    let path = file(workspace, id)?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// A file name for a run's report, such as
/// `getman-orders-2026-10-14-0507.html`.
pub(crate) fn report_name(run: &StoredRun, extension: &str) -> String {
    let stamp = getman_core::time::iso8601_millis(run.about.started);
    let stamp = format!(
        "{}-{}",
        stamp.get(..10).unwrap_or_default(),
        stamp.get(11..16).unwrap_or_default().replace(':', "")
    );
    let what = getman_core::workspace::slugify(&run.about.workspace);
    format!("getman-{what}-{stamp}.{extension}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use getman_core::http::{Header, Outgoing};

    fn run(id: &str, started: i64) -> StoredRun {
        let mut run = StoredRun::new(
            id.into(),
            About {
                workspace: "Shop".into(),
                started,
                ..About::default()
            },
        );
        run.record(&RunEvent::Start {
            requests: vec![RunItem {
                path: "a.yaml".into(),
                name: "A".into(),
                method: "GET".into(),
            }],
            iterations: 1,
        });
        run.record(&RunEvent::Request {
            iteration: 0,
            index: 0,
            execution: Box::new(Execution {
                name: "A".into(),
                path: Some("a.yaml".into()),
                request: Some(Outgoing {
                    method: "GET".into(),
                    url: "http://127.0.0.1:9/a".into(),
                    headers: vec![Header::new("Authorization", "Bearer s3cret")],
                    body: OutgoingBody::Bytes {
                        data: Payload(b"password=s3cret".to_vec()),
                    },
                }),
                ..Execution::default()
            }),
        });
        run.record(&RunEvent::Finish {
            summary: RunSummary {
                requests: 1,
                ..RunSummary::default()
            },
        });
        run
    }

    #[test]
    fn runs_are_kept_newest_first_up_to_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = Workspace::init(dir.path(), "Shop").unwrap();
        for n in 0..(LIMIT + 3) {
            let started = 1_000 + i64::try_from(n).unwrap();
            save(&workspace, &run(&format!("run-{n:02}"), started)).unwrap();
        }
        let listed = list(&workspace);
        assert_eq!(listed.len(), LIMIT);
        assert_eq!(listed[0].id, format!("run-{:02}", LIMIT + 2));
        assert!(read(&workspace, "run-00").is_err());

        let kept = read(&workspace, &listed[0].id).unwrap();
        assert_eq!(kept.items[0].name, "A");
        assert_eq!(kept.recorded().requests.len(), 1);
        let text = fs::read_to_string(dir.path().join(".getman/runs/run-22.json")).unwrap();
        assert!(!text.contains("s3cret"), "{text}");

        delete(&workspace, "run-22").unwrap();
        assert_eq!(list(&workspace).len(), LIMIT - 1);
    }

    #[test]
    fn ids_cannot_name_other_files() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = Workspace::init(dir.path(), "Shop").unwrap();
        for bad in ["../getman", "a/b", "", "x.json", &"a".repeat(65)] {
            assert!(read(&workspace, bad).is_err(), "{bad}");
            assert!(delete(&workspace, bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn report_names_carry_the_workspace_and_time() {
        let run = run("x", 1_791_954_429_120);
        assert_eq!(
            report_name(&run, "html"),
            "getman-shop-2026-10-14-0507.html"
        );
    }
}
