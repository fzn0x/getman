//! Drawing the terminal UI.

use getman_core::execute::Execution;
use getman_core::grpc::CallType;
use getman_core::http::Header;
use getman_core::model::{Auth, Body, RequestKind};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs, Wrap,
};

use super::app::{App, Focus, Mode, RequestPane, ResponsePane};
use super::oauth;
use super::stream::{self, Phase, Stream, clean};
use super::text::{kind_mark, method_color, size, status_color};

const ACCENT: Color = Color::Red;
const SPINNER: [&str; 4] = ["·  ", "·· ", "···", " ··"];

fn panel(title: &str, focused: bool) -> Block<'static> {
    Block::new()
        .borders(Borders::ALL)
        .border_type(if focused {
            BorderType::Thick
        } else {
            BorderType::Plain
        })
        .border_style(if focused {
            Style::new().fg(ACCENT)
        } else {
            Style::new().fg(Color::DarkGray)
        })
        .title(Span::styled(
            format!(" {title} "),
            if focused {
                Style::new().add_modifier(Modifier::BOLD)
            } else {
                Style::new()
            },
        ))
}

pub(crate) fn draw(frame: &mut Frame<'_>, app: &mut App) {
    let [top, body, foot] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(5),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    draw_top(frame, app, top);
    let sidebar = if body.width < 70 { 24 } else { 34 };
    let [left, right] =
        Layout::horizontal([Constraint::Length(sidebar), Constraint::Min(20)]).areas(body);
    draw_tree(frame, app, left);
    draw_main(frame, app, right);
    draw_foot(frame, app, foot);
    if app.help {
        draw_help(frame);
    }
    if app.mode == Mode::Pick {
        draw_pick(frame, app);
    }
    if app.run.is_some() {
        draw_run(frame, app);
    }
}

