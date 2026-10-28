//! `getman send` for gRPC requests: each response message as it arrives,
//! then the status.

use std::io::Write as _;

use getman_core::execute::{Context, Executor};
use getman_core::grpc::{Direction, LiveEvent};
use getman_core::http::Header;
use getman_core::model::Request;
use tokio::sync::mpsc;

use super::Outcome;
use super::send::{exit, print_problems, print_results};
use crate::cli::SendArgs;
use crate::style::{self, BOLD, DIM, GREEN, RED, paint};

pub(super) async fn send(
    executor: &Executor,
    path: &str,
    request: &Request,
    context: &mut Context,
    args: &SendArgs,
) -> Outcome {
    let (events, mut receiver) = mpsc::unbounded_channel();
    let run = executor.execute_watched(Some(path), request, context, events);
    let print = async {
        // Once the reader, such as `head`, stops reading, keep the call
        // going but print nothing more.
        let mut reading = true;
        let mut count = 0_usize;
        while let Some(event) = receiver.recv().await {
            if !reading {
                continue;
            }
            let printed = match event {
                LiveEvent::Headers { headers, .. } if args.include && !args.json => {
                    print_headers(&headers)
                }
                LiveEvent::Message { message } if message.direction == Direction::Received => {
                    count += 1;
                    let text = if args.json {
                        serde_json::to_string(&message.message)
                    } else {
                        serde_json::to_string_pretty(&message.message)
                    };
                    let mut out = anstream::stdout().lock();
                    writeln!(out, "{}", text.unwrap_or_default()).and_then(|()| out.flush())
                }
                _ => Ok(()),
            };
            if printed.is_err() {
                reading = false;
            }
        }
        (reading, count)
    };
    let (execution, (reading, count)) = tokio::join!(run, print);

    print_problems(&execution);
    let call = execution.grpc.as_ref();
    let status = call.and_then(|c| c.status.as_ref());
    if let (Some(call), Some(status)) = (call, status) {
        if reading && args.include && !args.json && !call.trailers.is_empty() {
            let _ = print_headers(&call.trailers);
        }
        let tone = if status.is_ok() { GREEN } else { RED };
        let mut line = format!(
            "{} · {} · {count} {}",
            paint(tone, &format!("{} {}", status.code, status.name)),
            style::duration(call.time_ms),
            if count == 1 { "message" } else { "messages" }
        );
        if !status.message.is_empty() {
            line = format!("{line}\n{}", status.message);
        }
        anstream::eprintln!("{}", paint(DIM, &line));
        if let Some(details) = &status.details {
            anstream::eprintln!(
                "{}",
                paint(
                    DIM,
                    &serde_json::to_string_pretty(details).unwrap_or_default()
                )
            );
        }
    }
    print_results(&execution);
    Ok(exit(
        execution.failed() || status.is_some_and(|s| !s.is_ok()),
    ))
}

fn print_headers(headers: &[Header]) -> std::io::Result<()> {
    let mut out = anstream::stdout().lock();
    for header in headers {
        writeln!(out, "{}: {}", paint(BOLD, &header.name), header.value)?;
    }
    writeln!(out)?;
    out.flush()
}
