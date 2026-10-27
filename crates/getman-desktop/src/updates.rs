//! In-app updates.
//!
//! While "Check for updates automatically" is on, the app reads
//! `latest.json` from the newest GitHub release once a day: the only
//! request it makes on its own. A newer version is offered in the UI, or,
//! with "Install updates without asking", downloaded and installed in the
//! background to run at the next launch. Every download is checked against
//! the minisign public key built into the app before it is installed.
//!
//! Copies that cannot replace themselves (development builds, the .deb and
//! .rpm packages, the portable zip) never look, and say why.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use semver::Version;
use serde::Serialize;
use tauri::{AppHandle, Emitter as _};
use tauri_plugin_updater::{Update, UpdaterExt as _};
use tokio::sync::Notify;

use crate::install::Install;
use crate::settings::Store;

/// How often the app looks for a new version.
pub(crate) const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);
/// How long after start it first looks, so it never slows the start.
const FIRST_CHECK_AFTER: Duration = Duration::from_secs(10);
/// How soon it looks again after a check or an install failed.
const RETRY_AFTER: Duration = Duration::from_secs(60 * 60);
/// The longest it sleeps without looking at the clock, so a computer that
/// slept through the time a check was due checks soon after it wakes.
const LOOK_AT_THE_CLOCK_EVERY: Duration = Duration::from_secs(60 * 60);

/// The event that carries the [`Status`] to the UI whenever it changes.
pub(crate) const EVENT: &str = "getman://updates";

/// How long until the daily check is due, given when the last one was.
pub(crate) fn due_in(last_check: Option<SystemTime>, now: SystemTime) -> Duration {
    let Some(last_check) = last_check else {
        return Duration::ZERO;
    };
    match now.duration_since(last_check) {
        Ok(since) => CHECK_EVERY.saturating_sub(since),
        // The clock went back past the last check: don't wait for it.
        Err(_) => Duration::ZERO,
    }
}

/// Whether a copy running `current` is offered `candidate`: only a newer
/// version, and a pre-release only to a copy that is a pre-release too.
pub(crate) fn should_offer(current: &Version, candidate: &Version) -> bool {
    candidate > current && (candidate.pre.is_empty() || !current.pre.is_empty())
}

/// Where an update stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub(crate) enum Phase {
    /// Nothing to tell.
    Idle,
    /// Looking for a new version; `manual` when the user asked.
    Checking { manual: bool },
    /// The user asked, and this is the newest version.
    UpToDate,
    /// A newer version is out, waiting for the user to install it.
    #[serde(rename_all = "camelCase")]
    Available {
        version: String,
        notes: String,
        date: Option<String>,
    },
    /// Downloading the new version; `total` is unknown until the server
    /// says.
    Downloading {
        version: String,
        done: u64,
        total: Option<u64>,
    },
    /// The new version is installed and runs once Getman restarts; on
    /// Windows it is downloaded and verified, and installs as Getman
    /// restarts or quits.
    Installed { version: String },
    /// The check or the install the user asked for failed.
    Failed { message: String },
}

/// What the UI shows in Settings › About and the update dialog.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Status {
    /// The version running.
    version: String,
    /// How this copy was installed.
    install: Install,
    /// Why this copy doesn't update itself, or `None` when it does.
    not_updating: Option<&'static str>,
    check_for_updates: bool,
    install_without_asking: bool,
    /// When the app last looked, in seconds since 1970.
    last_check: Option<u64>,
    /// Whether a new version installs when Getman restarts, as on Windows.
    installs_on_restart: bool,
    phase: Phase,
}

pub(crate) struct Updates {
    install: Install,
    version: Version,
    store: Arc<Store>,
    state: Mutex<State>,
    /// Wakes the schedule when the choices change.
    wake: Notify,
}

#[derive(Default)]
struct State {
    phase: Option<Phase>,
    /// The update found last, until it is installed.
    update: Option<Update>,
    /// On Windows, the verified installer to run when Getman restarts or
    /// quits.
    waiting: Option<(Update, Vec<u8>)>,
}

