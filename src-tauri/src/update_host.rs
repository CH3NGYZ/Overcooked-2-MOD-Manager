use crate::{commands::installer_update_command, game, update_migration::Migration};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const HOST_FLAG: &str = "--oc2-update-host";

#[derive(Serialize, Deserialize)]
struct UpdatePlan {
    parent_pid: u32,
    installer: PathBuf,
    installer_type: String,
    executable: PathBuf,
    #[serde(default)]
    target_executable: Option<PathBuf>,
    expected_version: String,
    failure_message: String,
    ready: PathBuf,
    log: PathBuf,
}

#[cfg(windows)]
pub async fn start(
    installer: &Path,
    installer_type: &str,
    executable: &Path,
    version: &str,
    failure_message: Option<&str>,
) -> Result<(), String> {
    let target_executable = crate::update_migration::update_executable(executable)?;
    let work = crate::user_paths::updates_dir()?.join(format!("host-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&work).map_err(|_| "无法准备更新助手")?;
    let plan = UpdatePlan {
        parent_pid: std::process::id(),
        installer: installer.to_owned(),
        installer_type: installer_type.into(),
        executable: executable.to_owned(),
        target_executable: Some(target_executable),
        expected_version: version.into(),
        failure_message: failure_message
            .unwrap_or("更新未能完成或启动，请手动打开管理器或运行已下载的安装器。详细信息：")
            .into(),
        ready: work.join("ready"),
        log: crate::user_paths::logs_dir()?.join("update.log"),
    };
    // A distinct image name survives the installer's taskkill /IM command.
    // Run before Tauri initialization, retaining the parent's elevated token.
    let host = work.join("OC2UpdateHost.exe");
    let plan_path = work.join("plan.json");
    fs::copy(executable, &host).map_err(|_| "无法准备更新助手")?;
    fs::write(
        &plan_path,
        serde_json::to_vec(&plan).map_err(|_| "无法准备更新助手")?,
    )
    .map_err(|_| "无法准备更新助手")?;
    let mut command = Command::new(&host);
    command.arg(HOST_FLAG).arg(&plan_path);
    hide_console(&mut command);
    let mut child = command.spawn().map_err(|_| "无法启动更新助手")?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        // Check the child first: a stale ready marker must not approve exit.
        if child
            .try_wait()
            .map_err(|_| "无法确认更新助手状态")?
            .is_some()
        {
            return Err("更新助手启动失败，请查看 logs/update.log".into());
        }
        if plan.ready.is_file() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("更新助手启动超时，当前程序保持运行".into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

pub fn run_if_requested() -> bool {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new(HOST_FLAG)) {
        return false;
    }
    let Some(path) = args.next().map(PathBuf::from) else {
        return true;
    };
    let Ok(bytes) = fs::read(&path) else {
        return true;
    };
    let Ok(plan) = serde_json::from_slice::<UpdatePlan>(&bytes) else {
        return true;
    };
    if let Err(error) = run(&plan) {
        append_log(&plan.log, &format!("FAILED: {error}"));
        show_failure(&plan, &error);
    }
    true
}

fn run(plan: &UpdatePlan) -> Result<(), String> {
    let parent = ParentProcess::open(plan.parent_pid)?;
    fs::create_dir_all(plan.log.parent().ok_or("更新日志目录无效")?)
        .map_err(|_| "无法创建更新日志")?;
    // Verify that result logging works before allowing the manager to exit.
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&plan.log)
        .map_err(|_| "无法创建更新日志")?;
    append_log(
        &plan.log,
        &format!(
            "READY: waiting for PID {}; target version {}",
            plan.parent_pid, plan.expected_version
        ),
    );
    fs::write(&plan.ready, b"ready").map_err(|_| "无法确认更新助手就绪")?;
    parent.wait(Duration::from_secs(30))?;
    let target = plan
        .target_executable
        .as_deref()
        .unwrap_or(&plan.executable);
    let migration = if target != plan.executable {
        if target.is_file() {
            wait_until_stopped(target)?;
        }
        let migration = Migration::prepare(
            plan.executable.parent().ok_or("安装目录无效")?,
            target.parent().ok_or("安装目录无效")?,
            &plan
                .ready
                .parent()
                .ok_or("更新临时目录无效")?
                .join("migration"),
        )?;
        append_log(
            &plan.log,
            &format!("Relocating manager to {}", migration.target.display()),
        );
        Some(migration)
    } else {
        None
    };
    let result = install_and_launch(plan, target, migration.as_ref());
    if result.is_err() {
        if let Some(migration) = &migration {
            migration.restore_old_binaries()?;
        }
    }
    result
}

fn install_and_launch(
    plan: &UpdatePlan,
    target: &Path,
    migration: Option<&Migration>,
) -> Result<(), String> {
    append_log(&plan.log, "Old manager exited; starting installer");
    let mut command = installer_update_command(
        &plan.installer,
        &plan.installer_type,
        target,
        migration.is_none(),
    )?;
    if plan.installer_type == "msi" {
        let msi_log = plan.log.with_file_name("update-msi.log");
        command.arg("/norestart").arg("/L*v").arg(msi_log);
    }
    hide_console(&mut command);
    let status = command
        .status()
        .map_err(|error| format!("无法启动更新安装器: {error}"))?;
    let code = status.code().ok_or("更新安装器异常结束")?;
    append_log(&plan.log, &format!("Installer exit code: {code}"));
    check_installer_exit(&plan.installer_type, code)?;
    let actual = game::read_file_version(target).ok_or("无法读取安装后的程序版本")?;
    if !version_matches(&plan.expected_version, &actual) {
        return Err(format!(
            "安装后版本不符: expected={}, actual={actual}",
            plan.expected_version
        ));
    }
    append_log(&plan.log, &format!("Installed version verified: {actual}"));
    launch_and_confirm(target, &plan.log)?;
    if let Some(migration) = migration {
        migration.cleanup()?;
        append_log(&plan.log, "Migration completed; old manager files removed");
    }
    Ok(())
}

fn launch_and_confirm(executable: &Path, log: &Path) -> Result<(), String> {
    // NSIS/MSI retain their automatic launch flags. Observe the result before
    // retrying so that there is no competing second startup/update process.
    if wait_for_application(executable, Duration::from_secs(10)) {
        append_log(log, "New manager process confirmed");
        return Ok(());
    }
    append_log(log, "Installer did not launch manager; retrying launch");
    let mut command = Command::new(executable);
    command.current_dir(executable.parent().ok_or("安装目录无效")?);
    hide_console(&mut command);
    command
        .spawn()
        .map_err(|error| format!("无法启动更新后的程序: {error}"))?;
    if !wait_for_application(executable, Duration::from_secs(10)) {
        return Err("安装成功，但新版本未能保持运行，请手动启动并查看日志".into());
    }
    append_log(log, "New manager process confirmed after retry");
    Ok(())
}

// New NSIS installers use this entry point for legacy clients that still pass
// their unsafe installation directory to /D during automatic updates.
pub(crate) fn run_migration_if_requested() {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new("--oc2-migrate-update")) {
        return;
    }
    let arguments = args.collect::<Vec<_>>();
    let result = migration_command(&arguments);
    if let Err(error) = &result {
        eprintln!("{error}");
    }
    std::process::exit(if result.is_ok() { 0 } else { 1 });
}

fn migration_command(args: &[std::ffi::OsString]) -> Result<(), String> {
    match args.first().and_then(|arg| arg.to_str()) {
        Some("prepare") if args.len() == 4 => {
            let source = Path::new(&args[1]);
            wait_until_stopped(&source.join("OC2ModManager.exe"))?;
            let target_exe = Path::new(&args[2]).join("OC2ModManager.exe");
            if target_exe.is_file() {
                wait_until_stopped(&target_exe)?;
            }
            let work = crate::user_paths::updates_dir()?
                .join(format!("relocate-{}", uuid::Uuid::new_v4()));
            let migration = Migration::prepare(source, Path::new(&args[2]), &work)?;
            fs::write(
                Path::new(&args[3]),
                serde_json::to_vec(&migration).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            fs::write(
                work.join("migration.json"),
                serde_json::to_vec(&migration).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())
        }
        Some(action @ ("finish" | "restore")) if args.len() == 2 => {
            let bytes = fs::read(Path::new(&args[1])).map_err(|e| e.to_string())?;
            let migration: Migration = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            if action == "restore" {
                return migration.restore_old_binaries();
            }
            let result = finish_legacy_migration(&migration);
            if result.is_err() {
                migration.restore_old_binaries()?;
            }
            result
        }
        _ => Err("迁移更新参数无效".into()),
    }
}

fn finish_legacy_migration(migration: &Migration) -> Result<(), String> {
    let executable = migration.target.join("OC2ModManager.exe");
    let actual = game::read_file_version(&executable).ok_or("无法读取安装后的程序版本")?;
    if !version_matches(env!("CARGO_PKG_VERSION"), &actual) {
        return Err("迁移后版本不符，已保留原管理器".into());
    }
    let log = crate::user_paths::logs_dir()?.join("update.log");
    fs::create_dir_all(log.parent().ok_or("更新日志目录无效")?).map_err(|e| e.to_string())?;
    launch_and_confirm(&executable, &log)?;
    migration.cleanup()?;
    append_log(&log, "Migration completed; old manager files removed");
    Ok(())
}

fn wait_until_stopped(executable: &Path) -> Result<(), String> {
    use sysinfo::{ProcessRefreshKind, System, UpdateKind};
    let target = fs::canonicalize(executable).map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut system = System::new();
    loop {
        system.refresh_processes_specifics(ProcessRefreshKind::new().with_exe(UpdateKind::Always));
        if !system.processes().values().any(|process| {
            process
                .exe()
                .and_then(|path| fs::canonicalize(path).ok())
                .as_ref()
                == Some(&target)
        }) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("旧程序未能及时退出，未开始迁移".into());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn check_installer_exit(kind: &str, code: i32) -> Result<(), String> {
    if code == 0 || (kind == "msi" && matches!(code, 1641 | 3010)) {
        Ok(())
    } else {
        Err(format!("更新安装失败，安装器退出码: {code}"))
    }
}

fn version_matches(expected: &str, actual: &str) -> bool {
    let Ok(expected) = semver::Version::parse(expected) else {
        return false;
    };
    let parts = actual
        .split('.')
        .map(str::parse::<u64>)
        .collect::<Result<Vec<_>, _>>();
    parts.is_ok_and(|parts| {
        parts.len() == 4 && parts[..3] == [expected.major, expected.minor, expected.patch]
    })
}

fn wait_for_application(executable: &Path, timeout: Duration) -> bool {
    use sysinfo::{ProcessRefreshKind, System, UpdateKind};
    let target = fs::canonicalize(executable).ok();
    let mut system = System::new();
    let deadline = Instant::now() + timeout;
    let mut seen = None;
    while Instant::now() < deadline {
        system.refresh_processes_specifics(ProcessRefreshKind::new().with_exe(UpdateKind::Always));
        let running = target.as_ref().is_some_and(|target| {
            system.processes().values().any(|process| {
                if executable.file_name().is_none_or(|name| {
                    !process.name().eq_ignore_ascii_case(&name.to_string_lossy())
                }) {
                    return false;
                }
                process
                    .exe()
                    .and_then(|path| fs::canonicalize(path).ok())
                    .as_ref()
                    == Some(target)
            })
        });
        if running {
            let since = seen.get_or_insert_with(Instant::now);
            if since.elapsed() >= Duration::from_secs(2) {
                return true;
            }
        } else {
            seen = None;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    false
}

fn append_log(path: &Path, message: &str) {
    if path
        .metadata()
        .is_ok_and(|meta| meta.len() > 8 * 1024 * 1024)
    {
        let _ = fs::rename(path, path.with_extension("log.1"));
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "[{}] {message}", chrono::Utc::now().to_rfc3339());
        let _ = file.flush();
    }
}

fn hide_console(command: &mut Command) {
    command.stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
}

#[cfg(windows)]
struct ParentProcess(winapi::um::winnt::HANDLE);

#[cfg(windows)]
impl ParentProcess {
    fn open(pid: u32) -> Result<Self, String> {
        let handle = unsafe { winapi::um::processthreadsapi::OpenProcess(0x00100000, 0, pid) };
        if handle.is_null() {
            return Err("无法监听旧程序退出，未开始安装".into());
        }
        Ok(Self(handle))
    }
    fn wait(&self, timeout: Duration) -> Result<(), String> {
        let result = unsafe {
            winapi::um::synchapi::WaitForSingleObject(self.0, timeout.as_millis() as u32)
        };
        if result == 0 {
            Ok(())
        } else {
            Err("旧程序未能及时退出，未开始安装".into())
        }
    }
}

#[cfg(windows)]
impl Drop for ParentProcess {
    fn drop(&mut self) {
        unsafe {
            winapi::um::handleapi::CloseHandle(self.0);
        }
    }
}

#[cfg(not(windows))]
struct ParentProcess;
#[cfg(not(windows))]
impl ParentProcess {
    fn open(_: u32) -> Result<Self, String> {
        Err("此更新方式仅支持 Windows".into())
    }
    fn wait(&self, _: Duration) -> Result<(), String> {
        Err("此更新方式仅支持 Windows".into())
    }
}

fn show_failure(plan: &UpdatePlan, error: &str) {
    #[cfg(windows)]
    {
        use winapi::um::winuser::{MB_ICONERROR, MB_OK, MessageBoxW};
        let text = format!(
            "{}\n{error}\n\n{}\n{}",
            plan.failure_message,
            plan.log.display(),
            plan.installer.display()
        );
        let wide = |text: &str| text.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                wide(&text).as_ptr(),
                wide("OC2 MOD Manager").as_ptr(),
                MB_OK | MB_ICONERROR,
            );
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (plan, error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_only_documented_success_codes_for_each_installer() {
        for code in [0, 1641, 3010] {
            assert!(check_installer_exit("msi", code).is_ok());
        }
        assert!(check_installer_exit("exe", 0).is_ok());
        for code in [1, 2, 1603, 1618, 3010] {
            assert!(check_installer_exit("exe", code).is_err());
        }
        assert!(check_installer_exit("msi", 1603).is_err());
    }
    #[test]
    fn detects_stale_or_wrong_installed_versions() {
        assert!(version_matches("1.1.0", "1.1.0.0"));
        assert!(version_matches("1.2.3-beta.1", "1.2.3.1001"));
        for actual in ["1.0.9.0", "1.1.1.0", "1.1.0", "garbage"] {
            assert!(!version_matches("1.1.0", actual));
        }
    }
    #[cfg(windows)]
    #[test]
    fn waits_for_real_process_handle_to_exit() {
        let mut command = Command::new("cmd.exe");
        command.args(["/C", "ping", "-n", "2", "127.0.0.1", ">nul"]);
        hide_console(&mut command);
        let mut child = command.spawn().unwrap();
        let parent = ParentProcess::open(child.id()).unwrap();
        assert!(parent.wait(Duration::from_millis(10)).is_err());
        parent.wait(Duration::from_secs(5)).unwrap();
        assert!(child.wait().unwrap().success());
    }

    #[cfg(windows)]
    #[test]
    fn failed_installer_is_detected_after_parent_exit_and_logged() {
        let directory =
            std::env::temp_dir().join(format!("oc2-update-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let target = directory.join("OC2ModManager.exe");
        fs::write(&target, b"old executable fixture").unwrap();
        let mut parent_command = Command::new("cmd.exe");
        parent_command.args(["/C", "ping", "-n", "2", "127.0.0.1", ">nul"]);
        hide_console(&mut parent_command);
        let mut parent = parent_command.spawn().unwrap();
        let plan = UpdatePlan {
            parent_pid: parent.id(),
            // where.exe rejects installer-only switches and returns a real error.
            installer: PathBuf::from("where.exe"),
            installer_type: "exe".into(),
            executable: target.clone(),
            target_executable: None,
            expected_version: "9.0.0".into(),
            failure_message: "test".into(),
            ready: directory.join("ready"),
            log: directory.join("logs/update.log"),
        };
        let error = run(&plan).unwrap_err();
        assert!(error.contains("安装器退出码"), "{error}");
        assert!(parent.try_wait().unwrap().is_some());
        assert!(plan.ready.is_file());
        let log = fs::read_to_string(&plan.log).unwrap();
        assert!(log.contains("Old manager exited; starting installer"));
        assert!(log.contains("Installer exit code:"));
        assert!(!log.contains("New manager process confirmed"));
        assert_eq!(fs::read(target).unwrap(), b"old executable fixture");
        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn failed_relocation_installer_keeps_the_old_manager_and_data() {
        let directory = std::env::temp_dir().join(format!(
            "oc2-update-migration-test-{}",
            uuid::Uuid::new_v4()
        ));
        let source = directory.join("game/BepInEx/plugins/Manager");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("OC2ModManager.exe"), b"original manager").unwrap();
        fs::write(source.join("config.toml"), b"original config").unwrap();
        let target = directory.join("default/OC2ModManager.exe");
        let mut command = Command::new("cmd.exe");
        command.args(["/C", "ping", "-n", "2", "127.0.0.1", ">nul"]);
        hide_console(&mut command);
        let mut parent = command.spawn().unwrap();
        let plan = UpdatePlan {
            parent_pid: parent.id(),
            installer: PathBuf::from("where.exe"),
            installer_type: "exe".into(),
            executable: source.join("OC2ModManager.exe"),
            target_executable: Some(target.clone()),
            expected_version: "9.0.0".into(),
            failure_message: "test".into(),
            ready: directory.join("ready"),
            log: directory.join("update.log"),
        };
        assert!(run(&plan).unwrap_err().contains("安装器退出码"));
        parent.wait().unwrap();
        assert_eq!(
            fs::read(source.join("OC2ModManager.exe")).unwrap(),
            b"original manager"
        );
        assert_eq!(
            fs::read(source.join("config.toml")).unwrap(),
            b"original config"
        );
        assert!(!target.exists());
        assert!(!directory.join("migration/source/config.toml").exists());
        assert!(
            !fs::read_to_string(&plan.log)
                .unwrap()
                .contains("old manager files removed")
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn confirms_matching_process_and_rejects_a_launch_that_exits() {
        let directory =
            std::env::temp_dir().join(format!("oc2-launch-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let fixture = directory.join("OC2LaunchFixture.exe");
        let cmd = PathBuf::from(std::env::var_os("WINDIR").unwrap()).join("System32/cmd.exe");
        fs::copy(cmd, &fixture).unwrap();
        let mut command = Command::new(&fixture);
        command.args(["/C", "ping", "-n", "20", "127.0.0.1", ">nul"]);
        hide_console(&mut command);
        let mut child = command.spawn().unwrap();
        let confirmed = wait_for_application(&fixture, Duration::from_secs(5));
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(confirmed);
        let mut command = Command::new(&fixture);
        command.args(["/C", "exit", "0"]);
        hide_console(&mut command);
        command.spawn().unwrap().wait().unwrap();
        assert!(!wait_for_application(&fixture, Duration::from_secs(3)));
        fs::remove_dir_all(directory).unwrap();
    }
}
