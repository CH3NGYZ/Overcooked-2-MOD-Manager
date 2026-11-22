use super::*;
use crate::config::ConfigService;
use std::fs;

fn fixture() -> StagedInstaller {
    let bytes = b"non-executable staging test fixture";
    let work = tempfile::tempdir().unwrap();
    let source = work.path().join(MANAGER_EXECUTABLE);
    fs::write(work.path().join(INSTALLER_FILE), bytes).unwrap();
    StagedInstaller {
        destination: source.clone(),
        source,
        work,
        version: "9.0.0".into(),
        digest: archive::sha256_reader(bytes.as_slice()).unwrap(),
    }
}

#[test]
fn staging_lock_excludes_file_changes_until_the_installer_is_started() {
    let staged = fixture();
    let path = staged.path();
    let lock = staged.lock_verified_file().unwrap();
    assert!(File::open(&path).is_ok());
    assert!(OpenOptions::new().write(true).open(&path).is_err());
    assert!(fs::rename(&path, path.with_extension("changed")).is_err());
    drop(lock);
    fs::write(&path, b"different payload").unwrap();
    assert_eq!(staged.lock_verified_file().unwrap_err(), UNAVAILABLE);
    fs::remove_file(&path).unwrap();
    assert_eq!(staged.lock_verified_file().unwrap_err(), UNAVAILABLE);
}

#[test]
fn explicit_apply_requires_a_staged_installer_but_normal_exit_can_continue_without_one() {
    let work = tempfile::tempdir().unwrap();
    let state = AppState::with_config(ConfigService::for_test(work.path().to_path_buf()));
    assert_eq!(handoff(&state, true).unwrap_err(), UNAVAILABLE);
    assert!(handoff(&state, false).is_ok());
}

#[test]
fn renaming_the_manager_cannot_silently_update_a_different_executable() {
    let work = tempfile::tempdir().unwrap();
    let source = work.path().join(MANAGER_EXECUTABLE);
    assert_eq!(validated_destination(&source), Ok(source));
    assert_eq!(
        validated_destination(&work.path().join("copied-manager.exe")),
        Err("update.formatUnsupported".into())
    );
}
