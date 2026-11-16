use super::storage::{ResetPaths, ResetPhase, ResetRecord, wait_for_lock};
use super::*;
use std::fs;
use std::process::{Command, Stdio};
use std::time::Instant;

fn fixture() -> (tempfile::TempDir, ResetPaths) {
    let temp = tempfile::tempdir().unwrap();
    let paths = ResetPaths::new(&temp.path().join("OC2ModManager")).unwrap();
    fs::create_dir_all(&paths.root).unwrap();
    (temp, paths)
}

fn request() -> ResetRecord {
    ResetRecord {
        token: uuid::Uuid::new_v4(),
        owner_pid: u32::MAX,
        owner_started: 0,
        phase: ResetPhase::Pending,
        error: None,
        reported: false,
    }
}

#[test]
fn clears_every_data_kind_without_touching_game_or_parent_files() {
    let (temp, paths) = fixture();
    let game = temp.path().join("Overcooked2/BepInEx/plugins/user.dll");
    fs::create_dir_all(game.parent().unwrap()).unwrap();
    fs::write(&game, b"installed mod").unwrap();
    fs::write(temp.path().join("unrelated.txt"), b"keep").unwrap();
    for file in [
        "config.toml",
        "logs/rust.log",
        "cache/a",
        "manifests/mod.json",
        "backups/a",
        "recycle-bin/a",
        "arbitrary-nested/user-data",
    ] {
        let path = paths.root.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"old data").unwrap();
    }
    let record = request();
    paths.write_record(&record).unwrap();
    paths.finish(record).unwrap();
    assert!(!paths.root.exists());
    assert_eq!(fs::read(game).unwrap(), b"installed mod");
    assert_eq!(
        fs::read(temp.path().join("unrelated.txt")).unwrap(),
        b"keep"
    );
    assert_eq!(
        paths.record().unwrap().unwrap().phase,
        ResetPhase::Completed
    );
}

#[test]
fn empty_missing_root_is_success_and_completed_record_cannot_reset_new_data() {
    let (_temp, paths) = fixture();
    fs::remove_dir(&paths.root).unwrap();
    let record = request();
    paths.write_record(&record).unwrap();
    paths.finish(record).unwrap();
    fs::create_dir(&paths.root).unwrap();
    fs::write(paths.root.join("config.toml"), b"new session").unwrap();
    paths.finish(paths.record().unwrap().unwrap()).unwrap();
    assert_eq!(
        fs::read(paths.root.join("config.toml")).unwrap(),
        b"new session"
    );
}

#[test]
fn interrupted_delete_only_resumes_its_owned_quarantine() {
    let (_temp, paths) = fixture();
    fs::write(paths.root.join("old"), b"pending").unwrap();
    let mut record = request();
    record.phase = ResetPhase::Deleting;
    let quarantine = paths
        .parent
        .join(format!(".OC2ModManager-clearing-{}", record.token));
    fs::rename(&paths.root, &quarantine).unwrap();
    paths.write_record(&record).unwrap();
    paths.finish(record).unwrap();
    assert!(!quarantine.exists());
    assert!(!paths.root.exists());
}

#[test]
fn deleting_without_quarantine_preserves_rebuilt_root_and_stops_retrying() {
    let (_temp, paths) = fixture();
    let mut record = request();
    record.phase = ResetPhase::Deleting;
    // The old delete completed, then the process died before recording Completed.
    fs::remove_dir(&paths.root).unwrap();
    paths.write_record(&record).unwrap();
    fs::create_dir(&paths.root).unwrap();
    fs::write(paths.root.join("new-config.toml"), b"new session").unwrap();
    assert_eq!(paths.finish(record), Err(UNSAFE.into()));
    let failed = paths.record().unwrap().unwrap();
    assert_eq!(failed.phase, ResetPhase::Failed);
    paths.finish(failed).unwrap();
    assert_eq!(
        fs::read(paths.root.join("new-config.toml")).unwrap(),
        b"new session"
    );
}