fn draw_top(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let dirty = app.open.as_ref().is_some_and(super::app::Open::dirty);
    let left = Line::from(vec![
        Span::styled(
            " GETMAN ",
            Style::new()
                .bg(ACCENT)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(app.name.clone(), Style::new().add_modifier(Modifier::BOLD)),
        if dirty {
            Span::styled("  changed", Style::new().fg(ACCENT))
        } else {
            Span::raw("")
        },
    ]);
    frame.render_widget(Paragraph::new(left), area);
    let right = Line::from(vec![
        Span::styled("env ", Style::new().fg(Color::DarkGray)),
        Span::styled(
            app.environment_name(),
            Style::new().add_modifier(Modifier::BOLD),
        ),
        Span::styled("  (e) ", Style::new().fg(Color::DarkGray)),
    ])
    .right_aligned();
    frame.render_widget(Paragraph::new(right), area);
}

fn draw_tree(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let focused = app.focus == Focus::Tree;
    let title = if app.mode == Mode::Filter || !app.filter.is_empty() {
        format!("Requests /{}", app.filter)
    } else {
        "Requests".to_owned()
    };
    let current = app.open.as_ref().map(|o| o.path.as_str());
    let items: Vec<ListItem<'_>> = app
        .rows
        .iter()
        .map(|row| {
            let indent = "  ".repeat(row.depth);
            let line = match &row.method {
                Some(method) => {
                    let short = match method.as_str() {
                        "DELETE" => "DEL",
                        "OPTIONS" => "OPT",
                        "PATCH" => "PAT",
                        other => other,
                    };
                    let mut name = Span::raw(row.name.clone());
                    if current == Some(row.path.as_str()) {
                        name = name.add_modifier(Modifier::BOLD).underlined();
                    }
                    Line::from(vec![
                        Span::raw(format!("{indent}  ")),
                        Span::styled(
                            format!("{short:<4} "),
                            Style::new().fg(method_color(method)),
                        ),
                        name,
                    ])
                }
                None => Line::from(vec![
                    Span::raw(indent),
                    Span::styled(
                        if row.open { "▾ " } else { "▸ " },
                        Style::new().fg(Color::DarkGray),
                    ),
                    Span::styled(row.name.clone(), Style::new().add_modifier(Modifier::BOLD)),
                ]),
            };
            ListItem::new(line)
        })
        .collect();
    let list = List::new(items)
        .block(panel(&title, focused))
        .highlight_style(if focused {
            Style::new().bg(Color::White).fg(Color::Black)
        } else {
            Style::new().add_modifier(Modifier::REVERSED)
        });
    let mut state =
        ListState::default().with_selected((!app.rows.is_empty()).then_some(app.selected));
    frame.render_stateful_widget(list, area, &mut state);
    if app.rows.is_empty() {
        let inner = area.inner(ratatui::layout::Margin::new(2, 1));
        let note = if app.filter.is_empty() {
            "No requests yet. Press a to add one."
        } else {
            "Nothing matches."
        };
        frame.render_widget(
            Paragraph::new(note)
                .fg(Color::DarkGray)
                .wrap(Wrap { trim: true }),
            inner,
        );
    }
}

fn draw_main(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    let Some(open) = &app.open else {
        let text = Text::from(vec![
            Line::from(Span::styled(
                "Pick a request in the tree and press Enter.",
                Style::new().add_modifier(Modifier::BOLD),
            )),
            Line::raw(""),
            Line::raw("j k     move           Enter  open"),
            Line::raw("/       filter         a      add a request"),
            Line::raw("e       environment    r      run the folder"),
            Line::raw("?       every key      q      quit"),
        ]);
        frame.render_widget(Paragraph::new(text).block(panel("Getman", false)), area);
        return;
    };
    let [line, editor, response] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Percentage(38),
        Constraint::Min(6),
    ])
    .areas(area);

    // The request line.
    let request = &open.request;
    let url_focused = app.mode == Mode::Url;
    let title = format!("{}{}", request.name, if open.dirty() { " *" } else { "" });
    // The method for HTTP; the kind for the rest, and for gRPC also the
    // method it calls.
    let (head, head_color) = match (kind_mark(request.kind), request.kind) {
        (None, _) => (
            format!("{} ", request.method),
            method_color(&request.method),
        ),
        (Some(mark), RequestKind::Grpc) => (format!("{mark} {} ", request.method), Color::Magenta),
        (Some(mark), _) => (format!("{mark} "), Color::Magenta),
    };
    let url = Line::from(vec![
        Span::styled(
            head.clone(),
            Style::new().fg(head_color).add_modifier(Modifier::BOLD),
        ),
        Span::raw(request.url.clone()),
    ]);
    let hint = if url_focused {
        if request.kind.is_http() {
            " Enter sends · Esc stops editing "
        } else {
            " Enter starts it · Esc stops editing "
        }
    } else {
        match request.kind {
            RequestKind::Http => " i edits · m method · s sends · w saves ",
            RequestKind::Sse => " i edits · s connects · x stops · w saves ",
            RequestKind::Websocket | RequestKind::Socketio => {
                " s connects · m saved message · t type one · x stops · w saves "
            }
            RequestKind::Grpc => " s calls · m saved message · t type · f ends · x cancels ",
        }
    };
    let block = panel(&title, app.focus == Focus::Request && !url_focused)
        .title_bottom(Line::from(hint).fg(Color::DarkGray).right_aligned());
    let block = if url_focused {
        block.border_style(Style::new().fg(Color::Yellow))
    } else {
        block
    };
    frame.render_widget(Paragraph::new(url).block(block), line);
    if url_focused {
        let prefix = head.chars().count();
        let x = line.x + 1 + u16::try_from(prefix + open.cursor).unwrap_or(u16::MAX);
        frame.set_cursor_position((x.min(line.right().saturating_sub(2)), line.y + 1));
    }

    // The request editor.
    let [tabs_area, pane_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(editor);
    let titles: Vec<String> = RequestPane::ALL
        .iter()
        .enumerate()
        .map(|(i, p)| format!("{} {}", i + 1, p.label()))
        .collect();
    let selected = RequestPane::ALL
        .iter()
        .position(|p| *p == app.request_pane)
        .unwrap_or(0);
    frame.render_widget(
        Tabs::new(titles)
            .select(selected)
            .highlight_style(Style::new().fg(ACCENT).add_modifier(Modifier::BOLD))
            .divider(" "),
        tabs_area,
    );
    let pane_text = request_pane(app);
    let pane_title = if app.request_pane == RequestPane::Auth
        && app.oauth.as_ref().is_some_and(|oauth| oauth.browser)
    {
        "E or o edits in your editor · b signs in with the browser"
    } else {
        "E or o edits in your editor"
    };
    frame.render_widget(
        Paragraph::new(pane_text)
            .block(panel(pane_title, app.focus == Focus::Request))
            .wrap(Wrap { trim: false }),
        pane_area,
    );

    draw_response(frame, app, response);
}

