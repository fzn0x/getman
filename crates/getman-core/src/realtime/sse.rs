//! Server-Sent Events: an HTTP response in the `text/event-stream`
//! format, read event by event as it arrives.

use std::time::Duration;

use super::{Direction, EventKind, Job, MAX_MESSAGE_BYTES, Message, Sink};
use crate::auth::answer_digest;
use crate::error::{Error, Result};
use crate::http::{Outgoing, describe, header_list, set_header};
use crate::model::Auth;

/// How long to wait before reconnecting when the server has not said.
const DEFAULT_RETRY_MS: u64 = 3000;

/// Checks that a URL can carry an event stream.
pub(super) fn check_url(raw: &str) -> Result<()> {
    let url = url::Url::parse(raw)
        .map_err(|e| Error::Invalid(format!("{raw:?} is not a valid URL: {e}")))?;
    if matches!(url.scheme(), "http" | "https") {
        Ok(())
    } else {
        Err(Error::Invalid(format!(
            "an event stream URL starts with http:// or https://, not {}://",
            url.scheme()
        )))
    }
}

/// Reads an event stream until it ends, and again after it ends if the
/// request asks to reconnect.
pub(super) async fn run(job: &mut Job, sink: &mut Sink) {
    let gone = sink.watch();
    let mut last_id = String::new();
    let mut retry_ms = DEFAULT_RETRY_MS;
    let mut reconnecting = false;
    loop {
        let mut outgoing = job.outgoing.clone();
        if !last_id.is_empty() {
            set_header(&mut outgoing.headers, "Last-Event-ID", last_id.clone());
        }
        let opened = tokio::select! {
            opened = open(job, &mut outgoing) => opened,
            () = job.close.notified() => {
                sink.closed(None, String::new(), true).await;
                return;
            }
            () = gone.closed() => return,
        };
        let (mut response, time_ms) = match opened {
            Ok(opened) => opened,
            Err(e) if reconnecting => {
                let note = format!("Could not reconnect: {e}");
                if !wait(job, sink, note, retry_ms).await {
                    return;
                }
                continue;
            }
            Err(e) => {
                sink.fail(e.to_string()).await;
                return;
            }
        };
        let status = response.status();
        if !status.is_success() {
            let body = excerpt(&mut response).await;
            sink.fail(format!(
                "The server answered {} {} instead of an event stream{}",
                status.as_u16(),
                status.canonical_reason().unwrap_or_default(),
                if body.is_empty() {
                    String::new()
                } else {
                    format!(": {body}")
                }
            ))
            .await;
            return;
        }
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let open = EventKind::Open {
            status: status.as_u16(),
            headers: header_list(response.headers()),
            protocol: None,
            time_ms,
        };
        if !sink.emit(open).await {
            return;
        }
        if !content_type
            .to_ascii_lowercase()
            .starts_with("text/event-stream")
        {
            let shown = if content_type.is_empty() {
                "no content type".to_owned()
            } else {
                content_type
            };
            sink.emit(EventKind::Warning {
                message: format!(
                    "The response has {shown}, not text/event-stream; reading it as events anyway"
                ),
            })
            .await;
        }

        let mut parser = Parser::default();
        let broke = loop {
            tokio::select! {
                chunk = response.chunk() => match chunk {
                    Ok(Some(bytes)) => {
                        let events = match parser.feed(&bytes) {
                            Ok(events) => events,
                            Err(problem) => {
                                sink.fail(problem).await;
                                return;
                            }
                        };
                        for event in events {
                            event.id.clone_into(&mut last_id);
                            let mut message = Message::text(Direction::Received, event.data);
                            message.event = Some(event.name);
                            message.id = Some(event.id).filter(|id| !id.is_empty());
                            message.retry = event.retry;
                            if !sink.received(message).await {
                                return;
                            }
                        }
                        if let Some(retry) = parser.retry {
                            retry_ms = retry;
                        }
                    }
                    Ok(None) => break None,
                    Err(e) => break Some(describe(&e)),
                },
                () = job.close.notified() => {
                    sink.closed(None, String::new(), true).await;
                    return;
                }
                command = job.commands.recv() => {
                    if command.is_none() {
                        return;
                    }
                    sink.emit(EventKind::Warning {
                        message: "An event stream only receives; nothing can be sent on it".into(),
                    })
                    .await;
                }
                () = gone.closed() => return,
            }
        };

        let why = match &broke {
            Some(error) => format!("The stream broke: {error}"),
            None => "The server ended the stream".to_owned(),
        };
        if !job.request.sse.reconnect {
            if broke.is_some() {
                sink.fail(why).await;
            } else {
                sink.closed(None, why, false).await;
            }
            return;
        }
        reconnecting = true;
        let mut note = format!(". Connecting again in {retry_ms} ms");
        if !last_id.is_empty() {
            note.push_str(&format!(" with Last-Event-ID {last_id}"));
        }
        if !wait(job, sink, format!("{why}{note}"), retry_ms).await {
            return;
        }
    }
}

