use super::{
    PreparedTransaction, check, diagnostics, durable, engine, paths, recovery_control, types::*,
};
use std::{fs, path::Path};
use uuid::Uuid;

pub(super) const CLEANED_PREFIX: &str = ".cleaned-";

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
        let mut blocked = None;
        for index in (0..self.journal.records.len()).rev() {
            if matches!(
                self.journal.records[index].phase,
                RecordPhase::Pending | RecordPhase::Reverted
            ) {
                continue;
            }
            let _step = diagnostics::step(index, "rollback_record");
            self.journal.records[index].phase = RecordPhase::Reverting;
            self.persist().inspect_err(|error| {
                diagnostics::record(
                    &self.journal,
                    &self.directory,
                    index,
                    "rollback_journal_before",
                    error,
                );
            })?;
            if let Err(error) = self.revert_record(index) {
                diagnostics::record(
                    &self.journal,
                    &self.directory,
                    index,
                    "rollback_record",
                    &error,
                );
                if self.journal.records[index].operation.guards_original() {
                    blocked.get_or_insert(error);
                    continue;
                }
                return Err(error);
            }
            self.journal.records[index].phase = RecordPhase::Reverted;
            self.persist().inspect_err(|error| {
                diagnostics::record(
                    &self.journal,
                    &self.directory,
                    index,
                    "rollback_journal_after",
                    error,
                );
            })?;
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
                Err(error) => {
                    log::warn!(
                        target: diagnostics::TARGET,
                        "整链事务回滚目录失败：事务={}，目录={:?}，OS码={:?}，分类={:?}",
                        self.id(), diagnostics::path(&path), error.raw_os_error(), error.kind()
                    );
                    return Err(paths::io_error(error));
                }
            }
        }
        if let Some(error) = blocked {
            return Err(error);
        }
        self.journal.state = JournalState::Cleaned;
        self.persist()?;
        self.cleanup()
    }

    fn revert_record(&mut self, index: usize) -> Result<(), String> {
        if self.journal.records[index].operation.creates_file() {
            return self.revert_new_file(index);
        }
        let guarded = self.journal.records[index].operation.guards_original();
        let unpublished = guarded && self.discard_unpublished_file(index)?;
        let record = self.journal.records[index].clone();
        match &record.operation {
            RecordedMutation::File {
                target,
                before,
                original_identity,
                after,
                backup,
                temporary,
                require_single_link,
                ..
            } => {
                if unpublished {
                    return super::publication::restore_unpublished_permissions(
                        target,
                        before,
                        original_identity.as_ref().ok_or(paths::INVALID)?,
                        &self.journal.roots,
                    );
                }
                let target = check::resolve(target, &self.journal.roots, check::TARGET)?;
                let temporary = check::resolve(temporary, &self.journal.roots, check::STAGED)?;
                let actual = check::capture(&target, check::TARGET)?;
                let published_identity = match (original_identity, &record.publication) {
                    (Some(_), _) if actual == *before => {
                        return check::expect(&temporary, check::STAGED, &Fingerprint::Missing);
                    }
                    (Some(original), _)
                        if engine::matches_writable(&actual, before)
                            && super::publication::capture_identity(&target, check::TARGET)?
                                .as_ref()
                                == Some(original) =>
                    {
                        None
                    }
                    (
                        Some(_),
                        Some(
                            FilePublication::Staged(identity)
                            | FilePublication::Restoring {
                                published: identity,
                                ..
                            },
                        ),
                    ) => {
                        super::publication::expect_identity(&target, check::TARGET, identity)?;
                        Some(identity)
                    }
                    (Some(_), _) => return Err(paths::INVALID.into()),
                    _ => None,
                };
                if !engine::matches_writable(&actual, before)
                    && !engine::matches_writable(&actual, after)
                {
                    check::compare(&target, check::TARGET, &actual, after, false)?;
                }
                // A write-ahead record exclusively owns this UUID temporary. Partial copies
                // are removable, while live targets always require an exact fingerprint.
                if guarded {
                    check::expect(&temporary, check::STAGED, &Fingerprint::Missing)?;
                } else {
                    remove_temporary(&temporary, false)?;
                }
                if *require_single_link {
                    // A newly linked file can be left alone if this record never changed it.
                    if actual == *before {
                        return Ok(());
                    }
                    if actual != Fingerprint::Missing {
                        paths::guard_single_link(&target)?;
                    }
                }
                if published_identity.is_none() && engine::matches_writable(&actual, before) {
                    if let Some(identity) = original_identity {
                        super::publication::expect_identity(&target, check::TARGET, identity)?;
                    }
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
                        |file| {
                            if published_identity.is_some() {
                                self.record_temporary(index, &temporary, file, published_identity)?;
                            }
                            Ok(())
                        },
                    )?;
                    let restoring = match &self.journal.records[index].publication {
                        Some(FilePublication::Restoring { temporary, .. }) => Some(temporary),
                        _ => None,
                    };
                    super::publication::prepare_file(&temporary, before, restoring)?;
                    check::expect(&target, check::TARGET, &actual)?;
                    if *require_single_link && actual != Fingerprint::Missing {
                        paths::guard_single_link(&target)?;
                    }
                    if let Some(identity) = published_identity {
                        super::publication::expect_identity(&target, check::TARGET, identity)?;
                    }
                    if let Some(identity) = restoring {
                        super::publication::expect_identity(&temporary, check::STAGED, identity)?;
                        paths::guard_single_link(&temporary)?;
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
                    if let Some(identity) = published_identity {
                        super::publication::expect_identity(&target, check::TARGET, identity)?;
                    }
                    durable::rename(&temporary, &target, actual != Fingerprint::Missing)?;
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
                let _step = diagnostics::step(index, "cleanup_record");
                let phase = self.journal.records[index].phase;
                if phase == RecordPhase::Pending {
                    continue;
                }
                if phase != RecordPhase::Purging {
                    super::tree::validate_cleanup(
                        &self.journal.records[index].operation,
                        &self.journal.roots,
                    )
                    .inspect_err(|error| {
                        diagnostics::record(
                            &self.journal,
                            &self.directory,
                            index,
                            "validate_cleanup",
                            error,
                        );
                    })?;
                    self.journal.records[index].phase = RecordPhase::Purging;
                    self.persist()?;
                }
                super::tree::cleanup(&self.journal.records[index].operation, &self.journal.roots)
                    .inspect_err(|error| {
                    diagnostics::record(&self.journal, &self.directory, index, "clean_tree", error);
                })?;
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
        for entry in fs::read_dir(&self.directory)
            .map_err(|error| paths::io_at(&self.directory, "list_cleaned_transaction", error))?
        {
            let entry = entry.map_err(paths::io_error)?;
            let name = entry.file_name();
            let name = name.to_str().ok_or(paths::UNSAFE)?;
            if matches!(
                name,
                durable::JOURNAL_FILE | recovery_control::DECISION_FILE
            ) {
                continue;
            }
            if !is_record_temporary(name) {
                return Err(paths::UNSAFE.into());
            }
            remove_temporary(&entry.path(), true)?;
        }
        let parent = self.directory.parent().ok_or(paths::UNSAFE)?;
        let cleaned = parent.join(format!("{CLEANED_PREFIX}{}", self.id()));
        match durable::rename_durable(&self.directory, &cleaned, false, check::WRITE) {
            Err(durable::RenameFailure::Unchanged(error)) => return Err(error),
            Err(durable::RenameFailure::Sync(error)) => {
                diagnostics::transaction(
                    &self.journal,
                    &cleaned,
                    "sync_cleaned_transaction",
                    &error,
                );
                return Ok(());
            }
            Ok(()) => (),
        }
        if let Err(error) = remove_temporary(&cleaned.join(recovery_control::DECISION_FILE), true)
            .and_then(|()| remove_temporary(&cleaned.join(durable::JOURNAL_FILE), true))
            .and_then(|()| {
                fs::remove_dir(&cleaned)
                    .map_err(|error| paths::io_at(&cleaned, "remove_cleaned_transaction", error))
            })
        {
            diagnostics::transaction(
                &self.journal,
                &cleaned,
                "remove_cleaned_transaction",
                &error,
            );
        }
        Ok(())
    }
}

fn is_record_temporary(name: &str) -> bool {
    ["journal-", "journal.json-", "recovery-decision.json-"]
        .into_iter()
        .filter_map(|prefix| name.strip_prefix(prefix)?.strip_suffix(".tmp"))
        .any(|id| Uuid::parse_str(id).is_ok_and(|uuid| uuid.to_string() == id))
}

fn remove_temporary(path: &Path, require_single_link: bool) -> Result<(), String> {
    let fingerprint = Fingerprint::capture(path)?;
    match fingerprint {
        Fingerprint::Missing => Ok(()),
        Fingerprint::File { .. } => {
            durable::remove_file_checked(path, &fingerprint, require_single_link)
        }
        _ => Err(paths::UNSAFE.into()),
    }
}

pub(crate) fn remove_private_tree(path: &Path) -> Result<(), String> {
    paths::validate_absolute(path)?;
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(paths::io_at(path, "private_cleanup_metadata", error)),
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
            fs::set_permissions(path, permissions)
                .map_err(|error| paths::io_at(path, "private_cleanup_permissions", error))?;
        }
        let read_error = |error| paths::io_at(path, "private_cleanup_list", error);
        for entry in fs::read_dir(path).map_err(read_error)? {
            remove_private_tree(&entry.map_err(read_error)?.path())?;
        }
        fs::remove_dir(path).map_err(|error| paths::io_at(path, "private_cleanup_rmdir", error))?;
        durable::sync_parent(path)
    } else {
        let fingerprint = Fingerprint::capture(path)?;
        durable::remove_file_checked(path, &fingerprint, false)
    }
}
