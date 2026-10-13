use crate::{game, install_guard};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const BINARY_NAMES: &[&str] = &["OC2ModManager.exe", "uninstall.exe"];

#[derive(Serialize, Deserialize)]
pub(crate) struct Migration {
    pub(crate) source: PathBuf,
    pub(crate) target: PathBuf,
    work: PathBuf,
    shared_plugins: bool,
}

#[cfg(any(windows, test))]
pub(crate) fn update_executable(executable: &Path) -> Result<PathBuf, String> {
    let directory = executable.parent().ok_or("安装目录无效")?;
    if !install_guard::is_plugins_directory(directory)? {
        install_guard::validate_install_dir(directory)?;
        return Ok(executable.to_owned());
    }
    let target = dirs::data_local_dir()
        .ok_or("无法确定默认安装目录")?
        .join("OC2ModManager");
    install_guard::validate_install_dir(&target)?;
    Ok(target.join("OC2ModManager.exe"))
}

fn resolved(path: &Path) -> Result<PathBuf, String> {
    game::resolve_existing_ancestor(&std::path::absolute(path).map_err(|e| e.to_string())?)
}

fn disjoint(source: &Path, target: &Path) -> Result<(), String> {
    let source = resolved(source)?;
    let target = resolved(target)?;
    #[cfg(windows)]
    let (source, target) = (
        PathBuf::from(source.to_string_lossy().to_lowercase()),
        PathBuf::from(target.to_string_lossy().to_lowercase()),
    );
    if source.starts_with(&target) || target.starts_with(&source) {
        return Err("迁移目录重叠，已停止更新".into());
    }
    Ok(())
}

// Only program files are relocated. Legacy installation-local data stays put.
fn reject_link(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("迁移数据包含目录链接，已保留原文件并停止更新".into());
        }
    }
    if metadata.file_type().is_symlink() {
        return Err("迁移数据包含目录链接，已保留原文件并停止更新".into());
    }
    Ok(())
}

fn copy_tree(source: &Path, target: &Path) -> Result<(), String> {
    reject_link(source)?;
    if source.is_dir() {
        fs::create_dir_all(target).map_err(|e| e.to_string())?;
        for entry in fs::read_dir(source).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            copy_tree(&entry.path(), &target.join(entry.file_name()))?;
        }
    } else {
        fs::copy(source, target).map_err(|e| e.to_string())?;
    }
    Ok(())
}

impl Migration {
    pub(crate) fn prepare(source: &Path, target: &Path, work: &Path) -> Result<Self, String> {
        if !install_guard::is_plugins_directory(source)?
            || !source.join("OC2ModManager.exe").is_file()
        {
            return Err("原目录没有需要迁移的管理器".into());
        }
        install_guard::validate_install_dir(target)?;
        disjoint(source, target)?;
        disjoint(source, work)?;
        disjoint(target, work)?;
        let source = resolved(source)?;
        let target = resolved(target)?;
        let snapshot = work.join("source");
        fs::create_dir_all(&snapshot).map_err(|e| e.to_string())?;
        for name in BINARY_NAMES {
            let path = source.join(name);
            if path.exists() {
                copy_tree(&path, &snapshot.join(name))?;
            }
        }
        let shared_plugins = source
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("plugins"));
        Ok(Self {
            source,
            target,
            work: resolved(work)?,
            shared_plugins,
        })
    }

    pub(crate) fn restore_old_binaries(&self) -> Result<(), String> {
        self.validate()?;
        fs::create_dir_all(&self.source).map_err(|e| e.to_string())?;
        for name in BINARY_NAMES {
            let snapshot = self.work.join("source").join(name);
            let target = self.source.join(name);
            if snapshot.exists() && !target.exists() {
                copy_tree(&snapshot, &target)?;
            }
        }
        Ok(())
    }

    pub(crate) fn cleanup(&self) -> Result<(), String> {
        self.validate()?;
        install_guard::validate_install_dir(&self.target)?;
        if !install_guard::is_plugins_directory(&self.source)? {
            return Err("原安装目录发生变化，已保留原文件".into());
        }
        disjoint(&self.source, &self.target)?;
        for name in BINARY_NAMES {
            // Generic directories inside plugins itself may belong to other MODs.
            if self.shared_plugins && self.source.join(name).is_dir() {
                continue;
            }
            remove_unchanged(
                &self.source.join(name),
                &self.work.join("source").join(name),
            )?;
        }
        // Never recursively delete the installation directory or the plugins container.
        if !self.shared_plugins {
            let _ = fs::remove_dir(&self.source);
        }
        Ok(())
    }

    fn validate(&self) -> Result<(), String> {
        let same_path = |left: &Path, right: &Path| {
            #[cfg(windows)]
            {
                left.to_string_lossy()
                    .eq_ignore_ascii_case(&right.to_string_lossy())
            }
            #[cfg(not(windows))]
            {
                left == right
            }
        };
        if !same_path(&resolved(&self.source)?, &self.source)
            || !same_path(&resolved(&self.target)?, &self.target)
            || !same_path(&resolved(&self.work)?, &self.work)
            || !install_guard::is_plugins_directory(&self.source)?
            || self.shared_plugins
                != self
                    .source
                    .file_name()
                    .is_some_and(|name| name.eq_ignore_ascii_case("plugins"))
        {
            return Err("迁移路径或记录无效，已保留原文件".into());
        }
        install_guard::validate_install_dir(&self.target)?;
        disjoint(&self.source, &self.target)?;
        disjoint(&self.source, &self.work)?;
        disjoint(&self.target, &self.work)
    }
}