#[test]
fn deleting_with_quarantine_only_removes_old_data_and_preserves_rebuilt_root() {
    let (_temp, paths) = fixture();
    fs::write(paths.root.join("old-config.toml"), b"old session").unwrap();
    let mut record = request();
    record.phase = ResetPhase::Deleting;
    let quarantine = paths
        .parent
        .join(format!(".OC2ModManager-clearing-{}", record.token));
    fs::rename(&paths.root, &quarantine).unwrap();
    paths.write_record(&record).unwrap();
    fs::create_dir(&paths.root).unwrap();
    fs::write(paths.root.join("new-config.toml"), b"new session").unwrap();
    paths.finish(record).unwrap();
    assert!(!quarantine.exists());
    assert_eq!(
        paths.record().unwrap().unwrap().phase,
        ResetPhase::Completed
    );
    assert_eq!(
        fs::read(paths.root.join("new-config.toml")).unwrap(),
        b"new session"
    );
}

#[test]
fn failed_reset_keeps_original_data_and_does_not_retry() {
    let (_temp, paths) = fixture();
    fs::write(paths.root.join("old"), b"preserve").unwrap();
    let record = request();
    let quarantine = paths
        .parent
        .join(format!(".OC2ModManager-clearing-{}", record.token));
    fs::create_dir(&quarantine).unwrap();
    fs::write(quarantine.join("recovery"), b"preserve recovery").unwrap();
    paths.write_record(&record).unwrap();
    assert!(paths.finish(record).is_err());
    let failed = paths.record().unwrap().unwrap();
    assert_eq!(failed.phase, ResetPhase::Failed);
    assert!(!failed.pending());
    paths.finish(failed).unwrap();
    assert_eq!(fs::read(paths.root.join("old")).unwrap(), b"preserve");
    assert_eq!(
        fs::read(quarantine.join("recovery")).unwrap(),
        b"preserve recovery"
    );
}

#[test]
fn unsafe_roots_and_overlapping_game_or_executable_are_rejected() {
    let (temp, paths) = fixture();
    assert!(ResetPaths::new(Path::new("OC2ModManager")).is_err());
    assert!(ResetPaths::new(&temp.path().join("wrong-name")).is_err());
    assert_eq!(
        paths.protect(&paths.root.join("OC2ModManager.exe")),
        Err(UNSAFE.into())
    );
    assert_eq!(paths.protect(&paths.parent), Err(UNSAFE.into()));
    assert!(paths.protect(&temp.path().join("game")).is_ok());
}

#[test]
fn final_validation_reloads_game_path_changed_during_helper_preparation() {
    let (temp, paths) = fixture();
    let state = crate::app_state::AppState::with_config(crate::config::ConfigService::for_test(
        paths.root.clone(),
    ));
    let executable = temp.path().join("manager.exe");
    let mut config = crate::config::default_config();
    config.game.path = temp.path().join("game").to_string_lossy().into_owned();
    state.config.save(&config).unwrap();
    validate_protected_paths(&paths, &state, &executable).unwrap();
    // Match save_settings/set_game_path: commit a changed game root under operation
    // after the initial check but before the destructive handoff.
    {
        let _operation = state.operation.lock().unwrap();
        config.game.path = paths.root.join("game").to_string_lossy().into_owned();
        state.config.save(&config).unwrap();
    }
    let _operation = state.operation.lock().unwrap();
    assert_eq!(
        validate_protected_paths(&paths, &state, &executable),
        Err(UNSAFE.into())
    );
    assert!(paths.root.join("config.toml").exists());
    assert!(paths.record().unwrap().is_none());
}

#[test]
fn final_protected_path_validation_is_under_operation_before_helper_commit() {
    let source = include_str!("../manager_data.rs");
    let clear = source.split("pub(crate) fn clear_and_exit").nth(1).unwrap();
    let final_lock = clear
        .rfind("let _operation = state.operation.try_lock()")
        .unwrap();
    let validation = clear
        .rfind("validate_protected_paths(&paths, state, &executable)?;")
        .unwrap();
    let commit = clear.find("helper.commit(&paths)?;").unwrap();
    assert!(final_lock < validation && validation < commit);
    assert!(!clear[final_lock..commit].contains("drop(_operation)"));
}

