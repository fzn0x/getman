//! Prepares the Tauri app: its configuration, icons, and the permissions
//! of the commands the UI may call.

fn main() {
    let commands = tauri_build::AppManifest::new().commands(&[
        "desktop_state",
        "set_update_preferences",
        "check_for_updates",
        "install_update",
        "restart",
        "open_workspace",
        "install_command",
    ]);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(commands))
        .expect("could not prepare the Tauri app");
}
