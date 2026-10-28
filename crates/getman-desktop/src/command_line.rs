//! "Install the getman command" on macOS. The app's executable is also the
//! `getman` command, so installing it is a link to the executable in a
//! folder on PATH: `/usr/local/bin` when it can write there, else
//! `~/.local/bin`. A file that is not a link is never replaced, so a
//! `getman` installed another way keeps working.

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

fn link(exe: &Path, folders: &[PathBuf]) -> Result<PathBuf, String> {
    let mut failure = String::from("There is no folder to put the getman command in.");
    for folder in folders {
        let link = folder.join("getman");
        if let Ok(meta) = fs::symlink_metadata(&link) {
            if !meta.file_type().is_symlink() {
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;

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
