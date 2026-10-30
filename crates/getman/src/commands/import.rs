//! `getman import`: bring in collections, specs, recordings and cURL
//! commands, in any format Getman reads.

use std::io::Read as _;
use std::path::Path;
use std::process::ExitCode;

use getman_core::import::{self, FilesImport, ImportReport, SourceFile, curl};

use super::Outcome;
use crate::cli::ImportArgs;
use crate::style;
use crate::workspaces;

/// Most files read from a folder.
const MAX_FILES: usize = 5_000;
/// Largest file read, in bytes.
const MAX_FILE_SIZE: u64 = 64 * 1024 * 1024;

pub(crate) fn import(args: &ImportArgs) -> Outcome {
    let workspace = workspaces::find(args.workspace.workspace.as_deref())?;
    let folder = if args.folder.is_empty() {
        String::new()
    } else {
        workspaces::item_path(&workspace, &args.folder)?
    };

    let one = |report: ImportReport| FilesImport {
        reports: vec![report],
        ..FilesImport::default()
    };
    let done = match (&args.curl, &args.file) {
        (Some(command), _) => {
            let request = curl::parse(command).map_err(|e| e.to_string())?;
            let path = workspace
                .create_request(&folder, &request.name, &request)
                .map_err(|e| e.to_string())?;
            one(ImportReport {
                kind: "request".into(),
                name: request.name,
                path,
                requests: 1,
                ..ImportReport::default()
            })
        }
        (None, Some(path)) if path.as_os_str() == "-" => {
            let mut text = String::new();
            std::io::stdin()
                .read_to_string(&mut text)
                .map_err(|e| format!("cannot read standard input: {e}"))?;
            one(
                import::import_text(&workspace, &folder, "", &text, args.root)
                    .map_err(|e| e.to_string())?,
            )
        }
        (None, Some(path)) if path.is_dir() => {
            let files = read_folder(path)?;
            import::import_files(&workspace, &folder, &files, args.root)
                .map_err(|e| format!("{}: {e}", path.display()))?
        }
        (None, Some(path)) => {
            let text =
                std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            one(
                import::import_text(&workspace, &folder, &name, &text, args.root)
                    .map_err(|e| format!("{}: {e}", path.display()))?,
            )
        }
        (None, None) => return Err("give a file or folder to import, or --curl".into()),
    };

    for report in &done.reports {
        anstream::println!(
            "{} {}",
            style::paint(style::GREEN.bold(), "Imported"),
            describe(report)
        );
        for warning in &report.warnings {
            style::warn(warning);
        }
    }
    if !done.passed_over.is_empty() {
        style::note(&format!(
            "Passed over {} in no format Getman imports: {}",
            count(done.passed_over.len(), "file", "files"),
            done.passed_over.join(", ")
        ));
    }
    for file in &done.failed {
        style::warn(&format!("{} was not imported: {}", file.path, file.error));
    }
    Ok(if done.failed.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// One line saying what an import created.
fn describe(report: &ImportReport) -> String {
    let name = style::paint(style::BOLD, &report.name);
    let format = if report.format.is_empty() {
        String::new()
    } else {
        format!(" ({})", report.format)
    };
    let files = |ids: &[String]| {
        ids.iter()
            .map(|id| format!("environments/{id}.yaml"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    match report.kind.as_str() {
        "request" => format!("the request {name}{format} as {}", report.path),
        "collection" => {
            let mut line = format!(
                "{name}{format}: {} in {}{}",
                count(report.requests, "request", "requests"),
                count(report.folders, "folder", "folders"),
                if report.path.is_empty() {
                    ", as the workspace".to_owned()
                } else {
                    format!(", into {}/", report.path)
                }
            );
            match report.environments.len() {
                0 => {}
                1 => line.push_str(&format!(
                    ", and the environment {}",
                    files(&report.environments)
                )),
                _ => line.push_str(&format!(
                    ", and the environments {}",
                    files(&report.environments)
                )),
            }
            line
        }
        "environment" if report.environments.len() > 1 => {
            format!("the environments {}{format}", files(&report.environments))
        }
        "environment" => format!(
            "the environment {name}{format} as environments/{}.yaml",
            report.path
        ),
        _ => "the globals into getman.local.yaml".to_owned(),
    }
}

fn count(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

/// Reads the text files of a folder, such as a Bruno collection, with
/// paths relative to it. Hidden folders and `node_modules` are skipped.
fn read_folder(root: &Path) -> Result<Vec<SourceFile>, String> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if name.starts_with('.') || name == "node_modules" {
                continue;
            }
            if kind.is_dir() {
                pending.push(path);
                continue;
            }
            if !kind.is_file() || entry.metadata().is_ok_and(|m| m.len() > MAX_FILE_SIZE) {
                continue;
            }
            // Binary files cannot be imported; skip them.
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            files.push(SourceFile {
                path: relative,
                text,
            });
            if files.len() > MAX_FILES {
                return Err(format!(
                    "{} holds more than {MAX_FILES} files; import a smaller folder",
                    root.display()
                ));
            }
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    if files.is_empty() {
        return Err(format!("{} holds no files to import", root.display()));
    }
    Ok(files)
}
