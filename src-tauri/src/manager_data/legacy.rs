//! Bridge the pre-relocation reset journal and lock before changing controllers.
use super::{
    LOCK_TIMEOUT,
    storage::{DataAction, ResetPaths, ResetRecord, normalized},
};
use std::{fs::File, path::Path};

fn legacy_paths(paths: &ResetPaths) -> Result<Option<ResetPaths>, String> {
    let Ok(default) = crate::user_paths::default_root() else {
        return Ok(None);
    };
    legacy_paths_at(paths, &default)
}

fn legacy_paths_at(paths: &ResetPaths, default: &Path) -> Result<Option<ResetPaths>, String> {
    let same_spelling = normalized(&paths.root) == normalized(default);
    let default = match crate::game::resolve_existing_ancestor(default) {
        Ok(default) => default,
        // A custom root must remain usable when redirected Documents is offline.
        // Resolve reachable aliases first so they still share the old controller.
        Err(_) if !same_spelling => return Ok(None),
        Err(error) => return Err(error),
    };
    if normalized(&paths.root) != normalized(&default) {
        return Ok(None);
    }
    let parent = paths.root.parent().ok_or(super::UNSAFE)?;
    Ok(Some(ResetPaths::with_control(&paths.root, parent)?))
}

/// A shared old lock prevents an old-version reset helper from deleting files
/// while a new-version normal session still uses the default data root.
pub(super) fn session(paths: &ResetPaths) -> Result<Option<File>, String> {
    let Some(legacy) = legacy_paths(paths)? else {
        return Ok(None);
    };
    let lock = legacy.open_lock()?;
    super::storage::wait_for_lock(&lock, true, LOCK_TIMEOUT)?;
    Ok(Some(lock))
}

pub(super) fn exclusive(paths: &ResetPaths) -> Result<Option<File>, String> {
    let Some(legacy) = legacy_paths(paths)? else {
        return Ok(None);
    };
    let lock = legacy.open_lock()?;
    super::storage::wait_for_lock(&lock, false, LOCK_TIMEOUT)?;
    Ok(Some(lock))
}

pub(super) fn exclusive_for_request(
    paths: &ResetPaths,
    record: &ResetRecord,
) -> Result<Option<File>, String> {
    let Ok(default) = crate::user_paths::default_root() else {
        return Ok(None);
    };
    exclusive_for_request_at(paths, record, &default)
}

fn exclusive_for_request_at(
    paths: &ResetPaths,
    record: &ResetRecord,
    default: &Path,
) -> Result<Option<File>, String> {
    let result = (|| {
        let (source, target) = match &record.action {
            DataAction::Migrate { source, target } => (source.as_path(), Some(target.as_path())),
            DataAction::Clear => (paths.root.as_path(), None),
        };
        let source_paths = ResetPaths::with_control(source, &paths.parent)?;
        let source_legacy = legacy_paths_at(&source_paths, default)?;
        let target_legacy = match target {
            Some(target) => {
                let target_paths = ResetPaths::with_control(target, &paths.parent)?;
                legacy_paths_at(&target_paths, default)?
            }
            None => None,
        };
        let migrating_to_default = target_legacy.is_some();
        let Some(legacy) = target_legacy.or(source_legacy) else {
            return Ok(None);
        };
        // A resumed request may already have published its new pointer. The old
        // controller must be excluded for either the source or the new destination.
        let lock = legacy.open_lock()?;
        super::storage::wait_for_lock(&lock, false, LOCK_TIMEOUT)?;
        if migrating_to_default && legacy.record()?.is_some_and(|old| old.pending()) {
            // Releasing the lock with an old pending clear would let its helper erase
            // the newly published destination. Preserve both requests and all data.
            return Err(crate::locale_tag::with_params(
                "tools.migrateDataFailed",
                &[(
                    "reason",
                    "The default data directory has a pending clear request; its recovery files are retained",
                )],
            ));
        }
        Ok(Some(lock))
    })();
    if let Err(error) = &result {
        // Both startup recovery and the helper propagate this result directly.
        // Persist a terminal state so a rejected handoff cannot retry forever.
        paths.mark_failed(record.clone(), error)?;
    }
    result
}

