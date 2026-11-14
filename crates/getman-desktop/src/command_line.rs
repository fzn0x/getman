//! "Install the getman command" on macOS. The app's executable is also the
//! `getman` command, so installing it is a link to the executable in a
//! folder on PATH: `/usr/local/bin` when it can write there, else
//! `~/.local/bin`. Only a link to a Getman app is replaced; a file, or a
//! link to anything else such as Homebrew's, is left, so a `getman`
//! installed another way keeps working.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// Whether this copy offers the command, and where it is installed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CommandLine {
    /// Whether this copy can install it: a macOS app that is not running
    /// from its disk image.
    pub(crate) offered: bool,
    /// The link to this copy, if there is one.
    pub(crate) installed: Option<PathBuf>,
}

impl CommandLine {
    pub(crate) fn current() -> Self {
        let Ok(exe) = std::env::current_exe() else {
            return Self::default();
        };
        let in_app = exe.to_string_lossy().contains(".app/Contents/MacOS/");
        Self {
            offered: cfg!(target_os = "macos") && in_app && !exe.starts_with("/Volumes"),
            installed: installed(&exe, &folders()),
        }
    }
}

/// The folders the link may go in, in order of preference.
fn folders() -> Vec<PathBuf> {
    let mut folders = vec![PathBuf::from("/usr/local/bin")];
    if let Some(home) = std::env::home_dir() {
        folders.push(home.join(".local").join("bin"));
    }
    folders
}

/// The `getman` in `folders` that links to `exe`, if one does.
fn installed(exe: &Path, folders: &[PathBuf]) -> Option<PathBuf> {
    folders
        .iter()
        .map(|folder| folder.join("getman"))
        .find(|link| fs::read_link(link).is_ok_and(|target| target == exe))
}

/// Links `getman` to this copy in the first folder that takes it.
pub(crate) fn install() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    link(&exe, &folders())
}

/// Whether a link pointing at `target` belongs to a Getman app, so that
/// installing may replace it. Homebrew's link points into its Cellar.
fn is_getman_app(target: &Path) -> bool {
    target
        .to_string_lossy()
        .contains("Getman.app/Contents/MacOS/")
}

fn link(exe: &Path, folders: &[PathBuf]) -> Result<PathBuf, String> {
    let mut failure = String::from("There is no folder to put the getman command in.");
    for folder in folders {
        let link = folder.join("getman");
        if let Ok(meta) = fs::symlink_metadata(&link) {
            let ours = meta.file_type().is_symlink()
                && fs::read_link(&link).is_ok_and(|target| is_getman_app(&target));
            if !ours {
                failure = format!(
                    "{} is a getman command installed another way; Getman leaves it as it is.",
                    link.display()
                );
                continue;
            }
            if let Err(e) = fs::remove_file(&link) {
                failure = format!("Could not replace {}: {e}", link.display());
                continue;
            }
        }
        match fs::create_dir_all(folder).and_then(|()| symlink(exe, &link)) {
            Ok(()) => return Ok(link),
            Err(e) => failure = format!("Could not create {}: {e}", link.display()),
        }
    }
    Err(failure)
}

#[cfg(unix)]
fn symlink(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(not(unix))]
fn symlink(_target: &Path, _link: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "the getman command is installed with Getman on this system",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_links_into_a_getman_app_may_be_replaced() {
        for ours in [
            "/Applications/Getman.app/Contents/MacOS/getman",
            "/Users/me/Applications/Getman.app/Contents/MacOS/getman",
            "/Volumes/Getman/Getman.app/Contents/MacOS/getman",
        ] {
            assert!(is_getman_app(Path::new(ours)), "{ours}");
        }
        for other in [
            "/usr/local/Cellar/getman/0.3.4/bin/getman",
            "../Cellar/getman/0.3.4/bin/getman",
            "/opt/homebrew/bin/getman",
            "/Applications/Other.app/Contents/MacOS/getman",
            "/Applications/Getman.app/Contents/Resources/getman",
        ] {
            assert!(!is_getman_app(Path::new(other)), "{other}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn links_into_the_first_folder_that_takes_it_and_spares_files() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("Getman.app/Contents/MacOS/getman");
        fs::create_dir_all(exe.parent().unwrap()).unwrap();
        fs::write(&exe, "").unwrap();
        let taken = dir.path().join("taken");
        let free = dir.path().join("free");
        fs::create_dir_all(&taken).unwrap();
        fs::write(taken.join("getman"), "another getman").unwrap();
        let folders = [taken.clone(), free.clone()];

        assert_eq!(installed(&exe, &folders), None);
        assert_eq!(link(&exe, &folders).unwrap(), free.join("getman"));
        assert_eq!(installed(&exe, &folders), Some(free.join("getman")));
        assert_eq!(
            fs::read_to_string(taken.join("getman")).unwrap(),
            "another getman"
        );
        // Linking again replaces the link.
        assert_eq!(link(&exe, &folders).unwrap(), free.join("getman"));
    }
}

#[cfg(all(test, unix))]
mod link_tests {
    use super::*;

    #[test]
    fn replaces_a_getman_link_and_spares_another_link() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("Getman.app/Contents/MacOS/getman");
        fs::create_dir_all(exe.parent().unwrap()).unwrap();
        fs::write(&exe, "").unwrap();
        let old = dir.path().join("Old/Getman.app/Contents/MacOS/getman");
        let brewed = dir.path().join("Cellar/getman/0.3.4/bin/getman");
        let stale = dir.path().join("stale");
        let brew = dir.path().join("brew");
        let free = dir.path().join("free");
        for folder in [&stale, &brew] {
            fs::create_dir_all(folder).unwrap();
        }
        symlink(&old, &stale.join("getman")).unwrap();
        symlink(&brewed, &brew.join("getman")).unwrap();

        // An old Getman link is replaced.
        assert_eq!(link(&exe, &[stale.clone()]).unwrap(), stale.join("getman"));
        assert_eq!(
            installed(&exe, &[stale]),
            Some(dir.path().join("stale/getman"))
        );
        // Homebrew's link is kept, and the next folder is used.
        assert_eq!(
            link(&exe, &[brew.clone(), free.clone()]).unwrap(),
            free.join("getman")
        );
        assert_eq!(fs::read_link(brew.join("getman")).unwrap(), brewed);
        // With no other folder, the failure says another getman is installed.
        let error = link(&exe, &[brew]).unwrap_err();
        assert!(error.contains("installed another way"), "{error}");
    }
}
