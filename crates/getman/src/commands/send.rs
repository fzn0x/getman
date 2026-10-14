//! `getman send`: send one request and print the response.

use std::io::{IsTerminal as _, Write as _};
use std::process::ExitCode;

use getman_core::execute::{Context, Executor, Overlay};
use getman_core::http::Response;

use super::Outcome;
use crate::cli::SendArgs;
use crate::style::{self, DIM, paint};
use crate::workspaces;

pub(crate) async fn send(args: SendArgs) -> Outcome {
    let workspace = workspaces::find(args.workspace.workspace.as_deref())?;
    let path = workspaces::item_path(&workspace, &args.request)?;
    let environment = workspaces::environment(&workspace, args.vars.env.as_deref())?;
    let mut overlay = Overlay::default();
    for (name, value) in &args.vars.vars {
        overlay.local.insert(name.clone(), Some(value.clone()));
    }
    let mut context = Context {
        environment,
        overlay,
        script_timeout: super::script_timeout(&args.vars),
        ..Context::default()
    };
    let executor = Executor::new(workspace);
    let execution = executor.execute_path(&path, &mut context).await;

    if args.json {
        let json = serde_json::to_string_pretty(&execution).map_err(|e| e.to_string())?;
        println!("{json}");
        return Ok(exit(execution.failed()));
    }

    for error in &execution.script_errors {
        style::error(&format!("script in {}: {}", error.origin, error.message));
    }
    for log in &execution.logs {
        style::note(&format!("[{}] {}", log.level, log.message));
    }
    for name in &execution.missing_variables {
        style::warn(&format!("{{{{{name}}}}} is not defined"));
    }
    if let Some(error) = &execution.error {
        style::error(error);
    }
    if let Some(response) = &execution.response {
        match print_response(response, args.include) {
            Ok(()) => {}
            // The reader, such as `head`, stopped reading: not an error.
            Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {
                return Ok(exit(execution.failed()));
            }
            Err(e) => return Err(e.to_string()),
        }
        style::note(&format!(
            "{} {} · {} · {}",
            response.status,
            response.status_text,
            style::duration(response.time_ms),
            style::size(response.size.body)
        ));
    }
    for result in execution.tests.iter().chain(&execution.checks) {
        if result.passed {
            anstream::eprintln!("{} {}", paint(style::GREEN, "✓"), paint(DIM, &result.name));
        } else {
            anstream::eprintln!(
                "{} {} {}",
                paint(style::RED, "✗"),
                result.name,
                paint(
                    DIM,
                    &format!("— {}", result.message.as_deref().unwrap_or_default())
                )
            );
        }
    }
    Ok(exit(execution.failed()))
}

/// Prints the body, after the status line and headers if asked.
fn print_response(response: &Response, include: bool) -> std::io::Result<()> {
    let mut out = anstream::stdout().lock();
    if include {
        writeln!(
            out,
            "{} {}",
            response.http_version,
            paint(
                style::status(response.status).bold(),
                &format!("{} {}", response.status, response.status_text)
            )
        )?;
        for header in &response.headers {
            writeln!(
                out,
                "{}: {}",
                paint(style::BOLD, &header.name),
                header.value
            )?;
        }
        writeln!(out)?;
    }
    out.write_all(&response.body.0)?;
    if !response.body.0.ends_with(b"\n") && std::io::stdout().is_terminal() {
        writeln!(out)?;
    }
    out.flush()
}

fn exit(failed: bool) -> ExitCode {
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
