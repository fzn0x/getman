//! Getman: a local-first API client.
//!
//! One program with three front ends over a workspace of plain YAML files:
//! the browser UI served on 127.0.0.1 (`getman`), a terminal UI
//! (`getman tui`), and a command line for scripts and CI (`getman run`,
//! `getman send`, `getman import`, `getman export`).
//!
//! The `getman` binary is a thin wrapper around [`main`]. The library is
//! there for the desktop app, which runs the same command line and starts
//! the local server in its own process with [`server::Server`].

mod cli;
mod commands;
mod examples;
mod report;
pub mod server;
mod style;
mod tui;
pub mod workspaces;

use std::ffi::OsString;
use std::process::ExitCode;

use clap::Parser;

/// Runs the command line with this process's arguments, exactly as the
/// `getman` binary does, and returns its exit code.
pub fn main() -> ExitCode {
    run(std::env::args_os())
}

/// Runs the command line with `args`, the first being the program's name.
pub fn run<I, T>(args: I) -> ExitCode
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = cli::Cli::parse_from(args);
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
