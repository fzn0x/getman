//! Drawing the terminal UI.

use getman_core::model::{Auth, Body};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs, Wrap,
};

use super::app::{App, Focus, Mode, RequestPane, ResponsePane};
use super::text::{method_color, size, status_color};

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
    let url = Line::from(vec![
        Span::styled(
            format!("{} ", request.method),
            Style::new()
                .fg(method_color(&request.method))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(request.url.clone()),
    ]);
    let block = panel(&title, app.focus == Focus::Request && !url_focused).title_bottom(
        Line::from(if url_focused {
            " Enter sends · Esc stops editing "
        } else {
            " i edits · m method · s sends · w saves "
        })
        .fg(Color::DarkGray)
        .right_aligned(),
    );
    let block = if url_focused {
        block.border_style(Style::new().fg(Color::Yellow))
    } else {
        block
    };
    frame.render_widget(Paragraph::new(url).block(block), line);
    if url_focused {
        let prefix = request.method.chars().count() + 1;
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
    frame.render_widget(
        Paragraph::new(pane_text)
            .block(panel(
                "E or o edits in your editor",
                app.focus == Focus::Request,
            ))
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
        RequestPane::Auth => match &request.auth {
            Auth::Inherit => vec![Line::styled("Inherits the folder's authentication.", dim)],
            Auth::None => vec![Line::raw("No authentication.")],
            other => serde_yaml_ng::to_string(other)
                .unwrap_or_default()
                .lines()
                .map(|l| Line::raw(l.to_owned()))
                .collect(),
        },
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

fn draw_response(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
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
    let block = Block::new()
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
        .title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(execution) = &app.execution else {
        let hint = if app.sending { "" } else { "Press s to send." };
        frame.render_widget(Paragraph::new(hint).fg(Color::DarkGray), inner);
        return;
    };
    let [tabs_area, content] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);
    let failed = execution
        .tests
        .iter()
        .chain(&execution.checks)
        .filter(|t| !t.passed)
        .count();
    let titles: Vec<Line<'static>> = ResponsePane::ALL
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let label = format!("{} {}", i + 1, p.label());
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
        .position(|p| *p == app.response_pane)
        .unwrap_or(0);
    frame.render_widget(
        Tabs::new(titles)
            .select(selected)
            .highlight_style(Style::new().fg(ACCENT).add_modifier(Modifier::BOLD))
            .divider(" "),
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
                for header in &response.headers {
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("{}: ", header.name),
                            Style::new().add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(header.value.clone()),
                    ]));
                }
            }
        }
        ResponsePane::Tests => {
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
            if lines.is_empty() {
                lines.push(Line::styled(
                    "No tests or checks.",
                    Style::new().fg(Color::DarkGray),
                ));
            }
        }
        ResponsePane::Console => {
            for log in &execution.logs {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("[{}] ", log.level),
                        Style::new().fg(Color::DarkGray),
                    ),
                    Span::raw(log.message.clone()),
                ]));
            }
        }
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
        _ => Line::styled(
            " Tab focus · s send · w save · e env · / filter · r run · ? keys · q quit",
            Style::new().fg(Color::DarkGray),
        ),
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
        ("s", "send the open request"),
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
        ("", ""),
        ("Request", ""),
        ("i or Enter", "edit the URL (Enter sends)"),
        ("m / M", "change the method"),
        ("1-5, [ ]", "params, headers, body, auth, checks"),
        ("E or o", "edit that part in $EDITOR"),
        ("", ""),
        ("Response", ""),
        ("j k, PgUp PgDn, g G", "scroll"),
        ("1-4, [ ]", "body, headers, tests, console"),
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
}
