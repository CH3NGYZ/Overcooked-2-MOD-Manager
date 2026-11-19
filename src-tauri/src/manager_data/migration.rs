//! Exit-time migration reuses the installer transaction's verified tree copier.
//! The fixed control journal remains accessible when either data volume is offline.
use super::storage::{ResetPaths, ResetPhase, ResetRecord, normalized, reject_link_if_present};
use crate::mod_changes::transaction::{self, Fingerprint};
#[cfg(test)]
use std::fs;
use std::path::{Component, Path, PathBuf};

const UNSAFE: &str = "tools.migrateDataUnsafe";

fn failed(error: impl std::fmt::Display) -> String {
    crate::locale_tag::with_params("tools.migrateDataFailed", &[("reason", &error.to_string())])
}

fn overlaps(left: &Path, right: &Path) -> bool {
    let left = normalized(left);
    let right = normalized(right);
    left.starts_with(&right) || right.starts_with(&left)
}

fn checked(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return Err(UNSAFE.into());
    }
    for ancestor in path.ancestors() {
        reject_link_if_present(ancestor).map_err(|_| UNSAFE)?;
    }
    crate::install_guard::validate_install_dir(path).map_err(|_| UNSAFE)?;
    crate::game::resolve_existing_ancestor(path).map_err(failed)
}

fn validate_pair(paths: &ResetPaths, source: &Path, target: &Path) -> Result<(), String> {
    let source = checked(source)?;
    let target = checked(target)?;
    if source
        .file_name()
        .is_none_or(|name| name != "OC2ModManager")
        || target
            .file_name()
            .is_none_or(|name| name != "OC2ModManager")
        || overlaps(&source, &target)
        || overlaps(&target, &paths.parent)
        || overlaps(&source, &paths.parent)
    {
        return Err(UNSAFE.into());
    }
    if !target.parent().is_some_and(Path::is_dir) {
        return Err(UNSAFE.into());
    }
    Ok(())
}

fn require_new_target(target: &Path) -> Result<(), String> {
    // Reject even an empty existing directory: it was not created by this request.
    if target.try_exists().map_err(failed)? {
        let path = crate::display_path::display_path(target);
        log::warn!("取消配置迁移：目标已存在，地址={path}");
        return Err(crate::locale_tag::with_params(
            "tools.migrateDataTargetExists",
            &[("path", &path)],
        ));
    }
    Ok(())
}

pub(super) fn prepare(
    paths: &ResetPaths,
    parent: &Path,
    state: &crate::app_state::AppState,
    executable: &Path,
) -> Result<PathBuf, String> {
    let parent = checked(parent)?;
    if !parent.is_dir() {
        return Err(UNSAFE.into());
    }
    let target = parent.join("OC2ModManager");
    validate_pair(paths, &paths.root, &target)?;
    require_new_target(&target)?;
    let destination = ResetPaths::with_control(&target, &paths.parent)?;
    destination
        .protect(executable.parent().ok_or(UNSAFE)?)
        .map_err(|_| UNSAFE)?;
    #[cfg(target_os = "macos")]
    if let Some(bundle) = executable
        .ancestors()
        .find(|path| path.extension().is_some_and(|ext| ext == "app"))
    {
        destination.protect(bundle).map_err(|_| UNSAFE)?;
    }
    let config = state.config.load_or_default();
    if !config.game.path.trim().is_empty() {
        destination
            .protect(Path::new(&config.game.path))
            .map_err(|_| UNSAFE)?;
    }
    if let Some(game) = state
        .game_state
        .lock()
        .map_err(|_| "lock.poisoned")?
        .as_ref()
        .and_then(|game| game.path.as_ref())
    {
        destination.protect(Path::new(game)).map_err(|_| UNSAFE)?;
    }
    // A read-only destination must fail while the user can still cancel.
    tempfile::NamedTempFile::new_in(parent).map_err(failed)?;
    Ok(target)
}

fn stage(target: &Path, token: uuid::Uuid) -> PathBuf {
    target.with_file_name(format!(".OC2ModManager-moving-{token}"))
}

fn retired(source: &Path, token: uuid::Uuid) -> PathBuf {
    source.with_file_name(format!(".OC2ModManager-migrated-{token}"))
}

