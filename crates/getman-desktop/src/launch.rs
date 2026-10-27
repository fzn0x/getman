//! Whether a start of this program opens the window or runs the command
//! line.
//!
//! On macOS and Linux the app's executable is also the `getman` command, so
//! it decides from how it was started: from the Dock, the app launcher or
//! with `getman desktop` it opens the window; with any other arguments it
//! is the command line, unchanged. On Windows a program is either a console
//! program or a window program, so the app is only ever the window and
//! `getman.exe` beside it is the command line.

use std::ffi::OsString;
use std::path::PathBuf;

/// What a start does.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Launch {
    /// Runs the command line with these arguments, the program's name first.
    CommandLine(Vec<OsString>),
    /// Opens the window on the workspace in this folder, or else the one
    /// used last.
    Desktop(Option<PathBuf>),
}

/// How this program was started.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct Host {
    /// Windows, where this program is only ever the window.
    pub(crate) windows: bool,
    /// Started from inside a macOS app bundle, as the Dock and Finder do.
    pub(crate) app_bundle: bool,
    /// Started as an AppImage, whose launcher passes `desktop`, from the
    /// app's desktop entry, before the arguments it was given.
    pub(crate) appimage: bool,
}

impl Host {
    pub(crate) fn current(args: &[OsString]) -> Self {
        Self {
            windows: cfg!(windows),
            app_bundle: cfg!(target_os = "macos")
                && args.first().is_some_and(|program| {
                    program.to_string_lossy().contains(".app/Contents/MacOS/")
                }),
            appimage: cfg!(target_os = "linux") && std::env::var_os("APPIMAGE").is_some(),
        }
    }
}

/// Decides what a start with `args` does.
pub(crate) fn decide(mut args: Vec<OsString>, host: Host) -> Launch {
    if args.is_empty() {
        args.push("getman".into());
    }
    if host.appimage && args.get(1).is_some_and(|a| a == "desktop") {
        args.remove(1);
    }
    let rest = &args[1..];
    let desktop = rest.first().is_some_and(|a| a == "desktop");
    if host.windows || desktop || (rest.is_empty() && (host.app_bundle || host.appimage)) {
        let folder = rest.get(usize::from(desktop)).map(PathBuf::from);
        return Launch::Desktop(folder);
    }
    Launch::CommandLine(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    const TERMINAL: Host = Host {
        windows: false,
        app_bundle: false,
        appimage: false,
    };

    #[test]
    fn commands_run_the_command_line_unchanged() {
        assert_eq!(
            decide(args(&["getman", "run", "-e", "staging"]), TERMINAL),
            Launch::CommandLine(args(&["getman", "run", "-e", "staging"]))
        );
        // Without arguments, `getman` in a terminal opens the browser UI.
        assert_eq!(
            decide(args(&["getman"]), TERMINAL),
            Launch::CommandLine(args(&["getman"]))
        );
    }

    #[test]
    fn desktop_opens_the_window() {
        assert_eq!(
            decide(args(&["getman", "desktop"]), TERMINAL),
            Launch::Desktop(None)
        );
        assert_eq!(
            decide(args(&["getman", "desktop", "api"]), TERMINAL),
            Launch::Desktop(Some("api".into()))
        );
    }

    #[test]
    fn the_dock_opens_the_window() {
        let dock = Host {
            app_bundle: true,
            ..TERMINAL
        };
        assert_eq!(
            decide(
                args(&["/Applications/Getman.app/Contents/MacOS/getman"]),
                dock
            ),
            Launch::Desktop(None)
        );
        assert_eq!(
            decide(args(&["getman", "send", "a.yaml"]), dock),
            Launch::CommandLine(args(&["getman", "send", "a.yaml"]))
        );
    }

    #[test]
    fn an_appimage_drops_the_desktop_its_launcher_adds() {
        let appimage = Host {
            appimage: true,
            ..TERMINAL
        };
        assert_eq!(
            decide(args(&["getman", "desktop"]), appimage),
            Launch::Desktop(None)
        );
        assert_eq!(
            decide(args(&["getman", "desktop", "run", "--bail"]), appimage),
            Launch::CommandLine(args(&["getman", "run", "--bail"]))
        );
    }

    #[test]
    fn on_windows_it_is_always_the_window() {
        let windows = Host {
            windows: true,
            ..TERMINAL
        };
        assert_eq!(
            decide(args(&["Getman.exe"]), windows),
            Launch::Desktop(None)
        );
        assert_eq!(
            decide(args(&["Getman.exe", "C:\\api"]), windows),
            Launch::Desktop(Some("C:\\api".into()))
        );
    }
}
