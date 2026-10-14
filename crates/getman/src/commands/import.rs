//! `getman import`: bring in Postman exports and cURL commands.

use std::io::Read as _;
use std::process::ExitCode;

use getman_core::import::{curl, postman};

use super::Outcome;
use crate::cli::ImportArgs;
use crate::style;
use crate::workspaces;

pub(crate) fn import(args: &ImportArgs) -> Outcome {
    let workspace = workspaces::find(args.workspace.workspace.as_deref())?;
    let folder = if args.folder.is_empty() {
        String::new()
    } else {
        workspaces::item_path(&workspace, &args.folder)?
    };

    let text = match (&args.curl, &args.file) {
        (Some(command), _) => command.clone(),
        (None, Some(path)) if path.as_os_str() == "-" => {
            let mut text = String::new();
            std::io::stdin()
                .read_to_string(&mut text)
                .map_err(|e| format!("cannot read standard input: {e}"))?;
            text
        }
        (None, Some(path)) => {
            std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?
        }
        (None, None) => return Err("give a file to import, or --curl".into()),
    };

    if text.trim_start().starts_with("curl") {
        let request = curl::parse(&text).map_err(|e| e.to_string())?;
        let path = workspace
            .create_request(&folder, &request.name, &request)
            .map_err(|e| e.to_string())?;
        anstream::println!("{} {path}", style::paint(style::GREEN.bold(), "Imported"));
        return Ok(ExitCode::SUCCESS);
    }

    let report =
        postman::import(&workspace, &folder, &text, args.root).map_err(|e| e.to_string())?;
    let what = match report.kind.as_str() {
        "collection" => format!(
            "the collection {} with {} requests in {} folders{}",
            style::paint(style::BOLD, &report.name),
            report.requests,
            report.folders,
            if report.path.is_empty() {
                String::new()
            } else {
                format!(" into {}", report.path)
            }
        ),
        "environment" => format!(
            "the environment {} as environments/{}.yaml",
            style::paint(style::BOLD, &report.name),
            report.path
        ),
        _ => "the globals into getman.local.yaml".to_owned(),
    };
    anstream::println!("{} {what}", style::paint(style::GREEN.bold(), "Imported"));
    for warning in &report.warnings {
        style::warn(warning);
    }
    Ok(ExitCode::SUCCESS)
}
