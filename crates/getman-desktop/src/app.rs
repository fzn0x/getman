//! The window: Getman's server started in this process on 127.0.0.1, and
//! its UI shown in a native webview instead of a browser tab.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use getman::server::{Options, Server};
use getman_core::Workspace;
use tauri::ipc::CapabilityBuilder;
use tauri::webview::{NewWindowResponse, PageLoadEvent};
use tauri::window::Color;
use tauri::{
    App, AppHandle, Manager as _, RunEvent, Theme, Url, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_window_state::StateFlags;

use crate::install::Install;
use crate::settings::Store;
use crate::updates::Updates;

/// The page's own colors (`--paper` in the UI), shown before it paints.
const PAPER: Color = Color(0xf2, 0xf0, 0xeb, 0xff);
const CARBON: Color = Color(0x0e, 0x0e, 0x0d, 0xff);

/// How long the window waits for the page before it shows anyway.
const SHOW_AT_THE_LATEST: Duration = Duration::from_secs(5);

/// Runs the app until its window closes.
pub(crate) fn run(folder: Option<PathBuf>) -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => return fail(&format!("Could not start: {e}")),
    };
    // Tauri and the server share one runtime.
    tauri::async_runtime::set(runtime.handle().clone());
    let app = tauri::Builder::default()
        // First, so a second start hands over to the first and quits
        // before it does anything else.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(
            tauri_plugin_window_state::Builder::new()
                // The window shows itself once the page has painted.
                .with_state_flags(StateFlags::all() - StateFlags::VISIBLE)
                .build(),
        )
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            crate::commands::desktop_state,
            crate::commands::set_update_preferences,
            crate::commands::check_for_updates,
            crate::commands::install_update,
            crate::commands::restart,
            crate::commands::open_workspace,
            crate::commands::install_command,
        ])
        .setup(move |app| setup(app, folder.as_deref()))
        .build(tauri::generate_context!());
    let app = match app {
        Ok(app) => app,
        Err(e) => return fail(&format!("Could not start: {e}")),
    };
    let code = app.run_return(|app, event| {
        if let RunEvent::Exit = event
            && let Some(updates) = app.try_state::<Arc<Updates>>()
        {
            updates.install_on_quit();
        }
    });
    drop(runtime);
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}

fn setup(app: &mut App, folder: Option<&Path>) -> Result<(), Box<dyn Error>> {
    let store = Arc::new(Store::open(
        app.path().app_config_dir()?.join("settings.json"),
    ));
    let workspace = first_workspace(folder, &store)?;
    let root = workspace.root().to_path_buf();

    // Ask for the port used last, so the page keeps the storage that
    // belongs to its address: theme, layout and open tabs.
    let remembered = store.get().port;
    let server = tauri::async_runtime::block_on(async {
        match Server::start(workspace.clone(), options(remembered.or(Some(0)))).await {
            Ok(server) => Ok(server),
            Err(_) if remembered.is_some() => Server::start(workspace, options(Some(0))).await,
            Err(e) => Err(e),
        }
    })?;
    let port = server.address().port();
    let _ = store.update(|s| {
        s.workspace = Some(root);
        s.port = Some(port);
    });

    // Only the page from this server may call the app's commands.
    app.add_capability(
        CapabilityBuilder::new("server")
            .remote(format!("http://127.0.0.1:{port}"))
            .window("main")
            .permission("core:event:allow-listen")
            .permission("core:event:allow-unlisten")
            .permission("allow-desktop-state")
            .permission("allow-set-update-preferences")
            .permission("allow-check-for-updates")
            .permission("allow-install-update")
            .permission("allow-restart")
            .permission("allow-open-workspace")
            .permission("allow-install-command"),
    )?;

    let url: Url = server.url_with_token().parse()?;
    window(app.handle(), url, port)?;

    let updates = Arc::new(Updates::new(
        Install::current(),
        app.package_info().version.clone(),
        Arc::clone(&store),
    ));
    app.manage(Arc::clone(&updates));
    app.manage(Arc::new(Workspaces { server, store }));
    tauri::async_runtime::spawn(updates.schedule(app.handle().clone()));
    Ok(())
}

fn options(port: Option<u16>) -> Options {
    Options {
        port,
        open_browser: false,
    }
}

