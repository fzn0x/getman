//! `getman send` on a WebSocket, event stream or Socket.IO request:
//! connect, send the saved messages asked for, and print what arrives
//! until a limit or Ctrl+C.

use std::io::Write as _;

use getman_core::execute::{Cancel, Context, Executor};
use getman_core::model::{Request, RequestKind};
use getman_core::realtime::{
    self, Direction, EventKind, Limits, Message, Outbound, Plan, StreamEvent,
};
use getman_core::vars::missing_reason;

use super::Outcome;
use crate::cli::{SendArgs, Until};
use crate::style::{self, BOLD, DIM, paint};

pub(crate) async fn stream(
    args: &SendArgs,
    executor: &Executor,
    path: &str,
    request: &Request,
    context: &mut Context,
) -> Outcome {
    // Find the messages first, so a misspelt name fails before connecting.
    let outbound = args
        .messages
        .iter()
        .map(|name| saved(request, name))
        .collect::<Result<Vec<_>, _>>()?;
    let plan = Plan {
        send: outbound,
        limits: limits(args, request),
    };

    // The first Ctrl+C disconnects politely, a second one stops waiting.
    let cancel = Cancel::new();
    context.cancel = Some(cancel.clone());
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            cancel.cancel();
            if tokio::signal::ctrl_c().await.is_ok() {
                std::process::exit(130);
            }
        }
    });

    let mut printer = Printer {
        json: args.json,
        include: args.include,
        kind: request.kind,
    };
    let mut failed = None;
    let execution = executor
        .execute_realtime(Some(path), request, context, plan, &mut |event| {
            match printer.print(event) {
                Ok(()) => true,
                // The reader, such as `head`, stopped reading: not an error.
                Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => false,
                Err(e) => {
                    failed = Some(e.to_string());
                    false
                }
            }
        })
        .await;
    if let Some(message) = failed {
        return Err(message);
    }

    // What the connection printed as it went is not printed again.
    if args.json {
        let ran = !(execution.tests.is_empty()
            && execution.checks.is_empty()
            && execution.logs.is_empty()
            && execution.script_errors.is_empty());
        if ran {
            let result = serde_json::json!({
                "type": "result",
                "tests": execution.tests,
                "checks": execution.checks,
                "logs": execution.logs,
                "script_errors": execution.script_errors,
                "changes": execution.changes,
            });
            println!("{result}");
        }
    } else {
        for error in &execution.script_errors {
            style::error(&format!("script in {}: {}", error.origin, error.message));
        }
        for log in &execution.logs {
            style::note(&format!("[{}] {}", log.level, log.message));
        }
        let shown = execution
            .realtime
            .as_ref()
            .map(|call| call.warnings.as_slice())
            .unwrap_or_default();
        for warning in execution.warnings.iter().filter(|w| !shown.contains(w)) {
            style::warn(warning);
        }
        if execution.error.is_some() && execution.realtime.is_none() {
            // Failed before it connected, so nothing was printed about it.
            if let Some(error) = &execution.error {
                style::error(error);
            }
        }
        super::send::print_results(&execution);
    }
    Ok(super::send::exit(execution.failed()))
}

/// When to end the connection: what `--until` says, else what the request
/// file's `until` says, else never (Ctrl+C ends it).
fn limits(args: &SendArgs, request: &Request) -> Limits {
    if args.until.is_empty() {
        return if request.until.is_empty() {
            Limits::default()
        } else {
            request.until.limits()
        };
    }
    Limits {
        messages: args.until.iter().find_map(|u| match u {
            Until::Messages(n) => Some(*n),
            Until::Time(_) => None,
        }),
        time: args
            .until
            .iter()
            .filter_map(|u| match u {
                Until::Time(time) => Some(*time),
                Until::Messages(_) => None,
            })
            .min(),
    }
}

