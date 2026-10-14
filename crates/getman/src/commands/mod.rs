//! What each command does.

mod export;
mod import;
mod init;
mod run;
mod send;

use std::process::ExitCode;
use std::time::Duration;

use clap::CommandFactory;

use crate::cli::{Cli, Command, VarArgs};

pub(crate) type Outcome = Result<ExitCode, String>;

pub(crate) async fn dispatch(cli: Cli) -> Outcome {
    match cli.command {
        Some(Command::Run(args)) => run::run(args).await,
        Some(Command::Send(args)) => send::send(args).await,
        Some(Command::Init(args)) => init::init(&args),
        Some(Command::Import(args)) => import::import(&args),
        Some(Command::Export(args)) => export::export(&args),
        None => {
            Cli::command().print_help().map_err(|e| e.to_string())?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// How long a script may run, from `--script-timeout`.
pub(crate) fn script_timeout(vars: &VarArgs) -> Duration {
    if vars.script_timeout.is_finite() && vars.script_timeout > 0.0 {
        Duration::from_secs_f64(vars.script_timeout)
    } else {
        getman_core::script::DEFAULT_TIMEOUT
    }
}
