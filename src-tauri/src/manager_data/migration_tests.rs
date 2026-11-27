use super::*;
use crate::manager_data::{completed_reset_token, storage::DataAction};

struct Fixture {
    _temp: tempfile::TempDir,
    paths: ResetPaths,
    source: PathBuf,
    target: PathBuf,
    pointer: PathBuf,
    record: ResetRecord,
}

fn fixture() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let base = fs::canonicalize(temp.path()).unwrap();
    let source = base.join("old/OC2ModManager");
    let target = base.join("new/OC2ModManager");
    fs::create_dir_all(source.join("logs")).unwrap();
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(source.join("logs/rust.log"), b"complete logs").unwrap();
    fs::write(source.join("config.toml"), b"settings and encrypted tokens").unwrap();
    let paths = ResetPaths::with_control(&source, &base.join("bootstrap")).unwrap();
    let pointer = paths.parent.join("data-location.json");
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
    Fixture {
        _temp: temp,
        paths,
        source,
        target,
        pointer,
        record,
    }
}

#[test]
fn copies_all_data_verifies_then_publishes_pointer_and_removes_old_root() {
    let f = fixture();
    for name in [
        "manifests/a",
        "backups/a",
        "cache/a",
        "recycle-bin/a",
        "custom/folder/a",
    ] {
        let path = f.source.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, name).unwrap();
    }
    finish_at(&f.paths, f.record, &f.source, &f.target, &f.pointer).unwrap();
    assert!(!f.source.exists());
    assert_eq!(
        fs::read(f.target.join("logs/rust.log")).unwrap(),
        b"complete logs"
    );
    assert_eq!(
        fs::read(f.target.join("recycle-bin/a")).unwrap(),
        b"recycle-bin/a"
    );
    assert_eq!(
        crate::user_paths::read_location_at(&f.pointer, &f.source).unwrap(),
        f.target
    );
    let record = f.paths.record().unwrap().unwrap();
    assert_eq!(record.phase, ResetPhase::Completed);
    assert_eq!(
        completed_reset_token(Some(&record), &f.target),
        None,
        "migration must retain WebView preferences"
    );
    assert!(!stage(&f.target, record.token).exists());
    assert!(!retired(&f.source, record.token).exists());
}

#[test]
fn preparation_reports_existing_child_but_allows_a_nonempty_parent() {
    let f = fixture();
    let state = crate::app_state::AppState::with_config(crate::config::ConfigService::for_test(
        f.source.clone(),
    ));
    let executable = f._temp.path().join("program/app.exe");
    fs::create_dir(executable.parent().unwrap()).unwrap();
    let parent = f.target.parent().unwrap();
    let sibling = parent.join("other notes.txt");
    fs::write(&sibling, b"keep").unwrap();
    fs::create_dir(&f.target).unwrap();
    assert_eq!(
        prepare(&f.paths, parent, &state, &executable),
        Err(existing_target_error(&f.target)),
    );
    assert!(f.paths.record().unwrap().is_none());
    assert!(f.target.is_dir());
    fs::remove_dir(&f.target).unwrap();
    assert_eq!(prepare(&f.paths, parent, &state, &executable), Ok(f.target));
    assert_eq!(fs::read(sibling).unwrap(), b"keep");
}

fn existing_target_error(target: &Path) -> String {
    let path = crate::display_path::display_path(target);
    format!(
        "tools.migrateDataTargetExists?path={}",
        crate::locale_tag::encode(&path)
    )
}

#[test]
fn existing_destination_is_never_overwritten_even_when_empty() {
    let f = fixture();
    fs::create_dir(&f.target).unwrap();
    let error = existing_target_error(&f.target);
    assert_eq!(
        finish_at(&f.paths, f.record, &f.source, &f.target, &f.pointer),
        Err(error.clone()),
    );
    assert!(f.source.join("config.toml").exists());
    assert_eq!(fs::read_dir(&f.target).unwrap().count(), 0);
    assert!(!f.pointer.exists());
    let record = f.paths.record().unwrap().unwrap();
    assert_eq!(record.phase, ResetPhase::Failed);
    assert_eq!(record.failed_from, Some(ResetPhase::Pending));
    assert_eq!(record.error, Some(error));
}

#[test]
fn rejects_self_nested_bepinex_control_and_relative_destinations() {
    let f = fixture();
    for target in [
        f.source.clone(),
        f.source.join("child/OC2ModManager"),
        f.paths.parent.join("OC2ModManager"),
        f.target.with_file_name("BepInEx").join("OC2ModManager"),
        PathBuf::from("relative/OC2ModManager"),
    ] {
        assert!(
            validate_pair(&f.paths, &f.source, &target).is_err(),
            "{}",
            target.display()
        );
    }
    assert!(validate_pair(&f.paths, &f.target, &f.source).is_ok());
}

#[test]
fn copying_interrupted_before_publish_restarts_private_staging() {
    let f = fixture();
    let mut record = f.record;
    record.phase = ResetPhase::Copying;
    record.fingerprint = Some(Fingerprint::capture(&f.source).unwrap());
    let stage = stage(&f.target, record.token);
    fs::create_dir(&stage).unwrap();
    fs::write(stage.join("partial"), b"incomplete").unwrap();
    finish_at(&f.paths, record, &f.source, &f.target, &f.pointer).unwrap();
    assert!(!f.target.join("partial").exists());
    assert!(f.target.join("config.toml").exists());
}