/// The saved message or event named `name`.
fn saved(request: &Request, name: &str) -> Result<Outbound, String> {
    let wanted = name.trim();
    let (found, names): (Option<Outbound>, Vec<&str>) = match request.kind {
        RequestKind::Websocket => {
            let messages = &request.websocket.messages;
            (
                messages
                    .iter()
                    .find(|m| m.name.trim().eq_ignore_ascii_case(wanted))
                    .cloned()
                    .map(Outbound::Message),
                messages.iter().map(|m| m.name.as_str()).collect(),
            )
        }
        RequestKind::Socketio => {
            let messages = &request.socketio.messages;
            let by = |m: &&realtime::SavedEmit| {
                if m.name.trim().is_empty() {
                    m.event.trim().eq_ignore_ascii_case(wanted)
                } else {
                    m.name.trim().eq_ignore_ascii_case(wanted)
                }
            };
            (
                messages.iter().find(by).cloned().map(Outbound::Emit),
                messages
                    .iter()
                    .map(|m| {
                        if m.name.trim().is_empty() {
                            m.event.as_str()
                        } else {
                            m.name.as_str()
                        }
                    })
                    .collect(),
            )
        }
        _ => {
            return Err(format!(
                "{} requests only receive, so there is no message {name:?} to send",
                request.kind.label()
            ));
        }
    };
    found.ok_or_else(|| {
        let names: Vec<&str> = names.into_iter().filter(|n| !n.trim().is_empty()).collect();
        if names.is_empty() {
            format!("the request has no saved messages, so none is named {name:?}")
        } else {
            format!(
                "no saved message is named {name:?}; the request has {}",
                names.join(", ")
            )
        }
    })
}

/// Prints events: received messages on standard output, one per line,
/// and the rest as notes on standard error. With `--json`, every event is
/// a line of JSON on standard output.
struct Printer {
    json: bool,
    include: bool,
    kind: RequestKind,
}

impl Printer {
    fn print(&mut self, event: &StreamEvent) -> std::io::Result<()> {
        let mut out = anstream::stdout().lock();
        if self.json {
            let line = serde_json::to_string(event).map_err(std::io::Error::other)?;
            writeln!(out, "{line}")?;
            return out.flush();
        }
        match &event.kind {
            EventKind::Connecting { url, missing, .. } => {
                for name in missing {
                    style::warn(&missing_reason(name));
                }
                style::note(&format!("Connecting to {url}"));
            }
            EventKind::Open {
                status,
                headers,
                protocol,
                time_ms,
            } => {
                let mut line = format!("Connected · {status} · {}", style::duration(*time_ms));
                if let Some(protocol) = protocol {
                    line.push_str(&format!(" · subprotocol {protocol}"));
                }
                style::note(&line);
                if self.include {
                    writeln!(out, "{}", paint(BOLD, &status.to_string()))?;
                    for header in headers {
                        writeln!(out, "{}: {}", paint(BOLD, &header.name), header.value)?;
                    }
                    writeln!(out)?;
                }
            }
            EventKind::Message(message) => match message.direction {
                Direction::Received => writeln!(out, "{}", self.line(message))?,
                Direction::Sent => {
                    let what = message
                        .name
                        .clone()
                        .unwrap_or_else(|| style::size(message.size));
                    style::note(&format!("→ {what}: {}", self.line(message)));
                }
            },
            EventKind::Note { message } => style::note(message),
            EventKind::Warning { message } => style::warn(message),
            EventKind::Error { message } => style::error(message),
            EventKind::Closed {
                code,
                reason,
                by_client,
            } => {
                let mut line = String::from(if *by_client {
                    "Disconnected"
                } else {
                    "The server closed the connection"
                });
                if let Some(code) = code {
                    line.push_str(&format!(" ({code})"));
                }
                if !reason.is_empty() {
                    line.push_str(&format!(": {reason}"));
                }
                anstream::eprintln!("{}", paint(DIM, &line));
            }
        }
        out.flush()
    }

    /// A message as one printed line: its data, with the Socket.IO event
    /// or acknowledgement before it.
    fn line(&self, message: &Message) -> String {
        let mut line = String::new();
        if self.kind == RequestKind::Socketio {
            match (&message.event, message.ack) {
                (Some(event), _) => {
                    line.push_str(event);
                    line.push(' ');
                }
                (None, Some(ack)) => line.push_str(&format!("ack {ack} ")),
                (None, None) => {}
            }
        }
        line.push_str(&message.data);
        if message.truncated {
            line.push_str(&format!(" … ({} in all)", style::size(message.size)));
        }
        line
    }
}
