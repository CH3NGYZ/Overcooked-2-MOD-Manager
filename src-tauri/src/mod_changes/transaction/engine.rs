use super::{PreparedTransaction, durable, paths, types::*};
use std::{fs, path::Path};

pub(in crate::mod_changes) enum RecordProgress {
    Applying(usize),
    Processed(usize),
    Failed(usize),
    Committed,
}

pub(super) fn verify_file_before(
    path: &Path,
    before: &Fingerprint,
    require_single_link: bool,
) -> Result<(), String> {
    if Fingerprint::capture(path)? != *before {
        return Err(paths::MODIFIED.into());
    }
    if require_single_link {
        paths::guard_single_link(path)?;
    }
    Ok(())
}

pub(super) fn verify_before(record: &Record, roots: &TransactionRoots) -> Result<(), String> {
    match &record.operation {
        RecordedMutation::File {
            target,
            before,
            require_single_link,
            ..
        } => verify_file_before(&target.resolve(roots)?, before, *require_single_link)?,
        RecordedMutation::Tree {
            target,
            before,
            backup,
            temporary,
            ..
        } => {
            if Fingerprint::capture(&target.resolve(roots)?)? != *before {
                return Err(paths::MODIFIED.into());
            }
            if Fingerprint::capture(&backup.resolve(roots)?)? != Fingerprint::Missing
                || Fingerprint::capture(&temporary.resolve(roots)?)? != Fingerprint::Missing
            {
                return Err(paths::CONFLICT.into());
            }
        }
        RecordedMutation::Move { from, to, before } => {
            if Fingerprint::capture(&from.resolve(roots)?)? != *before {
                return Err(paths::MODIFIED.into());
            }
            if Fingerprint::capture(&to.resolve(roots)?)? != Fingerprint::Missing {
                return Err(paths::CONFLICT.into());
            }
        }
    }
    Ok(())
}

fn verify_after(record: &Record, roots: &TransactionRoots) -> Result<(), String> {
    let valid = match &record.operation {
        RecordedMutation::File { target, after, .. }
        | RecordedMutation::Tree { target, after, .. } => {
            Fingerprint::capture(&target.resolve(roots)?)? == *after
        }
        RecordedMutation::Move { from, to, before } => {
            Fingerprint::capture(&from.resolve(roots)?)? == Fingerprint::Missing
                && Fingerprint::capture(&to.resolve(roots)?)? == *before
        }
    };
    if valid {
        Ok(())
    } else {
        Err(paths::MODIFIED.into())
    }
}

impl PreparedTransaction {
    pub(crate) fn id(&self) -> &str {
        &self.journal.id
    }

    #[cfg(test)]
    pub(crate) fn state(&self) -> JournalState {
        self.journal.state
    }

    #[cfg(test)]
    pub(crate) fn apply(self) -> Result<String, String> {
        self.apply_with_cancel(|| false, |_| {})
    }

    /// The caller holds the shared operation lock and has rechecked game/source revisions.
    /// Cancellation is observed between atomic filesystem operations; it rolls back the batch.
    pub(in crate::mod_changes) fn apply_with_cancel(
        mut self,
        cancelled: impl Fn() -> bool,
        mut observe: impl FnMut(RecordProgress),
    ) -> Result<String, String> {
        let result = self.apply_inner(&cancelled, &mut observe);
        if let Err(error) = result {
            if self.journal.state == JournalState::Committed {
                // A failed durable commit write can have reached disk. Only restart
                // recovery may decide from the published journal whether to undo it.
                log::error!("批次提交刷盘结果待核实，保留恢复材料：{error}");
                return Err("recycle.partialFailure".into());
            }
            self.journal.error = Some(error.clone());
            if let Err(restore_error) = self.rollback() {
                log::error!("链式变更失败且待恢复：安装={error}，恢复={restore_error}");
                return Err("recycle.partialFailure".into());
            }
            return Err(error);
        }
        let id = self.journal.id.clone();
        observe(RecordProgress::Committed);
        // A durable commit is final. Cleanup failure must never turn into a rollback.
        if let Err(error) = self.cleanup() {
            log::warn!("批次已提交，恢复材料待清理：{error}");
        }
        Ok(id)
    }

