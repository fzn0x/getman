//! The terminal UI's state and what each key does.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use getman_core::Workspace;
use getman_core::execute::{CANCELLED, Cancel, Context, Execution, Executor, persist_changes};
use getman_core::grpc::{Command, Live, LiveEvent};
use getman_core::model::{Auth, Body, KeyValue, Request, RequestKind};
use getman_core::realtime::{self, Options, Outbound, StreamEvent};
use getman_core::runner::{RunEvent, RunOptions};
use getman_core::workspace::{EnvironmentSummary, Node, TrashItem};
use tokio::sync::{mpsc, mpsc::UnboundedSender, oneshot};

use super::oauth::{self, Credentials, Opener, SignIn};
use super::stream::{self, Link, Stream, Tone};
use super::text;

/// How long a gRPC call gets to end by itself after it is cancelled,
/// before it is dropped. A call that is still connecting does not hear
/// the cancellation.
const CANCEL_GRACE: Duration = Duration::from_secs(2);

/// What the event loop hears back from work done in the background. The
/// number says which send, connection or sign-in the news is about, so
/// that news of one the user has left is ignored.
pub(crate) enum Message {
    Sent(u64, Box<Execution>),
    Run(RunEvent),
    /// Something happened on a WebSocket, event stream or Socket.IO
    /// connection.
    Realtime(u64, StreamEvent),
    /// That connection reports no more.
    RealtimeEnded(u64),
    /// Something happened on a gRPC call.
    Grpc(u64, LiveEvent),
    /// The gRPC call is over.
    GrpcDone(u64, Box<Execution>),
    /// The call turned down a message the user sent on its stream.
    GrpcRefused(u64, String),
    SignIn(u64, oauth::Event),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Focus {
    Tree,
    Request,
    Response,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Mode {
    Normal,
    Url,
    Filter,
    /// Typing the name of a new request in a folder.
    Name {
        folder: String,
    },
    /// Asking before moving a request or folder to the trash.
    Delete {
        path: String,
        name: String,
    },
    /// Choosing a saved message to send on a connection.
    Pick,
    /// Typing a message to send on a connection.
    Compose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RequestPane {
    Params,
    Headers,
    Body,
    Auth,
    Checks,
}

impl RequestPane {
    pub(crate) const ALL: [Self; 5] = [
        Self::Params,
        Self::Headers,
        Self::Body,
        Self::Auth,
        Self::Checks,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Params => "Params",
            Self::Headers => "Headers",
            Self::Body => "Body",
            Self::Auth => "Auth",
            Self::Checks => "Checks",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResponsePane {
    Body,
    Headers,
    Tests,
    Console,
}

impl ResponsePane {
    pub(crate) const ALL: [Self; 4] = [Self::Body, Self::Headers, Self::Tests, Self::Console];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Body => "Body",
            Self::Headers => "Headers",
            Self::Tests => "Tests",
            Self::Console => "Console",
        }
    }
}

/// A row of the tree as shown.
#[derive(Debug, Clone)]
pub(crate) struct Row {
    pub(crate) depth: usize,
    pub(crate) path: String,
    pub(crate) name: String,
    pub(crate) method: Option<String>,
    pub(crate) open: bool,
}

/// The request being edited.
#[derive(Debug, Clone)]
pub(crate) struct Open {
    pub(crate) path: String,
    pub(crate) request: Request,
    saved: String,
    pub(crate) cursor: usize,
}

impl Open {
    pub(crate) fn dirty(&self) -> bool {
        serde_yaml_ng::to_string(&self.request).unwrap_or_default() != self.saved
    }
}

/// Text handed to the user's editor, and where it goes back.
#[derive(Debug, Clone)]
pub(crate) struct Edit {
    pub(crate) pane: RequestPane,
    pub(crate) text: String,
    pub(crate) extension: &'static str,
}

pub(crate) struct RunView {
    pub(crate) title: String,
    pub(crate) lines: Vec<(bool, String)>,
    pub(crate) summary: Option<String>,
    pub(crate) scroll: u16,
    /// Set to stop the run.
    stop: Arc<AtomicBool>,
}

#[allow(clippy::struct_excessive_bools)]
pub(crate) struct App {
    pub(crate) executor: Executor,
    pub(crate) name: String,
    tree: Vec<Node>,
    expanded: HashSet<String>,
    pub(crate) rows: Vec<Row>,
    pub(crate) selected: usize,
    pub(crate) filter: String,
    pub(crate) environments: Vec<EnvironmentSummary>,
    pub(crate) environment: Option<usize>,
    pub(crate) open: Option<Open>,
    pub(crate) focus: Focus,
    pub(crate) mode: Mode,
    pub(crate) name_input: String,
    pub(crate) request_pane: RequestPane,
    pub(crate) response_pane: ResponsePane,
    pub(crate) response_scroll: u16,
    pub(crate) sending: bool,
    pub(crate) execution: Option<Box<Execution>>,
    pub(crate) response_lines: Vec<ratatui::text::Line<'static>>,
    pub(crate) message: Option<(String, bool, Instant)>,
    pub(crate) help: bool,
    pub(crate) run: Option<RunView>,
    pub(crate) quit: bool,
    quit_armed: bool,
    pub(crate) edit: Option<Edit>,
    pub(crate) spinner: usize,
    /// The last item moved to the trash, which `u` puts back.
    trashed: Option<TrashItem>,
    /// Counts sends, connections and sign-ins.
    serial: u64,
    /// The serial of the send or connection whose news is wanted.
    current: u64,
    /// Stops the request being sent, or the gRPC call being made.
    cancel: Option<Cancel>,
    /// The WebSocket, event stream, Socket.IO or gRPC session of the open
    /// request.
    pub(crate) stream: Option<Stream>,
    /// The saved message chosen in the list.
    pub(crate) pick: usize,
    /// The message being typed.
    pub(crate) compose: String,
    /// A browser sign-in under way.
    pub(crate) sign_in: Option<SignIn>,
    /// The OAuth 2.0 settings the open request uses, and their token.
    pub(crate) oauth: Option<Credentials>,
    /// Opens the browser.
    pub(crate) opener: Opener,
}

impl App {
    pub(crate) fn new(workspace: Workspace) -> Result<Self, String> {
        let tree = workspace.tree().map_err(|e| e.to_string())?;
        let environments = workspace.environments().map_err(|e| e.to_string())?;
        let mut app = Self {
            name: workspace.name(),
            executor: Executor::new(workspace),
            expanded: tree
                .iter()
                .filter(|n| matches!(n, Node::Folder { .. }))
                .map(|n| n.path().to_owned())
                .collect(),
            tree,
            rows: Vec::new(),
            selected: 0,
            filter: String::new(),
            environment: None,
            environments,
            open: None,
            focus: Focus::Tree,
            mode: Mode::Normal,
            name_input: String::new(),
            request_pane: RequestPane::Params,
            response_pane: ResponsePane::Body,
            response_scroll: 0,
            sending: false,
            execution: None,
            response_lines: Vec::new(),
            message: None,
            help: false,
            run: None,
            quit: false,
            quit_armed: false,
            edit: None,
            spinner: 0,
            trashed: None,
            serial: 0,
            current: 0,
            cancel: None,
            stream: None,
            pick: 0,
            compose: String::new(),
            sign_in: None,
            oauth: None,
            opener: oauth::open_browser,
        };
        app.rebuild_rows();
        Ok(app)
    }

    fn workspace(&self) -> &Workspace {
        &self.executor.workspace
    }

    pub(crate) fn environment_name(&self) -> String {
        self.environment
            .and_then(|i| self.environments.get(i))
            .map_or_else(|| "no environment".to_owned(), |e| e.name.clone())
    }

    fn environment_id(&self) -> Option<String> {
        self.environment
            .and_then(|i| self.environments.get(i))
            .map(|e| e.id.clone())
    }

    pub(crate) fn say(&mut self, text: impl Into<String>, error: bool) {
        self.message = Some((text.into(), error, Instant::now()));
    }

    pub(crate) fn rebuild_rows(&mut self) {
        let mut rows = Vec::new();
        let filter = self.filter.to_lowercase();
        walk(&self.tree, 0, &self.expanded, &filter, &mut rows);
        self.rows = rows;
        self.selected = self.selected.min(self.rows.len().saturating_sub(1));
    }

    fn refresh(&mut self) {
        match self.workspace().tree() {
            Ok(tree) => {
                self.tree = tree;
                self.rebuild_rows();
            }
            Err(e) => self.say(e.to_string(), true),
        }
    }

    // ---- Events -------------------------------------------------------------------

    pub(crate) fn on_message(&mut self, message: Message) {
        match message {
            Message::Sent(serial, execution) => {
                if serial != self.current {
                    return;
                }
                self.sending = false;
                self.cancel = None;
                self.response_scroll = 0;
                self.response_lines = text::response_lines(&execution);
                let failed = execution.failed();
                if failed && execution.response.is_some() {
                    self.response_pane = ResponsePane::Tests;
                }
                self.execution = Some(execution);
            }
            Message::Run(event) => {
                let Some(view) = &mut self.run else { return };
                match event {
                    RunEvent::Request {
                        execution,
                        iteration,
                        ..
                    } => {
                        let status = execution
                            .response
                            .as_ref()
                            .map_or_else(|| "error".to_owned(), |r| r.status.to_string());
                        let failures: Vec<String> = execution
                            .error
                            .iter()
                            .cloned()
                            .chain(
                                execution
                                    .script_errors
                                    .iter()
                                    .map(|e| format!("script: {}", e.message)),
                            )
                            .chain(
                                execution
                                    .tests
                                    .iter()
                                    .chain(&execution.checks)
                                    .filter(|t| !t.passed)
                                    .map(|t| {
                                        format!(
                                            "{} — {}",
                                            t.name,
                                            t.message.clone().unwrap_or_default()
                                        )
                                    }),
                            )
                            .collect();
                        view.lines.push((
                            !execution.failed(),
                            format!("#{} {} {}", iteration + 1, execution.name, status),
                        ));
                        for failure in failures {
                            view.lines.push((false, format!("    {failure}")));
                        }
                    }
                    RunEvent::Finish { summary } => {
                        view.summary = Some(format!(
                            "{} requests · {} failed · tests {}/{} · checks {}/{} · {:.0} ms",
                            summary.requests,
                            summary.failed_requests,
                            summary.tests_passed,
                            summary.tests_passed + summary.tests_failed,
                            summary.checks_passed,
                            summary.checks_passed + summary.checks_failed,
                            summary.duration_ms
                        ));
                    }
                    RunEvent::Start { .. } | RunEvent::Iteration { .. } => {}
                }
            }
            Message::Realtime(serial, event) => {
                if let Some(stream) = self.stream_of(serial) {
                    stream.on_realtime(event);
                }
            }
            Message::RealtimeEnded(serial) => {
                if let Some(stream) = self.stream_of(serial) {
                    stream.on_ended();
                }
            }
            Message::Grpc(serial, event) => {
                if let Some(stream) = self.stream_of(serial) {
                    stream.on_grpc(event);
                }
            }
            Message::GrpcRefused(serial, message) => {
                if let Some(stream) = self.stream_of(serial) {
                    stream.note(Tone::Error, &message);
                }
            }
            Message::GrpcDone(serial, execution) => {
                if let Some(stream) = self.stream_of(serial) {
                    stream.on_grpc_done(&execution);
                    self.cancel = None;
                    self.execution = Some(execution);
                }
            }
            Message::SignIn(serial, event) => self.on_sign_in(serial, event),
        }
    }

    /// The session of the open request, if news from `serial` is about it.
    fn stream_of(&mut self, serial: u64) -> Option<&mut Stream> {
        self.stream.as_mut().filter(|s| s.serial == serial)
    }

    fn on_sign_in(&mut self, serial: u64, event: oauth::Event) {
        let Some(sign_in) = self.sign_in.as_mut().filter(|s| s.serial == serial) else {
            return;
        };
        match event {
            oauth::Event::Waiting {
                authorization,
                opened,
            } => {
                sign_in.authorization = Some(authorization);
                sign_in.opened = opened;
                let note = if opened {
                    "Waiting for you to sign in in the browser · Esc cancels"
                } else {
                    "The browser did not open: open the address in the Auth pane · Esc cancels"
                };
                self.say(note, !opened);
            }
            oauth::Event::Done(Ok(token)) => {
                self.sign_in = None;
                self.say(
                    oauth::describe(&token, getman_core::time::now_millis()),
                    false,
                );
                if let Some(oauth) = &mut self.oauth {
                    oauth.token = Some(token);
                }
            }
            oauth::Event::Done(Err(message)) => {
                self.sign_in = None;
                self.say(message, true);
            }
        }
    }

    pub(crate) fn on_key(&mut self, key: KeyEvent, tx: &UnboundedSender<Message>) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        match self.mode.clone() {
            Mode::Url => return self.url_key(key, tx),
            Mode::Filter => return self.filter_key(key),
            Mode::Name { folder } => return self.name_key(key, &folder),
            Mode::Delete { path, .. } => return self.delete_key(key, &path),
            Mode::Pick => return self.pick_key(key),
            Mode::Compose => return self.compose_key(key, tx),
            Mode::Normal => {}
        }
        if self.help {
            self.help = false;
            return;
        }
        if self.run.is_some() {
            self.run_key(key);
            return;
        }
        let quit_armed = std::mem::take(&mut self.quit_armed);
        match key.code {
            KeyCode::Char('q') => {
                if self.open.as_ref().is_some_and(Open::dirty) && !quit_armed {
                    self.quit_armed = true;
                    self.say(
                        "The request has unsaved changes: w saves, q again quits",
                        true,
                    );
                } else {
                    self.quit = true;
                }
            }
            KeyCode::Char('?') => self.help = true,
            KeyCode::Tab => self.cycle_focus(true),
            KeyCode::BackTab => self.cycle_focus(false),
            KeyCode::Char('e') => self.cycle_environment(true),
            KeyCode::Char('E') if self.focus != Focus::Request => self.cycle_environment(false),
            KeyCode::Char('s') => self.send(tx),
            KeyCode::Esc => self.cancel_flight(false),
            KeyCode::Char('x') => self.cancel_flight(true),
            KeyCode::Char('m') if self.is_connection() => self.choose_message(tx),
            KeyCode::Char('t') if self.is_connection() => self.start_typing(),
            KeyCode::Char('f') if self.is_connection() => self.end_stream(),
            KeyCode::Char('b')
                if self.focus == Focus::Request && self.request_pane == RequestPane::Auth =>
            {
                self.start_sign_in(tx);
            }
            KeyCode::Char('w') => self.save(),
            KeyCode::Char('/') => {
                self.focus = Focus::Tree;
                self.mode = Mode::Filter;
            }
            KeyCode::Char('r') => self.run_selected(tx),
            KeyCode::Char('R') => self.refresh(),
            _ => match self.focus {
                Focus::Tree => self.tree_key(key),
                Focus::Request => self.request_key(key),
                Focus::Response => self.response_key(key),
            },
        }
    }

    fn cycle_focus(&mut self, forward: bool) {
        let mut next = step(
            &[Focus::Tree, Focus::Request, Focus::Response],
            self.focus,
            forward,
        );
        if self.open.is_none() && next != Focus::Tree {
            next = Focus::Tree;
        }
        self.focus = next;
    }

    fn cycle_environment(&mut self, forward: bool) {
        // Slot 0 is "no environment"; slot i + 1 is environment i.
        let count = self.environments.len() + 1;
        let current = self.environment.map_or(0, |i| i + 1);
        let next = wrap(current, count, forward);
        self.environment = next.checked_sub(1);
        self.refresh_credentials();
        self.say(format!("Environment: {}", self.environment_name()), false);
    }

    fn tree_key(&mut self, key: KeyEvent) {
        let last = self.rows.len().saturating_sub(1);
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => self.selected = (self.selected + 1).min(last),
            KeyCode::Up | KeyCode::Char('k') => self.selected = self.selected.saturating_sub(1),
            KeyCode::Char('g') | KeyCode::Home => self.selected = 0,
            KeyCode::Char('G') | KeyCode::End => self.selected = last,
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l' | 'o') => self.activate(),
            KeyCode::Left | KeyCode::Char('h') => self.collapse(),
            KeyCode::Char('a') => {
                let folder = self.selected_folder();
                self.name_input.clear();
                self.mode = Mode::Name { folder };
            }
            KeyCode::Char('d') | KeyCode::Delete => {
                if let Some(row) = self.rows.get(self.selected) {
                    self.mode = Mode::Delete {
                        path: row.path.clone(),
                        name: row.name.clone(),
                    };
                }
            }
            KeyCode::Char('u') => self.restore_trashed(),
            _ => {}
        }
    }

    /// Moves the item to the trash once `y` confirms it.
    fn delete_key(&mut self, key: KeyEvent, path: &str) {
        self.mode = Mode::Normal;
        if key.code != KeyCode::Char('y') {
            return;
        }
        let inside = |p: &str| p == path || p.starts_with(&format!("{path}/"));
        if self
            .open
            .as_ref()
            .is_some_and(|open| inside(&open.path) && open.dirty())
        {
            self.say("Save (w) or undo the changes before deleting it", true);
            return;
        }
        match self.workspace().trash(path) {
            Ok(item) => {
                if self.open.as_ref().is_some_and(|open| inside(&open.path)) {
                    self.abandon();
                    self.open = None;
                    self.execution = None;
                    self.response_lines.clear();
                    self.focus = Focus::Tree;
                }
                self.say(
                    format!("Moved {} to the trash · u puts it back", item.name),
                    false,
                );
                self.trashed = Some(item);
                self.refresh();
            }
            Err(e) => self.say(e.to_string(), true),
        }
    }

    /// Puts back the item deleted last.
    fn restore_trashed(&mut self) {
        let Some(item) = self.trashed.take() else {
            self.say("Nothing to put back", true);
            return;
        };
        match self.workspace().restore(&item.id) {
            Ok(path) => {
                if let Some((parent, _)) = path.rsplit_once('/') {
                    self.expanded.insert(parent.to_owned());
                }
                self.refresh();
                if let Some(at) = self.rows.iter().position(|r| r.path == path) {
                    self.selected = at;
                }
                self.say(format!("Put {} back", item.name), false);
            }
            Err(e) => self.say(e.to_string(), true),
        }
    }

    /// The folder the selected row is, or is in.
    fn selected_folder(&self) -> String {
        match self.rows.get(self.selected) {
            Some(row) if row.method.is_none() => row.path.clone(),
            Some(row) => row
                .path
                .rsplit_once('/')
                .map_or_else(String::new, |(parent, _)| parent.to_owned()),
            None => String::new(),
        }
    }

    fn activate(&mut self) {
        let Some(row) = self.rows.get(self.selected).cloned() else {
            return;
        };
        if row.method.is_none() {
            if !self.expanded.remove(&row.path) {
                self.expanded.insert(row.path.clone());
            }
            self.rebuild_rows();
        } else {
            self.open_request(&row.path);
        }
    }

    fn collapse(&mut self) {
        let Some(row) = self.rows.get(self.selected).cloned() else {
            return;
        };
        if row.method.is_none() && row.open {
            self.expanded.remove(&row.path);
            self.rebuild_rows();
        } else if let Some((parent, _)) = row.path.rsplit_once('/')
            && let Some(at) = self.rows.iter().position(|r| r.path == parent)
        {
            self.selected = at;
        }
    }

    pub(crate) fn open_request(&mut self, path: &str) {
        // Opening the request again must not drop its connection.
        if self.open.as_ref().is_some_and(|open| open.path == path)
            && self.stream.as_ref().is_some_and(Stream::is_live)
        {
            self.focus = Focus::Request;
            return;
        }
        if let Some(open) = &self.open
            && open.path != path
            && open.dirty()
        {
            self.say(
                "Save (w) or undo the changes before opening another request",
                true,
            );
            return;
        }
        match self.workspace().read_request(path) {
            Ok(request) => {
                let saved = serde_yaml_ng::to_string(&request).unwrap_or_default();
                let cursor = request.url.chars().count();
                self.open = Some(Open {
                    path: path.to_owned(),
                    request,
                    saved,
                    cursor,
                });
                self.abandon();
                self.execution = None;
                self.response_lines.clear();
                self.focus = Focus::Request;
                self.refresh_credentials();
            }
            Err(e) => self.say(e.to_string(), true),
        }
    }

    /// Stops whatever the request that was open is doing, and forgets the
    /// news it would still send.
    fn abandon(&mut self) {
        self.serial += 1;
        self.current = self.serial;
        self.sending = false;
        if let Some(cancel) = self.cancel.take() {
            cancel.cancel();
        }
        if let Some(mut stream) = self.stream.take() {
            stream.stop();
        }
        if let Some(sign_in) = self.sign_in.take() {
            sign_in.cancel(&self.executor);
        }
        if self.mode == Mode::Pick || self.mode == Mode::Compose {
            self.mode = Mode::Normal;
        }
    }

    fn request_key(&mut self, key: KeyEvent) {
        let Some(open) = &mut self.open else { return };
        match key.code {
            KeyCode::Char('i') | KeyCode::Enter => {
                open.cursor = open.request.url.chars().count();
                self.mode = Mode::Url;
            }
            KeyCode::Char('m') if open.request.kind.is_http() => {
                cycle_method(&mut open.request, true);
            }
            KeyCode::Char('M') if open.request.kind.is_http() => {
                cycle_method(&mut open.request, false);
            }
            KeyCode::Char(c @ '1'..='5') => {
                self.set_request_pane(RequestPane::ALL[(c as usize) - ('1' as usize)]);
            }
            KeyCode::Right | KeyCode::Char(']' | 'l') => {
                self.set_request_pane(step(&RequestPane::ALL, self.request_pane, true));
            }
            KeyCode::Left | KeyCode::Char('[' | 'h') => {
                self.set_request_pane(step(&RequestPane::ALL, self.request_pane, false));
            }
            KeyCode::Char('E' | 'o') => {
                let (text, extension) = editable(&open.request, self.request_pane);
                self.edit = Some(Edit {
                    pane: self.request_pane,
                    text,
                    extension,
                });
            }
            _ => {}
        }
    }

    fn response_key(&mut self, key: KeyEvent) {
        // Scrolling up leaves the end of a log; the log notices when the
        // user is back at the end.
        if matches!(
            key.code,
            KeyCode::Up | KeyCode::Char('k' | 'g') | KeyCode::PageUp | KeyCode::Home
        ) && let Some(stream) = &mut self.stream
        {
            stream.follow = false;
        }
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                self.response_scroll = self.response_scroll.saturating_add(1);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.response_scroll = self.response_scroll.saturating_sub(1);
            }
            KeyCode::PageDown | KeyCode::Char(' ') => {
                self.response_scroll = self.response_scroll.saturating_add(20);
            }
            KeyCode::PageUp => self.response_scroll = self.response_scroll.saturating_sub(20),
            KeyCode::Char('g') | KeyCode::Home => self.response_scroll = 0,
            KeyCode::Char('G') | KeyCode::End => self.response_scroll = u16::MAX / 2,
            KeyCode::Char(c @ '1'..='4') => {
                self.response_pane = ResponsePane::ALL[(c as usize) - ('1' as usize)];
                self.response_scroll = 0;
            }
            KeyCode::Right | KeyCode::Char(']' | 'l') => {
                self.response_pane = step(&ResponsePane::ALL, self.response_pane, true);
                self.response_scroll = 0;
            }
            KeyCode::Left | KeyCode::Char('[' | 'h') => {
                self.response_pane = step(&ResponsePane::ALL, self.response_pane, false);
                self.response_scroll = 0;
            }
            _ => {}
        }
    }

    fn url_key(&mut self, key: KeyEvent, tx: &UnboundedSender<Message>) {
        let Some(open) = &mut self.open else {
            self.mode = Mode::Normal;
            return;
        };
        let url = &mut open.request.url;
        let len = url.chars().count();
        let byte = |s: &str, at: usize| s.char_indices().nth(at).map_or(s.len(), |(i, _)| i);
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Enter => {
                self.mode = Mode::Normal;
                self.send(tx);
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                url.clear();
                open.cursor = 0;
            }
            KeyCode::Char(c) => {
                let at = byte(url, open.cursor);
                url.insert(at, c);
                open.cursor += 1;
            }
            KeyCode::Backspace if open.cursor > 0 => {
                let at = byte(url, open.cursor - 1);
                url.remove(at);
                open.cursor -= 1;
            }
            KeyCode::Delete if open.cursor < len => {
                let at = byte(url, open.cursor);
                url.remove(at);
            }
            KeyCode::Left => open.cursor = open.cursor.saturating_sub(1),
            KeyCode::Right => open.cursor = (open.cursor + 1).min(len),
            KeyCode::Home => open.cursor = 0,
            KeyCode::End => open.cursor = len,
            _ => {}
        }
    }

    fn filter_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.filter.clear();
                self.mode = Mode::Normal;
                self.rebuild_rows();
            }
            KeyCode::Enter => {
                self.mode = Mode::Normal;
                if self
                    .rows
                    .get(self.selected)
                    .is_some_and(|r| r.method.is_some())
                {
                    self.activate();
                }
            }
            KeyCode::Down => {
                self.selected = (self.selected + 1).min(self.rows.len().saturating_sub(1));
            }
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Backspace => {
                self.filter.pop();
                self.selected = 0;
                self.rebuild_rows();
            }
            KeyCode::Char(c) => {
                self.filter.push(c);
                self.selected = 0;
                self.rebuild_rows();
            }
            _ => {}
        }
    }

    fn name_key(&mut self, key: KeyEvent, folder: &str) {
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Enter => {
                self.mode = Mode::Normal;
                let name = self.name_input.trim().to_owned();
                if name.is_empty() {
                    return;
                }
                let request = Request {
                    name: name.clone(),
                    ..Request::default()
                };
                match self.workspace().create_request(folder, &name, &request) {
                    Ok(path) => {
                        if !folder.is_empty() {
                            self.expanded.insert(folder.to_owned());
                        }
                        self.refresh();
                        if let Some(at) = self.rows.iter().position(|r| r.path == path) {
                            self.selected = at;
                        }
                        self.open_request(&path);
                        self.mode = Mode::Url;
                    }
                    Err(e) => self.say(e.to_string(), true),
                }
            }
            KeyCode::Backspace => {
                self.name_input.pop();
            }
            KeyCode::Char(c) => self.name_input.push(c),
            _ => {}
        }
    }

    fn run_key(&mut self, key: KeyEvent) {
        let Some(view) = &mut self.run else { return };
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                // A run that is still going stops with its view.
                view.stop.store(true, Ordering::Relaxed);
                self.run = None;
            }
            KeyCode::Down | KeyCode::Char('j') => view.scroll = view.scroll.saturating_add(1),
            KeyCode::Up | KeyCode::Char('k') => view.scroll = view.scroll.saturating_sub(1),
            _ => {}
        }
    }

    // ---- Actions ------------------------------------------------------------------

    fn send(&mut self, tx: &UnboundedSender<Message>) {
        let Some(open) = &self.open else {
            self.say(
                "Open a request first: pick one in the tree and press Enter",
                true,
            );
            return;
        };
        if self.sending {
            return;
        }
        if self.stream.as_ref().is_some_and(Stream::is_live) {
            let how = if open.request.kind == RequestKind::Grpc {
                "x cancels the call"
            } else {
                "x disconnects"
            };
            self.say(format!("It is already running: {how}"), true);
            return;
        }
        match open.request.kind {
            RequestKind::Http => self.send_http(tx),
            RequestKind::Grpc => self.invoke(tx),
            RequestKind::Websocket | RequestKind::Sse | RequestKind::Socketio => {
                self.connect(tx);
            }
        }
    }

    fn next_serial(&mut self) -> u64 {
        self.serial += 1;
        self.current = self.serial;
        self.serial
    }

    fn send_http(&mut self, tx: &UnboundedSender<Message>) {
        let Some(open) = &self.open else { return };
        self.sending = true;
        self.focus = Focus::Response;
        let executor = self.executor.clone();
        let request = open.request.clone();
        let path = open.path.clone();
        let environment = self.environment_id();
        let serial = self.next_serial();
        let cancel = Cancel::new();
        self.cancel = Some(cancel.clone());
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut context = Context {
                environment: environment.clone(),
                cancel: Some(cancel),
                ..Context::default()
            };
            let execution = executor.execute(Some(&path), &request, &mut context).await;
            let _ = persist_changes(
                &executor.workspace,
                environment.as_deref(),
                &execution.changes,
            );
            let _ = tx.send(Message::Sent(serial, Box::new(execution)));
        });
    }

    /// Opens the WebSocket, event stream or Socket.IO connection.
    fn connect(&mut self, tx: &UnboundedSender<Message>) {
        let Some(open) = &self.open else { return };
        let context = Context {
            environment: self.environment_id(),
            ..Context::default()
        };
        let session = realtime::connect(
            &self.executor,
            Some(&open.path),
            &open.request,
            &context,
            Options { lossy: true },
        );
        let kind = open.request.kind;
        match session {
            Ok(session) => {
                let serial = self.next_serial();
                let (control, mut events) = session.into_parts();
                self.begin_stream(Stream::new(serial, kind, Link::Realtime(control)));
                let tx = tx.clone();
                tokio::spawn(async move {
                    while let Some(event) = events.recv().await {
                        if tx.send(Message::Realtime(serial, event)).is_err() {
                            return;
                        }
                    }
                    let _ = tx.send(Message::RealtimeEnded(serial));
                });
            }
            Err(e) => self.say(e.to_string(), true),
        }
    }

    /// Starts showing a new session in the response.
    fn begin_stream(&mut self, stream: Stream) {
        self.stream = Some(stream);
        self.execution = None;
        self.response_lines.clear();
        self.response_scroll = 0;
        self.response_pane = ResponsePane::Body;
        self.focus = Focus::Response;
    }

    /// Calls the gRPC method. Calls that stream from the client wait for
    /// messages to be sent.
    fn invoke(&mut self, tx: &UnboundedSender<Message>) {
        let Some(open) = &self.open else { return };
        let executor = self.executor.clone();
        let request = open.request.clone();
        let path = open.path.clone();
        let environment = self.environment_id();
        let serial = self.next_serial();
        let cancel = Cancel::new();
        self.cancel = Some(cancel.clone());
        let (commands, command_receiver) = mpsc::unbounded_channel();
        self.begin_stream(Stream::new(serial, RequestKind::Grpc, Link::Grpc(commands)));
        let tx = tx.clone();
        tokio::spawn(async move {
            let (events, mut event_receiver) = mpsc::unbounded_channel();
            let mut context = Context {
                environment: environment.clone(),
                ..Context::default()
            };
            let live = Live {
                commands: command_receiver,
                events,
            };
            let run = executor.execute_live(Some(&path), &request, &mut context, live);
            let forward = async {
                while let Some(event) = event_receiver.recv().await {
                    let _ = tx.send(Message::Grpc(serial, event));
                }
            };
            let work = async { tokio::join!(run, forward).0 };
            // Asking the call to stop is heard once it has started; this
            // frees the user from one that is still connecting.
            let impatient = async {
                cancel.cancelled().await;
                tokio::time::sleep(CANCEL_GRACE).await;
            };
            let execution = tokio::select! {
                execution = work => execution,
                () = impatient => Execution {
                    path: Some(path.clone()),
                    name: request.name.clone(),
                    error: Some(CANCELLED.to_owned()),
                    cancelled: true,
                    ..Execution::default()
                },
            };
            let _ = persist_changes(
                &executor.workspace,
                environment.as_deref(),
                &execution.changes,
            );
            let _ = tx.send(Message::GrpcDone(serial, Box::new(execution)));
        });
    }

    // ---- Connections ---------------------------------------------------------------

    /// Whether the open request is one that is connected to, not sent.
    fn is_connection(&self) -> bool {
        self.open
            .as_ref()
            .is_some_and(|open| !open.request.kind.is_http())
    }

    /// Stops what is in flight: the request being sent, the connection or
    /// call, the sign-in. `asked` is set when the user pressed the key
    /// that is only for this, so it may say that there was nothing to stop.
    fn cancel_flight(&mut self, asked: bool) {
        let mut stopped = false;
        if let Some(sign_in) = self.sign_in.take() {
            sign_in.cancel(&self.executor);
            self.say("Sign-in cancelled", false);
            stopped = true;
        }
        if let Some(stream) = &mut self.stream
            && stream.is_live()
        {
            stream.stop();
            let what = if stream.kind == RequestKind::Grpc {
                "Cancelling the call"
            } else {
                "Disconnecting"
            };
            if let Some(cancel) = &self.cancel {
                cancel.cancel();
            }
            self.say(what, false);
            stopped = true;
        } else if self.sending {
            if let Some(cancel) = &self.cancel {
                cancel.cancel();
            }
            self.say("Cancelling the request", false);
            stopped = true;
        }
        if !stopped && asked {
            self.say("Nothing is running", true);
        }
    }

    /// Whether messages can be sent now; says why not.
    fn can_send(&mut self) -> bool {
        let live = self.stream.as_ref().is_some_and(Stream::is_live);
        if !live {
            let how = if self
                .open
                .as_ref()
                .is_some_and(|o| o.request.kind == RequestKind::Grpc)
            {
                "Not calling: s invokes the method first"
            } else {
                "Not connected: s connects first"
            };
            self.say(how, true);
        }
        live
    }

    /// Whether the open gRPC call takes messages from the client; says
    /// why not.
    fn grpc_takes_messages(&mut self) -> bool {
        if !self.can_send() {
            return false;
        }
        match self.stream.as_ref().and_then(|s| s.call_type) {
            None => {
                self.say("The call is still starting; try again in a moment", true);
                false
            }
            Some(call_type) if !call_type.client_streams() => {
                self.say(
                    format!(
                        "A {} call sends its message when it starts",
                        stream::call_label(call_type)
                    ),
                    true,
                );
                false
            }
            Some(_) => true,
        }
    }

    /// `m`: sends a saved message. On a WebSocket or Socket.IO connection
    /// it opens the list; on a gRPC stream it sends the request's message.
    fn choose_message(&mut self, tx: &UnboundedSender<Message>) {
        let Some(open) = &self.open else { return };
        match open.request.kind {
            RequestKind::Grpc => {
                if !self.grpc_takes_messages() {
                    return;
                }
                let Some(open) = &self.open else { return };
                let text = open.request.grpc.message.clone();
                if text.trim().is_empty() {
                    self.say("The request has no saved message; t types one", true);
                } else {
                    self.send_grpc(text, tx);
                }
            }
            RequestKind::Sse => self.say("Event streams only receive", true),
            _ => {
                if !self.can_send() {
                    return;
                }
                let Some(open) = &self.open else { return };
                if stream::saved_messages(&open.request).is_empty() {
                    self.say("The request has no saved messages; t types one", true);
                } else {
                    self.pick = 0;
                    self.mode = Mode::Pick;
                }
            }
        }
    }

    /// `t`: starts typing a message to send.
    fn start_typing(&mut self) {
        let Some(open) = &self.open else { return };
        let ready = match open.request.kind {
            RequestKind::Sse => {
                self.say("Event streams only receive", true);
                false
            }
            RequestKind::Grpc => self.grpc_takes_messages(),
            _ => self.can_send(),
        };
        if ready {
            self.compose.clear();
            self.mode = Mode::Compose;
        }
    }

    /// `f`: ends the client's side of a gRPC stream.
    fn end_stream(&mut self) {
        if self
            .open
            .as_ref()
            .is_none_or(|o| o.request.kind != RequestKind::Grpc)
        {
            self.say(
                "Only a gRPC stream from the client ends; x disconnects",
                true,
            );
            return;
        }
        if !self.grpc_takes_messages() {
            return;
        }
        if let Some(stream) = &mut self.stream
            && let Link::Grpc(commands) = &stream.link
        {
            if commands.send(Command::End).is_ok() {
                stream.note(Tone::Note, "You ended your side of the stream");
            } else {
                self.say("The call has ended", true);
            }
        }
    }

    fn send_outbound(&mut self, outbound: Outbound) {
        if let Some(stream) = &self.stream
            && let Link::Realtime(control) = &stream.link
            && let Err(e) = control.send(outbound)
        {
            self.say(e.to_string(), true);
        }
    }

    /// Sends messages written as JSON on a gRPC stream.
    fn send_grpc(&mut self, text: String, tx: &UnboundedSender<Message>) {
        let Some(stream) = &self.stream else { return };
        let Link::Grpc(commands) = &stream.link else {
            return;
        };
        let (reply, answer) = oneshot::channel();
        if commands.send(Command::Send { text, reply }).is_err() {
            self.say("The call has ended", true);
            return;
        }
        let serial = stream.serial;
        let tx = tx.clone();
        tokio::spawn(async move {
            if let Ok(Err(message)) = answer.await {
                let _ = tx.send(Message::GrpcRefused(serial, message));
            }
        });
    }

    fn pick_key(&mut self, key: KeyEvent) {
        let saved = self
            .open
            .as_ref()
            .map(|open| stream::saved_messages(&open.request))
            .unwrap_or_default();
        let chosen = match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                self.pick = (self.pick + 1).min(saved.len().saturating_sub(1));
                return;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.pick = self.pick.saturating_sub(1);
                return;
            }
            KeyCode::Enter => self.pick,
            KeyCode::Char(c @ '1'..='9') => (c as usize) - ('1' as usize),
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = Mode::Normal;
                return;
            }
            _ => return,
        };
        self.mode = Mode::Normal;
        if let Some(saved) = saved.into_iter().nth(chosen)
            && self.can_send()
        {
            self.send_outbound(saved.outbound);
        }
    }

    fn compose_key(&mut self, key: KeyEvent, tx: &UnboundedSender<Message>) {
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Enter => {
                self.mode = Mode::Normal;
                let text = std::mem::take(&mut self.compose);
                if text.trim().is_empty() || !self.can_send() {
                    return;
                }
                let Some(kind) = self.open.as_ref().map(|o| o.request.kind) else {
                    return;
                };
                if kind == RequestKind::Grpc {
                    self.send_grpc(text, tx);
                } else {
                    match stream::typed(kind, &text) {
                        Ok(outbound) => self.send_outbound(outbound),
                        Err(e) => self.say(e, true),
                    }
                }
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.compose.clear();
            }
            KeyCode::Char(c) => self.compose.push(c),
            KeyCode::Backspace => {
                self.compose.pop();
            }
            _ => {}
        }
    }

    // ---- OAuth 2.0 -----------------------------------------------------------------

    fn set_request_pane(&mut self, pane: RequestPane) {
        self.request_pane = pane;
        if pane == RequestPane::Auth {
            self.refresh_credentials();
        }
    }

    /// Reads the OAuth 2.0 settings the open request ends up with, and the
    /// token kept for them.
    pub(crate) fn refresh_credentials(&mut self) {
        self.oauth = None;
        let Some(open) = &self.open else { return };
        let context = Context {
            environment: self.environment_id(),
            ..Context::default()
        };
        let Ok((effective, vars)) =
            self.executor
                .resolve(Some(&open.path), &open.request, &context)
        else {
            return;
        };
        if let Auth::Oauth2(settings) = &effective.auth {
            self.oauth = Some(Credentials {
                browser: settings.grant.uses_browser(),
                token: self.executor.authenticator.cached_token(settings, &vars),
            });
        }
    }

    /// `b` on the auth pane: opens the browser to sign in, then waits for
    /// the provider to come back.
    fn start_sign_in(&mut self, tx: &UnboundedSender<Message>) {
        let Some(open) = &self.open else { return };
        if self.sign_in.is_some() {
            self.say("Already waiting for the browser · Esc cancels", true);
            return;
        }
        let context = Context {
            environment: self.environment_id(),
            ..Context::default()
        };
        let prepared = self
            .executor
            .resolve(Some(&open.path), &open.request, &context)
            .and_then(|(effective, vars)| {
                let transport =
                    self.executor
                        .transport(Some(&open.path), &open.request, &context)?;
                Ok((effective.auth, vars, transport))
            });
        let (auth, vars, transport) = match prepared {
            Ok(prepared) => prepared,
            Err(e) => {
                self.say(e.to_string(), true);
                return;
            }
        };
        let Auth::Oauth2(settings) = auth else {
            self.say(
                "The request does not use OAuth 2.0: E edits the auth, or a folder sets it",
                true,
            );
            return;
        };
        if !settings.grant.uses_browser() {
            self.say(
                "This grant needs no browser: the token is fetched when a request needs it",
                true,
            );
            return;
        }
        self.serial += 1;
        self.sign_in = Some(SignIn::start(
            self.serial,
            &self.executor,
            settings,
            vars,
            transport,
            self.opener,
            tx.clone(),
        ));
        self.say("Opening your browser to sign in", false);
    }

    fn save(&mut self) {
        let Some(open) = &mut self.open else { return };
        match self
            .executor
            .workspace
            .write_request(&open.path, &open.request)
        {
            Ok(()) => {
                open.saved = serde_yaml_ng::to_string(&open.request).unwrap_or_default();
                let path = open.path.clone();
                self.refresh();
                self.say(format!("Saved {path}"), false);
            }
            Err(e) => self.say(e.to_string(), true),
        }
    }

    fn run_selected(&mut self, tx: &UnboundedSender<Message>) {
        let target = match self.rows.get(self.selected) {
            Some(row) if self.focus == Focus::Tree => row.path.clone(),
            _ => String::new(),
        };
        let title = if target.is_empty() {
            self.name.clone()
        } else {
            target.clone()
        };
        let stop = Arc::new(AtomicBool::new(false));
        self.run = Some(RunView {
            title,
            lines: Vec::new(),
            summary: None,
            scroll: 0,
            stop: Arc::clone(&stop),
        });
        let executor = self.executor.clone();
        let options = RunOptions {
            target,
            environment: self.environment_id(),
            ..RunOptions::default()
        };
        let tx = tx.clone();
        tokio::spawn(async move {
            let result = getman_core::runner::run(&executor, &options, &stop, |event| {
                let _ = tx.send(Message::Run(event));
            })
            .await;
            if let Err(e) = result {
                let _ = tx.send(Message::Run(RunEvent::Finish {
                    summary: getman_core::runner::RunSummary {
                        stopped: Some(e.to_string()),
                        ..Default::default()
                    },
                }));
            }
        });
    }

    /// Takes back text the user edited in their editor.
    pub(crate) fn apply_edit(&mut self, edit: &Edit, result: Result<String, String>) {
        let text = match result {
            Ok(text) => text,
            Err(e) => {
                self.say(e, true);
                return;
            }
        };
        let Some(open) = &mut self.open else { return };
        match apply(&mut open.request, edit.pane, &text) {
            Ok(()) => {
                self.say(format!("{} updated; w saves", edit.pane.label()), false);
                if edit.pane == RequestPane::Auth {
                    self.refresh_credentials();
                }
            }
            Err(e) => self.say(e, true),
        }
    }
}