pub(super) fn needs_import(paths: &ResetPaths) -> Result<bool, String> {
    let Some(legacy) = legacy_paths(paths)? else {
        return Ok(false);
    };
    let Some(old) = legacy.record()? else {
        return Ok(false);
    };
    Ok(paths.record()?.is_none() || old.pending())
}

/// Caller holds the fixed exclusive lock. Old helpers re-read their now-completed
/// journal under the legacy lock, so none can replay the imported request later.
pub(super) fn import(paths: &ResetPaths) -> Result<(), String> {
    let Some(legacy) = legacy_paths(paths)? else {
        return Ok(());
    };
    let _legacy = exclusive(paths)?;
    import_from(paths, &legacy)
}

pub(super) fn import_from(paths: &ResetPaths, legacy: &ResetPaths) -> Result<(), String> {
    let Some(mut record) = legacy.record()? else {
        return Ok(());
    };
    if let Some(current) = paths.record()? {
        if !record.pending() {
            return Ok(());
        }
        if current.pending() && current.token != record.token {
            return Err(super::BUSY.into());
        }
    }
    if record.pending() {
        let token = record.token;
        let result = match super::handoff::wait_for_owner(&record) {
            Ok(()) => legacy.finish(record),
            Err(error) => legacy.mark_failed(record, &error),
        };
        if let Err(error) = result
            && !legacy
                .record()?
                .is_some_and(|record| record.token == token && !record.pending())
        {
            return Err(error);
        }
        record = legacy.record()?.ok_or(super::UNSAFE)?;
    }
    record.data_root = Some(legacy.root.clone());
    paths.write_record(&record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manager_data::storage::ResetPhase;
    use std::{fs, path::PathBuf};

    struct MigrationFixture {
        _temp: tempfile::TempDir,
        paths: ResetPaths,
        source: PathBuf,
        target: PathBuf,
        default: PathBuf,
        record: ResetRecord,
    }

    fn migration_fixture(source_is_default: bool) -> MigrationFixture {
        let temp = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(temp.path()).unwrap();
        let source = base.join("source/OC2ModManager");
        let target = base.join("target/OC2ModManager");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(source.join("config.toml"), b"preserve source").unwrap();
        let paths = ResetPaths::with_control(&source, &base.join("bootstrap")).unwrap();
        let default = if source_is_default { &source } else { &target }.clone();
        let record = ResetRecord {
            token: uuid::Uuid::new_v4(),
            owner_pid: u32::MAX,
            owner_started: 0,
            phase: ResetPhase::Pending,
            error: None,
            reported: false,
            locale: None,
            failed_from: None,
            action: DataAction::Migrate {
                source: source.clone(),
                target: target.clone(),
            },
            fingerprint: None,
            data_root: Some(source.clone()),
        };
        MigrationFixture {
            _temp: temp,
            paths,
            source,
            target,
            default,
            record,
        }
    }

    #[test]
    fn unreachable_default_is_ignored_only_for_a_different_custom_root() {
        let fixture = migration_fixture(false);
        let parent_file = fixture.default.parent().unwrap().join("unavailable");
        fs::write(&parent_file, b"not a directory").unwrap();
        // Windows may report a missing child beneath a file as NotFound and let
        // ancestor resolution succeed. Invalid input also exercises its Err arm.
        let invalid = fixture.default.parent().unwrap().join("unavailable\0");
        let defaults = [
            parent_file.join("OC2ModManager"),
            invalid.join("OC2ModManager"),
        ];
        assert!(crate::game::resolve_existing_ancestor(&defaults[1]).is_err());
        for default in defaults {
            assert!(legacy_paths_at(&fixture.paths, &default).unwrap().is_none());
            let selected_default = ResetPaths {
                root: default.clone(),
                parent: fixture.paths.parent.clone(),
            };
            assert!(legacy_paths_at(&selected_default, &default).is_err());
        }
        assert!(fixture.source.join("config.toml").is_file());
    }

    #[cfg(unix)]
    fn directory_alias(alias: &Path, target: &Path) {
        std::os::unix::fs::symlink(target, alias).unwrap();
    }

    #[cfg(windows)]
    fn directory_alias(alias: &Path, target: &Path) {
        let output = std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(alias)
            .arg(target)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn reachable_default_alias_still_uses_the_same_legacy_controller() {
        let fixture = migration_fixture(true);
        let alias = fixture
            .paths
            .parent
            .parent()
            .unwrap()
            .join("documents-alias");
        directory_alias(&alias, fixture.source.parent().unwrap());
        let legacy = legacy_paths_at(&fixture.paths, &alias.join("OC2ModManager"))
            .unwrap()
            .unwrap();
        assert_eq!(normalized(&legacy.root), normalized(&fixture.source));
        assert_eq!(legacy.parent, fixture.source.parent().unwrap());
    }

    #[test]
    fn migration_into_default_holds_its_legacy_exclusive_lock() {
        let fixture = migration_fixture(false);
        let lock = exclusive_for_request_at(&fixture.paths, &fixture.record, &fixture.default)
            .unwrap()
            .unwrap();
        let legacy =
            ResetPaths::with_control(&fixture.default, fixture.default.parent().unwrap()).unwrap();
        let competing = legacy.open_lock().unwrap();
        assert!(matches!(
            competing.try_lock_shared(),
            Err(fs::TryLockError::WouldBlock)
        ));
        drop(lock);
        competing.try_lock().unwrap();
        assert_eq!(
            fs::read(fixture.source.join("config.toml")).unwrap(),
            b"preserve source"
        );
        assert!(!fixture.target.exists());
    }

    #[test]
    fn switched_request_still_locks_its_retired_default_source() {
        let mut fixture = migration_fixture(true);
        fs::rename(&fixture.source, fixture.source.with_file_name("retired")).unwrap();
        fs::create_dir(&fixture.target).unwrap();
        fixture.paths = ResetPaths::with_control(&fixture.target, &fixture.paths.parent).unwrap();
        fixture.record.phase = ResetPhase::Switched;
        let lock = exclusive_for_request_at(&fixture.paths, &fixture.record, &fixture.default)
            .unwrap()
            .unwrap();
        let legacy =
            ResetPaths::with_control(&fixture.default, fixture.default.parent().unwrap()).unwrap();
        let competing = legacy.open_lock().unwrap();
        assert!(matches!(
            competing.try_lock_shared(),
            Err(fs::TryLockError::WouldBlock)
        ));
        drop(lock);
        competing.try_lock().unwrap();
    }

    #[test]
    fn pending_default_clear_fails_migration_without_touching_source_or_old_request() {
        let fixture = migration_fixture(false);
        let legacy =
            ResetPaths::with_control(&fixture.default, fixture.default.parent().unwrap()).unwrap();
        let mut old = fixture.record.clone();
        old.token = uuid::Uuid::new_v4();
        old.action = DataAction::Clear;
        old.data_root = None;
        legacy.write_record(&old).unwrap();
        fixture.paths.write_record(&fixture.record).unwrap();

        let error = exclusive_for_request_at(&fixture.paths, &fixture.record, &fixture.default)
            .unwrap_err();
        assert!(error.starts_with("tools.migrateDataFailed?"));
        let fixed = fixture.paths.record().unwrap().unwrap();
        assert_eq!(fixed.token, fixture.record.token);
        assert_eq!(fixed.phase, ResetPhase::Failed);
        assert_eq!(fixed.error.as_deref(), Some(error.as_str()));
        assert!(!fixed.pending());
        let retained = legacy.record().unwrap().unwrap();
        assert_eq!(retained.token, old.token);
        assert_eq!(retained.phase, ResetPhase::Pending);
        assert_eq!(
            fs::read(fixture.source.join("config.toml")).unwrap(),
            b"preserve source"
        );
        assert!(!fixture.target.exists());
        assert!(!fixture.paths.parent.join("data-location.json").exists());
        legacy.open_lock().unwrap().try_lock().unwrap();
    }

    #[test]
    fn invalid_target_legacy_journal_marks_fixed_request_failed() {
        let fixture = migration_fixture(false);
        let legacy =
            ResetPaths::with_control(&fixture.default, fixture.default.parent().unwrap()).unwrap();
        fs::write(
            legacy.parent.join(".OC2ModManager-reset.json"),
            b"damaged journal",
        )
        .unwrap();
        fixture.paths.write_record(&fixture.record).unwrap();
        assert!(
            exclusive_for_request_at(&fixture.paths, &fixture.record, &fixture.default).is_err()
        );
        assert_eq!(
            fixture.paths.record().unwrap().unwrap().phase,
            ResetPhase::Failed
        );
        assert_eq!(
            fs::read(fixture.source.join("config.toml")).unwrap(),
            b"preserve source"
        );
        assert!(!fixture.target.exists());
        assert_eq!(
            fs::read(legacy.parent.join(".OC2ModManager-reset.json")).unwrap(),
            b"damaged journal"
        );
    }

    #[test]
    fn completed_old_clear_suppresses_legacy_resurrection() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("old/OC2ModManager");
        let old = ResetPaths::with_control(&root, root.parent().unwrap()).unwrap();
        let new = ResetPaths::with_control(&root, &temp.path().join("bootstrap")).unwrap();
        let bytes = format!(
            r#"{{"token":"{}","owner_pid":0,"owner_started":0,"phase":"completed","error":null}}"#,
            uuid::Uuid::new_v4()
        );
        std::fs::write(old.parent.join(".OC2ModManager-reset.json"), bytes).unwrap();
        import_from(&new, &old).unwrap();
        let record = new.record().unwrap().unwrap();
        assert_eq!(record.phase, ResetPhase::Completed);
        assert!(matches!(record.action, DataAction::Clear));
        assert_eq!(record.data_root, Some(old.root));
        assert!(super::super::completed_reset_token(Some(&record)).is_some());
    }

    #[test]
    fn pending_old_clear_finishes_before_import_and_cannot_repeat() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("old/OC2ModManager");
        let old = ResetPaths::with_control(&root, root.parent().unwrap()).unwrap();
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("config.toml"), b"old").unwrap();
        let new = ResetPaths::with_control(&root, &temp.path().join("bootstrap")).unwrap();
        let record = ResetRecord {
            token: uuid::Uuid::new_v4(),
            owner_pid: u32::MAX,
            owner_started: 0,
            phase: ResetPhase::Pending,
            error: None,
            reported: false,
            locale: None,
            failed_from: None,
            action: DataAction::Clear,
            fingerprint: None,
            data_root: None,
        };
        old.write_record(&record).unwrap();
        import_from(&new, &old).unwrap();
        assert!(!root.exists());
        assert_eq!(old.record().unwrap().unwrap().phase, ResetPhase::Completed);
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("config.toml"), b"fresh").unwrap();
        old.finish(old.record().unwrap().unwrap()).unwrap();
        assert_eq!(std::fs::read(root.join("config.toml")).unwrap(), b"fresh");
    }

    #[test]
    fn newer_legacy_pending_is_not_hidden_by_a_completed_fixed_record() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("old/OC2ModManager");
        let old = ResetPaths::with_control(&root, root.parent().unwrap()).unwrap();
        let new = ResetPaths::with_control(&root, &temp.path().join("bootstrap")).unwrap();
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("config.toml"), b"old").unwrap();
        let mut record = ResetRecord {
            token: uuid::Uuid::new_v4(),
            owner_pid: u32::MAX,
            owner_started: 0,
            phase: ResetPhase::Completed,
            error: None,
            reported: false,
            locale: None,
            failed_from: None,
            action: DataAction::Clear,
            fingerprint: None,
            data_root: None,
        };
        new.write_record(&record).unwrap();
        record.token = uuid::Uuid::new_v4();
        record.phase = ResetPhase::Pending;
        let token = record.token;
        old.write_record(&record).unwrap();
        import_from(&new, &old).unwrap();
        assert!(!root.exists());
        assert_eq!(new.record().unwrap().unwrap().token, token);
        assert_eq!(old.record().unwrap().unwrap().phase, ResetPhase::Completed);
    }
}
