//! `getman init`: create a workspace.

use std::process::ExitCode;

use getman_core::Workspace;

use super::Outcome;
use crate::cli::InitArgs;
use crate::style;

pub(crate) fn init(args: &InitArgs) -> Outcome {
    let dir = match &args.dir {
        Some(dir) => dir.clone(),
        None => std::env::current_dir().map_err(|e| e.to_string())?,
    };
    let dir = std::path::absolute(&dir).unwrap_or(dir);
    let name = args.name.clone().unwrap_or_else(|| {
        dir.file_name()
            .map_or_else(|| "My API".to_owned(), |n| n.to_string_lossy().into_owned())
    });
    let workspace = Workspace::init(&dir, &name).map_err(|e| e.to_string())?;
    if args.example {
        crate::examples::add(&workspace).map_err(|e| e.to_string())?;
    }
    anstream::println!(
        "{} the workspace {} in {}",
        style::paint(style::GREEN.bold(), "Created"),
        style::paint(style::BOLD, &name),
        dir.display()
    );
    println!();
    println!("  getman            open it in your browser");
    println!("  getman tui        open it in the terminal");
    if args.example {
        println!("  getman run posts  run the example requests");
    }
    println!();
    style::note("Commit getman.yaml, your request files and environments/*.yaml.");
    style::note("Values in *.local.yaml stay on this machine; .gitignore already lists them.");
    Ok(ExitCode::SUCCESS)
}
