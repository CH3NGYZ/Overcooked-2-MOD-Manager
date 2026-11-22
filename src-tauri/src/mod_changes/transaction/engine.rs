use super::{PreparedTransaction, check, durable, paths, types::*};
use crate::{directory_watch::CommittedPaths, file_check, file_metadata::Stamp};
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
    check::expect(path, check::TARGET, before)?;
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
        } => verify_file_before(
            &check::resolve(target, roots, check::TARGET)?,
            before,
            *require_single_link,
        )?,
        RecordedMutation::Tree {
            target,
            before,
            backup,
            temporary,
            ..
        } => {
            check::expect(
                &check::resolve(target, roots, check::TARGET)?,
                check::TARGET,
                before,
            )?;
            check::expect(
                &check::resolve(backup, roots, check::BACKUP)?,
                check::BACKUP,
                &Fingerprint::Missing,
            )?;
            check::expect(
                &check::resolve(temporary, roots, check::STAGED)?,
                check::STAGED,
                &Fingerprint::Missing,
            )?;
        }
        RecordedMutation::Move { from, to, before } => {
            check::expect(
                &check::resolve(from, roots, check::TARGET)?,
                check::TARGET,
                before,
            )?;
            check::expect(
                &check::resolve(to, roots, check::TARGET)?,
                check::TARGET,
                &Fingerprint::Missing,
            )?;
        }
    }
    Ok(())
}

fn verify_after(record: &Record, roots: &TransactionRoots) -> Result<(), String> {
    check_after(record, roots, |path, _, _| Fingerprint::capture(path))
}

