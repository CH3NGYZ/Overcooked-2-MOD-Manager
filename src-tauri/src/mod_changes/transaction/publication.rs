use super::{PreparedTransaction, check, durable, engine, paths, types::*};
use crate::{file_check, file_metadata::FileIdentity};
use std::{
    fs::File,
    path::{Path, PathBuf},
};

impl PreparedTransaction {
    pub(super) fn publish_file(
        &mut self,
        index: usize,
        payload: &Path,
        temporary: &Path,
        target: &Path,
        payload_fingerprint: &Fingerprint,
    ) -> Result<(), String> {
        durable::copy_verified(
            payload,
            temporary,
            payload_fingerprint,
            check::STAGED,
            check::WRITE,
            |file| self.record_temporary(index, temporary, file, None),
        )?;
        let Record {
            operation:
                RecordedMutation::File {
                    after,
                    before,
                    original_identity,
                    ..
                },
            publication: Some(FilePublication::Staged(identity)),
            ..
        } = &self.journal.records[index]
        else {
            return Err(paths::INVALID.into());
        };
        prepare_file(temporary, after, Some(identity))?;
        if let Some(original) = original_identity {
            engine::verify_file_before(target, before, true)?;
            expect_identity(target, check::TARGET, original)?;
            engine::make_writable(target, before)
                .map_err(|error| check::writing(target, &error))?;
            expect_identity(target, check::TARGET, original)?;
        }
        match durable::rename_durable(temporary, target, original_identity.is_some(), check::WRITE)
        {
            Ok(()) => Ok(()),
            Err(durable::RenameFailure::Unchanged(error)) => {
                self.journal.records[index].publication =
                    Some(FilePublication::Unpublished(identity.clone()));
                self.persist()?;
                Err(error)
            }
            Err(durable::RenameFailure::Sync(error)) => Err(error),
        }
    }

    pub(super) fn record_temporary(
        &mut self,
        index: usize,
        path: &Path,
        file: &File,
        published: Option<&FileIdentity>,
    ) -> Result<(), String> {
        let identity = FileIdentity::from_file(file)
            .map_err(|error| identity_error(path, check::STAGED, error))?;
        self.journal.records[index].publication = Some(match published {
            Some(published) => FilePublication::Restoring {
                published: published.clone(),
                temporary: identity,
            },
            None => FilePublication::Staged(identity),
        });
        self.persist()?;
        paths::guard_single_link(path)
    }

    pub(super) fn discard_unpublished_file(&mut self, index: usize) -> Result<bool, String> {
        let record = &self.journal.records[index];
        let RecordedMutation::File { temporary, .. } = &record.operation else {
            return Err(paths::INVALID.into());
        };
        let temporary = check::resolve(temporary, &self.journal.roots, check::STAGED)?;
        if let Some(actual) = capture_identity(&temporary, check::STAGED)? {
            let unpublished = match &record.publication {
                Some(FilePublication::Staged(expected)) if expected == &actual => {
                    self.journal.records[index].publication =
                        Some(FilePublication::Unpublished(actual.clone()));
                    self.persist()?;
                    true
                }
                Some(FilePublication::Unpublished(expected)) if expected == &actual => true,
                Some(FilePublication::Restoring { temporary, .. }) if temporary == &actual => false,
                _ => {
                    return Err(file_check::report(
                        check::STAGED,
                        &temporary,
                        file_check::CONTENT,
                    ));
                }
            };
            let fingerprint = check::capture(&temporary, check::STAGED)?;
            expect_identity(&temporary, check::STAGED, &actual)?;
            durable::remove_file_checked(&temporary, &fingerprint, true)?;
            return Ok(unpublished);
        }
        Ok(matches!(
            record.publication,
            Some(FilePublication::Unpublished(_) | FilePublication::NotStaged)
        ))
    }

    pub(super) fn revert_new_file(&mut self, index: usize) -> Result<(), String> {
        if self.discard_unpublished_file(index)? {
            return Ok(());
        }
        let record = &self.journal.records[index];
        let RecordedMutation::File { target, after, .. } = &record.operation else {
            return Err(paths::INVALID.into());
        };
        let target = check::resolve(target, &self.journal.roots, check::TARGET)?;
        let actual = check::capture(&target, check::TARGET)?;
        if actual == Fingerprint::Missing {
            return Ok(());
        }
        let Some(FilePublication::Staged(identity)) = &record.publication else {
            return Err(file_check::report(
                check::TARGET,
                &target,
                file_check::CONTENT,
            ));
        };
        if !engine::matches_writable(&actual, after) {
            check::compare(&target, check::TARGET, &actual, after, false)?;
        }
        expect_identity(&target, check::TARGET, identity)?;
        durable::remove_file_checked(&target, &actual, true)
    }
}