fn request_pane(app: &App) -> Text<'static> {
    let Some(open) = &app.open else {
        return Text::raw("");
    };
    let request = &open.request;
    let dim = Style::new().fg(Color::DarkGray);
    let kv = |name: &str, value: &str, on: bool| {
        Line::from(vec![
            Span::styled(
                format!("{name}: "),
                if on {
                    Style::new().add_modifier(Modifier::BOLD)
                } else {
                    dim
                },
            ),
            Span::styled(value.to_owned(), if on { Style::new() } else { dim }),
        ])
    };
    let lines: Vec<Line<'static>> = match app.request_pane {
        RequestPane::Params => {
            let (_, query) = request.url.split_once('?').unwrap_or((&request.url, ""));
            let mut lines: Vec<Line<'static>> = query
                .split('&')
                .filter(|p| !p.is_empty())
                .map(|pair| {
                    let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
                    kv(name, value, true)
                })
                .collect();
            lines.extend(
                request
                    .disabled_params
                    .iter()
                    .map(|p| kv(&p.name, &p.value, false)),
            );
            lines.extend(
                request
                    .path_params
                    .iter()
                    .map(|(name, value)| kv(&format!(":{name}"), value, true)),
            );
            if lines.is_empty() {
                lines.push(Line::styled("No parameters.", dim));
            }
            lines
        }
        RequestPane::Headers => {
            let lines: Vec<Line<'static>> = request
                .headers
                .iter()
                .map(|h| kv(&h.name, &h.value, h.enabled))
                .collect();
            if lines.is_empty() {
                vec![Line::styled(
                    "No headers of its own; folders may add some.",
                    dim,
                )]
            } else {
                lines
            }
        }
        RequestPane::Body if request.kind == RequestKind::Grpc => {
            let (service, method) = request
                .method
                .trim()
                .trim_start_matches('/')
                .split_once('/')
                .unwrap_or(("", request.method.trim()));
            let schema = if request.grpc.proto.trim().is_empty() {
                "server reflection".to_owned()
            } else {
                request.grpc.proto.clone()
            };
            let mut lines = vec![
                kv("Service", service, !service.is_empty()),
                kv("Method", method, !method.is_empty()),
                kv("Schema", &schema, true),
                Line::raw(""),
            ];
            if request.grpc.message.trim().is_empty() {
                lines.push(Line::styled(
                    "No message. The method gets an empty one.",
                    dim,
                ));
            } else {
                lines.extend(
                    request
                        .grpc
                        .message
                        .lines()
                        .map(|l| Line::raw(l.to_owned())),
                );
            }
            lines
        }
        RequestPane::Body => match &request.body {
            Body::None => vec![Line::styled("No body.", dim)],
            Body::Json { content } | Body::Xml { content } | Body::Text { content, .. } => {
                content.lines().map(|l| Line::raw(l.to_owned())).collect()
            }
            Body::Graphql { query, .. } => query.lines().map(|l| Line::raw(l.to_owned())).collect(),
            other => serde_yaml_ng::to_string(other)
                .unwrap_or_default()
                .lines()
                .map(|l| Line::raw(l.to_owned()))
                .collect(),
        },
        RequestPane::Auth => {
            let mut lines = sign_in_lines(app);
            match &request.auth {
                Auth::Inherit => {
                    lines.push(Line::styled("Inherits the folder's authentication.", dim));
                }
                Auth::None => lines.push(Line::raw("No authentication.")),
                other => lines.extend(
                    serde_yaml_ng::to_string(other)
                        .unwrap_or_default()
                        .lines()
                        .map(|l| Line::raw(l.to_owned())),
                ),
            }
            lines
        }
        RequestPane::Checks => {
            let results = app
                .execution
                .as_ref()
                .map(|e| e.checks.clone())
                .unwrap_or_default();
            let lines: Vec<Line<'static>> = request
                .checks
                .iter()
                .map(|check| {
                    let mark = match results.iter().find(|r| r.name == *check) {
                        Some(r) if r.passed => Span::styled("✓ ", Style::new().fg(Color::Green)),
                        Some(_) => Span::styled("✗ ", Style::new().fg(Color::Red)),
                        None => Span::raw("  "),
                    };
                    Line::from(vec![mark, Span::raw(check.clone())])
                })
                .collect();
            if lines.is_empty() {
                vec![Line::styled(
                    "No checks. Add lines like: status == 200",
                    dim,
                )]
            } else {
                lines
            }
        }
    };
    Text::from(lines)
}