#[test]
fn modified_source_after_verification_retains_both_trees_without_switching() {
    let f = fixture();
    let mut record = f.record;
    let expected = Fingerprint::capture(&f.source).unwrap();
    transaction::stage_tree(&f.source, &stage(&f.target, record.token), &expected).unwrap();
    record.fingerprint = Some(expected);
    record.phase = ResetPhase::Verified;
    fs::write(f.source.join("config.toml"), b"modified").unwrap();
    assert!(finish_at(&f.paths, record, &f.source, &f.target, &f.pointer).is_err());
    assert!(!f.pointer.exists());
    assert_eq!(fs::read(f.source.join("config.toml")).unwrap(), b"modified");
    assert!(f.target.join("config.toml").exists());
}

#[test]
fn crash_after_publishing_target_or_pointer_resumes_without_overwriting() {
    for published_pointer in [false, true] {
        let f = fixture();
        let mut record = f.record;
        let expected = Fingerprint::capture(&f.source).unwrap();
        transaction::stage_tree(&f.source, &f.target, &expected).unwrap();
        record.phase = ResetPhase::Verified;
        record.fingerprint = Some(expected);
        if published_pointer {
            crate::user_paths::write_location_at(&f.pointer, &f.target).unwrap();
        }
        finish_at(&f.paths, record, &f.source, &f.target, &f.pointer).unwrap();
        assert!(!f.source.exists());
        assert!(f.target.join("config.toml").exists());
    }
}

#[test]
fn crash_after_retiring_old_root_cleans_only_uuid_tree_not_recreated_old_root() {
    let f = fixture();
    let mut record = f.record;
    let expected = Fingerprint::capture(&f.source).unwrap();
    transaction::stage_tree(&f.source, &f.target, &expected).unwrap();
    crate::user_paths::write_location_at(&f.pointer, &f.target).unwrap();
    fs::rename(&f.source, retired(&f.source, record.token)).unwrap();
    fs::create_dir(&f.source).unwrap();
    fs::write(f.source.join("new-data"), b"do not delete").unwrap();
    record.phase = ResetPhase::Cleaning;
    record.fingerprint = Some(expected);
    finish_at(&f.paths, record, &f.source, &f.target, &f.pointer).unwrap();
    assert_eq!(
        fs::read(f.source.join("new-data")).unwrap(),
        b"do not delete"
    );
}

#[test]
fn unavailable_target_parent_keeps_source_and_pointer_unchanged() {
    let f = fixture();
    fs::remove_dir(f.target.parent().unwrap()).unwrap();
    assert!(finish_at(&f.paths, f.record, &f.source, &f.target, &f.pointer).is_err());
    assert!(!f.pointer.exists());
    assert!(f.source.join("config.toml").exists());
}

#[test]
fn clear_request_cannot_delete_another_selected_root() {
    let f = fixture();
    let mut record = f.record;
    record.action = DataAction::Clear;
    record.data_root = Some(f.target);
    assert!(f.paths.finish(record).is_err());
    assert!(f.source.join("config.toml").exists());
}

#[test]
fn switched_or_cleaning_missing_or_changed_target_never_discards_recovery_data() {
    for phase in [ResetPhase::Switched, ResetPhase::Cleaning] {
        for remove_target in [false, true] {
            let f = fixture();
            let mut record = f.record;
            let expected = Fingerprint::capture(&f.source).unwrap();
            transaction::stage_tree(&f.source, &f.target, &expected).unwrap();
            crate::user_paths::write_location_at(&f.pointer, &f.target).unwrap();
            let preserved = if phase == ResetPhase::Cleaning {
                let retired = retired(&f.source, record.token);
                fs::rename(&f.source, &retired).unwrap();
                retired
            } else {
                f.source.clone()
            };
            if remove_target {
                transaction::remove_private_tree(&f.target).unwrap();
            } else {
                fs::write(f.target.join("config.toml"), b"changed after crash").unwrap();
            }
            record.phase = phase;
            record.fingerprint = Some(expected);
            assert!(finish_at(&f.paths, record, &f.source, &f.target, &f.pointer).is_err());
            assert!(preserved.join("config.toml").exists());
            assert_eq!(f.paths.record().unwrap().unwrap().phase, ResetPhase::Failed);
        }
    }
}

#[cfg(windows)]
#[test]
fn root_junction_is_rejected_before_canonicalization_can_hide_it() {
    let f = fixture();
    transaction::remove_private_tree(&f.source).unwrap();
    let real = f.source.with_file_name("important");
    fs::create_dir(&real).unwrap();
    fs::write(real.join("keep"), b"unrelated").unwrap();
    let status = std::process::Command::new("cmd")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(&f.source)
        .arg(&real)
        .output()
        .unwrap();
    assert!(status.status.success());
    assert!(ResetPaths::with_control(&f.source, &f.paths.parent).is_err());
    assert_eq!(fs::read(real.join("keep")).unwrap(), b"unrelated");
    fs::remove_dir(&f.source).unwrap();
}

#[cfg(windows)]
#[test]
fn normalized_windows_path_compares_verbatim_and_regular_same_root() {
    assert_eq!(
        normalized(Path::new(r"\\?\C:\Users\Me\OC2ModManager")),
        normalized(Path::new(r"C:\users\me\OC2ModManager"))
    );
    assert_eq!(
        normalized(Path::new(r"\\?\UNC\Server\Share\OC2ModManager")),
        normalized(Path::new(r"\\server\share\OC2ModManager"))
    );
}