/// Sends the request, answering a Digest challenge if the request uses
/// Digest authentication.
async fn open(job: &Job, outgoing: &mut Outgoing) -> Result<(reqwest::Response, f64)> {
    let (response, time_ms) = job.http.open(outgoing, job.transport.clone()).await?;
    if let Auth::Digest { username, password } = &job.auth
        && response.status() == reqwest::StatusCode::UNAUTHORIZED
    {
        let headers = header_list(response.headers());
        let username = job.vars.render(username);
        let password = job.vars.render(password);
        if let Some(answer) = answer_digest(&headers, &username, &password, outgoing) {
            set_header(&mut outgoing.headers, "Authorization", answer?);
            return job.http.open(outgoing, job.transport.clone()).await;
        }
    }
    Ok((response, time_ms))
}

/// Says why and waits before reconnecting. Returns false when the session
/// was closed or abandoned meanwhile.
async fn wait(job: &mut Job, sink: &mut Sink, note: String, retry_ms: u64) -> bool {
    if !sink.emit(EventKind::Note { message: note }).await {
        return false;
    }
    let gone = sink.watch();
    tokio::select! {
        () = tokio::time::sleep(Duration::from_millis(retry_ms)) => true,
        () = job.close.notified() => {
            sink.closed(None, String::new(), true).await;
            false
        }
        () = gone.closed() => false,
    }
}

/// The start of a body, for explaining an error response.
async fn excerpt(response: &mut reqwest::Response) -> String {
    let mut body = Vec::new();
    let read = async {
        while body.len() < 2048 {
            match response.chunk().await {
                Ok(Some(chunk)) => body.extend_from_slice(&chunk),
                _ => break,
            }
        }
    };
    let _ = tokio::time::timeout(Duration::from_secs(2), read).await;
    String::from_utf8_lossy(&body)
        .trim()
        .chars()
        .take(200)
        .collect()
}

/// An event read from a stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Event {
    /// Its type: `message` unless the stream named one.
    pub(crate) name: String,
    /// Its data, lines joined with line feeds.
    pub(crate) data: String,
    /// The last event id the stream set, possibly by an earlier event.
    pub(crate) id: String,
    /// The reconnection time set in the same block, in milliseconds.
    pub(crate) retry: Option<u64>,
}

/// Reads the `text/event-stream` format as it arrives, in chunks that may
/// split a line, a line ending or a character anywhere.
#[derive(Debug, Default)]
pub(crate) struct Parser {
    line: Vec<u8>,
    /// The last chunk ended with a carriage return, so a line feed at the
    /// start of the next one ends the same line.
    after_cr: bool,
    started: bool,
    data: String,
    event: String,
    last_id: String,
    block_retry: Option<u64>,
    /// The latest reconnection time the stream asked for.
    pub(crate) retry: Option<u64>,
}