fn walk(
    nodes: &[Node],
    depth: usize,
    expanded: &HashSet<String>,
    filter: &str,
    rows: &mut Vec<Row>,
) {
    for node in nodes {
        match node {
            Node::Folder {
                path,
                name,
                children,
            } => {
                if filter.is_empty() {
                    let open = expanded.contains(path);
                    rows.push(Row {
                        depth,
                        path: path.clone(),
                        name: name.clone(),
                        method: None,
                        open,
                    });
                    if open {
                        walk(children, depth + 1, expanded, filter, rows);
                    }
                } else {
                    walk(children, depth, expanded, filter, rows);
                }
            }
            Node::Request {
                path,
                name,
                method,
                request_kind,
                ..
            } => {
                let matches = filter.is_empty()
                    || name.to_lowercase().contains(filter)
                    || path.to_lowercase().contains(filter);
                if matches {
                    let mark = text::kind_mark(*request_kind).map(str::to_owned);
                    rows.push(Row {
                        depth,
                        path: path.clone(),
                        name: name.clone(),
                        method: Some(mark.unwrap_or_else(|| method.clone())),
                        open: false,
                    });
                }
            }
        }
    }
}

/// The index after or before `at`, wrapping around `len`.
fn wrap(at: usize, len: usize, forward: bool) -> usize {
    if forward {
        (at + 1) % len
    } else {
        (at + len - 1) % len
    }
}

