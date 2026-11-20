use super::*;
use std::os::windows::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const FIXTURE: &str = "OC2_ELEVATION_TEST_DIRECTORY";
const ID: &str = "OC2_ELEVATION_TEST_IDENTIFIER";
const ROLE: &str = "OC2_ELEVATION_TEST_ROLE";
const TOKEN: &str = "OC2_ELEVATION_TEST_TOKEN";

struct FixtureChild(Child);

impl Drop for FixtureChild {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn fixture_child(
    directory: &Path,
    identifier: &str,
    role: &str,
    handoff: Option<(&ManagerProcess, uuid::Uuid)>,
) -> FixtureChild {
    let mut command = Command::new(std::env::current_exe().unwrap());
    if let Some((owner, token)) = handoff {
        command
            .env("OC2_ELEVATION_TEST_OWNER_PID", owner.pid.to_string())
            .env("OC2_ELEVATION_TEST_OWNER_TIME", owner.created.to_string())
            .env(TOKEN, token.to_string());
    }
    let child = command
        .args([
            "--ignored",
            "--exact",
            "elevation::tests::single_instance_fixture",
        ])
        .env(FIXTURE, directory)
        .env(ID, identifier)
        .env(ROLE, role)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .spawn()
        .unwrap();
    FixtureChild(child)
}

fn wait_for_file(file: &Path, child: &mut FixtureChild) -> io::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(8);
    while !file.exists() {
        if let Some(status) = child.0.try_wait()? {
            return Err(io::Error::other(format!("Exited before setup: {status}")));
        }
        if Instant::now() >= deadline {
            return Err(io::ErrorKind::TimedOut.into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

#[test]
fn successor_reaches_tauri_setup_after_the_old_instance_exits() {
    let fixture = tempfile::tempdir().unwrap();
    let directory = fixture.path();
    let identifier = format!("com.oc2.handoff-test-{}", uuid::Uuid::new_v4());
    let mut owner = fixture_child(directory, &identifier, "owner", None);
    wait_for_file(&directory.join("owner"), &mut owner).unwrap();
    let created = std::fs::read_to_string(directory.join("owner"))
        .unwrap()
        .parse()
        .unwrap();
    let manager = ManagerProcess {
        pid: owner.0.id(),
        created,
    };
    // A regular launch still uses the single-instance lock and cannot enter setup.
    let mut regular = fixture_child(directory, &identifier, "ordinary", None);
    let error = wait_for_file(&directory.join("ordinary"), &mut regular).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Other);
    assert!(
        error
            .to_string()
            .contains("Exited before setup: exit code: 0")
    );

    let token = uuid::Uuid::new_v4();
    let events = HandoffEvents::named(token, true).unwrap();
    let mut successor = fixture_child(directory, &identifier, "successor", Some((&manager, token)));
    events
        .commit_when_ready(successor.0.as_raw_handle().cast(), 8_000)
        .unwrap();
    assert!(successor.0.try_wait().unwrap().is_none());
    assert!(!directory.join("successor").exists());
    // Stretch the old incarnation's exit window to make overlap deterministic.
    std::fs::write(directory.join("exit-owner"), b"exit").unwrap();
    wait_for_file(&directory.join("successor"), &mut successor).unwrap();
    assert!(successor.0.wait().unwrap().success());
    assert!(owner.0.wait().unwrap().success());
}

#[test]
fn owner_wait_is_bounded_and_never_waits_on_a_reused_pid_or_wrong_executable() {
    let executable = std::env::current_exe().unwrap();
    let manager = ManagerProcess::current().unwrap();
    assert!(
        ManagerProcess::parse(
            Some(manager.pid.to_string().into()),
            Some(manager.created.to_string().into())
        )
        .is_err()
    );
    assert_eq!(
        ManagerProcess {
            pid: manager.pid,
            created: manager.created - 1
        }
        .open(&executable)
        .unwrap_err()
        .kind(),
        io::ErrorKind::InvalidInput,
    );
    assert_eq!(
        wait_for(
            &ManagerProcess::current()
                .unwrap()
                .open(&executable)
                .unwrap(),
            25,
            "still running"
        )
        .unwrap_err()
        .kind(),
        io::ErrorKind::TimedOut
    );
    let fixture = tempfile::tempdir().unwrap();
    let wrong_executable = fixture.path().join("other.exe");
    std::fs::write(&wrong_executable, b"unrelated").unwrap();
    assert_eq!(
        ManagerProcess::current()
            .unwrap()
            .open(&wrong_executable)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
}

#[test]
fn early_child_exit_or_timeout_does_not_commit_the_restart() {
    let fixture = tempfile::tempdir().unwrap();
    let directory = fixture.path();
    let token = uuid::Uuid::new_v4();
    let events = HandoffEvents::named(token, true).unwrap();
    assert!(HandoffEvents::named(token, true).is_err());
    let mut child = fixture_child(directory, "unused", "early-exit", None);
    let error = events
        .commit_when_ready(child.0.as_raw_handle().cast(), 8_000)
        .unwrap_err();
    assert!(error.to_string().contains("exited before it was ready"));
    assert!(child.0.wait().unwrap().success());
    assert_eq!(
        wait_for(&events.proceed, 0, "not committed")
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut
    );
    let current = ManagerProcess::current()
        .unwrap()
        .open(&std::env::current_exe().unwrap())
        .unwrap();
    assert_eq!(
        events
            .commit_when_ready(raw(&current), 25)
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut
    );
    // A late-ready child must still not enter setup after the abandoned attempt.
    let child_events = HandoffEvents::named(token, false).unwrap();
    assert_eq!(
        child_events
            .yield_until_owner_exits(&current, 25)
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut
    );
}

#[test]
fn cancelled_elevation_and_failed_file_recovery_leave_the_previous_instance_running() {
    let fixture = tempfile::tempdir().unwrap();
    let mut child = fixture_child(fixture.path(), "unused", "cancel", None);
    wait_for_file(&fixture.path().join("cancel-complete"), &mut child).unwrap();
    assert!(child.0.wait().unwrap().success());
}

fn cancel_relaunch(directory: &Path) {
    use crate::{archive, commands::system, level_sets::recompress};
    recompress::set_game_running(false);
    // An active update rejects elevation without touching the native launch.
    let update = archive::begin_app_update().unwrap();
    assert_eq!(
        system::relaunch_with(|| panic!("must not start during update"))
            .err()
            .unwrap(),
        "update.inProgress"
    );
    drop(update);
    let result = system::relaunch_with(|| {
        assert_eq!(
            system::request_user_exit().err().unwrap(),
            "update.inProgress"
        );
        assert!(archive::begin_app_update().is_err());
        assert!(!recompress::take_rescan_request());
        Err(io::Error::from_raw_os_error(
            winapi::shared::winerror::ERROR_CANCELLED as i32,
        ))
    })
    .unwrap();
    assert!(!result.ok);
    assert_eq!(result.message, "update.elevationFailed");
    assert!(archive::begin_app_update().is_ok());
    assert!(recompress::take_rescan_request());
    assert!(!recompress::take_rescan_request());

    let blocked_parent = directory.join("pending-map");
    let backup = directory.join("retained-backup");
    std::fs::write(&backup, b"original").unwrap();
    let map = blocked_parent.join("map");
    let _failed_write = recompress::PendingRecoveryFixture::new(map.clone(), backup.clone());
    assert_eq!(
        system::relaunch_with(|| panic!("must restore before launching"))
            .err()
            .unwrap(),
        "assetBundle.recoveryPending"
    );
    assert_eq!(std::fs::read(&backup).unwrap(), b"original");
    assert!(archive::begin_app_update().is_ok());
    std::fs::create_dir(&blocked_parent).unwrap();
    let result = system::relaunch_with(|| Err(io::ErrorKind::BrokenPipe.into())).unwrap();
    assert!(!result.ok);
    assert_eq!(std::fs::read(map).unwrap(), b"original");
    assert!(recompress::take_rescan_request());
    std::fs::write(directory.join("cancel-complete"), b"passed").unwrap();
}

#[test]
#[ignore = "spawned by the isolated single-instance test"]
fn single_instance_fixture() {
    let directory = std::path::PathBuf::from(std::env::var_os(FIXTURE).unwrap());
    let role = std::env::var(ROLE).unwrap();
    match role.as_str() {
        "early-exit" => return,
        "cancel" => return cancel_relaunch(&directory),
        "successor" => wait_for_relaunch(
            [
                "OC2_ELEVATION_TEST_OWNER_PID",
                "OC2_ELEVATION_TEST_OWNER_TIME",
                TOKEN,
            ]
            .into_iter()
            .map(|name| std::env::var_os(name).unwrap()),
            true,
        )
        .unwrap(),
        _ => (),
    }
    let mut context = tauri::generate_context!();
    context.config_mut().identifier = std::env::var(ID).unwrap();
    context.config_mut().app.windows.clear();
    tauri::Builder::default()
        .any_thread()
        .plugin(tauri_plugin_single_instance::init(|_, _, _| {}))
        .setup(move |_| {
            std::fs::write(
                directory.join(&role),
                ManagerProcess::current()?.created.to_string(),
            )?;
            if role == "owner" {
                std::thread::spawn(move || {
                    while !directory.join("exit-owner").exists() {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    std::process::exit(0);
                });
                Ok(())
            } else {
                std::process::exit(0)
            }
        })
        .build(context)
        .unwrap()
        .run(|_, _| {});
}
