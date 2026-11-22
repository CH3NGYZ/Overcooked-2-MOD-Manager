use super::storage::{ResetPaths, ResetPhase, ResetRecord};
use super::{HELPER_FLAG, LOCK_TIMEOUT, failed};
use std::fs::{self, OpenOptions};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessRefreshKind, ProcessStatus, System};

pub(super) struct PreparedHelper {
    child: Child,
    token: uuid::Uuid,
    ready: std::path::PathBuf,
    committed: bool,
}

impl PreparedHelper {
    pub(super) fn start(paths: &ResetPaths, executable: &Path) -> Result<Self, String> {
        let token = uuid::Uuid::new_v4();
        let ready = paths.ready(token);
        let mut command = Command::new(executable);
        command
            .arg(HELPER_FLAG)
            .arg(token.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        let mut prepared = Self {
            child: command.spawn().map_err(failed)?,
            token,
            ready,
            committed: false,
        };
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            match fs::symlink_metadata(&prepared.ready) {
                Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                    return Ok(prepared);
                }
                Ok(_) => return Err(super::UNSAFE.into()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(failed(error)),
            }
            if let Some(status) = prepared.child.try_wait().map_err(failed)? {
                return Err(failed(format!(
                    "Reset helper exited before readiness: {status}"
                )));
            }
            if Instant::now() >= deadline {
                return Err(failed("Reset helper did not become ready"));
            }
            std::thread::sleep(Duration::from_millis(40));
        }
    }

    pub(super) fn commit(
        &mut self,
        paths: &ResetPaths,
        action: super::storage::DataAction,
        locale: Option<String>,
    ) -> Result<(), String> {
        if self.child.try_wait().map_err(failed)?.is_some() {
            return Err(failed("Reset helper stopped before handoff"));
        }
        let owner_pid = std::process::id();
        let mut system = System::new();
        system.refresh_process(Pid::from_u32(owner_pid));
        let owner_started = system
            .process(Pid::from_u32(owner_pid))
            .ok_or_else(|| failed("Could not identify the current manager process"))?
            .start_time();
        paths.write_record(&ResetRecord {
            token: self.token,
            owner_pid,
            owner_started,
            phase: ResetPhase::Pending,
            error: None,
            reported: false,
            locale,
            failed_from: None,
            action,
            fingerprint: None,
            data_root: Some(paths.root.clone()),
        })?;
        self.committed = true;
        Ok(())
    }
}

impl Drop for PreparedHelper {
    fn drop(&mut self) {
        if !self.committed {
            let _ = self.child.kill();
            let _ = self.child.wait();
            let _ = fs::remove_file(&self.ready);
        }
    }
}

pub(super) fn run_helper(paths: &ResetPaths, token: uuid::Uuid) -> Result<(), String> {
    paths.validate_root()?;
    let ready = paths.ready(token);
    let _ready = ReadyFile(ready.clone());
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(ready)
        .map_err(failed)?;
    let deadline = Instant::now() + LOCK_TIMEOUT;
    // No journal is changed until the IPC has prepared everything successfully.
    loop {
        if let Some(record) = paths.record()?.filter(|record| record.token == token) {
            if !record.pending() {
                return Ok(());
            }
            break;
        }
        if Instant::now() >= deadline {
            return Err(failed("Reset handoff was not committed"));
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    let lock = paths.open_lock()?;
    loop {
        // A startup may complete this exact request before this helper gets the
        // lock. Do not wait behind its fresh session or delete that new session.
        let record = paths
            .record()?
            .ok_or_else(|| failed("Reset journal disappeared"))?;
        if record.token != token || !record.pending() {
            return Ok(());
        }
        match lock.try_lock() {
            Ok(()) => {
                let record = paths
                    .record()?
                    .ok_or_else(|| failed("Reset journal disappeared"))?;
                if record.token != token || !record.pending() {
                    return Ok(());
                }
                let _legacy = super::legacy::exclusive_for_request(paths, &record)?;
                if let Err(error) = wait_for_owner(&record) {
                    paths.mark_failed(record, &error)?;
                    return Err(error);
                }
                return paths.finish(record);
            }
            Err(fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(fs::TryLockError::WouldBlock) => {
                return Err(failed("A manager process is still using its data"));
            }
            Err(fs::TryLockError::Error(error)) => return Err(failed(error)),
        }
    }
}

struct ReadyFile(std::path::PathBuf);
impl Drop for ReadyFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

pub(super) fn wait_for_owner(record: &ResetRecord) -> Result<(), String> {
    let pid = Pid::from_u32(record.owner_pid);
    let mut system = System::new();
    let deadline = Instant::now() + LOCK_TIMEOUT;
    loop {
        let refreshed = system.refresh_process_specifics(pid, ProcessRefreshKind::new());
        let alive = refreshed
            && system.process(pid).is_some_and(|process| {
                process.start_time() == record.owner_started
                    && !matches!(
                        process.status(),
                        ProcessStatus::Zombie | ProcessStatus::Dead
                    )
            });
        if !alive {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(failed("The previous manager process did not exit"));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

pub(super) fn show_error(message: &str) {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let wide = |text: &str| {
            std::ffi::OsStr::new(text)
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<_>>()
        };
        unsafe {
            winapi::um::winuser::MessageBoxW(
                std::ptr::null_mut(),
                wide(message).as_ptr(),
                wide("OC2 MOD Manager").as_ptr(),
                winapi::um::winuser::MB_OK | winapi::um::winuser::MB_ICONERROR,
            );
        }
    }
    #[cfg(target_os = "macos")]
    {
        // argv passes diagnostics as data rather than interpolating AppleScript.
        let _ = Command::new("/usr/bin/osascript").args([
            "-e", "on run argv\ndisplay alert \"OC2 MOD Manager\" message (item 1 of argv) as critical\nend run",
            message,
        ]).status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelling_prepared_helper_stops_it_without_committing_or_clearing() {
        let temp = tempfile::tempdir().unwrap();
        let paths = ResetPaths::new(&temp.path().join("OC2ModManager")).unwrap();
        fs::create_dir(&paths.root).unwrap();
        fs::write(paths.root.join("config.toml"), b"preserve").unwrap();
        let child = Command::new(std::env::current_exe().unwrap())
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
        let pid = Pid::from_u32(child.id());
        let token = uuid::Uuid::new_v4();
        let ready = paths.ready(token);
        fs::write(&ready, b"ready").unwrap();
        let helper = PreparedHelper {
            child,
            token,
            ready: ready.clone(),
            committed: false,
        };
        drop(helper);
        let mut system = System::new();
        let deadline = Instant::now() + Duration::from_secs(3);
        while system.refresh_process(pid)
            && system.process(pid).is_some_and(|process| {
                !matches!(
                    process.status(),
                    ProcessStatus::Dead | ProcessStatus::Zombie
                )
            })
        {
            // Windows can briefly retain a terminated process object after wait.
            assert!(Instant::now() < deadline, "prepared helper is still alive");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!ready.exists());
        assert!(paths.record().unwrap().is_none());
        assert_eq!(
            fs::read(paths.root.join("config.toml")).unwrap(),
            b"preserve"
        );
    }
}
