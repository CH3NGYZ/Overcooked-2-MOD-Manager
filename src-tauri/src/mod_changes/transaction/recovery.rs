use super::types::RecoveryReport;
use super::{PreparedTransaction, check, durable, engine, paths, types::*};
use std::{fs, path::Path};
use uuid::Uuid;

pub(crate) fn has_pending(transaction_root: &Path) -> Result<bool, String> {
    paths::validate_absolute(transaction_root)?;
    if !transaction_root.exists() {
        return Ok(false);
    }
    for entry in fs::read_dir(transaction_root).map_err(paths::io_error)? {
        let entry = entry.map_err(paths::io_error)?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(paths::io_error)?;
        if paths::is_link(&metadata) {
            return Err(paths::UNSAFE.into());
        }
        if metadata.is_dir() {
            let journal = entry.path().join("journal.json");
            if journal.exists() {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// The caller holds the operation lock and checks that the game is stopped first.
/// Wrong roots, damaged journals, or changed targets retain all recovery material.
pub(crate) fn recover_all(
    transaction_root: &Path,
    roots: &TransactionRoots,
) -> Result<RecoveryReport, String> {
    paths::validate_roots(transaction_root, roots)?;
    let mut report = RecoveryReport::default();
    if !transaction_root.exists() {
        return Ok(report);
    }
    let mut entries = fs::read_dir(transaction_root)
        .map_err(paths::io_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(paths::io_error)?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let directory = entry.path();
        if paths::is_link(&fs::symlink_metadata(&directory).map_err(paths::io_error)?) {
            report.blocked.push(directory.display().to_string());
            continue;
        }
        if !directory.is_dir() {
            continue;
        }
        let id = entry.file_name().to_string_lossy().into_owned();
        if !directory.join("journal.json").exists() {
            // A preparation interrupted before publication touched no live targets.
            // Keep its staging for diagnosis; it must never be guessed into a rollback.
            continue;
        }
        let result = (|| {
            let journal = durable::read_journal(&directory)?;
            if journal.version != 1
                || journal.id != id
                || Uuid::parse_str(&id).is_err()
                || journal.roots != *roots
                || journal.created_directory_count > journal.created_directories.len()
            {
                return Err(paths::INVALID.to_string());
            }
            paths::validate_records(&journal.records, transaction_root, roots)?;
            for parent in &journal.created_directories {
                let absolute = parent.resolve(roots)?;
                if paths::overlaps(&absolute, transaction_root) {
                    return Err(paths::UNSAFE.to_string());
                }
            }
            let mut pending = PreparedTransaction { directory, journal };
            if matches!(
                pending.journal.state,
                JournalState::Committed | JournalState::Cleaned
            ) {
                pending.cleanup()?;
                report.cleaned.push(id.clone());
            } else {
                pending.rollback()?;
                report.recovered.push(id.clone());
            }
            Ok(())
        })();
        if let Err(error) = result {
            log::error!("批次 {id} 自动恢复被阻止，保留全部材料：{error}");
            report.blocked.push(id);
        }
    }
    Ok(report)
}

impl PreparedTransaction {
    pub(super) fn rollback(&mut self) -> Result<(), String> {
        if self.journal.state == JournalState::Prepared {
            // The initial validation can fail because the user changed a target. Pending
            // operations never owned that target and must not attempt to restore it.
            self.journal.state = JournalState::Cleaned;
            self.persist()?;
            return self.cleanup();
        }
        self.journal.state = JournalState::RecoveryRequired;
        self.persist()?;
        for index in (0..self.journal.records.len()).rev() {
            if matches!(
                self.journal.records[index].phase,
                RecordPhase::Pending | RecordPhase::Reverted
            ) {
                continue;
            }
            self.journal.records[index].phase = RecordPhase::Reverting;
            self.persist()?;
            if let Err(error) = self.revert_record(index) {
                self.journal.error = Some(error.clone());
                let _ = self.persist();
                return Err(error);
            }
            self.journal.records[index].phase = RecordPhase::Reverted;
            self.persist()?;
        }
        for parent in self.journal.created_directories[..self.journal.created_directory_count]
            .iter()
            .rev()
        {
            let path = parent.resolve(&self.journal.roots)?;
            match fs::remove_dir(&path) {
                Ok(()) => durable::sync_parent(&path)?,
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
                    ) =>
                {
                    ()
                }
                Err(error) => return Err(paths::io_error(error)),
            }
        }
        self.journal.state = JournalState::Cleaned;
        self.persist()?;
        self.cleanup()
    }

    fn revert_record(&self, index: usize) -> Result<(), String> {
        match &self.journal.records[index].operation {
            RecordedMutation::File {
                target,
                before,
                after,
                backup,
                temporary,
                require_single_link,
                ..
            } => {
                let target = check::resolve(target, &self.journal.roots, check::TARGET)?;
                let temporary = check::resolve(temporary, &self.journal.roots, check::STAGED)?;
                let actual = check::capture(&target, check::TARGET)?;
                if !engine::matches_writable(&actual, before)
                    && !engine::matches_writable(&actual, after)
                {
                    check::compare(&target, check::TARGET, &actual, after, false)?;
                }
                // A write-ahead record exclusively owns this UUID temporary. Partial copies
                // are removable, while live targets always require an exact fingerprint.
                remove_temporary(&temporary)?;
                if *require_single_link {
                    // A newly linked file can be left alone if this record never changed it.
                    if actual == *before {
                        return Ok(());
                    }
                    if actual != Fingerprint::Missing {
                        paths::guard_single_link(&target)?;
                    }
                }
                if engine::matches_writable(&actual, before) {
                    if let Some(permissions) = before.permissions() {
                        permissions
                            .restore(&target)
                            .map_err(|error| check::writing(&target, &error))?;
                    }
                    return Ok(());
                }
                if let Some(backup) = backup {
                    let backup = durable::private_path(&self.directory, backup, check::BACKUP)?;
                    let stored = check::capture(&backup, check::BACKUP)?;
                    check::compare(&backup, check::BACKUP, &stored, before, true)?;
                    durable::copy_verified(
                        &backup,
                        &temporary,
                        &stored,
                        check::BACKUP,
                        check::WRITE,
                    )?;
                    before
                        .permissions()
                        .ok_or(paths::INVALID)?
                        .restore(&temporary)
                        .map_err(|error| check::writing(&temporary, &error))?;
                    check::expect(&target, check::TARGET, &actual)?;
                    if *require_single_link && actual != Fingerprint::Missing {
                        paths::guard_single_link(&target)?;
                    }
                    if actual != Fingerprint::Missing {
                        let mut permissions = actual.permissions().ok_or(paths::INVALID)?.clone();
                        permissions.readonly = false;
                        #[cfg(unix)]
                        {
                            permissions.mode |= 0o200;
                        }
                        permissions
                            .restore(&target)
                            .map_err(|error| check::writing(&target, &error))?;
                    }
                    durable::rename(&temporary, &target, true)?;
                } else {
                    durable::remove_file_checked(&target, &actual, *require_single_link)?;
                }
                check::expect(&target, check::RESULT, before)?;
            }
            RecordedMutation::Move { from, to, before } => {
                let from = check::resolve(from, &self.journal.roots, check::TARGET)?;
                let to = check::resolve(to, &self.journal.roots, check::TARGET)?;
                let source = check::capture(&from, check::TARGET)?;
                let destination = check::capture(&to, check::TARGET)?;
                if source == *before && destination == Fingerprint::Missing {
                    return Ok(());
                }
                check::compare(&from, check::TARGET, &source, &Fingerprint::Missing, false)?;
                check::compare(&to, check::TARGET, &destination, before, false)?;
                durable::rename(&to, &from, false)?;
                check::expect(&from, check::RESULT, before)?;
            }
            operation @ RecordedMutation::Tree { .. } => {
                super::tree::revert(operation, &self.journal.roots)?
            }
        }
        Ok(())
    }

    pub(super) fn cleanup(&mut self) -> Result<(), String> {
        if !matches!(
            self.journal.state,
            JournalState::Committed | JournalState::Cleaned
        ) {
            return Err(paths::INVALID.into());
        }
        for index in 0..self.journal.records.len() {
            if matches!(
                self.journal.records[index].operation,
                RecordedMutation::Tree { .. }
            ) {
                let phase = self.journal.records[index].phase;
                if phase == RecordPhase::Pending {
                    continue;
                }
                if phase != RecordPhase::Purging {
                    super::tree::validate_cleanup(
                        &self.journal.records[index].operation,
                        &self.journal.roots,
                    )?;
                    self.journal.records[index].phase = RecordPhase::Purging;
                    self.persist()?;
                }
                super::tree::cleanup(&self.journal.records[index].operation, &self.journal.roots)?;
            }
        }
        // Keep journal until all private payloads are gone. Interrupted committed cleanup
        // remains an unambiguous committed transaction on the next startup.
        for name in ["before", "after"] {
            let path = self.directory.join(name);
            if path.exists() {
                remove_private_tree(&path)?;
            }
        }
        self.journal.state = JournalState::Cleaned;
        self.persist()?;
        remove_private_tree(&self.directory)
    }
}

fn remove_temporary(path: &Path) -> Result<(), String> {
    let fingerprint = Fingerprint::capture(path)?;
    match fingerprint {
        Fingerprint::Missing => Ok(()),
        Fingerprint::File { .. } => durable::remove_file_checked(path, &fingerprint, false),
        _ => Err(paths::UNSAFE.into()),
    }
}

pub(crate) fn remove_private_tree(path: &Path) -> Result<(), String> {
    paths::validate_absolute(path)?;
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(paths::io_error(error)),
    };
    if paths::is_link(&metadata) {
        return Err(paths::UNSAFE.into());
    }
    if metadata.is_dir() {
        if metadata.permissions().readonly() {
            let mut permissions = metadata.permissions();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                permissions.set_mode(permissions.mode() | 0o700);
            }
            #[cfg(not(unix))]
            permissions.set_readonly(false);
            fs::set_permissions(path, permissions).map_err(paths::io_error)?;
        }
        for entry in fs::read_dir(path).map_err(paths::io_error)? {
            remove_private_tree(&entry.map_err(paths::io_error)?.path())?;
        }
        fs::remove_dir(path).map_err(paths::io_error)?;
        durable::sync_parent(path)
    } else {
        let fingerprint = Fingerprint::capture(path)?;
        durable::remove_file_checked(path, &fingerprint, false)
    }
}
