use super::UpdateTarget;
use semver::Version;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;

const ATTEMPT: &str = "attempt.json";
const OUTCOME: &str = "outcome.json";
const LOCK: &str = "work.lock";
const MAX_BYTES: u64 = 32 * 1024;
const INVALID: &str = "update.installFailedManual";
const SCHEMA: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Outcome {
    Pending,
    Abandoned,
    Failed,
    Installed,
    RestartFailed,
    RebootRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Installation {
    Failed,
    Applying,
    RebootRequired,
    RestartFailed,
}

impl Installation {
    pub(crate) fn update_block_reason(self) -> &'static str {
        match self {
            Self::Failed => "update.installFailedManual",
            Self::Applying => "update.inProgress",
            Self::RebootRequired => "update.rebootRequired",
            Self::RestartFailed => "update.restartFailed",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct InstallationState {
    pub(crate) status: Installation,
    pub(crate) version: Option<Version>,
}

impl InstallationState {
    fn unbound(status: Installation) -> Self {
        Self {
            status,
            version: None,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Attempt {
    schema: u32,
    pub(crate) id: Uuid,
    pub(crate) source: PathBuf,
    pub(crate) destination: PathBuf,
    pub(crate) version: String,
    pub(crate) source_version: String,
    pub(crate) installer_type: String,
    arch: String,
    pub(crate) restart: bool,
    boot_time: u64,
    #[serde(skip)]
    work: PathBuf,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Status {
    id: Uuid,
    outcome: Outcome,
}

impl Attempt {
    pub(crate) fn begin(
        source: &Path,
        destination: &Path,
        version: &str,
        installer_type: &str,
        restart: bool,
    ) -> Result<Self, String> {
        Self::create(
            &attempts_dir()?,
            source,
            destination,
            version,
            installer_type,
            restart,
        )
    }

    fn create(
        root: &Path,
        source: &Path,
        destination: &Path,
        version: &str,
        installer_type: &str,
        restart: bool,
    ) -> Result<Self, String> {
        let id = Uuid::new_v4();
        let attempt = Self {
            schema: SCHEMA,
            id,
            source: source.to_owned(),
            destination: destination.to_owned(),
            version: version.into(),
            source_version: env!("CARGO_PKG_VERSION").into(),
            installer_type: installer_type.into(),
            arch: std::env::consts::ARCH.into(),
            restart,
            boot_time: sysinfo::System::boot_time(),
            work: root.join(id.to_string()),
        };
        attempt.validate(id)?;
        validate_dir(root)?;
        fs::create_dir_all(root).map_err(receipt_error)?;
        fs::create_dir(&attempt.work).map_err(receipt_error)?;
        validate_dir(root)?;
        write_json(&attempt.work.join(ATTEMPT), &attempt, false)?;
        Ok(attempt)
    }

    #[cfg(windows)]
    pub(crate) fn read(id: Uuid) -> Result<Self, String> {
        Self::read_at(&attempts_dir()?, id)
    }

    fn read_at(root: &Path, id: Uuid) -> Result<Self, String> {
        let work = root.join(id.to_string());
        let attempt = Self {
            work: work.clone(),
            ..read_json(&work.join(ATTEMPT))?.ok_or(INVALID)?
        };
        attempt.validate(id)?;
        Ok(attempt)
    }

    fn validate(&self, id: Uuid) -> Result<(), String> {
        if self.schema != SCHEMA
            || self.id != id
            || id.get_version_num() != 4
            || self.arch != std::env::consts::ARCH
            || !matches!(self.installer_type.as_str(), "exe" | "msi" | "app")
            || !valid_path(&self.source)
            || !valid_path(&self.destination)
            || version(&self.version)?
                .cmp_precedence(&version(&self.source_version)?)
                .is_le()
        {
            return Err(INVALID.into());
        }
        Ok(())
    }

    #[cfg(windows)]
    pub(crate) fn work_path(&self) -> &Path {
        &self.work
    }

    pub(crate) fn lock_work(&self) -> Result<File, String> {
        self.try_lock_work()?
            .ok_or_else(|| "update.inProgress".into())
    }

    fn try_lock_work(&self) -> Result<Option<File>, String> {
        let path = self.work.join(LOCK);
        validate_file(&path)?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            use winapi::um::winbase::FILE_FLAG_OPEN_REPARSE_POINT;
            use winapi::um::winnt::{FILE_SHARE_READ, FILE_SHARE_WRITE};
            options
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
        }
        let lock = options.open(path).map_err(receipt_error)?;
        if !regular(&lock.metadata().map_err(receipt_error)?, false) {
            return Err(INVALID.into());
        }
        match lock.try_lock() {
            Ok(()) => Ok(Some(lock)),
            Err(std::fs::TryLockError::WouldBlock) => Ok(None),
            Err(std::fs::TryLockError::Error(error)) => Err(receipt_error(error)),
        }
    }

    pub(crate) fn record_outcome(&self, outcome: Outcome) -> Result<(), String> {
        write_json(
            &self.work.join(OUTCOME),
            &Status {
                id: self.id,
                outcome,
            },
            true,
        )?;
        log::info!(
            "[update] 安装结果：尝试={}，版本={}，结果={outcome:?}",
            self.id,
            self.version
        );
        Ok(())
    }

    fn outcome(&self) -> Result<Outcome, String> {
        match read_json::<Status>(&self.work.join(OUTCOME))? {
            Some(status) if status.id == self.id => Ok(status.outcome),
            None => Ok(Outcome::Pending),
            _ => Err(INVALID.into()),
        }
    }
}

pub(crate) fn status(target: UpdateTarget) -> Option<InstallationState> {
    read_status(target).unwrap_or_else(|error| {
        log::warn!("[update] 无法核验安装回执，改用手动更新：{error}");
        Some(InstallationState::unbound(Installation::Failed))
    })
}

fn read_status(target: UpdateTarget) -> Result<Option<InstallationState>, String> {
    if tauri::is_dev() {
        return Ok(None);
    }
    #[cfg(windows)]
    let applying = super::staged::windows::installation_in_progress();
    #[cfg(windows)]
    if crate::is_elevated() {
        return Ok(applying.then(|| InstallationState::unbound(Installation::Applying)));
    }
    let recorded = (|| {
        inspect(
            &attempts_dir()?,
            &std::env::current_exe().map_err(receipt_error)?,
            target.kind,
            &version(env!("CARGO_PKG_VERSION"))?,
            sysinfo::System::boot_time(),
        )
    })();
    #[cfg(windows)]
    if applying {
        return Ok(Some(
            recorded
                .ok()
                .flatten()
                .filter(|record| record.status == Installation::Applying)
                .unwrap_or_else(|| InstallationState::unbound(Installation::Applying)),
        ));
    }
    recorded
}

fn inspect(
    root: &Path,
    executable: &Path,
    kind: &str,
    current: &Version,
    boot_time: u64,
) -> Result<Option<InstallationState>, String> {
    validate_dir(root)?;
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(receipt_error(error)),
    };
    let mut found = None;
    for entry in entries {
        let entry = entry.map_err(receipt_error)?;
        let name = entry.file_name();
        let Some(id) = name
            .to_str()
            .and_then(|name| Uuid::parse_str(name).ok())
            .filter(|id| name == id.to_string().as_str())
        else {
            continue;
        };
        let check = || -> Result<Option<InstallationState>, String> {
            let attempt = Attempt::read_at(root, id)?;
            if !same_installation(executable, &attempt.source, kind)
                && !same_installation(executable, &attempt.destination, kind)
            {
                return Ok(None);
            }
            let outcome = attempt.outcome()?;
            let target_version = version(&attempt.version)?;
            let meets_target = current.cmp_precedence(&target_version).is_ge();
            let lock = attempt.try_lock_work()?;
            if meets_target || outcome == Outcome::Abandoned {
                if let Some(lock) = lock {
                    cleanup(attempt, lock);
                }
                return Ok(None);
            }
            if kind != attempt.installer_type {
                return Ok(None);
            }
            let same_boot = boot_time.abs_diff(attempt.boot_time) < 10;
            let status = match outcome {
                Outcome::Pending if lock.is_none() => Installation::Applying,
                Outcome::RebootRequired if same_boot => Installation::RebootRequired,
                Outcome::Installed | Outcome::RestartFailed => Installation::RestartFailed,
                _ => Installation::Failed,
            };
            Ok(Some(InstallationState {
                status,
                version: Some(target_version),
            }))
        };
        match check() {
            Ok(Some(next))
                if found.as_ref().is_none_or(|current: &InstallationState| {
                    (priority(next.status), &next.version)
                        > (priority(current.status), &current.version)
                }) =>
            {
                found = Some(next);
            }
            Ok(_) => {}
            Err(error) => log::warn!("[update] 跳过无法核验的安装记录：尝试={id}，原因={error}"),
        }
    }
    Ok(found)
}

fn priority(status: Installation) -> u8 {
    match status {
        Installation::Applying => 4,
        Installation::RebootRequired => 3,
        Installation::Failed => 2,
        Installation::RestartFailed => 1,
    }
}

fn same_installation(current: &Path, recorded: &Path, kind: &str) -> bool {
    let identity = |path: &Path| {
        let path = match kind {
            "app" => bundle_root(path).unwrap_or(path),
            _ => path,
        };
        crate::game::resolve_existing_ancestor(path).unwrap_or_else(|_| path.to_owned())
    };
    let (current, recorded) = (identity(current), identity(recorded));
    #[cfg(windows)]
    return current
        .to_string_lossy()
        .eq_ignore_ascii_case(&recorded.to_string_lossy());
    #[cfg(not(windows))]
    return current == recorded;
}

pub(super) fn bundle_root(executable: &Path) -> Option<&Path> {
    let macos = executable.parent().filter(|path| path.ends_with("MacOS"))?;
    let contents = macos.parent().filter(|path| path.ends_with("Contents"))?;
    contents
        .parent()
        .filter(|path| path.extension().is_some_and(|ext| ext == "app"))
}

fn cleanup(attempt: Attempt, lock: File) {
    let remove = |name: &str| -> Result<(), String> {
        let path = attempt.work.join(name);
        if validate_file(&path)? {
            fs::remove_file(path).map_err(receipt_error)?;
        }
        Ok(())
    };
    let result = (|| -> Result<(), String> {
        for file in [
            "update.exe",
            "update.msi",
            "update-result.txt",
            "update-msi.log",
            "update-supervisor.exe",
            "update.signature",
            "windows-install-result.txt",
            "windows-restart-result.txt",
        ] {
            remove(file)?;
        }
        drop(lock);
        remove(LOCK)?;
        remove(OUTCOME)?;
        remove(ATTEMPT)?;
        fs::remove_dir(&attempt.work).map_err(receipt_error)
    })();
    if let Err(error) = result {
        log::warn!(
            "[update] 已升级，暂存目录仍有保留文件：尝试={}，原因={error}",
            attempt.id
        );
    }
}

fn attempts_dir() -> Result<PathBuf, String> {
    Ok(crate::user_paths::bootstrap_dir()?.join("app-update-attempts"))
}

fn version(input: &str) -> Result<Version, String> {
    Version::parse(input).map_err(receipt_error)
}

fn valid_path(path: &Path) -> bool {
    path.is_absolute()
        && path.file_name().is_some()
        && !path
            .components()
            .any(|component| component == Component::ParentDir)
}

fn regular(metadata: &Metadata, directory: bool) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use winapi::um::winnt::FILE_ATTRIBUTE_REPARSE_POINT;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return false;
        }
    }
    (if directory {
        metadata.is_dir()
    } else {
        metadata.is_file()
    }) && !metadata.is_symlink()
}

fn verify_type(path: &Path, directory: bool) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if regular(&metadata, directory) => Ok(true),
        Ok(_) => Err(INVALID.into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(receipt_error(error)),
    }
}

fn validate_dir(path: &Path) -> Result<(), String> {
    for ancestor in path.ancestors() {
        verify_type(ancestor, true)?;
    }
    Ok(())
}

fn validate_file(path: &Path) -> Result<bool, String> {
    validate_dir(path.parent().ok_or(INVALID)?)?;
    verify_type(path, false)
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, String> {
    if !validate_file(path)? {
        return Ok(None);
    }
    let file = File::open(path).map_err(receipt_error)?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(receipt_error)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(INVALID.into());
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(receipt_error)
}

fn write_json(path: &Path, value: &impl Serialize, replace: bool) -> Result<(), String> {
    validate_file(path)?;
    let bytes = serde_json::to_vec(value).map_err(receipt_error)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(INVALID.into());
    }
    let mut temporary =
        tempfile::NamedTempFile::new_in(path.parent().ok_or(INVALID)?).map_err(receipt_error)?;
    temporary.write_all(&bytes).map_err(receipt_error)?;
    temporary.as_file().sync_all().map_err(receipt_error)?;
    validate_file(path)?;
    crate::mod_changes::transaction::durable_rename(&temporary.into_temp_path(), path, replace)
        .map_err(receipt_error)
}

fn receipt_error(error: impl std::fmt::Display) -> String {
    log::warn!("[update] 无法读写安装结果：{error}");
    INVALID.into()
}

#[cfg(test)]
mod tests;
