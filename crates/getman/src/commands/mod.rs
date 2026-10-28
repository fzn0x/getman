//! What each command does.

mod export;
mod import;
mod init;
mod mock;
mod run;
mod send;
mod send_grpc;
mod stream;

use std::process::ExitCode;
use std::time::Duration;

use crate::cli::{Cli, Command, OpenArgs, VarArgs};

pub(crate) type Outcome = Result<ExitCode, String>;

pub(crate) async fn dispatch(cli: Cli) -> Outcome {
    match cli.command {
        Some(Command::Run(args)) => run::run(args).await,
        Some(Command::Send(args)) => send::send(args).await,
        Some(Command::Init(args)) => init::init(&args),
        Some(Command::Import(args)) => import::import(&args),
        Some(Command::Export(args)) => export::export(&args),
        Some(Command::Mock(args)) => mock::mock(args).await,
        Some(Command::Mcp(args)) => {
            let workspace = crate::workspaces::find(args.dir.as_deref())?;
            crate::mcp::serve(workspace).await?;
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Open(args)) => open(&args).await,
        Some(Command::Tui(args)) => {
            let workspace = crate::workspaces::find_or_personal(args.dir.as_deref())?;
            crate::tui::run(workspace).await?;
            Ok(ExitCode::SUCCESS)
        }
        None => open(&cli.open).await,
    }
}

async fn open(args: &OpenArgs) -> Outcome {
    let workspace = crate::workspaces::find_or_personal(args.dir.as_deref())?;
    crate::server::serve(workspace, args.port, !args.no_open).await?;
    Ok(ExitCode::SUCCESS)
}

/// How long a script may run, from `--script-timeout`.
pub(crate) fn script_timeout(vars: &VarArgs) -> Duration {
    if vars.script_timeout.is_finite() && vars.script_timeout > 0.0 {
        Duration::from_secs_f64(vars.script_timeout)
    } else {
        getman_core::script::DEFAULT_TIMEOUT
    }
}