/// What the auth pane says about signing in, above the settings, when
/// the request uses OAuth 2.0.
fn sign_in_lines(app: &App) -> Vec<Line<'static>> {
    let Some(oauth) = &app.oauth else {
        return Vec::new();
    };
    let dim = Style::new().fg(Color::DarkGray);
    let mut lines = Vec::new();
    if let Some(sign_in) = &app.sign_in {
        lines.push(Line::styled(
            "Waiting for you to sign in in the browser · Esc cancels",
            Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ));
        if let Some(authorization) = &sign_in.authorization {
            lines.push(Line::styled(
                if sign_in.opened {
                    "The browser was opened at:"
                } else {
                    "The browser did not open. Open this address:"
                },
                dim,
            ));
            lines.push(Line::raw(authorization.url.clone()));
            lines.push(Line::styled(
                format!(
                    "Getman listens for the provider at {}",
                    authorization.redirect_uri
                ),
                dim,
            ));
            for note in &authorization.notes {
                lines.push(Line::styled(note.clone(), Style::new().fg(Color::Yellow)));
            }
        }
    } else if let Some(token) = &oauth.token {
        lines.push(Line::styled(
            oauth::describe(token, getman_core::time::now_millis()),
            Style::new().fg(Color::Green),
        ));
        if oauth.browser {
            lines.push(Line::styled("b signs in again.", dim));
        }
    } else if oauth.browser {
        lines.push(Line::styled(
            "Not signed in. b opens your browser to sign in.",
            Style::new().add_modifier(Modifier::BOLD),
        ));
    } else {
        lines.push(Line::styled(
            "No token yet. It is fetched when a request needs one.",
            dim,
        ));
    }
    lines.push(Line::raw(""));
    lines
}

fn response_block(title: Line<'static>, focused: bool) -> Block<'static> {
    Block::new()
        .borders(Borders::ALL)
        .border_type(if focused {
            BorderType::Thick
        } else {
            BorderType::Plain
        })
        .border_style(if focused {
            Style::new().fg(ACCENT)
        } else {
            Style::new().fg(Color::DarkGray)
        })
        .title(title)
}

/// The tabs of the response; `first` names the first, and `failed` counts
/// the tests that failed.
fn draw_response_tabs(
    frame: &mut Frame<'_>,
    pane: ResponsePane,
    first: &str,
    failed: usize,
    area: Rect,
) {
    let titles: Vec<Line<'static>> = ResponsePane::ALL
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let name = if *p == ResponsePane::Body {
                first
            } else {
                p.label()
            };
            let label = format!("{} {name}", i + 1);
            if *p == ResponsePane::Tests && failed > 0 {
                Line::from(vec![
                    Span::raw(label),
                    Span::styled(format!(" {failed}✗"), Style::new().fg(Color::Red)),
                ])
            } else {
                Line::raw(label)
            }
        })
        .collect();
    let selected = ResponsePane::ALL
        .iter()
        .position(|p| *p == pane)
        .unwrap_or(0);
    frame.render_widget(
        Tabs::new(titles)
            .select(selected)
            .highlight_style(Style::new().fg(ACCENT).add_modifier(Modifier::BOLD))
            .divider(" "),
        area,
    );
}

fn header_lines(headers: &[Header]) -> Vec<Line<'static>> {
    headers
        .iter()
        .map(|header| {
            Line::from(vec![
                Span::styled(
                    format!("{}: ", clean(&header.name)),
                    Style::new().add_modifier(Modifier::BOLD),
                ),
                Span::raw(clean(&header.value)),
            ])
        })
        .collect()
}

fn test_lines(execution: Option<&Execution>) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    if let Some(execution) = execution {
        for error in &execution.script_errors {
            lines.push(Line::styled(
                format!("✗ script in {}: {}", error.origin, error.message),
                Style::new().fg(Color::Red),
            ));
        }
        for result in execution.checks.iter().chain(&execution.tests) {
            if result.passed {
                lines.push(Line::from(vec![
                    Span::styled("✓ ", Style::new().fg(Color::Green)),
                    Span::raw(result.name.clone()),
                ]));
            } else {
                lines.push(Line::from(vec![
                    Span::styled("✗ ", Style::new().fg(Color::Red)),
                    Span::raw(result.name.clone()),
                    Span::styled(
                        format!("  {}", result.message.clone().unwrap_or_default()),
                        Style::new().fg(Color::Red),
                    ),
                ]));
            }
        }
    }
    if lines.is_empty() {
        lines.push(Line::styled(
            "No tests or checks.",
            Style::new().fg(Color::DarkGray),
        ));
    }
    lines
}

fn console_lines(execution: Option<&Execution>) -> Vec<Line<'static>> {
    execution
        .map(|execution| {
            execution
                .logs
                .iter()
                .map(|log| {
                    Line::from(vec![
                        Span::styled(
                            format!("[{}] ", log.level),
                            Style::new().fg(Color::DarkGray),
                        ),
                        Span::raw(log.message.clone()),
                    ])
                })
                .collect()
        })
        .unwrap_or_default()
}

fn failed_count(execution: Option<&Execution>) -> usize {
    execution.map_or(0, |execution| {
        execution
            .tests
            .iter()
            .chain(&execution.checks)
            .filter(|t| !t.passed)
            .count()
    })
}