pub(super) fn prepare_file(
    path: &Path,
    fingerprint: &Fingerprint,
    identity: Option<&FileIdentity>,
) -> Result<(), String> {
    let check_identity = || {
        if let Some(identity) = identity {
            expect_identity(path, check::STAGED, identity)?;
            paths::guard_single_link(path)?;
        }
        Ok::<(), String>(())
    };
    check_identity()?;
    fingerprint
        .permissions()
        .ok_or(paths::INVALID)?
        .restore(path)
        .map_err(|error| check::writing(path, &error))?;
    check::expect(path, check::STAGED, fingerprint)?;
    check_identity()
}

pub(super) struct HeldRemoval<'a> {
    target: &'a TransactionPath,
    temporary: &'a TransactionPath,
    before: &'a Fingerprint,
    identity: &'a FileIdentity,
}

impl<'a> HeldRemoval<'a> {
    pub(super) fn from_operation(operation: &'a RecordedMutation) -> Option<Self> {
        let RecordedMutation::File {
            target,
            temporary,
            before,
            original_identity: Some(identity),
            payload: None,
            after: Fingerprint::Missing,
            ..
        } = operation
        else {
            return None;
        };
        Some(Self {
            target,
            temporary,
            before,
            identity,
        })
    }

    pub(super) fn apply(&self, target: &Path, temporary: &Path) -> Result<(), String> {
        engine::verify_file_before(target, self.before, true)?;
        expect_identity(target, check::TARGET, self.identity)?;
        durable::rename_checked(target, temporary, false, check::WRITE)
    }

    pub(super) fn verify_applied(&self, roots: &TransactionRoots) -> Result<(), String> {
        let held = check::resolve(self.temporary, roots, check::BACKUP)?;
        check::expect(&held, check::BACKUP, self.before)?;
        self.verify_identity(&held)
    }

    fn verify_identity(&self, held: &Path) -> Result<(), String> {
        paths::guard_single_link(held)?;
        expect_identity(held, check::BACKUP, self.identity)
    }

    fn held_file(&self, roots: &TransactionRoots) -> Result<Option<PathBuf>, String> {
        let held = check::resolve(self.temporary, roots, check::BACKUP)?;
        let actual = check::capture(&held, check::BACKUP)?;
        if actual == Fingerprint::Missing {
            return Ok(None);
        }
        check::compare(&held, check::BACKUP, &actual, self.before, false)?;
        self.verify_identity(&held)?;
        Ok(Some(held))
    }

    pub(super) fn revert(&self, roots: &TransactionRoots) -> Result<(), String> {
        let target = check::resolve(self.target, roots, check::TARGET)?;
        if let Some(held) = self.held_file(roots)? {
            check::expect(&target, check::TARGET, &Fingerprint::Missing)?;
            durable::rename_checked(&held, &target, false, check::BACKUP)?;
            expect_identity(&target, check::RESULT, self.identity)?;
            check::expect(&target, check::RESULT, self.before)?;
        } else if check::capture(&target, check::TARGET)? == Fingerprint::Missing {
            return Err(file_check::report(
                check::BACKUP,
                &target,
                file_check::MISSING,
            ));
        }
        Ok(())
    }

    pub(super) fn cleanup(&self, roots: &TransactionRoots) -> Result<(), String> {
        if let Some(held) = self.held_file(roots)? {
            durable::remove_file_checked(&held, self.before, true)?;
        }
        Ok(())
    }
}

pub(super) fn restore_unpublished_permissions(
    target: &TransactionPath,
    before: &Fingerprint,
    original: &FileIdentity,
    roots: &TransactionRoots,
) -> Result<(), String> {
    let Some(permissions) = before
        .permissions()
        .filter(|permissions| permissions.readonly)
    else {
        return Ok(());
    };
    let Ok(path) = check::resolve(target, roots, check::TARGET) else {
        return Ok(());
    };
    if !capture_identity(&path, check::TARGET).is_ok_and(|actual| actual.as_ref() == Some(original))
    {
        return Ok(());
    }
    let actual = check::capture(&path, check::TARGET)?;
    if actual != *before && engine::matches_writable(&actual, before) {
        paths::guard_single_link(&path)?;
        expect_identity(&path, check::TARGET, original)?;
        permissions
            .restore(&path)
            .map_err(|error| check::writing(&path, &error))?;
    }
    Ok(())
}

pub(super) fn expect_identity(
    path: &Path,
    stage: &str,
    expected: &FileIdentity,
) -> Result<(), String> {
    if capture_identity(path, stage)?.as_ref() == Some(expected) {
        return Ok(());
    }
    Err(file_check::report(stage, path, file_check::CONTENT))
}

pub(super) fn capture_identity(path: &Path, stage: &str) -> Result<Option<FileIdentity>, String> {
    FileIdentity::capture(path).map_err(|error| identity_error(path, stage, error))
}

fn identity_error(path: &Path, stage: &str, error: std::io::Error) -> String {
    durable::diagnostic_io(stage, path, error, file_check::READ)
}
