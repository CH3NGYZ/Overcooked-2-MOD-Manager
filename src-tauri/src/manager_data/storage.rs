use super::{BUSY, UNSAFE, failed};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ResetPhase {
    Pending,
    Deleting,
    Copying,
    Verified,
    Switched,
    Cleaning,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct ResetRecord {
    pub token: uuid::Uuid,
    pub owner_pid: u32,
    pub owner_started: u64,
    pub phase: ResetPhase,
    pub error: Option<String>,
    #[serde(default)]
    pub reported: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed_from: Option<ResetPhase>,
    #[serde(default)]
    pub action: DataAction,
    #[serde(default)]
    pub fingerprint: Option<crate::mod_changes::transaction::Fingerprint>,
    #[serde(default)]
    pub data_root: Option<PathBuf>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum DataAction {
    #[default]
    Clear,
    Migrate {
        source: PathBuf,
        target: PathBuf,
    },
}

impl ResetRecord {
    pub(super) fn pending(&self) -> bool {
        !matches!(self.phase, ResetPhase::Completed | ResetPhase::Failed)
    }
}

pub(super) struct ResetPaths {
    pub root: PathBuf,
    pub parent: PathBuf,
}

impl ResetPaths {
    #[cfg(test)]
    pub(super) fn new(root: &Path) -> Result<Self, String> {
        Self::with_control(root, root.parent().ok_or(UNSAFE)?)
    }

    pub(super) fn current() -> Result<Self, String> {
        let pointer = crate::user_paths::location_file()?;
        Self::with_control(&crate::user_paths::root()?, pointer.parent().ok_or(UNSAFE)?)
    }

    pub(super) fn with_control(root: &Path, control: &Path) -> Result<Self, String> {
        if !root.is_absolute() || root.file_name().is_none_or(|name| name != "OC2ModManager") {
            return Err(UNSAFE.into());
        }
        // Resolving first would hide a link at the destructive root boundary.
        reject_link_if_present(root)?;
        fs::create_dir_all(control).map_err(failed)?;
        reject_link_if_present(control)?;
        let parent = fs::canonicalize(control).map_err(failed)?;
        let resolved = crate::game::resolve_existing_ancestor(root).map_err(failed)?;
        if resolved
            .file_name()
            .is_none_or(|name| name != "OC2ModManager")
        {
            return Err(UNSAFE.into());
        }
        Ok(Self {
            root: resolved,
            parent,
        })
    }

    fn journal(&self) -> PathBuf {
        self.parent.join(".OC2ModManager-reset.json")
    }

    pub(super) fn ready(&self, token: uuid::Uuid) -> PathBuf {
        self.parent
            .join(format!(".OC2ModManager-reset-{token}.ready"))
    }

    fn quarantine(&self, token: uuid::Uuid) -> PathBuf {
        self.root
            .parent()
            .expect("validated absolute data root")
            .join(format!(".OC2ModManager-clearing-{token}"))
    }

    pub(super) fn open_lock(&self) -> Result<File, String> {
        self.open_named_lock(".OC2ModManager-session.lock")
    }

    fn open_named_lock(&self, name: &str) -> Result<File, String> {
        let path = self.parent.join(name);
        reject_link_if_present(&path)?;
        OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)
            .map_err(failed)
    }

    pub(super) fn record(&self) -> Result<Option<ResetRecord>, String> {
        let path = self.journal();
        reject_link_if_present(&path)?;
        match fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(failed),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(failed(error)),
        }
    }

    pub(super) fn write_record(&self, record: &ResetRecord) -> Result<(), String> {
        let _lock = self.lock_record()?;
        self.write_locked_record(record)
    }

    fn lock_record(&self) -> Result<File, String> {
        let lock = self.open_named_lock(".OC2ModManager-journal.lock")?;
        wait_for_lock(&lock, false, Duration::from_secs(2))?;
        Ok(lock)
    }

    pub(super) fn claim_failure_notice(
        &self,
        token: uuid::Uuid,
    ) -> Result<Option<ResetRecord>, String> {
        let _lock = self.lock_record()?;
        let Some(mut record) = self.record()?.filter(|record| {
            record.token == token && record.phase == ResetPhase::Failed && !record.reported
        }) else {
            return Ok(None);
        };
        record.reported = true;
        self.write_locked_record(&record)?;
        Ok(Some(record))
    }

    fn write_locked_record(&self, record: &ResetRecord) -> Result<(), String> {
        reject_link_if_present(&self.journal())?;
        let mut temp = tempfile::NamedTempFile::new_in(&self.parent).map_err(failed)?;
        temp.write_all(&serde_json::to_vec(record).map_err(failed)?)
            .map_err(failed)?;
        temp.as_file().sync_all().map_err(failed)?;
        let temp = temp.into_temp_path();
        crate::mod_changes::transaction::durable_rename(&temp, &self.journal(), true)
            .map_err(failed)?;
        Ok(())
    }

    pub(super) fn validate_root(&self) -> Result<(), String> {
        crate::install_guard::validate_install_dir(&self.root).map_err(|_| UNSAFE)?;
        reject_link_if_present(&self.root)?;
        match fs::symlink_metadata(&self.root) {
            Ok(metadata) if metadata.is_dir() => Ok(()),
            Ok(_) => Err(UNSAFE.into()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(failed(error)),
        }
    }

    pub(super) fn mark_failed(&self, mut record: ResetRecord, error: &str) -> Result<(), String> {
        record.failed_from = Some(record.phase);
        record.phase = ResetPhase::Failed;
        record.error = Some(error.to_string());
        record.reported = false;
        self.write_record(&record)
    }

    pub(super) fn protect(&self, path: &Path) -> Result<(), String> {
        let protected = crate::game::resolve_existing_ancestor(path).map_err(failed)?;
        let root = normalized(&self.root);
        let protected = normalized(&protected);
        if protected.starts_with(&root) || root.starts_with(&protected) {
            return Err(UNSAFE.into());
        }
        Ok(())
    }

    /// Caller owns the exclusive session lock. An interrupted delete resumes only
    /// its UUID-owned quarantine, never an arbitrary path from the journal.
    pub(super) fn finish(&self, mut record: ResetRecord) -> Result<(), String> {
        if !record.pending() {
            return Ok(());
        }
        if let DataAction::Migrate { source, target } = &record.action {
            return super::migration::finish(self, record.clone(), source, target);
        }
        if record
            .data_root
            .as_ref()
            .is_some_and(|root| normalized(root) != normalized(&self.root))
        {
            self.mark_failed(record, UNSAFE)?;
            return Err(UNSAFE.into());
        }
        let quarantine = self.quarantine(record.token);
        let result = (|| {
            reject_link_if_present(&quarantine)?;
            if record.phase == ResetPhase::Pending {
                self.validate_root()?;
                if quarantine.try_exists().map_err(failed)? {
                    return Err(UNSAFE.into());
                }
                record.phase = ResetPhase::Deleting;
                self.write_record(&record)?;
                if self.root.try_exists().map_err(failed)? {
                    fs::rename(&self.root, &quarantine).map_err(failed)?;
                }
            } else if !quarantine.try_exists().map_err(failed)? {
                // Deleting can mean the old root was already removed before a
                // crash. A root now at this path may belong to a fresh session;
                // without the UUID quarantine we cannot prove ownership of it.
                match fs::symlink_metadata(&self.root) {
                    Ok(_) => return Err(UNSAFE.into()),
                    Err(error) if error.kind() == ErrorKind::NotFound => {}
                    Err(error) => return Err(failed(error)),
                }
            }
            // std removes links themselves and does not follow directory links.
            // In particular a user-created link into BepInEx is never traversed.
            match fs::remove_dir_all(&quarantine) {
                Ok(()) => {}
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => return Err(failed(error)),
            }
            record.phase = ResetPhase::Completed;
            record.error = None;
            self.write_record(&record)
        })();
        if let Err(error) = result {
            // Retain whatever remains at the original path and never retry a
            // failed reset automatically on every subsequent startup.
            if fs::symlink_metadata(&self.root)
                .is_err_and(|error| error.kind() == ErrorKind::NotFound)
                && reject_link_if_present(&quarantine).is_ok()
                && quarantine.is_dir()
            {
                let _ = fs::rename(&quarantine, &self.root);
            }
            self.mark_failed(record, &error)?;
            return Err(error);
        }
        Ok(())
    }
}

pub(super) fn normalized(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let text = path.to_string_lossy().to_lowercase();
        if let Some(unc) = text.strip_prefix("\\\\?\\unc\\") {
            PathBuf::from(format!("\\\\{unc}"))
        } else {
            PathBuf::from(text.strip_prefix("\\\\?\\").unwrap_or(&text))
        }
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

pub(super) fn reject_link_if_present(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 {
                    return Err(UNSAFE.into());
                }
            }
            if metadata.file_type().is_symlink() {
                return Err(UNSAFE.into());
            }
            Ok(())
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(failed(error)),
    }
}

pub(super) fn wait_for_lock(file: &File, shared: bool, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        let result = if shared {
            file.try_lock_shared()
        } else {
            file.try_lock()
        };
        match result {
            Ok(()) => return Ok(()),
            Err(fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(fs::TryLockError::WouldBlock) => return Err(BUSY.into()),
            Err(fs::TryLockError::Error(error)) => return Err(failed(error)),
        }
    }
}
