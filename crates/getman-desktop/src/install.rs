//! How this copy of Getman was installed, which decides whether it can
//! replace itself with a new version.

use std::path::Path;

use serde::Serialize;
use tauri::utils::config::BundleType;

/// How this copy was installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Install {
    /// Built from source for development.
    Development,
    /// The Windows setup program.
    Nsis,
    /// The Windows Installer package.
    Msi,
    /// The Windows zip, run from wherever it was unpacked.
    Portable,
    /// The macOS app.
    App,
    /// The Linux AppImage.
    AppImage,
    /// The Debian package.
    Deb,
    /// The RPM package.
    Rpm,
    /// Anything else, such as a release build copied by hand.
    Other,
}

impl Install {
    /// This copy's kind.
    pub(crate) fn current() -> Self {
        let exe = std::env::current_exe().unwrap_or_default();
        detect(
            tauri::is_dev(),
            tauri::utils::platform::bundle_type().as_ref(),
            std::env::consts::OS,
            &exe,
            std::env::var_os("APPIMAGE").is_some(),
            Path::exists,
        )
    }

    /// Why this copy does not replace itself with a new version, or `None`
    /// when it does.
    pub(crate) fn why_not_updating(self) -> Option<&'static str> {
        match self {
            Self::Nsis | Self::Msi | Self::App | Self::AppImage => None,
            Self::Development => Some("Development builds don't update themselves."),
            Self::Deb => Some(
                "Getman was installed from its .deb package, so your package manager updates it: install the new .deb from the releases page.",
            ),
            Self::Rpm => Some(
                "Getman was installed from its .rpm package, so your package manager updates it: install the new .rpm from the releases page.",
            ),
            Self::Portable => Some(
                "This is the portable copy, which doesn't replace itself: download the new zip from the releases page.",
            ),
            Self::Other => Some(
                "This copy wasn't installed by a Getman installer, so it doesn't replace itself.",
            ),
        }
    }

    /// Whether a new version installs only when Getman restarts, as on
    /// Windows, where the running program cannot be replaced.
    pub(crate) fn installs_on_restart(self) -> bool {
        matches!(self, Self::Nsis | Self::Msi)
    }
}

/// Tells how a copy was installed: from whether it is a development
/// build, the bundle type the bundler marked it with, the operating
/// system, where its executable is and whether it runs as an AppImage.
fn detect(
    development: bool,
    bundle: Option<&BundleType>,
    os: &str,
    exe: &Path,
    appimage: bool,
    exists: impl Fn(&Path) -> bool,
) -> Install {
    if development {
        return Install::Development;
    }
    match bundle {
        Some(BundleType::Deb) => return Install::Deb,
        Some(BundleType::Rpm) => return Install::Rpm,
        Some(BundleType::AppImage) => return Install::AppImage,
        Some(BundleType::Msi) => return Install::Msi,
        Some(BundleType::Nsis) => return Install::Nsis,
        _ => {}
    }
    match os {
        // The bundler marks no macOS build, so check it runs from an app.
        "macos" if exe.to_string_lossy().contains(".app/Contents/MacOS/") => Install::App,
        // The installers mark the copies they install; the zip's is not.
        "windows" => Install::Portable,
        "linux" if appimage => Install::AppImage,
        // A package whose mark was lost still lives in /usr, which only
        // the package manager writes to.
        "linux" if exe.starts_with("/usr") => {
            if exists(Path::new("/var/lib/dpkg/info/getman.list")) {
                Install::Deb
            } else {
                Install::Rpm
            }
        }
        _ => Install::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(bundle: Option<&BundleType>, os: &str, exe: &str) -> Install {
        detect(false, bundle, os, Path::new(exe), false, |_| false)
    }

    #[test]
    fn development_builds_never_update() {
        let kind = detect(
            true,
            Some(&BundleType::Nsis),
            "windows",
            Path::new("C:\\Getman\\getman-desktop.exe"),
            false,
            |_| true,
        );
        assert_eq!(kind, Install::Development);
        assert!(kind.why_not_updating().is_some());
    }

    #[test]
    fn the_bundle_mark_tells_the_installer() {
        assert_eq!(
            kind(Some(&BundleType::Nsis), "windows", "C:\\x.exe"),
            Install::Nsis
        );
        assert_eq!(
            kind(Some(&BundleType::Msi), "windows", "C:\\x.exe"),
            Install::Msi
        );
        assert_eq!(
            kind(
                Some(&BundleType::AppImage),
                "linux",
                "/tmp/.mount/usr/bin/getman"
            ),
            Install::AppImage
        );
        assert_eq!(
            kind(None, "windows", "D:\\Tools\\Getman\\getman-desktop.exe"),
            Install::Portable
        );
        assert_eq!(
            kind(
                Some(&BundleType::App),
                "macos",
                "/Applications/Getman.app/Contents/MacOS/getman"
            ),
            Install::App
        );
        assert_eq!(kind(None, "macos", "/Users/me/getman"), Install::Other);
    }

    #[test]
    fn deb_and_rpm_installs_leave_updates_to_the_package_manager() {
        for bundle in [BundleType::Deb, BundleType::Rpm] {
            let kind = kind(Some(&bundle), "linux", "/usr/bin/getman");
            assert!(matches!(kind, Install::Deb | Install::Rpm));
            assert!(
                kind.why_not_updating()
                    .is_some_and(|why| why.contains("package manager"))
            );
        }
        // Without the mark, /usr and dpkg's list of the package still tell.
        let dpkg = |path: &Path| path == Path::new("/var/lib/dpkg/info/getman.list");
        assert_eq!(
            detect(
                false,
                None,
                "linux",
                Path::new("/usr/bin/getman"),
                false,
                dpkg
            ),
            Install::Deb
        );
        assert_eq!(kind(None, "linux", "/usr/bin/getman"), Install::Rpm);
        assert_eq!(kind(None, "linux", "/home/me/bin/getman"), Install::Other);
    }

    #[test]
    fn installers_update_and_windows_installs_on_restart() {
        for kind in [Install::Nsis, Install::Msi, Install::App, Install::AppImage] {
            assert_eq!(kind.why_not_updating(), None);
        }
        assert!(Install::Nsis.installs_on_restart());
        assert!(!Install::App.installs_on_restart());
        assert!(Install::Portable.why_not_updating().is_some());
    }
}
