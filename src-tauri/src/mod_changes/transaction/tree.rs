//! Whole-directory replacement uses same-volume sibling renames and a retained
//! original tree. No recursive live deletion occurs before the durable commit.
use super::{durable, paths, types::*};
use std::{fs, path::Path};

pub(crate) fn copy_tree(
    source: &Path,
    target: &Path,
    expected: &Fingerprint,
) -> Result<Fingerprint, String> {
    if !matches!(expected, Fingerprint::Directory { .. })
        || Fingerprint::capture(source)? != *expected
    {
        return Err(paths::MODIFIED.into());
    }
    copy_directory(source, target)?;
    let after = Fingerprint::capture(target)?;
    if after != *expected || Fingerprint::capture(source)? != *expected {
        return Err(paths::MODIFIED.into());
    }
    Ok(after)
}

fn copy_directory(source: &Path, target: &Path) -> Result<(), String> {
    let expected = Fingerprint::capture(source)?;
    if !matches!(expected, Fingerprint::Directory { .. }) {
        return Err(paths::UNSAFE.into());
    }
    fs::create_dir(target).map_err(paths::io_error)?;
    for entry in fs::read_dir(source).map_err(paths::io_error)? {
        let entry = entry.map_err(paths::io_error)?;
        let source = entry.path();
        let target = target.join(entry.file_name());
        let fingerprint = Fingerprint::capture(&source)?;
        match fingerprint {
            Fingerprint::File { .. } => {
                durable::copy_verified(&source, &target, &fingerprint)?;
                fingerprint
                    .permissions()
                    .ok_or(paths::INVALID)?
                    .restore(&target)?;
            }
            Fingerprint::Directory { .. } => copy_directory(&source, &target)?,
            Fingerprint::Missing => return Err(paths::MODIFIED.into()),
        }
    }
    expected
        .permissions()
        .ok_or(paths::INVALID)?
        .restore(target)?;
    durable::sync_parent(target)
}

pub(crate) fn preserve_replaced_permissions(before: &Path, staged: &Path) -> Result<(), String> {
    let original = Fingerprint::capture(before)?;
    if original == Fingerprint::Missing {
        return Ok(());
    }
    let replacement = Fingerprint::capture(staged)?;
    match (&original, &replacement) {
        (Fingerprint::File { .. }, Fingerprint::File { .. }) => original
            .permissions()
            .ok_or(paths::INVALID)?
            .restore(staged),
        (Fingerprint::Directory { .. }, Fingerprint::Directory { .. }) => {
            for entry in fs::read_dir(staged).map_err(paths::io_error)? {
                let entry = entry.map_err(paths::io_error)?;
                preserve_replaced_permissions(&before.join(entry.file_name()), &entry.path())?;
            }
            original
                .permissions()
                .ok_or(paths::INVALID)?
                .restore(staged)
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
    let target = target.resolve(roots)?;
    let backup = backup.resolve(roots)?;
    let temporary = temporary.resolve(roots)?;
    if Fingerprint::capture(&target)? != *before
        || Fingerprint::capture(&backup)? != Fingerprint::Missing
        || Fingerprint::capture(&temporary)? != Fingerprint::Missing
    {
        return Err(paths::MODIFIED.into());
    }
    if let Some(payload) = payload {
        let payload = durable::private_path(directory, payload)?;
        let stored = Fingerprint::capture(&payload)?;
        if !stored.same_content(after) {
            return Err(paths::MODIFIED.into());
        }
        copy_tree(&payload, &temporary, &stored)?;
        after
            .permissions()
            .ok_or(paths::INVALID)?
            .restore(&temporary)?;
        if Fingerprint::capture(&temporary)? != *after {
            return Err(paths::MODIFIED.into());
        }
    } else if *after != Fingerprint::Missing {
        return Err(paths::INVALID.into());
    }
    if Fingerprint::capture(&target)? != *before {
        return Err(paths::MODIFIED.into());
    }
    if *before != Fingerprint::Missing {
        durable::rename(&target, &backup, false)?;
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
    let target = target.resolve(roots)?;
    let backup = backup.resolve(roots)?;
    let temporary = temporary.resolve(roots)?;
    let actual = Fingerprint::capture(&target)?;
    let original = Fingerprint::capture(&backup)?;
    // Already before or interrupted during the initial copy. The UUID sibling is
    // private, but the live tree still must match the exact authorized state.
    if original == Fingerprint::Missing && actual == *before {
        remove_private_tree(&temporary)?;
        return Ok(());
    }
    if *before == Fingerprint::Missing {
        if original != Fingerprint::Missing || (actual != *after && actual != Fingerprint::Missing)
        {
            return Err(paths::MODIFIED.into());
        }
    } else if original != *before || (actual != *after && actual != Fingerprint::Missing) {
        return Err(paths::MODIFIED.into());
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
    if Fingerprint::capture(&target)? != *before {
        return Err(paths::MODIFIED.into());
    }
    Ok(())
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
    let actual = Fingerprint::capture(&backup.resolve(roots)?)?;
    if actual != Fingerprint::Missing && actual != *before {
        return Err(paths::MODIFIED.into());
    }
    if Fingerprint::capture(&temporary.resolve(roots)?)? != Fingerprint::Missing {
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
