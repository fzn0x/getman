//! `getman mock`: serve saved examples as a mock API.

use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use getman_core::mock::EXAMPLE_HEADER;

use super::Outcome;
use crate::cli::MockArgs;
use crate::mock::{self, Hit};
use crate::style::{self, BOLD, CYAN, DIM, GREEN, RED, YELLOW, paint};
use crate::workspaces;

pub(crate) async fn mock(args: MockArgs) -> Outcome {
    // A directory on disk names its own workspace; anything else is a
    // folder of the workspace given with -w or around the current one.
    let (workspace, folder) = match args.path.as_deref() {
        Some(path) if Path::new(path).is_dir() => {
            let workspace = workspaces::find(Some(Path::new(path)))?;
            let folder = workspaces::item_path(&workspace, path)?;
            (workspace, folder)
        }
        Some(path) => {
            let workspace = workspaces::find(args.workspace.workspace.as_deref())?;
            let folder = workspaces::item_path(&workspace, path)?;
            (workspace, folder)
        }
        None => (
            workspaces::find(args.workspace.workspace.as_deref())?,
            String::new(),
        ),
    };

    let listener = mock::bind(args.port).await?;
    let name = workspace.name();
    let running = mock::start(
        workspace,
        &folder,
        listener,
        Duration::from_millis(args.delay),
        Box::new(log),
    )?;

    anstream::println!(
        "{} {}{} at {}",
        paint(CYAN.bold(), "getman mock"),
        paint(BOLD, &name),
        if folder.is_empty() {
            String::new()
        } else {
            format!(" / {folder}")
        },
        paint(BOLD, &format!("{}/", running.url()))
    );
    let mocked = running.mocked();
    if mocked.is_empty() {
        style::warn(&format!(
            "there are no examples in {} yet; save a response as an example in the UI, or add `examples:` to a request file. They are picked up while the server runs.",
            if folder.is_empty() {
                "the workspace"
            } else {
                &folder
            }
        ));
    }
    let width = mocked
        .iter()
        .map(|m| m.path.chars().count())
        .max()
        .unwrap_or(0)
        .min(48);
    for entry in &mocked {
        let examples: Vec<String> = entry
            .examples
            .iter()
            .map(|e| format!("{} {}", e.name, e.status))
            .collect();
        anstream::println!(
            "  {} {:<width$}  {}  {}",
            paint(DIM, &format!("{:<7}", entry.method)),
            entry.path,
            examples.join(", "),
            paint(DIM, &entry.request),
        );
    }
    style::note(&format!(
        "Pick an example with the {EXAMPLE_HEADER} header; otherwise the first 2xx answers.{} Press Ctrl+C to stop.",
        if args.delay > 0 {
            format!(" Every answer waits {} ms.", args.delay)
        } else {
            String::new()
        }
    ));
    println!();

    let _ = tokio::signal::ctrl_c().await;
    running.stop();
    Ok(ExitCode::SUCCESS)
}

/// Prints a request the mock answered.
fn log(hit: &Hit) {
    let status = paint(
        if hit.status == 404 && hit.example.is_none() {
            YELLOW
        } else if hit.status < 400 {
            GREEN
        } else {
            RED
        },
        &hit.status.to_string(),
    );
    let source = match (&hit.example, &hit.request) {
        (Some(example), Some(request)) => paint(DIM, &format!("{example} from {request}")),
        _ => paint(DIM, "no example"),
    };
    let time = getman_core::time::iso8601_millis(hit.at);
    anstream::println!(
        "{} {} {} → {status} {source}",
        paint(DIM, time.get(11..19).unwrap_or_default()),
        paint(BOLD, &format!("{:<7}", hit.method)),
        hit.path,
    );
}