fn remove_unchanged(path: &Path, snapshot: &Path) -> Result<(), String> {
    if !path.try_exists().map_err(|e| e.to_string())? || !snapshot.exists() {
        return Ok(());
    }
    reject_link(path)?;
    reject_link(snapshot)?;
    if path.is_dir() && snapshot.is_dir() {
        for entry in fs::read_dir(snapshot).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            remove_unchanged(&path.join(entry.file_name()), &entry.path())?;
        }
        let _ = fs::remove_dir(path);
    } else if path.is_file() && snapshot.is_file() {
        use sha2::{Digest, Sha256};
        use std::io::Read;
        let digest = |path: &Path| -> Result<_, String> {
            let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
            let mut hasher = Sha256::new();
            let mut buffer = [0; 64 * 1024];
            loop {
                let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
                if count == 0 {
                    break;
                }
                hasher.update(&buffer[..count]);
            }
            Ok(hasher.finalize())
        };
        if digest(path)? == digest(snapshot)? {
            fs::remove_file(path).map_err(|e| e.to_string())?;
        }
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

// Installer-only relocation entry point, shared by legacy clients and Updater.
pub(crate) fn run_if_requested() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relocation_rejects_stale_or_wrong_installed_versions() {
        assert!(version_matches("1.1.0", "1.1.0.0"));
        assert!(version_matches("1.2.3-beta.1", "1.2.3.1001"));
        for actual in ["1.0.9.0", "1.1.1.0", "1.1.0", "garbage"] {
            assert!(!version_matches("1.1.0", actual));
        }
    }

    #[cfg(windows)]
    #[test]
    fn confirms_matching_relocated_process_and_rejects_a_launch_that_exits() {
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

    struct Workspace(PathBuf);
    impl Workspace {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("oc2-migration-test-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(root.join("game/BepInEx/plugins/Manager")).unwrap();
            Self(root)
        }
        fn source(&self) -> PathBuf {
            self.0.join("game/BepInEx/plugins/Manager")
        }
        fn target(&self) -> PathBuf {
            self.0.join("default/OC2ModManager")
        }
        fn work(&self) -> PathBuf {
            self.0.join("work")
        }
        fn write(&self, path: &Path, bytes: &[u8]) {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
        fn prepare(&self) -> Migration {
            self.write(&self.source().join("OC2ModManager.exe"), b"old manager");
            Migration::prepare(&self.source(), &self.target(), &self.work()).unwrap()
        }
    }
    impl Drop for Workspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn redirects_only_plugins_installations_to_the_default_directory() {
        let workspace = Workspace::new();
        let normal = workspace.target().join("OC2ModManager.exe");
        assert_eq!(update_executable(&normal).unwrap(), normal);
        let unsafe_exe = workspace.source().join("OC2ModManager.exe");
        assert_eq!(
            update_executable(&unsafe_exe).unwrap(),
            dirs::data_local_dir()
                .unwrap()
                .join("OC2ModManager/OC2ModManager.exe")
        );
    }

    #[test]
    fn fresh_install_keeps_legacy_data_and_removes_only_manager_binaries() {
        let workspace = Workspace::new();
        for name in [
            "config.toml",
            "config.previous.toml",
            "manifests/mod.json",
            "recycle-bin/item/data.dll",
            "backups/framework/plugins/mod.dll",
            "logs/rust.log",
            "cache/mod.zip",
        ] {
            workspace.write(&workspace.source().join(name), b"source data");
        }
        workspace.write(
            &workspace.source().join("uninstall.exe"),
            b"old uninstaller",
        );
        workspace.write(&workspace.source().join("AnotherMod.dll"), b"unrelated MOD");
        workspace.write(&workspace.target().join("config.toml"), b"previous config");
        workspace.write(&workspace.target().join("foreign.txt"), b"keep");
        let migration = workspace.prepare();
        assert_eq!(
            fs::read(workspace.source().join("OC2ModManager.exe")).unwrap(),
            b"old manager"
        );
        assert_eq!(
            fs::read(workspace.target().join("config.toml")).unwrap(),
            b"previous config"
        );
        assert!(!workspace.work().join("source/config.toml").exists());
        migration.cleanup().unwrap();
        assert!(!workspace.source().join("OC2ModManager.exe").exists());
        assert!(!workspace.source().join("uninstall.exe").exists());
        assert_eq!(
            fs::read(workspace.source().join("config.toml")).unwrap(),
            b"source data"
        );
        assert!(workspace.source().join("backups").is_dir());
        assert_eq!(
            fs::read(workspace.source().join("AnotherMod.dll")).unwrap(),
            b"unrelated MOD"
        );
        assert_eq!(
            fs::read(workspace.target().join("foreign.txt")).unwrap(),
            b"keep"
        );
    }

    #[test]
    fn preserves_generic_directories_when_manager_is_in_plugins_itself() {
        let workspace = Workspace::new();
        let plugins = workspace.source().parent().unwrap().to_owned();
        workspace.write(&plugins.join("OC2ModManager.exe"), b"manager");
        workspace.write(&plugins.join("config.toml"), b"manager config");
        workspace.write(&plugins.join("logs/other-mod.log"), b"foreign log");
        workspace.write(&plugins.join("OtherMod.dll"), b"foreign MOD");
        let migration =
            Migration::prepare(&plugins, &workspace.target(), &workspace.work()).unwrap();
        migration.cleanup().unwrap();
        assert!(!plugins.join("OC2ModManager.exe").exists());
        assert!(plugins.is_dir());
        assert_eq!(
            fs::read(plugins.join("logs/other-mod.log")).unwrap(),
            b"foreign log"
        );
        assert_eq!(
            fs::read(plugins.join("OtherMod.dll")).unwrap(),
            b"foreign MOD"
        );
    }

    #[test]
    fn relocation_does_not_read_or_replace_old_configuration() {
        let workspace = Workspace::new();
        workspace.write(&workspace.source().join("config.toml"), b"source config");
        workspace.write(
            &workspace.source().join("config.previous.toml"),
            b"source previous",
        );
        workspace.write(&workspace.target().join("config.toml"), b"target config");
        let migration = workspace.prepare();
        assert!(!migration.work.join("source/config.previous.toml").exists());
        assert_eq!(
            fs::read(workspace.target().join("config.toml")).unwrap(),
            b"target config"
        );
        assert!(!workspace.target().join("config.previous.toml").exists());
        assert_eq!(
            fs::read(workspace.source().join("config.toml")).unwrap(),
            b"source config"
        );
        assert!(workspace.source().join("OC2ModManager.exe").is_file());
    }

    #[test]
    fn restores_binaries_removed_by_an_unsuccessful_installer() {
        let workspace = Workspace::new();
        workspace.write(
            &workspace.source().join("uninstall.exe"),
            b"old uninstaller",
        );
        let migration = workspace.prepare();
        fs::remove_file(workspace.source().join("OC2ModManager.exe")).unwrap();
        fs::remove_file(workspace.source().join("uninstall.exe")).unwrap();
        migration.restore_old_binaries().unwrap();
        assert_eq!(
            fs::read(workspace.source().join("OC2ModManager.exe")).unwrap(),
            b"old manager"
        );
        assert_eq!(
            fs::read(workspace.source().join("uninstall.exe")).unwrap(),
            b"old uninstaller"
        );
    }

    #[test]
    fn cleanup_preserves_files_changed_since_the_snapshot_and_new_files() {
        let workspace = Workspace::new();
        workspace.write(&workspace.source().join("config.toml"), b"old config");
        let migration = workspace.prepare();
        workspace.write(&workspace.source().join("config.toml"), b"changed config");
        workspace.write(&workspace.source().join("new-mod.dll"), b"new MOD");
        migration.cleanup().unwrap();
        assert_eq!(
            fs::read(workspace.source().join("config.toml")).unwrap(),
            b"changed config"
        );
        assert_eq!(
            fs::read(workspace.source().join("new-mod.dll")).unwrap(),
            b"new MOD"
        );
    }

    #[test]
    fn rejects_forged_records_and_unsafe_or_overlapping_destinations() {
        let workspace = Workspace::new();
        let mut migration = workspace.prepare();
        migration.source = workspace.target();
        assert!(migration.cleanup().is_err());
        assert!(
            Migration::prepare(
                &workspace.source(),
                &workspace.source().join("new"),
                &workspace.work()
            )
            .is_err()
        );
        assert!(workspace.source().join("OC2ModManager.exe").exists());
    }

    #[cfg(windows)]
    #[test]
    fn leaves_linked_legacy_data_untouched_without_traversing_it() {
        let workspace = Workspace::new();
        workspace.write(
            &workspace.source().join("OC2ModManager.exe"),
            b"old manager",
        );
        let link = workspace.source().join("backups");
        let junction = std::process::Command::new("cmd")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(link.to_string_lossy().replace('/', "\\"))
            .arg(workspace.source().to_string_lossy().replace('/', "\\"))
            .output()
            .unwrap();
        assert!(
            junction.status.success(),
            "{}",
            String::from_utf8_lossy(&junction.stderr)
        );
        let migration =
            Migration::prepare(&workspace.source(), &workspace.target(), &workspace.work())
                .unwrap();
        assert!(!migration.work.join("source/backups").exists());
        assert!(!workspace.target().exists());
        assert!(workspace.source().join("OC2ModManager.exe").is_file());
        migration.cleanup().unwrap();
        assert!(fs::symlink_metadata(&link).is_ok());
        fs::remove_dir(link).unwrap();
    }
}
