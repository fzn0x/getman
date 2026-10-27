//! Getman: a local-first API client.
//!
//! One binary with three front ends over a workspace of plain YAML files:
//! the browser UI served on 127.0.0.1 (`getman`), a terminal UI
//! (`getman tui`), and a command line for scripts and CI (`getman run`,
//! `getman send`, `getman import`, `getman export`).

mod cli;
mod commands;
mod examples;
mod mock;
mod report;
mod runs;
mod server;
mod style;
mod tui;
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
