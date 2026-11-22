//! Whole-directory replacement uses same-volume sibling renames and a retained
//! original tree. No recursive live deletion occurs before the durable commit.
use super::{check, durable, paths, types::*};
use crate::file_check;
use std::{fs, path::Path};

pub(crate) fn copy_tree(
    source: &Path,
    target: &Path,
    expected: &Fingerprint,
) -> Result<Fingerprint, String> {
    if !matches!(expected, Fingerprint::Directory { .. }) {
        return Err(file_check::report(check::STAGED, source, file_check::TYPE));
    }
    check::expect(source, check::STAGED, expected)?;
    copy_directory(source, target)?;
    let after = check::capture(target, check::STAGED)?;
    check::compare(target, check::STAGED, &after, expected, false)?;
    check::expect(source, check::STAGED, expected)?;
    Ok(after)
}

fn copy_directory(source: &Path, target: &Path) -> Result<(), String> {
    let expected = check::capture(source, check::STAGED)?;
    if !matches!(expected, Fingerprint::Directory { .. }) {
        let reason = if expected == Fingerprint::Missing {
            file_check::MISSING
        } else {
            file_check::TYPE
        };
        return Err(file_check::report(check::STAGED, source, reason));
    }
    fs::create_dir(target)
        .map_err(|error| durable::diagnostic_io(check::WRITE, target, error, file_check::WRITE))?;
    let read_error = |error| durable::diagnostic_io(check::STAGED, source, error, file_check::READ);
    for entry in fs::read_dir(source).map_err(read_error)? {
        let entry = entry.map_err(read_error)?;
        let source = entry.path();
        let target = target.join(entry.file_name());
        let fingerprint = check::capture(&source, check::STAGED)?;
        match fingerprint {
            Fingerprint::File { .. } => {
                durable::copy_verified(
                    &source,
                    &target,
                    &fingerprint,
                    check::STAGED,
                    check::WRITE,
                )?;
                fingerprint
                    .permissions()
                    .ok_or(paths::INVALID)?
                    .restore(&target)
                    .map_err(|error| check::writing(&target, &error))?;
            }
            Fingerprint::Directory { .. } => copy_directory(&source, &target)?,
            Fingerprint::Missing => {
                return Err(file_check::report(
                    check::STAGED,
                    &source,
                    file_check::MISSING,
                ));
            }
        }
    }
    expected
        .permissions()
        .ok_or(paths::INVALID)?
        .restore(target)
        .map_err(|error| check::writing(target, &error))?;
    durable::sync_parent(target).map_err(|error| check::writing(target, &error))
}

pub(crate) fn preserve_replaced_permissions(before: &Path, staged: &Path) -> Result<(), String> {
    let original = check::capture(before, check::TARGET)?;
    if original == Fingerprint::Missing {
        return Ok(());
    }
    let replacement = check::capture(staged, check::STAGED)?;
    match (&original, &replacement) {
        (Fingerprint::File { .. }, Fingerprint::File { .. }) => original
            .permissions()
            .ok_or(paths::INVALID)?
            .restore(staged)
            .map_err(|error| check::writing(staged, &error)),
        (Fingerprint::Directory { .. }, Fingerprint::Directory { .. }) => {
            let read_error =
                |error| file_check::report_io(check::STAGED, staged, error, file_check::READ);
            for entry in fs::read_dir(staged).map_err(read_error)? {
                let entry = entry.map_err(read_error)?;
                preserve_replaced_permissions(&before.join(entry.file_name()), &entry.path())?;
            }
            original
                .permissions()
                .ok_or(paths::INVALID)?
                .restore(staged)
                .map_err(|error| check::writing(staged, &error))
        }
        // A file-to-directory transition has no matching permission object.
        _ => Ok(()),
    }
}

