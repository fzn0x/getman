//! What the UI may ask of the app. Only the UI served by the app's own
//! server can call these, as the capability added in `app.rs` allows.

// Tauri hands commands their arguments by value.
#![allow(clippy::needless_pass_by_value)]

use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, State, WebviewWindow};

use crate::app::Workspaces;
use crate::command_line::{self, CommandLine};
use crate::updates::{Status, Updates};

/// Everything Settings › About shows.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DesktopState {
    updates: Status,
    command_line: CommandLine,
}

#[tauri::command]
pub(crate) fn desktop_state(updates: State<'_, Arc<Updates>>) -> DesktopState {
    DesktopState {
        updates: updates.status(),
        command_line: CommandLine::current(),
    }
}

#[tauri::command]
pub(crate) fn set_update_preferences(
    app: AppHandle,
    updates: State<'_, Arc<Updates>>,
    check: bool,
    install: bool,
) -> Result<Status, String> {
    updates.set_preferences(&app, check, install)
}

#[tauri::command]
pub(crate) async fn check_for_updates(
    app: AppHandle,
    updates: State<'_, Arc<Updates>>,
) -> Result<Status, String> {
    updates.check_now(&app).await
}

#[tauri::command]
pub(crate) async fn install_update(
    app: AppHandle,
    updates: State<'_, Arc<Updates>>,
) -> Result<Status, String> {
    updates.install(&app).await
}

#[tauri::command]
pub(crate) fn restart(app: AppHandle, updates: State<'_, Arc<Updates>>) {
    updates.restart(&app);
}

/// Asks for a folder with the system's folder picker and opens the
/// workspace in it, creating one when there is none. Returns the folder,
/// or `None` when the user cancelled; the UI then reloads.
#[tauri::command]
pub(crate) async fn open_workspace(
    window: WebviewWindow,
    workspaces: State<'_, Arc<Workspaces>>,
) -> Result<Option<String>, String> {
    let Some(folder) = workspaces.pick_folder(&window).await else {
        return Ok(None);
    };
    let root = workspaces.open(&folder)?;
    Ok(Some(root.display().to_string()))
}

#[tauri::command]
pub(crate) fn install_command() -> Result<CommandLine, String> {
    command_line::install()?;
    Ok(CommandLine::current())
}
