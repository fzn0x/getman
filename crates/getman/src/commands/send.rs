//! `getman send`: send one request and print the response.

use std::io::{IsTerminal as _, Write as _};
use std::process::ExitCode;

use getman_core::execute::{Context, Execution, Executor, Overlay};
use getman_core::http::Response;
use getman_core::model::RequestKind;
use getman_core::vars::missing_reason;

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
        settings: args.net.settings(),
        ..Context::default()
    };
    let executor = Executor::new(workspace);
    if let Ok(request) = executor.workspace.read_request(&path)
        && request.kind.is_realtime()
    {
        return super::stream::stream(&args, &executor, &path, &request, &context).await;
    }
    if let Ok(request) = executor.workspace.read_request(&path)
        && request.kind == RequestKind::Grpc
    {
        return super::send_grpc::send(&executor, &path, &request, &mut context, &args).await;
    }
    if !args.messages.is_empty() || !args.until.is_empty() {
        style::warn(
            "--message and --until are for WebSocket, event stream and Socket.IO requests; this one is HTTP",
        );
    }
    let execution = executor.execute_path(&path, &mut context).await;

    if args.json {
        let json = serde_json::to_string_pretty(&execution).map_err(|e| e.to_string())?;
        println!("{json}");
        return Ok(exit(execution.failed()));
    }

    print_problems(&execution);
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
        if args.include {
            for line in details(response) {
                style::note(&line);
            }
        }
    }
    print_results(&execution);
    Ok(exit(execution.failed()))
}

/// Prints failed scripts, script logs, missing variables and the error,
/// to stderr.
pub(super) fn print_problems(execution: &Execution) {
    for error in &execution.script_errors {
        style::error(&format!("script in {}: {}", error.origin, error.message));
    }
    for log in &execution.logs {
        style::note(&format!("[{}] {}", log.level, log.message));
    }
    for name in &execution.missing_variables {
        style::warn(&missing_reason(name));
    }
    if let Some(error) = &execution.error {
        style::error(error);
    }
}

/// Prints the results of tests and checks, to stderr.
pub(super) fn print_results(execution: &Execution) {
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

/// Where the time went, the TLS connection, and whether HTTP/3 is on
/// offer, one line each.
fn details(response: &Response) -> Vec<String> {
    let mut lines = Vec::new();
    if !response.hops.is_empty() {
        let mut route: Vec<String> = response
            .hops
            .iter()
            .map(|hop| format!("{} {} ({})", hop.status, hop.url, fine(hop.time_ms)))
            .collect();
        route.push(format!("{} {}", response.status, response.url));
        lines.push(format!("Redirects: {}", route.join(" → ")));
    }
    if let Some(timings) = &response.timings {
        let setup = if timings.reused {
            "reused connection".to_owned()
        } else {
            let connect = timings.connect_ms.map_or_else(
                || "connect in first byte (QUIC)".to_owned(),
                |ms| format!("connect {}", fine(ms)),
            );
            format!("DNS {} · {connect}", fine(timings.dns_ms))
        };
        lines.push(format!(
            "Timing: {setup} · first byte {} · download {}",
            fine(timings.first_byte_ms),
            fine(timings.download_ms)
        ));
    }
    if let Some(tls) = &response.tls {
        let mut line = format!(
            "TLS: {}",
            tls.version.as_deref().unwrap_or("unknown version")
        );
        if let Some(cert) = &tls.certificate {
            line.push_str(&format!(
                " · {} · issued by {} · expires {}",
                cert.subject,
                cert.issuer,
                getman_core::time::iso8601_millis(cert.not_after)
                    .split('T')
                    .next()
                    .unwrap_or_default()
            ));
        }
        lines.push(line);
    }
    if response.http3_available {
        lines.push(
            "The server offers HTTP/3 (Alt-Svc); try it with --http-version http3.".to_owned(),
        );
    }
    lines
}

/// Milliseconds with a decimal while they are small.
fn fine(ms: f64) -> String {
    if ms < 10.0 {
        format!("{ms:.1} ms")
    } else {
        style::duration(ms)
    }
}

pub(super) fn exit(failed: bool) -> ExitCode {
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