pub(super) fn apply(
    directory: &Path,
    operation: &RecordedMutation,
    roots: &TransactionRoots,
) -> Result<(), String> {
    let RecordedMutation::Tree {
        target,
        before,
        after,
        payload,
        backup,
        temporary,
    } = operation
    else {
        return Err(paths::INVALID.into());
    };
    let target = check::resolve(target, roots, check::TARGET)?;
    let backup = check::resolve(backup, roots, check::BACKUP)?;
    let temporary = check::resolve(temporary, roots, check::STAGED)?;
    check::expect(&target, check::TARGET, before)?;
    check::expect(&backup, check::BACKUP, &Fingerprint::Missing)?;
    check::expect(&temporary, check::STAGED, &Fingerprint::Missing)?;
    if let Some(payload) = payload {
        let payload = durable::private_path(directory, payload, check::STAGED)?;
        let stored = check::capture(&payload, check::STAGED)?;
        check::compare(&payload, check::STAGED, &stored, after, true)?;
        copy_tree(&payload, &temporary, &stored)?;
        after
            .permissions()
            .ok_or(paths::INVALID)?
            .restore(&temporary)
            .map_err(|error| check::writing(&temporary, &error))?;
        check::expect(&temporary, check::STAGED, after)?;
    } else if *after != Fingerprint::Missing {
        return Err(paths::INVALID.into());
    }
    check::expect(&target, check::TARGET, before)?;
    if *before != Fingerprint::Missing {
        durable::rename_checked(&target, &backup, false, check::WRITE_BACKUP)?;
    }
    if *after != Fingerprint::Missing {
        durable::rename(&temporary, &target, false)?;
    }
    Ok(())
}

pub(super) fn revert(operation: &RecordedMutation, roots: &TransactionRoots) -> Result<(), String> {
    let RecordedMutation::Tree {
        target,
        before,
        after,
        backup,
        temporary,
        ..
    } = operation
    else {
        return Err(paths::INVALID.into());
    };
    let target = check::resolve(target, roots, check::TARGET)?;
    let backup = check::resolve(backup, roots, check::BACKUP)?;
    let temporary = check::resolve(temporary, roots, check::STAGED)?;
    let actual = check::capture(&target, check::TARGET)?;
    let original = check::capture(&backup, check::BACKUP)?;
    // Already before or interrupted during the initial copy. The UUID sibling is
    // private, but the live tree still must match the exact authorized state.
    if original == Fingerprint::Missing && actual == *before {
        remove_private_tree(&temporary)?;
        return Ok(());
    }
    check::compare(&backup, check::BACKUP, &original, before, false)?;
    if actual != Fingerprint::Missing {
        check::compare(&target, check::TARGET, &actual, after, false)?;
    }
    remove_private_tree(&temporary)?;
    if actual != Fingerprint::Missing {
        // Keep the after tree under the owned UUID until before has been restored.
        // A crash at either rename is recognized by this same state machine.
        durable::rename(&target, &temporary, false)?;
    }
    if *before != Fingerprint::Missing {
        durable::rename(&backup, &target, false)?;
    }
    remove_private_tree(&temporary)?;
    check::expect(&target, check::RESULT, before)
}

pub(super) fn validate_cleanup(
    operation: &RecordedMutation,
    roots: &TransactionRoots,
) -> Result<(), String> {
    let RecordedMutation::Tree {
        before,
        backup,
        temporary,
        ..
    } = operation
    else {
        return Ok(());
    };
    let backup = backup.resolve(roots)?;
    let actual = Fingerprint::capture(&backup)?;
    if actual != Fingerprint::Missing && actual != *before {
        check::log_mismatch(&backup, check::BACKUP, &actual, before, false);
        return Err(paths::MODIFIED.into());
    }
    let temporary = temporary.resolve(roots)?;
    let actual = Fingerprint::capture(&temporary)?;
    if actual != Fingerprint::Missing {
        check::log_mismatch(
            &temporary,
            check::STAGED,
            &actual,
            &Fingerprint::Missing,
            false,
        );
        return Err(paths::MODIFIED.into());
    }
    Ok(())
}

pub(super) fn cleanup(
    operation: &RecordedMutation,
    roots: &TransactionRoots,
) -> Result<(), String> {
    let RecordedMutation::Tree {
        before,
        backup,
        temporary,
        ..
    } = operation
    else {
        return Ok(());
    };
    let backup = backup.resolve(roots)?;
    let actual = Fingerprint::capture(&backup)?;
    // Purging was durably recorded after checking before. A partially removed
    // tree remains private on restart; live target is never recursively removed.
    if actual != Fingerprint::Missing && !matches!(before, Fingerprint::Directory { .. }) {
        return Err(paths::MODIFIED.into());
    }
    remove_private_tree(&backup)?;
    remove_private_tree(&temporary.resolve(roots)?)
}

fn remove_private_tree(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    super::recovery::remove_private_tree(path)
}