fn check_after(
    record: &Record,
    roots: &TransactionRoots,
    mut capture: impl FnMut(&Path, RootKind, bool) -> Result<Fingerprint, String>,
) -> Result<(), String> {
    let mut verify = |target: &TransactionPath, tree: bool, expected: &Fingerprint| {
        let path = check::resolve(target, roots, check::RESULT)?;
        let observed = capture(&path, target.root, tree)
            .map_err(|error| check::context(check::RESULT, &path, &error))?;
        check::compare(&path, check::RESULT, &observed, expected, false)
    };
    match &record.operation {
        RecordedMutation::File { target, after, .. } => verify(target, false, after),
        RecordedMutation::Tree { target, after, .. } => verify(target, true, after),
        RecordedMutation::Move { from, to, before } => {
            let tree = matches!(before, Fingerprint::Directory { .. });
            verify(from, tree, &Fingerprint::Missing)?;
            verify(to, tree, before)
        }
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
    pub(crate) fn apply(self) -> Result<CommittedPaths, String> {
        self.apply_with_cancel(|| false, |_| {})
    }

    #[cfg(test)]
    pub(in crate::mod_changes) fn apply_with_cancel(
        self,
        cancelled: impl Fn() -> bool,
        observe: impl FnMut(RecordProgress),
    ) -> Result<CommittedPaths, String> {
        self.apply_checked(cancelled, || Ok(()), || Ok(()), observe)
    }

    /// Run domain checks around live writes before publishing a durable commit.
    /// Cancellation is observed between atomic filesystem operations and rolls back the batch.
    pub(in crate::mod_changes) fn apply_checked(
        mut self,
        cancelled: impl Fn() -> bool,
        before_apply: impl FnOnce() -> Result<(), String>,
        before_commit: impl FnOnce() -> Result<(), String>,
        mut observe: impl FnMut(RecordProgress),
    ) -> Result<CommittedPaths, String> {
        let result = self.apply_inner(&cancelled, before_apply, before_commit, &mut observe);
        let mut committed = match result {
            Ok(committed) => committed,
            Err(error) => {
                if self.journal.state == JournalState::Committed {
                    // A failed durable commit write can have reached disk. Only restart
                    // recovery may decide from the published journal whether to undo it.
                    log::error!("批次提交刷盘结果待核实，保留恢复材料：{error}");
                    return Err(check::recovery_pending(&error));
                }
                self.journal.error = Some(error.clone());
                if let Err(restore_error) = self.rollback() {
                    log::error!("链式变更失败且待恢复：安装={error}，恢复={restore_error}");
                    return Err(check::recovery_pending(&restore_error));
                }
                log::warn!("整链事务已回滚：事务={}，原因={error}", self.journal.id);
                return Err(error);
            }
        };
        observe(RecordProgress::Committed);
        // A durable commit is final. Cleanup failure must never turn into a rollback.
        if let Err(error) = self.cleanup() {
            log::warn!("批次已提交，恢复材料待清理：{error}");
        }
        self.finish_watch_paths(&mut committed);
        Ok(committed)
    }

    fn apply_inner(
        &mut self,
        cancelled: &impl Fn() -> bool,
        before_apply: impl FnOnce() -> Result<(), String>,
        before_commit: impl FnOnce() -> Result<(), String>,
        observe: &mut impl FnMut(RecordProgress),
    ) -> Result<CommittedPaths, String> {
        if self.journal.state != JournalState::Prepared {
            return Err(paths::INVALID.into());
        }
        for (index, record) in self.journal.records.iter().enumerate() {
            verify_before(record, &self.journal.roots)
                .inspect_err(|_| observe(RecordProgress::Failed(index)))?;
        }
        for parent in &self.journal.created_directories {
            check::expect(
                &check::resolve(parent, &self.journal.roots, check::TARGET)?,
                check::TARGET,
                &Fingerprint::Missing,
            )?;
        }
        if cancelled() {
            return Err("download.cancelled".into());
        }
        before_apply()?;
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
        let mut committed = CommittedPaths::default();
        for (index, record) in self.journal.records.iter().enumerate() {
            check_after(record, &self.journal.roots, |path, root, tree| {
                if root != RootKind::Game {
                    return Fingerprint::capture(path);
                }
                let actual = Fingerprint::capture_for_watch(path, &mut committed)?;
                if actual == Fingerprint::Missing {
                    committed.missing(path.to_path_buf(), tree);
                }
                Ok(actual)
            })
            .inspect_err(|_| observe(RecordProgress::Failed(index)))?;
        }
        before_commit()?;
        self.journal.state = JournalState::Committed;
        self.persist()?;
        Ok(committed)
    }

    /// Only now do we know the final state of private siblings, and the change
    /// times of parent directories affected by creating/removing those names.
    fn finish_watch_paths(&self, committed: &mut CommittedPaths) {
        let roots = &self.journal.roots;
        for record in &self.journal.records {
            let (private, tree) = match &record.operation {
                RecordedMutation::File { temporary, .. } => ([Some(temporary), None], false),
                RecordedMutation::Tree {
                    temporary, backup, ..
                } => ([Some(temporary), Some(backup)], true),
                RecordedMutation::Move { .. } => continue,
            };
            for path in private.into_iter().flatten() {
                if path.root == RootKind::Game
                    && let Ok(path) = path.resolve(roots)
                {
                    committed.missing(path, tree);
                }
            }
        }
        for parent in &self.journal.created_directories[..self.journal.created_directory_count] {
            if parent.root == RootKind::Game
                && let Ok(path) = parent.resolve(roots)
                && let Ok(stamp) = Stamp::capture(&path)
            {
                committed.present(&path, stamp);
            }
        }
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
            fs::create_dir(&absolute).map_err(|error| {
                file_check::report_io(check::WRITE, &absolute, error, file_check::WRITE)
            })?;
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
                    let payload = durable::private_path(&self.directory, payload, check::STAGED)?;
                    let payload_fingerprint = check::capture(&payload, check::STAGED)?;
                    check::compare(&payload, check::STAGED, &payload_fingerprint, after, true)?;
                    durable::copy_verified(
                        &payload,
                        &temporary,
                        &payload_fingerprint,
                        check::STAGED,
                        check::WRITE,
                    )?;
                    after
                        .permissions()
                        .ok_or(paths::INVALID)?
                        .restore(&temporary)
                        .map_err(|error| check::writing(&temporary, &error))?;
                    check::expect(&temporary, check::STAGED, after)?;
                    verify_file_before(&target, before, *require_single_link)?;
                    make_writable(&target, before)
                        .map_err(|error| check::writing(&target, &error))?;
                    durable::rename_checked(&temporary, &target, true, check::WRITE)?;
                } else {
                    durable::remove_file_checked(&target, before, *require_single_link)?;
                }
            }
            RecordedMutation::Move { from, to, .. } => {
                let target = to.resolve(&self.journal.roots)?;
                durable::rename_checked(
                    &from.resolve(&self.journal.roots)?,
                    &target,
                    false,
                    check::WRITE,
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
