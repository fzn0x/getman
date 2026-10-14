//! Getman: a local-first API client.
//!
//! The command line works on a workspace of plain YAML files: `getman run`
//! runs it, in a terminal or in CI.

mod cli;
mod commands;
mod report;
mod style;
mod workspaces;

use std::process::ExitCode;

use clap::Parser;

fn main() -> ExitCode {
    let cli = cli::Cli::parse();
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("getman: could not start: {e}");
            return ExitCode::from(2);
        }
    };
    match runtime.block_on(commands::dispatch(cli)) {
        Ok(code) => code,
        Err(e) => {
            style::error(&e);
            ExitCode::from(2)
        }
    }
}