fn draw_response(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    if app.stream.is_some() {
        draw_stream(frame, app, area);
        return;
    }
    let focused = app.focus == Focus::Response;
    let title: Line<'static> = if app.sending {
        Line::from(format!(
            " Sending {} ",
            SPINNER[app.spinner % SPINNER.len()]
        ))
    } else if let Some(execution) = &app.execution {
        match &execution.response {
            Some(response) => Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    format!("{} {}", response.status, response.status_text),
                    Style::new()
                        .fg(status_color(response.status))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!(
                    " · {:.0} ms · {} ",
                    response.time_ms,
                    size(response.size.body)
                )),
            ]),
            None => Line::from(Span::styled(
                " No response ",
                Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
            )),
        }
    } else {
        Line::from(" Response ")
    };
    let block = response_block(title, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(execution) = &app.execution else {
        let hint = match &app.open {
            _ if app.sending => "",
            Some(open) if open.request.kind == RequestKind::Grpc => "Press s to call the method.",
            Some(open) if !open.request.kind.is_http() => "Press s to connect.",
            _ => "Press s to send.",
        };
        frame.render_widget(Paragraph::new(hint).fg(Color::DarkGray), inner);
        return;
    };
    let [tabs_area, content] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);
    draw_response_tabs(
        frame,
        app.response_pane,
        ResponsePane::Body.label(),
        failed_count(Some(execution)),
        tabs_area,
    );
    let mut lines: Vec<Line<'static>> = Vec::new();
    if let Some(error) = &execution.error {
        lines.push(Line::styled(
            error.clone(),
            Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
        lines.push(Line::raw(""));
    }
    match app.response_pane {
        ResponsePane::Body => lines.extend(app.response_lines.iter().cloned()),
        ResponsePane::Headers => {
            if let Some(response) = &execution.response {
                lines.extend(header_lines(&response.headers));
            }
        }
        ResponsePane::Tests => lines.extend(test_lines(Some(execution))),
        ResponsePane::Console => lines.extend(console_lines(Some(execution))),
    }
    let max = u16::try_from(lines.len())
        .unwrap_or(u16::MAX)
        .saturating_sub(content.height);
    app.response_scroll = app.response_scroll.min(max);
    frame.render_widget(
        Paragraph::new(lines).scroll((app.response_scroll, 0)),
        content,
    );
}

/// The title of a connection or call: how far it is, then how it ended.
fn stream_title(stream: &Stream, spinner: usize) -> Line<'static> {
    let counts = format!("↑{} ↓{}", stream.sent, stream.received);
    let spin = SPINNER[spinner % SPINNER.len()];
    let grpc = stream.kind == RequestKind::Grpc;
    let bold = |text: String, color: Color| {
        Span::styled(text, Style::new().fg(color).add_modifier(Modifier::BOLD))
    };
    if stream.stopping && stream.is_live() {
        return Line::from(vec![
            Span::raw(" "),
            bold(format!("Stopping {spin}"), ACCENT),
        ]);
    }
    match (stream.phase, &stream.result) {
        (Phase::Connecting, _) => Line::from(vec![
            Span::raw(" "),
            bold(
                format!("{} {spin} ", if grpc { "Calling" } else { "Connecting" }),
                Color::Yellow,
            ),
        ]),
        (Phase::Open, _) => {
            let what = match (grpc, stream.call_type) {
                (true, Some(CallType::Unary) | None) => format!("Calling {spin}"),
                (true, Some(_)) => format!("Streaming {spin}"),
                (false, _) => "Connected".to_owned(),
            };
            let mut spans = vec![Span::raw(" "), bold(what, Color::Green)];
            if let Some(connected) = &stream.connected {
                spans.push(Span::raw(format!(" · {connected}")));
            }
            spans.push(Span::raw(format!(" · {counts} ")));
            Line::from(spans)
        }
        (Phase::Ended, result) => {
            let (text, ok) = result
                .clone()
                .unwrap_or_else(|| ("Closed".to_owned(), true));
            Line::from(vec![
                Span::raw(" "),
                bold(text, if ok { Color::Green } else { Color::Red }),
                Span::raw(format!(" · {counts} ")),
            ])
        }
    }
}

