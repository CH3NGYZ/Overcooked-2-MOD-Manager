use super::types::RecoveryReport;
use super::{PreparedTransaction, check, diagnostics, durable, engine, paths, types::*};
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
        let journal_path = directory.join("journal.json");
        if !journal_path.exists() {
            // A preparation interrupted before publication touched no live targets.
            // Keep its staging for diagnosis; it must never be guessed into a rollback.
            continue;
        }
        let result = (|| {
            let _scope = diagnostics::enter(&id);
            let journal = durable::read_journal(&directory)?;
            let invalid_field = if !matches!(journal.version, 1 | JOURNAL_VERSION) {
                Some("schema_version")
            } else if journal.id != id || Uuid::parse_str(&id).is_err() {
                Some("transaction_id")
            } else if journal.roots != *roots {
                Some("roots")
            } else if journal.created_directory_count > journal.created_directories.len() {
                Some("parent_count")
            } else if journal.version == JOURNAL_VERSION
                && journal
                    .records
                    .iter()
                    .any(|record| record.publication.is_some() != record.operation.creates_file())
            {
                Some("file_publication")
            } else {
                None
            };
            if let Some(field) = invalid_field {
                log::warn!(
                    target: diagnostics::TARGET,
                    "整链恢复记录校验失败：{}路径={:?}，字段={field}，实际版本={}，支持版本=1/{JOURNAL_VERSION}，ID与目录一致={}，父目录已建={}，计划父目录={}",
                    diagnostics::Location, diagnostics::path(&journal_path), journal.version,
                    journal.id == id, journal.created_directory_count, journal.created_directories.len(),
                );
                if field == "roots" {
                    log::warn!(
                        target: diagnostics::TARGET,
                        "整链恢复根目录不符：{}记录游戏根={:?}，当前游戏根={:?}，记录数据根={:?}，当前数据根={:?}",
                        diagnostics::Location,
                        diagnostics::path(&journal.roots.game_root), diagnostics::path(&roots.game_root),
                        diagnostics::path(&journal.roots.data_root), diagnostics::path(&roots.data_root),
                    );
                }
                return Err(paths::INVALID.to_string());
            }
            paths::validate_records(&journal.records, transaction_root, roots).inspect_err(|error| {
                log::warn!(
                    target: diagnostics::TARGET,
                    "整链恢复变更记录校验失败：{}路径={:?}，记录数={}，原因={:?}",
                    diagnostics::Location, diagnostics::path(&journal_path), journal.records.len(),
                    crate::locale_tag::diagnostic_tag(error),
                );
            })?;
            for parent in &journal.created_directories {
                let absolute = parent.resolve(roots).inspect_err(|error| {
                    log::warn!(
                        target: diagnostics::TARGET,
                        "整链恢复父目录无效：{}父目录={:?}，原因={:?}",
                        diagnostics::Location, diagnostics::endpoint(roots, parent),
                        crate::locale_tag::diagnostic_tag(error),
                    );
                })?;
                if paths::overlaps(&absolute, transaction_root) {
                    log::warn!(
                        target: diagnostics::TARGET,
                        "整链恢复父目录与事务根重叠：{}父目录={:?}，事务根={:?}",
                        diagnostics::Location, diagnostics::path(&absolute), diagnostics::path(transaction_root),
                    );
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
            log::error!(
                "批次 {:?} 自动恢复被阻止，保留恢复材料：记录={:?}，原因={:?}",
                crate::file_check::log_field(&id),
                diagnostics::path(&journal_path),
                crate::locale_tag::diagnostic_tag(&error)
            );
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
                self.journal.error = Some(error.clone());
                let _ = self.persist();
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
        self.journal.state = JournalState::Cleaned;
        self.persist()?;
        self.cleanup()
    }

    fn revert_record(&mut self, index: usize) -> Result<(), String> {
        let record = &self.journal.records[index];
        if record.operation.creates_file() {
            return self.revert_new_file(index);
        }
        match &record.operation {
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
                        |_| Ok(()),
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
