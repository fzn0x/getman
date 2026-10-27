//! The app's own settings, kept in `settings.json` in its configuration
//! folder: the workspace used last, the update choices, when it last
//! looked for an update, and the port its UI was served on.
//!
//! Only what differs from the defaults is written, and a missing or broken
//! file reads as the defaults, so a bad file never stops the app.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct Settings {
    /// The folder of the workspace open last.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) workspace: Option<PathBuf>,
    /// Whether the app looks for a new version once a day; on until the
    /// user turns it off.
    #[serde(skip_serializing_if = "Option::is_none")]
    check_for_updates: Option<bool>,
    /// Whether it installs new versions without asking.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    install_without_asking: bool,
    /// When it last looked, in seconds since 1970.
    #[serde(skip_serializing_if = "Option::is_none")]
    last_check: Option<u64>,
    /// The port the UI was served on, asked for again so the page keeps
    /// its own storage, which belongs to its address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) port: Option<u16>,
}

impl Settings {
    /// Whether the app looks for a new version once a day.
    pub(crate) fn checks(&self) -> bool {
        self.check_for_updates.unwrap_or(true)
    }

    /// Whether it installs new versions without asking, which it does only
    /// while it looks for them.
    pub(crate) fn installs_without_asking(&self) -> bool {
        self.checks() && self.install_without_asking
    }

    /// Sets both update choices; installing without asking needs checks.
    pub(crate) fn set_updates(&mut self, checks: bool, install_without_asking: bool) {
        self.check_for_updates = Some(checks);
        self.install_without_asking = checks && install_without_asking;
    }

    pub(crate) fn last_check(&self) -> Option<SystemTime> {
        self.last_check
            .map(|seconds| UNIX_EPOCH + Duration::from_secs(seconds))
    }

    /// When it last looked, in seconds since 1970, for the UI.
    pub(crate) fn last_check_seconds(&self) -> Option<u64> {
        self.last_check
    }

    pub(crate) fn set_last_check(&mut self, at: SystemTime) {
        self.last_check = at.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs());
    }

    /// Reads the settings in `path`, or the defaults when there are none
    /// or they cannot be read.
    pub(crate) fn load(path: &Path) -> Self {
        fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Writes the settings to `path` through a temporary file, so a crash
    /// halfway leaves the old ones.
    pub(crate) fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        let temporary = path.with_extension("json.tmp");
        fs::write(&temporary, json)?;
        fs::rename(&temporary, path)
    }
}

/// The settings shared by the parts of the app, saved on every change.
pub(crate) struct Store {
    path: PathBuf,
    settings: Mutex<Settings>,
}

impl Store {
    pub(crate) fn open(path: PathBuf) -> Self {
        let settings = Mutex::new(Settings::load(&path));
        Self { path, settings }
    }

    pub(crate) fn get(&self) -> Settings {
        self.settings
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Changes the settings and saves them, returning them as changed.
    pub(crate) fn update(&self, change: impl FnOnce(&mut Settings)) -> Result<Settings, String> {
        let mut settings = self
            .settings
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut changed = settings.clone();
        change(&mut changed);
        changed.save(&self.path).map_err(|e| {
            format!(
                "Could not save the settings in {}: {e}",
                self.path.display()
            )
        })?;
        *settings = changed.clone();
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_are_on_and_installs_ask_until_the_user_chooses() {
        let settings = Settings::default();
        assert!(settings.checks());
        assert!(!settings.installs_without_asking());
    }

    #[test]
    fn installing_without_asking_needs_checks() {
        let mut settings = Settings::default();
        settings.set_updates(true, true);
        assert!(settings.installs_without_asking());
        settings.set_updates(false, true);
        assert!(!settings.checks());
        assert!(!settings.installs_without_asking());
    }

    #[test]
    fn settings_survive_a_save_and_write_only_what_changed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("settings.json");
        Settings::default().save(&path).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "{}");

        let mut settings = Settings {
            workspace: Some(dir.path().join("api")),
            port: Some(51234),
            ..Settings::default()
        };
        settings.set_updates(false, false);
        settings.set_last_check(UNIX_EPOCH + Duration::from_secs(1_800_000_000));
        settings.save(&path).unwrap();
        let loaded = Settings::load(&path);
        assert_eq!(loaded, settings);
        assert!(!loaded.checks());
        assert_eq!(loaded.last_check_seconds(), Some(1_800_000_000));
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn a_missing_or_broken_file_reads_as_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        assert_eq!(Settings::load(&path), Settings::default());
        fs::write(&path, "{ not json").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
        fs::write(&path, r#"{"checkForUpdates": false, "fromLater": 1}"#).unwrap();
        assert!(!Settings::load(&path).checks());
    }

    #[test]
    fn the_store_saves_every_change() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let store = Store::open(path.clone());
        store.update(|s| s.set_updates(true, true)).unwrap();
        assert!(Store::open(path).get().installs_without_asking());
    }
}