/// The response of a WebSocket, event stream, Socket.IO or gRPC request:
/// the log of its messages, headers, and for gRPC the results of checks.
fn draw_stream(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Response;
    let Some(stream) = app.stream.as_mut() else {
        return;
    };
    let block = response_block(stream_title(stream, app.spinner), focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [tabs_area, content] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);
    draw_response_tabs(
        frame,
        app.response_pane,
        "Messages",
        failed_count(app.execution.as_deref()),
        tabs_area,
    );
    let dim = Style::new().fg(Color::DarkGray);
    let height = usize::from(content.height);
    let lines: Vec<Line<'static>> = match app.response_pane {
        ResponsePane::Body => {
            let max = stream.total_lines().saturating_sub(height);
            let mut scroll = usize::from(app.response_scroll).min(max);
            if stream.follow {
                scroll = max;
            } else if scroll == max {
                stream.follow = true;
            }
            app.response_scroll = u16::try_from(scroll).unwrap_or(u16::MAX);
            stream.window(scroll, height)
        }
        ResponsePane::Headers => {
            let mut lines = Vec::new();
            if !stream.trailers.is_empty() {
                lines.push(Line::styled(
                    "Headers",
                    Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
                ));
            }
            lines.extend(header_lines(&stream.headers));
            if !stream.trailers.is_empty() {
                lines.push(Line::raw(""));
                lines.push(Line::styled(
                    "Trailers",
                    Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
                ));
                lines.extend(header_lines(&stream.trailers));
            }
            if lines.is_empty() {
                lines.push(Line::styled("No headers yet.", dim));
            }
            lines
        }
        ResponsePane::Tests => test_lines(app.execution.as_deref()),
        ResponsePane::Console => console_lines(app.execution.as_deref()),
    };
    if app.response_pane == ResponsePane::Body {
        frame.render_widget(Paragraph::new(lines), content);
    } else {
        let max = u16::try_from(lines.len())
            .unwrap_or(u16::MAX)
            .saturating_sub(content.height);
        app.response_scroll = app.response_scroll.min(max);
        frame.render_widget(
            Paragraph::new(lines).scroll((app.response_scroll, 0)),
            content,
        );
    }
}

fn draw_foot(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let line = match (&app.mode, &app.message) {
        (Mode::Name { folder }, _) => Line::from(vec![
            Span::styled(" New request in ", Style::new().fg(Color::DarkGray)),
            Span::raw(if folder.is_empty() {
                "the workspace".to_owned()
            } else {
                folder.clone()
            }),
            Span::styled(": ", Style::new().fg(Color::DarkGray)),
            Span::styled(
                format!("{}▏", app.name_input),
                Style::new().add_modifier(Modifier::BOLD),
            ),
        ]),
        (Mode::Compose, _) => {
            let what = match app.open.as_ref().map(|o| o.request.kind) {
                Some(RequestKind::Socketio) => "Event and arguments",
                Some(RequestKind::Grpc) => "Message (JSON)",
                _ => "Message",
            };
            Line::from(vec![
                Span::styled(format!(" {what}: "), Style::new().fg(Color::DarkGray)),
                Span::styled(
                    format!("{}▏", app.compose),
                    Style::new().add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "  Enter sends · Esc cancels",
                    Style::new().fg(Color::DarkGray),
                ),
            ])
        }
        (Mode::Pick, _) => Line::styled(
            " j k choose · Enter or 1-9 sends · Esc closes",
            Style::new().fg(Color::DarkGray),
        ),
        (Mode::Delete { name, .. }, _) => Line::from(vec![
            Span::styled(" Move ", Style::new().fg(Color::DarkGray)),
            Span::styled(name.clone(), Style::new().add_modifier(Modifier::BOLD)),
            Span::styled(
                " to the trash? y moves it · any other key keeps it",
                Style::new().fg(Color::DarkGray),
            ),
        ]),
        (_, Some((text, error, at))) if at.elapsed().as_secs() < 5 => Line::styled(
            format!(" {text}"),
            if *error {
                Style::new().fg(Color::Red)
            } else {
                Style::new().fg(Color::Green)
            },
        ),
        (Mode::Filter, _) => Line::styled(
            " Type to filter · Enter opens · Esc clears",
            Style::new().fg(Color::DarkGray),
        ),
        _ => {
            let running = app.sending
                || app.sign_in.is_some()
                || app.stream.as_ref().is_some_and(Stream::is_live);
            let keys = if running {
                " Esc or x stops it · Tab focus · ? keys · q quit"
            } else if app.stream.is_some() {
                " m saved message · t type one · s again · Tab focus · ? keys · q quit"
            } else {
                " Tab focus · s send · w save · e env · / filter · r run · ? keys · q quit"
            };
            Line::styled(keys, Style::new().fg(Color::DarkGray))
        }
    };
    frame.render_widget(Paragraph::new(line), area);
}