#[test]
fn helper_preparation_failure_neither_clears_data_nor_commits_a_request() {
    let (_temp, paths) = fixture();
    fs::write(paths.root.join("config.toml"), b"preserve").unwrap();
    assert!(
        handoff::PreparedHelper::start(&paths, &paths.parent.join("missing-helper.exe")).is_err()
    );
    assert!(paths.record().unwrap().is_none());
    assert_eq!(
        fs::read(paths.root.join("config.toml")).unwrap(),
        b"preserve"
    );
}

#[test]
fn shared_sessions_allow_secondary_launch_but_exclusive_cleanup_waits() {
    let (_temp, paths) = fixture();
    let first = paths.open_lock().unwrap();
    let second = paths.open_lock().unwrap();
    let cleanup = paths.open_lock().unwrap();
    first.lock_shared().unwrap();
    second.try_lock_shared().unwrap();
    assert!(matches!(
        cleanup.try_lock(),
        Err(fs::TryLockError::WouldBlock)
    ));
    drop(first);
    assert!(matches!(
        cleanup.try_lock(),
        Err(fs::TryLockError::WouldBlock)
    ));
    drop(second);
    cleanup.try_lock().unwrap();
    let startup = paths.open_lock().unwrap();
    assert!(matches!(
        startup.try_lock_shared(),
        Err(fs::TryLockError::WouldBlock)
    ));
    drop(cleanup);
    startup.try_lock_shared().unwrap();
}

#[test]
fn cleanup_waits_for_old_writer_then_blocks_new_start_until_finished() {
    let (temp, paths) = fixture();
    fs::write(paths.root.join("old"), b"old session").unwrap();
    let old = paths.open_lock().unwrap();
    old.lock_shared().unwrap();
    let mut helper = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "manager_data::tests::reset_lock_child",
            "--ignored",
            "--nocapture",
        ])
        .env("OC2_RESET_FIXTURE", temp.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let started = temp.path().join("child-started");
    wait_until(|| started.exists());
    assert!(!temp.path().join("child-exclusive").exists());
    assert!(paths.root.join("old").exists());
    drop(old);
    wait_until(|| temp.path().join("child-exclusive").exists());
    let next = paths.open_lock().unwrap();
    assert!(matches!(
        next.try_lock_shared(),
        Err(fs::TryLockError::WouldBlock)
    ));
    fs::write(temp.path().join("child-proceed"), b"go").unwrap();
    assert!(helper.wait().unwrap().success());
    wait_for_lock(&next, true, Duration::from_secs(2)).unwrap();
    assert!(!paths.root.exists());
}

fn wait_until(ready: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while !ready() {
        assert!(Instant::now() < deadline, "fixture child did not progress");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[ignore = "fixture-only child process entry"]
fn reset_lock_child() {
    let base = std::env::var_os("OC2_RESET_FIXTURE").expect("test fixture only");
    let paths = ResetPaths::new(&Path::new(&base).join("OC2ModManager")).unwrap();
    fs::write(paths.parent.join("child-started"), b"ready").unwrap();
    let lock = paths.open_lock().unwrap();
    wait_for_lock(&lock, false, Duration::from_secs(8)).unwrap();
    fs::write(paths.parent.join("child-exclusive"), b"held").unwrap();
    wait_until(|| paths.parent.join("child-proceed").exists());
    let record = request();
    paths.write_record(&record).unwrap();
    paths.finish(record).unwrap();
}

#[test]
fn owner_identity_reuse_does_not_wait_for_unrelated_pid() {
    let mut record = request();
    record.owner_pid = std::process::id();
    record.owner_started = u64::MAX;
    handoff::wait_for_owner(&record).unwrap();
}

#[test]
fn owner_exit_after_first_live_refresh_does_not_wait_for_cached_process() {
    let (temp, _paths) = fixture();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "manager_data::tests::owner_exit_child",
            "--ignored",
            "--nocapture",
        ])
        .env("OC2_RESET_FIXTURE", temp.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    wait_until(|| temp.path().join("owner-ready").exists());
    let mut system = sysinfo::System::new();
    let pid = sysinfo::Pid::from_u32(child.id());
    assert!(system.refresh_process(pid));
    let mut record = request();
    record.owner_pid = child.id();
    record.owner_started = system.process(pid).unwrap().start_time();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        let _ = done_tx.send(handoff::wait_for_owner(&record));
    });
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    // Keep the child alive across several 50ms refreshes, then really exit it.
    assert!(done_rx.recv_timeout(Duration::from_millis(200)).is_err());
    fs::write(temp.path().join("owner-exit"), b"exit").unwrap();
    assert!(child.wait().unwrap().success());
    done_rx
        .recv_timeout(Duration::from_secs(3))
        .unwrap()
        .unwrap();
}