impl Updates {
    pub(crate) fn new(install: Install, version: Version, store: Arc<Store>) -> Self {
        Self {
            install,
            version,
            store,
            state: Mutex::new(State::default()),
            wake: Notify::new(),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn status(&self) -> Status {
        let settings = self.store.get();
        let phase = self.lock().phase.clone().unwrap_or(Phase::Idle);
        Status {
            version: self.version.to_string(),
            install: self.install,
            not_updating: self.install.why_not_updating(),
            check_for_updates: settings.checks(),
            install_without_asking: settings.installs_without_asking(),
            last_check: settings.last_check_seconds(),
            installs_on_restart: self.install.installs_on_restart(),
            phase,
        }
    }

    fn set_phase(&self, app: &AppHandle, phase: Phase) {
        self.lock().phase = Some(phase);
        self.announce(app);
    }

    fn announce(&self, app: &AppHandle) {
        let _ = app.emit(EVENT, self.status());
    }

    /// Turns the daily check and installing without asking on or off.
    pub(crate) fn set_preferences(
        &self,
        app: &AppHandle,
        checks: bool,
        install_without_asking: bool,
    ) -> Result<Status, String> {
        self.store
            .update(|s| s.set_updates(checks, install_without_asking))?;
        self.wake.notify_one();
        self.announce(app);
        Ok(self.status())
    }

    /// Looks for a new version now, as the user asked, and offers it.
    pub(crate) async fn check_now(&self, app: &AppHandle) -> Result<Status, String> {
        if let Some(why) = self.install.why_not_updating() {
            return Err(why.to_owned());
        }
        match self.look(app, true).await {
            Ok(Some(update)) => self.offer(app, &update),
            Ok(None) => {
                if !matches!(self.lock().phase, Some(Phase::Installed { .. })) {
                    self.set_phase(app, Phase::UpToDate);
                }
            }
            Err(message) => self.set_phase(app, Phase::Failed { message }),
        }
        Ok(self.status())
    }

    /// Asks the release for its newest version: `Some` when it is one to
    /// offer and not the one installed already.
    async fn look(&self, app: &AppHandle, manual: bool) -> Result<Option<Update>, String> {
        let before = self.lock().phase.clone();
        self.set_phase(app, Phase::Checking { manual });
        let found = async {
            app.updater_builder()
                .version_comparator(|current, release| should_offer(&current, &release.version))
                .build()
                .map_err(|e| e.to_string())?
                .check()
                .await
                .map_err(|e| format!("Could not look for a new version: {e}"))
        }
        .await;
        // Put back what the check interrupted, such as an installed
        // version waiting for a restart.
        self.lock().phase = before;
        let found = found?;
        let _ = self.store.update(|s| s.set_last_check(SystemTime::now()));
        if let (Some(update), Some(Phase::Installed { version })) = (&found, &self.lock().phase)
            && &update.version == version
        {
            return Ok(None);
        }
        self.announce(app);
        Ok(found)
    }

    fn offer(&self, app: &AppHandle, update: &Update) {
        let phase = Phase::Available {
            version: update.version.clone(),
            notes: update.body.clone().unwrap_or_default(),
            date: update
                .raw_json
                .get("pub_date")
                .and_then(|date| date.as_str())
                .map(str::to_owned),
        };
        self.lock().update = Some(update.clone());
        self.set_phase(app, phase);
    }

    /// Downloads the update offered, checks its signature and installs it,
    /// or on Windows keeps it to install when Getman restarts or quits.
    pub(crate) async fn install(&self, app: &AppHandle) -> Result<Status, String> {
        let update = self
            .lock()
            .update
            .clone()
            .ok_or("There is no new version to install. Check for updates first.")?;
        if let Err(message) = self.download_and_install(app, update).await {
            self.set_phase(
                app,
                Phase::Failed {
                    message: message.clone(),
                },
            );
            return Err(message);
        }
        Ok(self.status())
    }

    async fn download_and_install(&self, app: &AppHandle, update: Update) -> Result<(), String> {
        let version = update.version.clone();
        self.set_phase(
            app,
            Phase::Downloading {
                version: version.clone(),
                done: 0,
                total: None,
            },
        );
        let mut done = 0_u64;
        let mut shown = 0_u64;
        let bytes = update
            .download(
                |chunk, total| {
                    done += chunk as u64;
                    // Tell the UI at every whole percent, not every chunk.
                    let step = total.map_or(256 * 1024, |t| (t / 100).max(1));
                    if done - shown >= step {
                        shown = done;
                        self.set_phase(
                            app,
                            Phase::Downloading {
                                version: version.clone(),
                                done,
                                total,
                            },
                        );
                    }
                },
                || {},
            )
            .await
            .map_err(|e| format!("Could not download Getman {version}: {e}"))?;
        if self.install.installs_on_restart() {
            self.lock().waiting = Some((update, bytes));
        } else {
            let update = update.clone();
            tokio::task::spawn_blocking(move || update.install(bytes))
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| format!("Could not install Getman {version}: {e}"))?;
        }
        self.lock().update = None;
        self.set_phase(app, Phase::Installed { version });
        Ok(())
    }

    /// Restarts Getman, installing a new version that waits for it.
    pub(crate) fn restart(&self, app: &AppHandle) {
        let waiting = self.lock().waiting.take();
        if let Some((update, bytes)) = waiting {
            // The installer quits Getman and starts the new version.
            let _ = update.restart_after_install(true).install(bytes);
        }
        app.restart();
    }

    /// Installs a new version that waits for Getman to quit, so it runs at
    /// the next launch.
    pub(crate) fn install_on_quit(&self) {
        let waiting = self.lock().waiting.take();
        if let Some((update, bytes)) = waiting {
            let _ = update.restart_after_install(false).install(bytes);
        }
    }

    /// Looks for a new version once a day while the user wants it, and
    /// offers it or installs it as they chose. Runs until the app quits.
    pub(crate) async fn schedule(self: Arc<Self>, app: AppHandle) {
        if self.install.why_not_updating().is_some() {
            return;
        }
        tokio::time::sleep(FIRST_CHECK_AFTER).await;
        // Holds checks off after one, even if the time of the last check
        // could not be saved.
        let mut not_before = SystemTime::now();
        loop {
            let settings = self.store.get();
            let now = SystemTime::now();
            let wait = if settings.checks() {
                due_in(settings.last_check(), now)
                    .max(not_before.duration_since(now).unwrap_or_default())
            } else {
                LOOK_AT_THE_CLOCK_EVERY
            };
            if wait.is_zero() {
                not_before = now + self.check_on_schedule(&app).await;
                continue;
            }
            tokio::select! {
                () = tokio::time::sleep(wait.min(LOOK_AT_THE_CLOCK_EVERY)) => {}
                () = self.wake.notified() => {}
            }
        }
    }

    /// The daily check: returns how long to wait before the next one.
    async fn check_on_schedule(&self, app: &AppHandle) -> Duration {
        let update = match self.look(app, false).await {
            Ok(Some(update)) => update,
            Ok(None) => return CHECK_EVERY,
            Err(_) => return RETRY_AFTER,
        };
        if !self.store.get().installs_without_asking() {
            self.offer(app, &update);
            return CHECK_EVERY;
        }
        self.lock().update = Some(update.clone());
        if self.download_and_install(app, update).await.is_err() {
            // As when GitHub lists the release before its files are up.
            self.set_phase(app, Phase::Idle);
            return RETRY_AFTER;
        }
        CHECK_EVERY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn only_newer_versions_are_offered() {
        assert!(should_offer(&version("0.1.0"), &version("0.2.0")));
        assert!(should_offer(&version("0.9.9"), &version("0.10.0")));
        assert!(should_offer(&version("1.2.3"), &version("1.2.4")));
        assert!(!should_offer(&version("0.2.0"), &version("0.2.0")));
        assert!(!should_offer(&version("0.10.0"), &version("0.9.9")));
    }

    #[test]
    fn pre_releases_go_only_to_pre_releases() {
        assert!(!should_offer(&version("0.1.0"), &version("0.2.0-beta.1")));
        assert!(should_offer(
            &version("0.2.0-beta.1"),
            &version("0.2.0-beta.2")
        ));
        assert!(should_offer(&version("0.2.0-beta.2"), &version("0.2.0")));
        assert!(!should_offer(&version("0.2.0"), &version("0.2.0-rc.1")));
    }

    #[test]
    fn checks_come_once_a_day() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
        let hour = Duration::from_secs(60 * 60);
        assert_eq!(due_in(None, now), Duration::ZERO);
        assert_eq!(due_in(Some(now), now), CHECK_EVERY);
        assert_eq!(
            due_in(Some(now - hour), now),
            CHECK_EVERY.checked_sub(hour).unwrap()
        );
        assert_eq!(due_in(Some(now - CHECK_EVERY), now), Duration::ZERO);
        assert_eq!(due_in(Some(now - 3 * CHECK_EVERY), now), Duration::ZERO);
        // A last check in the future means the clock went back.
        assert_eq!(due_in(Some(now + hour), now), Duration::ZERO);
    }

    #[test]
    fn the_status_says_why_a_copy_does_not_update() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(dir.path().join("settings.json")));
        let updates = Updates::new(Install::Development, version("0.1.0"), store);
        let status = serde_json::to_value(updates.status()).unwrap();
        assert_eq!(status["install"], "development");
        assert_eq!(status["checkForUpdates"], true);
        assert_eq!(status["installWithoutAsking"], false);
        assert_eq!(status["phase"]["state"], "idle");
        assert!(
            status["notUpdating"]
                .as_str()
                .unwrap()
                .contains("Development")
        );
    }
}