fn centered(frame: &Frame<'_>, width: u16, height: u16) -> Rect {
    let area = frame.area();
    let width = width.min(area.width.saturating_sub(2));
    let height = height.min(area.height.saturating_sub(2));
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

fn draw_help(frame: &mut Frame<'_>) {
    let rows = [
        ("Anywhere", ""),
        (
            "Tab / Shift Tab",
            "move between the tree, request and response",
        ),
        ("s", "send the open request, or connect or call"),
        ("Esc or x", "stop what is running"),
        ("w", "save it"),
        ("e / E", "next or previous environment"),
        ("/", "filter requests"),
        ("r", "run the selected folder, or everything"),
        ("R", "reload the tree from disk"),
        ("q", "quit"),
        ("", ""),
        ("Tree", ""),
        ("j k / arrows", "move"),
        ("Enter l / h", "open or expand / collapse"),
        ("a", "add a request in the folder"),
        ("d or Del", "move to the trash (asks first)"),
        ("u", "put back what was moved to the trash last"),
        ("", ""),
        ("Request", ""),
        ("i or Enter", "edit the URL (Enter sends)"),
        ("m / M", "change the method"),
        ("1-5, [ ]", "params, headers, body, auth, checks"),
        ("E or o", "edit that part in $EDITOR"),
        ("b", "sign in with the browser (OAuth 2.0, on Auth)"),
        ("", ""),
        ("Response", ""),
        ("j k, PgUp PgDn, g G", "scroll"),
        ("1-4, [ ]", "body, headers, tests, console"),
        ("", ""),
        ("WebSocket, SSE, Socket.IO, gRPC", ""),
        ("m", "send a saved message (gRPC: the saved message)"),
        ("t", "type a message (Socket.IO: event, then JSON)"),
        ("f", "end a gRPC client stream"),
        ("x", "disconnect or cancel the call"),
    ];
    let lines: Vec<Line<'static>> = rows
        .iter()
        .map(|(key, what)| {
            if what.is_empty() {
                Line::styled(
                    (*key).to_owned(),
                    Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
                )
            } else {
                Line::from(vec![
                    Span::styled(
                        format!("{key:<22}"),
                        Style::new().add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(*what),
                ])
            }
        })
        .collect();
    let area = centered(frame, 72, u16::try_from(lines.len()).unwrap_or(30) + 2);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).block(panel("Keys · any key closes", true)),
        area,
    );
}

