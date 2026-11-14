//! Getman on Android and iOS. As in the desktop app, Getman's server runs
//! in the app's own process on 127.0.0.1, and the app's webview shows its
//! UI.
//!
//! A phone has no folders to open, so the app keeps one workspace in its
//! documents, made with example requests on first start. iOS shows it in
//! the Files app, under On My iPhone › Getman. On Android it is in the
//! app's folder on the shared storage, which a computer sees over USB.

use std::error::Error;
use std::fs;
use std::path::Path;

use getman::server::{Options, Server};
use tauri::{App, Manager as _, Url, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_opener::OpenerExt as _;

#[cfg(target_os = "android")]
mod android;

/// Starts the app. Android and iOS call it from their own entry points.
///
/// # Panics
///
/// When Tauri cannot start, which the phone reports as a crash.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(setup)
        .run(tauri::generate_context!())
        .expect("Getman could not start");
}

fn setup(app: &mut App) -> Result<(), Box<dyn Error>> {
    #[cfg(target_os = "android")]
    android::use_the_system_certificates()?;

    let workspace =
        getman::workspaces::personal_in(&app.path().document_dir()?.join("My workspace"))?;

    // The UI keeps its theme, layout and open tabs in the storage of its
    // address, so the port stays the same from one start to the next.
    let port_file = app.path().app_data_dir()?.join("port");
    let remembered = fs::read_to_string(&port_file)
        .ok()
        .and_then(|text| text.trim().parse().ok());
    let server = tauri::async_runtime::block_on(async {
        match Server::start(workspace.clone(), options(remembered)).await {
            Err(_) if remembered.is_some() => Server::start(workspace, options(None)).await,
            started => started,
        }
    })?;
    let port = server.address().port();
    remember(&port_file, port);

    let url: Url = server.url_with_token().parse()?;
    let opener = app.handle().clone();
    WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url))
        .on_navigation(move |url| {
            if stays_in_the_app(url, port) {
                return true;
            }
            // Links out of the UI, such as to the docs, open in the
            // phone's browser.
            if matches!(url.scheme(), "http" | "https" | "mailto") {
                let _ = opener.opener().open_url(url.as_str(), None::<&str>);
            }
            false
        })
        .build()?;
    // The server stops when it is dropped.
    app.manage(server);
    Ok(())
}

/// Starts the server on `port`, or any free port.
fn options(port: Option<u16>) -> Options {
    Options {
        port: Some(port.unwrap_or(0)),
        open_browser: false,
        app_routes: None,
    }
}

fn remember(file: &Path, port: u16) {
    if let Some(dir) = file.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let _ = fs::write(file, port.to_string());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_apps_own_pages_stay_in_the_app() {
        let url = |text: &str| Url::parse(text).unwrap();
        assert!(stays_in_the_app(&url("http://127.0.0.1:7901/"), 7901));
        assert!(stays_in_the_app(&url("about:srcdoc"), 7901));
        assert!(stays_in_the_app(&url("blob:http://127.0.0.1:7901/x"), 7901));
        assert!(!stays_in_the_app(&url("http://127.0.0.1:7902/"), 7901));
        assert!(!stays_in_the_app(
            &url("https://fzn0x.github.io/getman/"),
            7901
        ));
        assert!(!stays_in_the_app(&url("file:///etc/passwd"), 7901));
    }

    #[test]
    fn the_port_is_remembered_for_the_next_start() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("app").join("port");
        remember(&file, 7901);
        assert_eq!(fs::read_to_string(&file).unwrap(), "7901");
    }
}
