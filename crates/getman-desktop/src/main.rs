//! Getman's desktop app: the browser UI in a native window, with Getman's
//! server running in the same process, and in-app updates.
//!
//! On macOS and Linux this one executable is also the `getman` command
//! (see `launch.rs`). On Windows it is a window program, so it never opens
//! a console, and the installers put `getman.exe` beside it.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod command_line;
mod commands;
mod install;
mod launch;
mod settings;
mod updates;

use std::ffi::OsString;
use std::process::ExitCode;

use launch::{Host, Launch};

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().collect();
    let host = Host::current(&args);
    match launch::decide(args, host) {
        Launch::CommandLine(args) => command_line(args),
        Launch::Desktop(folder) => app::run(folder),
    }
}

#[cfg(not(windows))]
fn command_line(args: Vec<OsString>) -> ExitCode {
    getman::run(args)
}

/// Windows never runs the command line from the app (see `launch.rs`), so
/// it is left out there, which keeps the app smaller.
#[cfg(windows)]
fn command_line(_args: Vec<OsString>) -> ExitCode {
    unreachable!("on Windows the app is only ever the window")
}