/// The workspace to open first: the one in `folder`, else the one used
/// last, else the personal workspace.
fn first_workspace(folder: Option<&Path>, store: &Store) -> Result<Workspace, String> {
    if let Some(folder) = folder {
        return getman::workspaces::open_or_create(folder);
    }
    if let Some(last) = store.get().workspace
        && let Some(root) = Workspace::discover(&last)
        && let Ok(workspace) = Workspace::open(root)
    {
        return Ok(workspace);
    }
    getman::workspaces::personal()
}

/// Opens the main window on the UI at `url`, hidden until the page has
/// painted so it never flashes white.
fn window(app: &AppHandle, url: Url, port: u16) -> tauri::Result<WebviewWindow> {
    let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url))
        .title("Getman")
        .inner_size(1280.0, 820.0)
        .min_inner_size(640.0, 480.0)
        .center()
        .visible(false)
        .on_navigation(move |url| {
            if stays_in_the_app(url, port) {
                return true;
            }
            open_outside(url);
            false
        })
        .on_new_window(|url, _features| {
            open_outside(&url);
            NewWindowResponse::Deny
        })
        .on_page_load(|window, page| {
            if page.event() == PageLoadEvent::Finished {
                show(&window);
            }
        })
        .build()?;
    paint_background(&window, window.theme().unwrap_or(Theme::Light));
    let painted = window.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::ThemeChanged(theme) = event {
            paint_background(&painted, *theme);
        }
    });
    let late = window.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SHOW_AT_THE_LATEST).await;
        show(&late);
    });
    Ok(window)
}

fn show(window: &WebviewWindow) {
    if !window.is_visible().unwrap_or(true) {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn paint_background(window: &WebviewWindow, theme: Theme) {
    let color = if theme == Theme::Dark { CARBON } else { PAPER };
    let _ = window.set_background_color(Some(color));
}

/// Whether a navigation stays in the app: the UI's own pages, and the
/// blank, blob and data documents its previews show in frames.
fn stays_in_the_app(url: &Url, port: u16) -> bool {
    match url.scheme() {
        "http" => url.host_str() == Some("127.0.0.1") && url.port() == Some(port),
        "about" | "blob" | "data" => true,
        _ => false,
    }
}

/// Opens a link that leaves the app in the system's browser or mail app.
fn open_outside(url: &Url) {
    if matches!(url.scheme(), "http" | "https" | "mailto") {
        let _ = open::that_detached(url.as_str());
    }
}

/// Switches the window between workspaces.
pub(crate) struct Workspaces {
    server: Server,
    store: Arc<Store>,
}

impl Workspaces {
    /// Asks for a folder with the system's folder picker.
    pub(crate) async fn pick_folder(&self, window: &WebviewWindow) -> Option<PathBuf> {
        let (picked, chosen) = tokio::sync::oneshot::channel();
        let parent = window.clone();
        // The picker is made on the main thread and awaited here.
        window
            .run_on_main_thread(move || {
                let dialog = rfd::AsyncFileDialog::new()
                    .set_title("Open a workspace folder")
                    .set_parent(&parent)
                    .pick_folder();
                let _ = picked.send(dialog);
            })
            .ok()?;
        let handle = chosen.await.ok()?.await?;
        Some(handle.path().to_path_buf())
    }

    /// Opens the workspace in `folder`, or creates one there, and returns
    /// its root.
    pub(crate) fn open(&self, folder: &Path) -> Result<PathBuf, String> {
        let workspace = getman::workspaces::open_or_create(folder)?;
        let root = workspace.root().to_path_buf();
        self.server.open(workspace);
        self.store.update(|s| s.workspace = Some(root.clone()))?;
        Ok(root)
    }
}

/// Says why the app could not start, where the user sees it.
fn fail(message: &str) -> ExitCode {
    eprintln!("getman: {message}");
    let _ = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("Getman")
        .set_description(message)
        .show();
    ExitCode::from(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_apps_own_pages_stay_in_the_window() {
        let url = |text: &str| Url::parse(text).unwrap();
        assert!(stays_in_the_app(&url("http://127.0.0.1:7901/"), 7901));
        assert!(stays_in_the_app(&url("about:srcdoc"), 7901));
        assert!(stays_in_the_app(&url("blob:http://127.0.0.1:7901/x"), 7901));
        assert!(!stays_in_the_app(&url("http://127.0.0.1:7902/"), 7901));
        assert!(!stays_in_the_app(&url("http://localhost:7901/"), 7901));
        assert!(!stays_in_the_app(
            &url("https://fzn0x.github.io/getman/"),
            7901
        ));
        assert!(!stays_in_the_app(&url("file:///etc/passwd"), 7901));
    }
}