fn draw_pick(frame: &mut Frame<'_>, app: &App) {
    let Some(open) = &app.open else { return };
    let saved = stream::saved_messages(&open.request);
    let items: Vec<ListItem<'_>> = saved
        .iter()
        .enumerate()
        .map(|(i, message)| {
            let mut spans = vec![Span::styled(
                format!("{} ", i + 1),
                Style::new().fg(Color::DarkGray),
            )];
            spans.push(Span::styled(
                clean(&message.label),
                Style::new().add_modifier(Modifier::BOLD),
            ));
            if message.preview != message.label {
                spans.push(Span::styled(
                    format!("  {}", clean(&message.preview)),
                    Style::new().fg(Color::DarkGray),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();
    let height = u16::try_from(items.len())
        .unwrap_or(u16::MAX)
        .saturating_add(2);
    let area = centered(frame, 72, height);
    frame.render_widget(Clear, area);
    let list = List::new(items)
        .block(panel("Send a saved message · Esc closes", true))
        .highlight_style(Style::new().bg(Color::White).fg(Color::Black));
    let mut state = ListState::default().with_selected(Some(app.pick));
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_run(frame: &mut Frame<'_>, app: &App) {
    let Some(view) = &app.run else { return };
    let area = centered(frame, 90, frame.area().height.saturating_sub(4));
    frame.render_widget(Clear, area);
    let mut lines: Vec<Line<'static>> = view
        .lines
        .iter()
        .map(|(ok, text)| {
            if text.starts_with("    ") {
                Line::styled(text.clone(), Style::new().fg(Color::Red))
            } else {
                Line::from(vec![
                    if *ok {
                        Span::styled("✓ ", Style::new().fg(Color::Green))
                    } else {
                        Span::styled("✗ ", Style::new().fg(Color::Red))
                    },
                    Span::raw(text.clone()),
                ])
            }
        })
        .collect();
    lines.push(Line::raw(""));
    lines.push(match &view.summary {
        Some(summary) => Line::styled(summary.clone(), Style::new().add_modifier(Modifier::BOLD)),
        None => Line::styled(
            format!("Running {}", SPINNER[app.spinner % SPINNER.len()]),
            Style::new().fg(Color::DarkGray),
        ),
    });
    frame.render_widget(
        Paragraph::new(lines)
            .block(panel(&format!("Run {} · Esc closes", view.title), true))
            .scroll((view.scroll, 0)),
        area,
    );
}

#[cfg(test)]
mod tests {
    use getman_core::Workspace;
    use getman_core::model::Request;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;

    fn screen(app: &mut App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| draw(frame, app)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol().to_owned())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn draws_the_tree_request_and_response() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = Workspace::init(dir.path(), "Shop").unwrap();
        let folder = workspace.create_folder("", "Orders").unwrap();
        let request = Request {
            method: "POST".into(),
            url: "{{base}}/orders".into(),
            checks: vec!["status == 201".into()],
            ..Request::default()
        };
        let path = workspace
            .create_request(&folder, "Create order", &request)
            .unwrap();
        let mut app = App::new(workspace).unwrap();
        let empty = screen(&mut app, 100, 30);
        assert!(empty.contains("GETMAN"), "{empty}");
        assert!(empty.contains("Orders"), "{empty}");
        assert!(empty.contains("Pick a request"), "{empty}");

        app.open_request(&path);
        app.request_pane = RequestPane::Checks;
        let open = screen(&mut app, 100, 30);
        assert!(open.contains("POST {{base}}/orders"), "{open}");
        assert!(open.contains("status == 201"), "{open}");
        assert!(open.contains("Press s to send"), "{open}");

        app.help = true;
        assert!(screen(&mut app, 100, 40).contains("edit that part in $EDITOR"));
    }

    #[test]
    fn draws_a_connection_log_and_the_saved_messages() {
        use getman_core::model::RequestKind;
        use getman_core::realtime::{
            Direction, EventKind, Message as Frame, SavedMessage, StreamEvent, WebSocketSettings,
        };

        use super::super::app::{Message, Mode};
        use super::super::stream::{Link, Stream};

        let dir = tempfile::tempdir().unwrap();
        let workspace = Workspace::init(dir.path(), "Chat").unwrap();
        let request = Request {
            kind: RequestKind::Websocket,
            url: "ws://chat.test/room".into(),
            websocket: WebSocketSettings {
                messages: vec![SavedMessage {
                    name: "Hello".into(),
                    content: "{\"type\": \"hello\"}".into(),
                    ..SavedMessage::default()
                }],
                ..WebSocketSettings::default()
            },
            ..Request::default()
        };
        let path = workspace.create_request("", "Room", &request).unwrap();
        let mut app = App::new(workspace).unwrap();
        app.open_request(&path);
        let idle = screen(&mut app, 110, 30);
        assert!(idle.contains("WS ws://chat.test/room"), "{idle}");
        assert!(idle.contains("Press s to connect."), "{idle}");

        // A stand-in link: the log is drawn from what the events report.
        let (commands, _commands) = tokio::sync::mpsc::unbounded_channel();
        app.stream = Some(Stream::new(1, RequestKind::Websocket, Link::Grpc(commands)));
        let frame = |at, kind| Message::Realtime(1, StreamEvent { at, kind });
        app.on_message(frame(
            1_000,
            EventKind::Open {
                status: 101,
                headers: Vec::new(),
                protocol: None,
                time_ms: 12.0,
            },
        ));
        app.on_message(frame(
            1_250,
            EventKind::Message(Frame {
                direction: Direction::Received,
                data: "{\"user\":\"ada\"}".into(),
                binary: false,
                size: 14,
                truncated: false,
                name: None,
                event: None,
                id: None,
                retry: None,
                ack: None,
            }),
        ));
        let log = screen(&mut app, 110, 30);
        assert!(log.contains("Connected · 101 · 12 ms"), "{log}");
        assert!(log.contains("+0.250s ← {"), "{log}");
        assert!(log.contains("\"user\": \"ada\""), "{log}");
        assert!(log.contains("1 Messages"), "{log}");

        app.mode = Mode::Pick;
        let pick = screen(&mut app, 110, 30);
        assert!(pick.contains("Send a saved message"), "{pick}");
        assert!(pick.contains("Hello"), "{pick}");

        app.mode = Mode::Normal;
        app.help = true;
        assert!(screen(&mut app, 110, 40).contains("sign in with the browser"));
    }

    #[test]
    fn the_tree_deletes_into_the_trash_and_puts_back() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let dir = tempfile::tempdir().unwrap();
        let workspace = Workspace::init(dir.path(), "Shop").unwrap();
        workspace
            .create_request("", "Ping", &Request::default())
            .unwrap();
        let mut app = App::new(workspace).unwrap();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let press = |app: &mut App, code: KeyCode| {
            app.on_key(KeyEvent::new(code, KeyModifiers::NONE), &tx);
        };

        press(&mut app, KeyCode::Char('d'));
        assert!(screen(&mut app, 100, 30).contains("Move Ping to the trash?"));
        press(&mut app, KeyCode::Char('n'));
        assert_eq!(app.rows.len(), 1);

        press(&mut app, KeyCode::Delete);
        press(&mut app, KeyCode::Char('y'));
        assert!(app.rows.is_empty());
        assert_eq!(app.executor.workspace.trash_list().unwrap().len(), 1);

        press(&mut app, KeyCode::Char('u'));
        assert_eq!(app.rows.len(), 1);
        assert!(app.executor.workspace.trash_list().unwrap().is_empty());
    }
}
