use super::receipt::Attempt;
#[cfg(windows)]
use super::receipt::Outcome;
use super::{UpdateTarget, download::VerifiedInstallerDownload};
use crate::models::{AppUpdateInfo, AppUpdateInstallMode};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
use tauri::AppHandle;

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
const APPLY_FAILED: &str = "update.installFailedManual";

#[derive(Default)]
pub(crate) struct AppUpdates {
    slot: Mutex<Slot>,
    stage_finished: Condvar,
}

#[derive(Default)]
struct Slot {
    download: Option<Arc<AtomicBool>>,
    ready: Option<StagedInstaller>,
    recovery: Option<Recovery>,
    closing: bool,
}

pub(crate) enum DownloadStart<'a> {
    Ready(Box<AppUpdateInfo>),
    Started(StageGuard<'a>),
}

#[derive(Clone)]
pub(crate) struct Recovery {
    pub(crate) update: AppUpdateInfo,
    pub(crate) alternate_link: String,
}

impl AppUpdates {
    pub(crate) fn start_download(
        &self,
        mode: AppUpdateInstallMode,
    ) -> Result<DownloadStart<'_>, String> {
        let mut slot = self.slot.lock().map_err(|_| "lock.poisoned")?;
        if slot.recovery.is_some() {
            return Err(APPLY_FAILED.into());
        }
        if slot.closing || slot.download.is_some() {
            return Err(IN_PROGRESS.into());
        }
        if let Some(ready) = &slot.ready
            && ready.info.install_mode == mode
        {
            return Ok(DownloadStart::Ready(Box::new(ready.info.clone())));
        }
        slot.ready = None;
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
        if let Some(cancelled) = &slot.download {
            cancelled.store(true, Ordering::Release);
        }
        Ok(Closing(self))
    }

    pub(crate) fn require_recovery(&self, recovery: Recovery) -> Result<(), String> {
        let mut slot = self.slot.lock().map_err(|_| "lock.poisoned")?;
        if let Some(cancelled) = &slot.download {
            cancelled.store(true, Ordering::Release);
        }
        slot.ready = None;
        slot.recovery = Some(recovery);
        Ok(())
    }

    pub(crate) fn recovery(&self) -> Result<Option<Recovery>, String> {
        Ok(self
            .slot
            .lock()
            .map_err(|_| "lock.poisoned")?
            .recovery
            .clone())
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
        if self.cancelled() || slot.closing || slot.recovery.is_some() {
            return Err("download.cancelled".into());
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum UpdateOnExit {
    Skip,
    Install,
    Restart,
}

impl Closing<'_> {
    pub(crate) fn wait_for_download(&self, timeout: Duration) -> Result<bool, String> {
        let slot = self.0.slot.lock().map_err(|_| "lock.poisoned")?;
        let (slot, _) = self
            .0
            .stage_finished
            .wait_timeout_while(slot, timeout, |slot| slot.download.is_some())
            .map_err(|_| "lock.poisoned")?;
        Ok(slot.download.is_none())
    }

    pub(crate) fn take_ready(
        &self,
        update: UpdateOnExit,
    ) -> Result<Option<StagedInstaller>, String> {
        let mut slot = self.0.slot.lock().map_err(|_| "lock.poisoned")?;
        if slot.recovery.is_some() {
            return Err(APPLY_FAILED.into());
        }
        match update {
            UpdateOnExit::Skip => {
                slot.ready = None;
                Ok(None)
            }
            _ if deferred(&slot) => Ok(slot.ready.take()),
            UpdateOnExit::Restart => Err(UNAVAILABLE.into()),
            UpdateOnExit::Install => Ok(None),
        }
    }

    pub(crate) fn has_deferred(&self) -> Result<bool, String> {
        let slot = self.0.slot.lock().map_err(|_| "lock.poisoned")?;
        Ok(deferred(&slot))
    }

    #[cfg(windows)]
    pub(crate) fn administrator_destination(
        &self,
        version: &str,
    ) -> Result<std::path::PathBuf, String> {
        let slot = self.0.slot.lock().map_err(|_| "lock.poisoned")?;
        Ok(administrator_ready(&slot, version)?
            .prepared
            .destination()
            .to_owned())
    }

    #[cfg(windows)]
    pub(crate) fn take_administrator(&self, version: &str) -> Result<StagedInstaller, String> {
        let mut slot = self.0.slot.lock().map_err(|_| "lock.poisoned")?;
        administrator_ready(&slot, version)?;
        slot.ready.take().ok_or_else(|| UNAVAILABLE.into())
    }
}

fn deferred(slot: &Slot) -> bool {
    slot.ready
        .as_ref()
        .is_some_and(|ready| ready.info.install_mode == AppUpdateInstallMode::Deferred)
}

#[cfg(windows)]
fn administrator_ready<'slot>(
    slot: &'slot Slot,
    version: &str,
) -> Result<&'slot StagedInstaller, String> {
    if slot.recovery.is_some() {
        return Err(APPLY_FAILED.into());
    }
    slot.ready
        .as_ref()
        .filter(|ready| {
            ready.info.install_mode == AppUpdateInstallMode::InstallAsAdministrator
                && ready.info.version == version
        })
        .ok_or_else(|| UNAVAILABLE.into())
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
}

impl StagedInstaller {
    pub(crate) fn prepare(
        download: VerifiedInstallerDownload,
        info: AppUpdateInfo,
    ) -> Result<Self, String> {
        let prepared = Prepared::prepare(download, &info)?;
        Ok(Self { info, prepared })
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

    #[cfg(windows)]
    pub(crate) fn start_administrator(self) -> Result<(), String> {
        self.prepared.start_admin()
    }
}

#[cfg(test)]
mod tests;