#[test]
#[ignore = "fixture-only child process entry"]
fn owner_exit_child() {
    let base = std::env::var_os("OC2_RESET_FIXTURE").expect("test fixture only");
    let base = Path::new(&base);
    fs::write(base.join("owner-ready"), b"alive").unwrap();
    wait_until(|| base.join("owner-exit").exists());
}

#[cfg(unix)]
#[test]
fn root_symlink_is_rejected_and_child_symlink_never_reaches_game() {
    use std::os::unix::fs::symlink;
    let (temp, paths) = fixture();
    let game = temp.path().join("game");
    fs::create_dir(&game).unwrap();
    fs::write(game.join("mod.dll"), b"keep").unwrap();
    fs::remove_dir(&paths.root).unwrap();
    symlink(&game, &paths.root).unwrap();
    assert_eq!(paths.validate_root(), Err(UNSAFE.into()));
    fs::remove_file(&paths.root).unwrap();
    fs::create_dir(&paths.root).unwrap();
    symlink(&game, paths.root.join("game-link")).unwrap();
    let record = request();
    paths.write_record(&record).unwrap();
    paths.finish(record).unwrap();
    assert_eq!(fs::read(game.join("mod.dll")).unwrap(), b"keep");
    let record = request();
    let quarantine = paths
        .parent
        .join(format!(".OC2ModManager-clearing-{}", record.token));
    symlink(&game, &quarantine).unwrap();
    paths.write_record(&record).unwrap();
    assert!(paths.finish(record).is_err());
    assert!(
        !paths.root.exists(),
        "failure must not restore an unowned link"
    );
    assert_eq!(fs::read(game.join("mod.dll")).unwrap(), b"keep");
}

#[cfg(windows)]
#[test]
fn root_junction_is_rejected_and_child_junction_never_reaches_game() {
    let (temp, paths) = fixture();
    let game = temp.path().join("game");
    fs::create_dir(&game).unwrap();
    fs::write(game.join("mod.dll"), b"keep").unwrap();
    fs::remove_dir(&paths.root).unwrap();
    let junction = |source: &Path, target: &Path| {
        // Fixture-controlled absolute paths only; no deletion crosses shells.
        assert!(
            Command::new("cmd.exe")
                .args(["/d", "/c", "mklink", "/J"])
                .arg(source)
                .arg(target)
                .stdout(Stdio::null())
                .status()
                .unwrap()
                .success()
        );
    };
    junction(&paths.root, &game);
    assert_eq!(paths.validate_root(), Err(UNSAFE.into()));
    fs::remove_dir(&paths.root).unwrap();
    fs::create_dir(&paths.root).unwrap();
    junction(&paths.root.join("game-link"), &game);
    let record = request();
    paths.write_record(&record).unwrap();
    paths.finish(record).unwrap();
    assert_eq!(fs::read(game.join("mod.dll")).unwrap(), b"keep");
    let record = request();
    let quarantine = paths
        .parent
        .join(format!(".OC2ModManager-clearing-{}", record.token));
    junction(&quarantine, &game);
    paths.write_record(&record).unwrap();
    assert!(paths.finish(record).is_err());
    assert!(
        !paths.root.exists(),
        "failure must not restore an unowned link"
    );
    assert_eq!(fs::read(game.join("mod.dll")).unwrap(), b"keep");
}