pub(super) fn finish(
    paths: &ResetPaths,
    record: ResetRecord,
    source: &Path,
    target: &Path,
) -> Result<(), String> {
    let pointer = paths.parent.join("data-location.json");
    finish_at(paths, record, source, target, &pointer)
}

fn finish_at(
    paths: &ResetPaths,
    mut record: ResetRecord,
    source: &Path,
    target: &Path,
    pointer: &Path,
) -> Result<(), String> {
    let result = (|| {
        validate_pair(paths, source, target)?;
        let current = crate::user_paths::read_location_at(pointer, source)?;
        if normalized(&current) != normalized(source) && normalized(&current) != normalized(target)
        {
            return Err(UNSAFE.into());
        }
        let stage = stage(target, record.token);
        let retired = retired(source, record.token);
        if record.phase == ResetPhase::Pending {
            if normalized(&current) != normalized(source)
                || stage.try_exists().map_err(failed)?
                || retired.try_exists().map_err(failed)?
            {
                return Err(UNSAFE.into());
            }
            require_new_target(target)?;
            let fingerprint = Fingerprint::capture(source).map_err(failed)?;
            if !matches!(fingerprint, Fingerprint::Directory { .. }) {
                return Err(UNSAFE.into());
            }
            record.fingerprint = Some(fingerprint);
            record.phase = ResetPhase::Copying;
            paths.write_record(&record)?;
        }
        let fingerprint = record.fingerprint.as_ref().ok_or(UNSAFE)?.clone();
        if record.phase == ResetPhase::Copying {
            if normalized(&current) != normalized(source) {
                return Err(UNSAFE.into());
            }
            require_new_target(target)?;
            if stage.try_exists().map_err(failed)? {
                // This private UUID tree is the only incomplete copy we may discard.
                transaction::remove_private_tree(&stage).map_err(failed)?;
            }
            transaction::stage_tree(source, &stage, &fingerprint).map_err(failed)?;
            record.phase = ResetPhase::Verified;
            paths.write_record(&record)?;
        }
        if record.phase == ResetPhase::Verified {
            // Covers a crash after rename but before the next journal write.
            if !target.try_exists().map_err(failed)? {
                if Fingerprint::capture(&stage).map_err(failed)? != fingerprint {
                    return Err(UNSAFE.into());
                }
                transaction::durable_rename(&stage, target, false).map_err(failed)?;
            } else if stage.try_exists().map_err(failed)? {
                return Err(UNSAFE.into());
            }
            if Fingerprint::capture(target).map_err(failed)? != fingerprint
                || Fingerprint::capture(source).map_err(failed)? != fingerprint
            {
                return Err(UNSAFE.into());
            }
            crate::user_paths::write_location_at(pointer, target)?;
            record.phase = ResetPhase::Switched;
            paths.write_record(&record)?;
        }
        if record.phase == ResetPhase::Switched {
            if Fingerprint::capture(target).map_err(failed)? != fingerprint {
                return Err(UNSAFE.into());
            }
            if normalized(&crate::user_paths::read_location_at(pointer, source)?)
                != normalized(target)
            {
                return Err(UNSAFE.into());
            }
            // Never delete from the public old path on a resumed partial cleanup.
            if source.try_exists().map_err(failed)? {
                if retired.try_exists().map_err(failed)?
                    || Fingerprint::capture(source).map_err(failed)? != fingerprint
                {
                    return Err(UNSAFE.into());
                }
                transaction::durable_rename(source, &retired, false).map_err(failed)?;
            }
            record.phase = ResetPhase::Cleaning;
            paths.write_record(&record)?;
        }
        if record.phase != ResetPhase::Cleaning {
            return Err(UNSAFE.into());
        }
        if Fingerprint::capture(target).map_err(failed)? != fingerprint {
            return Err(UNSAFE.into());
        }
        if normalized(&crate::user_paths::read_location_at(pointer, source)?) != normalized(target)
        {
            return Err(UNSAFE.into());
        }
        transaction::remove_private_tree(&retired).map_err(failed)?;
        record.phase = ResetPhase::Completed;
        record.error = None;
        paths.write_record(&record)
    })();
    if let Err(error) = &result {
        // A published pointer remains authoritative, even when old-volume cleanup fails.
        // Failed requests retain private trees and are never retried destructively.
        paths.mark_failed(record, error)?;
    }
    result
}

#[cfg(test)]
#[path = "migration_tests.rs"]
mod tests;
