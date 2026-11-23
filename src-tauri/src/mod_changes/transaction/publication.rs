use super::{PreparedTransaction, check, durable, engine, paths, types::*};
use crate::{file_check, file_metadata::FileIdentity};
use std::path::Path;

impl PreparedTransaction {
    pub(super) fn publish_new_file(
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
            |file| {
                let identity = FileIdentity::from_file(file)
                    .map_err(|error| identity_error(temporary, check::STAGED, error))?;
                self.journal.records[index].publication = Some(FilePublication::Staged(identity));
                self.persist()
            },
        )?;
        let Record {
            operation: RecordedMutation::File { after, .. },
            publication: Some(FilePublication::Staged(identity)),
            ..
        } = &self.journal.records[index]
        else {
            return Err(paths::INVALID.into());
        };
        prepare_file(temporary, after)?;
        expect_identity(temporary, check::STAGED, identity)?;
        match durable::rename_durable(temporary, target, false, check::WRITE) {
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

    pub(super) fn revert_new_file(&mut self, index: usize) -> Result<(), String> {
        let record = &self.journal.records[index];
        let RecordedMutation::File {
            target,
            temporary,
            after,
            ..
        } = &record.operation
        else {
            return Err(paths::INVALID.into());
        };
        let temporary = check::resolve(temporary, &self.journal.roots, check::STAGED)?;
        if let Some(actual) = capture_identity(&temporary, check::STAGED)? {
            match &record.publication {
                Some(FilePublication::Staged(expected)) if expected == &actual => {
                    self.journal.records[index].publication =
                        Some(FilePublication::Unpublished(actual.clone()));
                    self.persist()?;
                }
                Some(FilePublication::Unpublished(expected)) if expected == &actual => (),
                _ => {
                    return Err(file_check::report(
                        check::STAGED,
                        &temporary,
                        file_check::CONTENT,
                    ));
                }
            }
            let fingerprint = check::capture(&temporary, check::STAGED)?;
            expect_identity(&temporary, check::STAGED, &actual)?;
            return durable::remove_file_checked(&temporary, &fingerprint, true);
        }
        if matches!(
            record.publication,
            Some(FilePublication::Unpublished(_) | FilePublication::NotStaged)
        ) {
            return Ok(());
        }
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

pub(super) fn prepare_file(path: &Path, fingerprint: &Fingerprint) -> Result<(), String> {
    fingerprint
        .permissions()
        .ok_or(paths::INVALID)?
        .restore(path)
        .map_err(|error| check::writing(path, &error))?;
    check::expect(path, check::STAGED, fingerprint)
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

fn capture_identity(path: &Path, stage: &str) -> Result<Option<FileIdentity>, String> {
    FileIdentity::capture(path).map_err(|error| identity_error(path, stage, error))
}

fn identity_error(path: &Path, stage: &str, error: std::io::Error) -> String {
    durable::diagnostic_io(stage, path, error, file_check::READ)
}
