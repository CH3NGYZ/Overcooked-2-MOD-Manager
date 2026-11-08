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
    let directory = executable
        .parent()
        .ok_or("migrate.invalidInstallDirectory")?;
    if !install_guard::is_plugins_directory(directory)? {
        install_guard::validate_install_dir(directory)?;
        return Ok(executable.to_owned());
    }
    let target = dirs::data_local_dir()
        .ok_or("migrate.defaultDirectoryUnknown")?
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
        return Err("migrate.directoriesOverlap".into());
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
            return Err("migrate.containsLinks".into());
        }
    }
    if metadata.file_type().is_symlink() {
        return Err("migrate.containsLinks".into());
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
            return Err("migrate.noManagerFiles".into());
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
            return Err("migrate.sourceChanged".into());
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
            return Err("migrate.recordInvalid".into());
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
    command.current_dir(
        executable
            .parent()
            .ok_or("migrate.invalidInstallDirectory")?,
    );
    hide_console(&mut command);
    command.spawn().map_err(|error| {
        crate::locale_tag::with_params("migrate.launchFailed", &[("reason", &error.to_string())])
    })?;
    if !wait_for_application(executable, Duration::from_secs(10)) {
        return Err("migrate.newProcessNotStable".into());
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
        _ => Err("migrate.invalidArguments".into()),
    }
}

fn finish_legacy_migration(migration: &Migration) -> Result<(), String> {
    let executable = migration.target.join("OC2ModManager.exe");
    let actual = crate::file_metadata::read_file_version(&executable)
        .ok_or("migrate.installedVersionUnreadable")?;
    if !version_matches(env!("CARGO_PKG_VERSION"), &actual) {
        return Err("migrate.versionMismatch".into());
    }
    let log = crate::user_paths::logs_dir()?.join("update.log");
    fs::create_dir_all(log.parent().ok_or("migrate.logDirectoryInvalid")?)
        .map_err(|e| e.to_string())?;
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
            return Err("migrate.oldProcessStillRunning".into());
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
#[path = "update_migration/tests.rs"]
mod tests;
