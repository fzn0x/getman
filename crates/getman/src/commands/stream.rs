//! `getman send` on a WebSocket, event stream or Socket.IO request:
//! connect, send the saved messages asked for, and print what arrives
//! until a limit or Ctrl+C.

use std::io::Write as _;
use std::process::ExitCode;

use getman_core::execute::{Context, Executor};
use getman_core::model::{Request, RequestKind};
use getman_core::realtime::{self, Direction, EventKind, Message, Options, Outbound, StreamEvent};
use getman_core::vars::missing_reason;

use super::Outcome;
use crate::cli::{SendArgs, Until};
use crate::style::{self, BOLD, DIM, paint};

pub(crate) async fn stream(
    args: &SendArgs,
    executor: &Executor,
    path: &str,
    request: &Request,
    context: &Context,
) -> Outcome {
    // Find the messages first, so a misspelt name fails before connecting.
    let mut outbound = args
        .messages
        .iter()
        .map(|name| saved(request, name))
        .collect::<Result<Vec<_>, _>>()?;
    let most = args.until.iter().find_map(|u| match u {
        Until::Messages(n) => Some(*n),
        Until::Time(_) => None,
    });
    let deadline = args
        .until
        .iter()
        .filter_map(|u| match u {
            Until::Time(time) => Some(*time),
            Until::Messages(_) => None,
        })
        .min()
        .map(|time| tokio::time::Instant::now() + time);

    let mut session = realtime::connect(executor, Some(path), request, context, Options::default())
        .map_err(|e| e.to_string())?;
    let control = session.control();
    let mut printer = Printer {
        json: args.json,
        include: args.include,
        kind: request.kind,
    };
    let mut received = 0_u64;
    let (mut opened, mut failed, mut closing) = (false, false, false);
    if most == Some(0) {
        control.close();
        closing = true;
    }
    let mut interrupt = Box::pin(tokio::signal::ctrl_c());
    let mut time_up = Box::pin(async {
        match deadline {
            Some(deadline) => tokio::time::sleep_until(deadline).await,
            None => std::future::pending().await,
        }
    });
    loop {
        let event = tokio::select! {
            event = session.next() => match event {
                Some(event) => event,
                None => break,
            },
            _ = &mut interrupt => {
                if closing {
                    // A second Ctrl+C: stop waiting for the close.
                    break;
                }
                closing = true;
                control.close();
                interrupt = Box::pin(tokio::signal::ctrl_c());
                continue;
            }
            () = &mut time_up, if !closing => {
                closing = true;
                control.close();
                continue;
            }
        };
        match printer.print(&event) {
            Ok(()) => {}
            // The reader, such as `head`, stopped reading: not an error.
            Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {
                control.close();
                break;
            }
            Err(e) => return Err(e.to_string()),
        }
        match &event.kind {
            EventKind::Open { .. } if !opened => {
                opened = true;
                for message in outbound.drain(..) {
                    control.send(message).map_err(|e| e.to_string())?;
                }
            }
            EventKind::Message(message) if message.direction == Direction::Received => {
                received += 1;
                if !closing && most.is_some_and(|most| received >= most) {
                    closing = true;
                    control.close();
                }
            }
            EventKind::Error { .. } => failed = true,
            _ => {}
        }
    }
    Ok(if failed || !opened {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
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