fn step<T: Copy + PartialEq>(all: &[T], current: T, forward: bool) -> T {
    let at = all.iter().position(|x| *x == current).unwrap_or(0);
    all[wrap(at, all.len(), forward)]
}

const METHODS: [&str; 7] = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

fn cycle_method(request: &mut Request, forward: bool) {
    let at = METHODS
        .iter()
        .position(|m| *m == request.method)
        .unwrap_or(0);
    request.method = METHODS[wrap(at, METHODS.len(), forward)].to_owned();
}

/// A part of the request as text for an editor, and the file extension
/// that gets it highlighted.
pub(crate) fn editable(request: &Request, pane: RequestPane) -> (String, &'static str) {
    let pairs = |rows: &[KeyValue], separator: &str| {
        rows.iter()
            .map(|row| {
                format!(
                    "{}{}{separator}{}",
                    if row.enabled { "" } else { "# " },
                    row.name,
                    row.value
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    match pane {
        RequestPane::Params => {
            let mut lines = vec![
                format!("# URL: {}", request.url),
                "# One parameter per line as name=value; # switches one off.".to_owned(),
            ];
            let (_, query) = request.url.split_once('?').unwrap_or((&request.url, ""));
            lines.extend(
                query
                    .split('&')
                    .filter(|p| !p.is_empty())
                    .map(str::to_owned),
            );
            lines.extend(
                request
                    .disabled_params
                    .iter()
                    .map(|p| format!("# {}={}", p.name, p.value)),
            );
            (lines.join("\n") + "\n", "txt")
        }
        RequestPane::Headers => (pairs(&request.headers, ": ") + "\n", "txt"),
        RequestPane::Body if request.kind == RequestKind::Grpc => {
            (request.grpc.message.clone(), "json")
        }
        RequestPane::Body => match &request.body {
            Body::Json { content } => (content.clone(), "json"),
            Body::Xml { content } => (content.clone(), "xml"),
            Body::Text { content, .. } => (content.clone(), "txt"),
            Body::Graphql { query, .. } => (query.clone(), "graphql"),
            Body::None => (String::new(), "json"),
            other => (serde_yaml_ng::to_string(other).unwrap_or_default(), "yaml"),
        },
        RequestPane::Auth => (
            serde_yaml_ng::to_string(&request.auth).unwrap_or_default(),
            "yaml",
        ),
        RequestPane::Checks => (request.checks.join("\n") + "\n", "txt"),
    }
}

/// Puts edited text back into the request.
pub(crate) fn apply(request: &mut Request, pane: RequestPane, text: &str) -> Result<(), String> {
    let lines = || {
        text.lines()
            .map(str::trim_end)
            .filter(|l| !l.trim().is_empty())
    };
    match pane {
        RequestPane::Params => {
            let base = request
                .url
                .split_once('?')
                .map_or(request.url.as_str(), |(b, _)| b)
                .to_owned();
            let mut enabled = Vec::new();
            request.disabled_params.clear();
            for line in lines() {
                if line.starts_with("# URL:") || line.starts_with("# One parameter") {
                    continue;
                }
                if let Some(off) = line.strip_prefix('#') {
                    let (name, value) = off.trim().split_once('=').unwrap_or((off.trim(), ""));
                    request.disabled_params.push(KeyValue {
                        enabled: false,
                        ..KeyValue::new(name, value)
                    });
                } else {
                    enabled.push(line.trim().to_owned());
                }
            }
            request.url = if enabled.is_empty() {
                base
            } else {
                format!("{base}?{}", enabled.join("&"))
            };
        }
        RequestPane::Headers => {
            request.headers = lines()
                .map(|line| {
                    let off = line.trim_start().starts_with('#');
                    let line = line.trim_start().trim_start_matches('#').trim();
                    let (name, value) = line.split_once(':').unwrap_or((line, ""));
                    KeyValue {
                        enabled: !off,
                        ..KeyValue::new(name.trim(), value.trim())
                    }
                })
                .collect();
        }
        RequestPane::Body if request.kind == RequestKind::Grpc => {
            request.grpc.message = text.to_owned();
        }
        RequestPane::Body => {
            request.body = match &request.body {
                Body::Json { .. } => Body::Json {
                    content: text.to_owned(),
                },
                Body::Xml { .. } => Body::Xml {
                    content: text.to_owned(),
                },
                Body::Text { content_type, .. } => Body::Text {
                    content: text.to_owned(),
                    content_type: content_type.clone(),
                },
                Body::Graphql {
                    variables,
                    operation_name,
                    ..
                } => Body::Graphql {
                    query: text.to_owned(),
                    variables: variables.clone(),
                    operation_name: operation_name.clone(),
                },
                Body::None if text.trim().is_empty() => Body::None,
                Body::None => {
                    if serde_json::from_str::<serde_json::Value>(text).is_ok() {
                        Body::Json {
                            content: text.to_owned(),
                        }
                    } else {
                        Body::Text {
                            content: text.to_owned(),
                            content_type: None,
                        }
                    }
                }
                _ => serde_yaml_ng::from_str(text)
                    .map_err(|e| format!("the body is not valid: {e}"))?,
            };
        }
        RequestPane::Auth => {
            request.auth =
                serde_yaml_ng::from_str(text).map_err(|e| format!("the auth is not valid: {e}"))?;
        }
        RequestPane::Checks => request.checks = lines().map(|l| l.trim().to_owned()).collect(),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_round_trip() {
        let mut request = Request {
            url: "https://x.test/a?b=1&c=2".into(),
            headers: vec![
                KeyValue::new("Accept", "application/json"),
                KeyValue {
                    enabled: false,
                    ..KeyValue::new("X-Off", "1")
                },
            ],
            checks: vec!["status == 200".into()],
            ..Request::default()
        };
        for pane in RequestPane::ALL {
            let (text, _) = editable(&request, pane);
            let before = request.clone();
            apply(&mut request, pane, &text).unwrap();
            assert_eq!(request, before, "{pane:?}");
        }
    }

    #[test]
    fn realtime_requests_show_their_kind() {
        let tree = [Node::Request {
            path: "chat.yaml".into(),
            name: "Chat".into(),
            method: "GET".into(),
            request_kind: getman_core::model::RequestKind::Websocket,
            error: None,
        }];
        let mut rows = Vec::new();
        walk(&tree, 0, &HashSet::new(), "", &mut rows);
        assert_eq!(rows[0].method.as_deref(), Some("WS"));
    }

    #[test]
    fn params_can_be_switched_off() {
        let mut request = Request {
            url: "https://x.test/a?b=1".into(),
            ..Request::default()
        };
        apply(&mut request, RequestPane::Params, "b=1\n# c=2\nd=3\n").unwrap();
        assert_eq!(request.url, "https://x.test/a?b=1&d=3");
        assert_eq!(request.disabled_params[0].name, "c");
    }
}
