use super::receipt::Attempt;
#[cfg(windows)]
use super::receipt::Outcome;
use super::{UpdateTarget, download::VerifiedInstallerDownload};
use crate::models::{AppUpdateInfo, AppUpdateInstallMode};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
use tauri::AppHandle;

pub(crate) fn release_single_instance(app: &AppHandle) {
    static RELEASED: std::sync::Once = std::sync::Once::new();
    RELEASED.call_once(|| tauri_plugin_single_instance::destroy(app));
}

#[cfg(any(windows, target_os = "macos"))]
fn native_message(key: &str, locale: &str) -> &'static str {
    static MESSAGES: std::sync::LazyLock<serde_json::Value> = std::sync::LazyLock::new(|| {
        serde_json::from_str(include_str!("../../../src/locale/updateNative.json"))
            .expect("bundled update translations")
    });
    MESSAGES[key][locale]
        .as_str()
        .expect("complete update translations")
}

#[cfg(target_os = "macos")]
pub(crate) mod mac_quit;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
pub(crate) mod windows;

#[cfg(target_os = "macos")]
use macos::Prepared;
#[cfg(windows)]
use windows::Prepared;

const UNAVAILABLE: &str = "update.stagedUnavailable";
const IN_PROGRESS: &str = "update.inProgress";

#[derive(Default)]
pub(crate) struct AppUpdates {
    slot: Mutex<Slot>,
    stage_finished: Condvar,
}

#[derive(Default)]
struct Slot {
    download: Option<Arc<AtomicBool>>,
    ready: Option<StagedInstaller>,
    offer: Option<AppUpdateInfo>,
    closing: bool,
}

pub(crate) enum DownloadStart<'a> {
    Ready(Box<AppUpdateInfo>),
    Started(StageGuard<'a>),
}

impl AppUpdates {
    pub(crate) fn start_download(&self, version: &str) -> Result<DownloadStart<'_>, String> {
        let mut slot = self.slot.lock().map_err(|_| "lock.poisoned")?;
        slot.check_deferred(version)?;
        if slot.closing || slot.download.is_some() {
            return Err(IN_PROGRESS.into());
        }
        if let Some(ready) = &slot.ready {
            return Ok(DownloadStart::Ready(Box::new(ready.info.clone())));
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        slot.download = Some(cancelled.clone());
        Ok(DownloadStart::Started(StageGuard {
            updates: self,
            cancelled,
        }))
    }

    pub(crate) fn closing(&self) -> Result<Closing<'_>, String> {
        let mut slot = self.slot.lock().map_err(|_| "lock.poisoned")?;
        if slot.closing {
            return Err(IN_PROGRESS.into());
        }
        slot.closing = true;
        Ok(Closing(self))
    }

    pub(crate) fn defer(&self, version: &str) -> Result<(), String> {
        let mut slot = self.slot.lock().map_err(|_| "lock.poisoned")?;
        if slot.closing {
            return Err(IN_PROGRESS.into());
        }
        slot.check_deferred(version)?;
        let ready = slot.ready.as_mut().ok_or(UNAVAILABLE)?;
        ready.install_on_quit = true;
        Ok(())
    }

    pub(crate) fn confirm_offer(
        &self,
        checked: Option<AppUpdateInfo>,
        receipt_offer: Option<AppUpdateInfo>,
        mode: AppUpdateInstallMode,
    ) -> Result<Option<AppUpdateInfo>, String> {
        let mut slot = self.slot.lock().map_err(|_| "lock.poisoned")?;
        if slot.closing {
            return Ok(slot.offer.clone());
        }
        slot.consider_offer(checked, true);
        slot.consider_offer(receipt_offer, false);
        if let Some(offer) = &mut slot.offer {
            offer.install_mode = match mode {
                AppUpdateInstallMode::Deferred if !offer.has_signed_package() => {
                    AppUpdateInstallMode::Manual
                }
                mode => mode,
            };
        }
        if slot.ready.as_ref().is_some_and(|ready| {
            slot.offer
                .as_ref()
                .is_none_or(|offer| !matches_package(offer, &ready.info))
        }) {
            slot.ready = None;
        }
        Ok(slot.offer.clone())
    }

    pub(crate) fn offer(&self) -> Result<Option<AppUpdateInfo>, String> {
        Ok(self.slot.lock().map_err(|_| "lock.poisoned")?.offer.clone())
    }
}

fn matches_package(offer: &AppUpdateInfo, staged: &AppUpdateInfo) -> bool {
    offer.install_mode == AppUpdateInstallMode::Deferred
        && offer.version == staged.version
        && offer.installer_type == staged.installer_type
        && offer.installer_name == staged.installer_name
        && offer.installer_url == staged.installer_url
        && offer.installer_signature == staged.installer_signature
}