    fn apply_inner(
        &mut self,
        cancelled: &impl Fn() -> bool,
        observe: &mut impl FnMut(RecordProgress),
    ) -> Result<(), String> {
        if self.journal.state != JournalState::Prepared {
            return Err(paths::INVALID.into());
        }
        for (index, record) in self.journal.records.iter().enumerate() {
            verify_before(record, &self.journal.roots)
                .inspect_err(|_| observe(RecordProgress::Failed(index)))?;
        }
        for parent in &self.journal.created_directories {
            if Fingerprint::capture(&parent.resolve(&self.journal.roots)?)? != Fingerprint::Missing
            {
                return Err(paths::MODIFIED.into());
            }
        }
        if cancelled() {
            return Err("download.cancelled".into());
        }
        self.journal.state = JournalState::Applying;
        self.persist()?;
        for index in 0..self.journal.records.len() {
            if cancelled() {
                return Err("download.cancelled".into());
            }
            self.apply_record(index, || observe(RecordProgress::Applying(index)))
                .inspect_err(|_| observe(RecordProgress::Failed(index)))?;
            observe(RecordProgress::Processed(index));
        }
        for (index, record) in self.journal.records.iter().enumerate() {
            verify_after(record, &self.journal.roots)
                .inspect_err(|_| observe(RecordProgress::Failed(index)))?;
        }
        self.journal.state = JournalState::Committed;
        self.persist()
    }

    pub(super) fn persist(&self) -> Result<(), String> {
        durable::write_journal(&self.directory, &self.journal)
    }

    pub(super) fn apply_record(
        &mut self,
        index: usize,
        started: impl FnOnce(),
    ) -> Result<(), String> {
        verify_before(&self.journal.records[index], &self.journal.roots)?;
        self.journal.records[index].phase = RecordPhase::Applying;
        self.persist()?;
        started();
        let destination = match &self.journal.records[index].operation {
            RecordedMutation::File { target, .. } | RecordedMutation::Tree { target, .. } => target,
            RecordedMutation::Move { to, .. } => to,
        };
        let root = destination.root;
        let relative = paths::key(Path::new(&destination.relative));
        while self.journal.created_directory_count < self.journal.created_directories.len() {
            let path = &self.journal.created_directories[self.journal.created_directory_count];
            // Preparation collects parents in first-use order. Keep that prefix
            // for recovery, but don't create directories for a later record.
            if path.root != root
                || !relative
                    .strip_prefix(&paths::key(Path::new(&path.relative)))
                    .is_some_and(|rest| rest.starts_with('/'))
            {
                break;
            }
            let absolute = path.resolve(&self.journal.roots)?;
            // create_dir is exclusive: an externally created directory never
            // becomes ours merely because the planned target was missing.
            fs::create_dir(&absolute).map_err(paths::io_error)?;
            self.journal.created_directory_count += 1;
            durable::sync_parent(&absolute)?;
            self.persist()?;
        }
        let record = &self.journal.records[index];
        match &record.operation {
            RecordedMutation::File {
                target,
                before,
                after,
                payload,
                temporary,
                require_single_link,
                ..
            } => {
                let target = target.resolve(&self.journal.roots)?;
                let temporary = temporary.resolve(&self.journal.roots)?;
                if let Some(payload) = payload {
                    let payload = durable::private_path(&self.directory, payload)?;
                    let payload_fingerprint = Fingerprint::capture(&payload)?;
                    if !payload_fingerprint.same_content(after) {
                        return Err(paths::MODIFIED.into());
                    }
                    durable::copy_verified(&payload, &temporary, &payload_fingerprint)?;
                    after
                        .permissions()
                        .ok_or(paths::INVALID)?
                        .restore(&temporary)?;
                    if Fingerprint::capture(&temporary)? != *after {
                        return Err(paths::MODIFIED.into());
                    }
                    verify_file_before(&target, before, *require_single_link)?;
                    make_writable(&target, before)?;
                    durable::rename(&temporary, &target, true)?;
                } else {
                    durable::remove_file_checked(&target, before, *require_single_link)?;
                }
            }
            RecordedMutation::Move { from, to, .. } => {
                durable::rename(
                    &from.resolve(&self.journal.roots)?,
                    &to.resolve(&self.journal.roots)?,
                    false,
                )?;
            }
            operation @ RecordedMutation::Tree { .. } => {
                super::tree::apply(&self.directory, operation, &self.journal.roots)?
            }
        }
        verify_after(record, &self.journal.roots)?;
        self.journal.records[index].phase = RecordPhase::Applied;
        self.persist()
    }
}

fn make_writable(path: &Path, expected: &Fingerprint) -> Result<(), String> {
    if expected == &Fingerprint::Missing {
        return Ok(());
    }
    let permissions = expected.permissions().ok_or(paths::INVALID)?;
    if !permissions.readonly {
        return Ok(());
    }
    let mut writable = permissions.clone();
    writable.readonly = false;
    #[cfg(unix)]
    {
        writable.mode |= 0o200;
    }
    writable.restore(path)
}

pub(super) fn matches_writable(actual: &Fingerprint, original: &Fingerprint) -> bool {
    if actual == original {
        return true;
    }
    let mut writable = original.clone();
    if let Fingerprint::File { permissions, .. } = &mut writable {
        permissions.readonly = false;
        #[cfg(unix)]
        {
            permissions.mode |= 0o200;
        }
    }
    actual == &writable
}
