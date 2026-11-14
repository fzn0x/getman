//! Prepares the Tauri app: its configuration and icons. The app has no
//! commands of its own, so the UI gets no permissions.

fn main() {
    tauri_build::build();
}