impl Slot {
    fn consider_offer(&mut self, candidate: Option<AppUpdateInfo>, accept_same_version: bool) {
        if let Some(candidate) = candidate
            && let Ok(version) = semver::Version::parse(&candidate.version)
            && self
                .offer
                .as_ref()
                .and_then(|offer| semver::Version::parse(&offer.version).ok())
                .is_none_or(|previous| {
                    let order = version.cmp_precedence(&previous);
                    order.is_gt() || accept_same_version && order.is_eq()
                })
        {
            self.offer = Some(candidate);
        }
    }

    fn check_deferred(&self, version: &str) -> Result<(), String> {
        if self.offer.as_ref().is_some_and(|offer| {
            offer.install_mode == AppUpdateInstallMode::Deferred && offer.version == version
        }) {
            Ok(())
        } else {
            Err(UNAVAILABLE.into())
        }
    }
}

pub(crate) struct StageGuard<'a> {
    updates: &'a AppUpdates,
    cancelled: Arc<AtomicBool>,
}

impl StageGuard<'_> {
    pub(crate) fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub(crate) fn publish(&self, staged: StagedInstaller) -> Result<AppUpdateInfo, String> {
        let mut slot = self.updates.slot.lock().map_err(|_| "lock.poisoned")?;
        if self.cancelled() || slot.closing {
            return Err("download.cancelled".into());
        }
        if !slot
            .offer
            .as_ref()
            .is_some_and(|offer| matches_package(offer, &staged.info))
        {
            return Err(UNAVAILABLE.into());
        }
        let info = staged.info.clone();
        slot.ready = Some(staged);
        Ok(info)
    }
}

impl Drop for StageGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut slot) = self.updates.slot.lock() {
            slot.download = None;
            self.updates.stage_finished.notify_all();
        }
    }
}

pub(crate) struct Closing<'a>(&'a AppUpdates);

impl Closing<'_> {
    pub(crate) fn installs_on_quit(&self) -> Result<bool, String> {
        let slot = self.0.slot.lock().map_err(|_| "lock.poisoned")?;
        Ok(slot
            .ready
            .as_ref()
            .is_some_and(|ready| ready.install_on_quit))
    }

    pub(crate) fn wait_for_download(&self, timeout: Duration) -> Result<bool, String> {
        let slot = self.0.slot.lock().map_err(|_| "lock.poisoned")?;
        if let Some(cancelled) = &slot.download {
            cancelled.store(true, Ordering::Release);
        }
        let (slot, _) = self
            .0
            .stage_finished
            .wait_timeout_while(slot, timeout, |slot| slot.download.is_some())
            .map_err(|_| "lock.poisoned")?;
        Ok(slot.download.is_none())
    }

    pub(crate) fn take_ready(&self, version: &str) -> Result<StagedInstaller, String> {
        let mut slot = self.0.slot.lock().map_err(|_| "lock.poisoned")?;
        slot.check_deferred(version)?;
        slot.ready.take().ok_or_else(|| UNAVAILABLE.into())
    }

    pub(crate) fn discard_ready(&self) {
        if let Ok(mut slot) = self.0.slot.lock() {
            slot.ready = None;
        }
    }
}

impl Drop for Closing<'_> {
    fn drop(&mut self) {
        if let Ok(mut slot) = self.0.slot.lock() {
            slot.closing = false;
        }
    }
}

pub(crate) struct StagedInstaller {
    info: AppUpdateInfo,
    prepared: Prepared,
    install_on_quit: bool,
}

impl StagedInstaller {
    pub(crate) fn prepare(
        download: VerifiedInstallerDownload,
        info: AppUpdateInfo,
    ) -> Result<Self, String> {
        if info.install_mode != AppUpdateInstallMode::Deferred {
            return Err(UNAVAILABLE.into());
        }
        let prepared = Prepared::prepare(download, &info)?;
        Ok(Self {
            info,
            prepared,
            install_on_quit: false,
        })
    }

    pub(crate) fn handoff(self, app: &AppHandle, restart: bool) -> Result<(), String> {
        if self.info.install_mode != AppUpdateInstallMode::Deferred
            || UpdateTarget::current()?.install_mode() != AppUpdateInstallMode::Deferred
        {
            return Err(UNAVAILABLE.into());
        }
        let attempt = Attempt::begin(
            self.prepared.source(),
            self.prepared.destination(),
            &self.info.version,
            &self.info.installer_type,
            restart,
        )?;
        #[cfg(windows)]
        {
            let _ = app;
            if let Err(error) = self.prepared.start(&attempt) {
                let outcome = if matches!(error.as_str(), UNAVAILABLE | IN_PROGRESS) {
                    Outcome::Abandoned
                } else {
                    Outcome::Failed
                };
                attempt.record_outcome(outcome)?;
                return Err(error);
            }
            Ok(())
        }
        #[cfg(target_os = "macos")]
        {
            self.prepared.apply(app, &attempt)
        }
    }
}

#[cfg(test)]
mod tests;
