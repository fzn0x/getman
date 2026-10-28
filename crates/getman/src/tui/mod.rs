//! The terminal UI: the same workspace, keyboard first, over SSH too.

mod app;
mod oauth;
mod stream;
#[cfg(test)]
mod tests;
mod text;
mod view;

use std::process::Command;
use std::time::Duration;

use crossterm::event::{Event, EventStream, KeyEventKind};
use futures_util::StreamExt as _;
use getman_core::Workspace;
use tokio::sync::mpsc;

use app::{App, Edit};

/// Runs the terminal UI until the user quits.
pub(crate) async fn run(workspace: Workspace) -> Result<(), String> {
    let mut app = App::new(workspace)?;
    let mut terminal = ratatui::init();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut events = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_millis(200));

    let result = loop {
        if let Err(e) = terminal.draw(|frame| view::draw(frame, &mut app)) {
            break Err(e.to_string());
        }
        tokio::select! {
            event = events.next() => match event {
                Some(Ok(Event::Key(key))) if key.kind != KeyEventKind::Release => app.on_key(key, &tx),
                Some(Ok(_)) => {}
                Some(Err(e)) => break Err(e.to_string()),
                None => break Ok(()),
            },
            Some(message) = rx.recv() => {
                app.on_message(message);
                // A busy connection sends faster than the screen needs
                // drawing: take what is waiting before the next draw.
                for _ in 0..500 {
                    let Ok(message) = rx.try_recv() else { break };
                    app.on_message(message);
                }
            }
            _ = tick.tick() => app.spinner = app.spinner.wrapping_add(1),
        }
        if let Some(edit) = app.edit.take() {
            // Hand the terminal to the editor, then take it back.
            ratatui::restore();
            let result = edit_externally(&edit);
            terminal = ratatui::init();
            let _ = terminal.clear();
            app.apply_edit(&edit, result);
            events = EventStream::new();
        }
        if app.quit {
            break Ok(());
        }
    };
    ratatui::restore();
    result
}

/// Opens text in `$VISUAL`, `$EDITOR`, or the platform's default editor,
/// and returns what was saved.
fn edit_externally(edit: &Edit) -> Result<String, String> {
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| {
            if cfg!(windows) {
                "notepad".to_owned()
            } else {
                "vi".to_owned()
            }
        });
    let mut parts = editor.split_whitespace();
    let program = parts.next().ok_or("no editor is set; set $EDITOR")?;
    let file = std::env::temp_dir().join(format!(
        "getman-{}-{}.{}",
        std::process::id(),
        edit.pane.label().to_lowercase(),
        edit.extension
    ));
    std::fs::write(&file, &edit.text)
        .map_err(|e| format!("cannot write {}: {e}", file.display()))?;
    let status = Command::new(program)
        .args(parts)
        .arg(&file)
        .status()
        .map_err(|e| format!("cannot start {program}: {e}; set $EDITOR to your editor"))?;
    let text = std::fs::read_to_string(&file).map_err(|e| e.to_string());
    let _ = std::fs::remove_file(&file);
    if !status.success() {
        return Err(format!("{program} exited with {status}; nothing changed"));
    }
    text
}