impl Parser {
    /// Reads a chunk and returns the events it completed. Fails when a line
    /// or an event grows past [`MAX_MESSAGE_BYTES`].
    pub(crate) fn feed(&mut self, mut chunk: &[u8]) -> Result<Vec<Event>, String> {
        let mut events = Vec::new();
        if std::mem::take(&mut self.after_cr) && chunk.first() == Some(&b'\n') {
            chunk = &chunk[1..];
        }
        let mut start = 0;
        let mut at = 0;
        while at < chunk.len() {
            let byte = chunk[at];
            if byte != b'\n' && byte != b'\r' {
                at += 1;
                continue;
            }
            self.line.extend_from_slice(&chunk[start..at]);
            let line = std::mem::take(&mut self.line);
            self.process(&line, &mut events);
            if byte == b'\r' {
                match chunk.get(at + 1) {
                    Some(b'\n') => at += 1,
                    Some(_) => {}
                    None => self.after_cr = true,
                }
            }
            at += 1;
            start = at;
        }
        self.line.extend_from_slice(&chunk[start..]);
        if self.line.len() > MAX_MESSAGE_BYTES || self.data.len() > MAX_MESSAGE_BYTES {
            return Err(format!(
                "An event is larger than {} MB, the most Getman reads",
                MAX_MESSAGE_BYTES / 1024 / 1024
            ));
        }
        Ok(events)
    }

    fn process(&mut self, raw: &[u8], events: &mut Vec<Event>) {
        let decoded = String::from_utf8_lossy(raw);
        let mut line: &str = &decoded;
        if !std::mem::replace(&mut self.started, true) {
            line = line.strip_prefix('\u{feff}').unwrap_or(line);
        }
        if line.is_empty() {
            self.dispatch(events);
            return;
        }
        if line.starts_with(':') {
            return;
        }
        let (field, value) = match line.split_once(':') {
            Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
            None => (line, ""),
        };
        match field {
            "event" => value.clone_into(&mut self.event),
            "data" => {
                self.data.push_str(value);
                self.data.push('\n');
            }
            "id" if !value.contains('\0') => value.clone_into(&mut self.last_id),
            "retry" if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) => {
                if let Ok(retry) = value.parse() {
                    self.retry = Some(retry);
                    self.block_retry = Some(retry);
                }
            }
            _ => {}
        }
    }

    fn dispatch(&mut self, events: &mut Vec<Event>) {
        let retry = self.block_retry.take();
        if self.data.is_empty() {
            self.event.clear();
            return;
        }
        let mut data = std::mem::take(&mut self.data);
        data.pop();
        let event = std::mem::take(&mut self.event);
        events.push(Event {
            name: if event.is_empty() {
                "message".to_owned()
            } else {
                event
            },
            data,
            id: self.last_id.clone(),
            retry,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(chunks: &[&str]) -> Vec<Event> {
        let mut parser = Parser::default();
        chunks
            .iter()
            .flat_map(|chunk| parser.feed(chunk.as_bytes()).unwrap())
            .collect()
    }

    #[test]
    fn events_follow_the_format() {
        let events = read(&[
            "\u{feff}: a comment\n",
            "event: greeting\ndata: hello\n\n",
            "data: line one\ndata:line two\nid: 7\nretry: 1500\n\n",
            "data\n\n",
            "event: ignored\n\n",
            "data: after\n\n",
        ]);
        let summary: Vec<_> = events
            .iter()
            .map(|e| (e.name.as_str(), e.data.as_str(), e.id.as_str(), e.retry))
            .collect();
        assert_eq!(
            summary,
            [
                ("greeting", "hello", "", None),
                ("message", "line one\nline two", "7", Some(1500)),
                ("message", "", "7", None),
                ("message", "after", "7", None),
            ]
        );
    }

    #[test]
    fn lines_and_line_endings_may_be_split_anywhere() {
        let events = read(&["da", "ta: a\r", "\ndata: b\r\r", "data: c\r\n\r", "\n"]);
        let data: Vec<_> = events.iter().map(|e| e.data.as_str()).collect();
        assert_eq!(data, ["a\nb", "c"]);
        let mut parser = Parser::default();
        let bytes = "data: é\n\n".as_bytes();
        let mut events = parser.feed(&bytes[..7]).unwrap();
        events.extend(parser.feed(&bytes[7..]).unwrap());
        assert_eq!(events[0].data, "é");
    }

    #[test]
    fn ids_with_nul_are_ignored_and_huge_events_refused() {
        let events = read(&["id: 1\ndata: a\n\n", "id: 2\0\ndata: b\n\n"]);
        assert_eq!(events[1].id, "1");
        let mut parser = Parser::default();
        let huge = vec![b'x'; MAX_MESSAGE_BYTES + 1];
        assert!(parser.feed(&huge).is_err());
    }
}
