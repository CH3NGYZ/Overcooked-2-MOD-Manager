use crate::app_update::download::VerifiedInstallerDownload;
use crate::app_update::receipt::{Attempt, Outcome};
use crate::app_update::windows_session::{PermissionRoute, permission_route};
use crate::app_update::{UpdateTarget, installer_directory, verify_download};
use crate::elevation::{HandoffEvents, ManagerProcess, owned_handle, wide};
use crate::update_migration::{MANAGER_EXECUTABLE, update_executable};
use crate::{
    archive, file_metadata,
    models::{AppUpdateInfo, AppUpdateInstallMode},
};
use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, Write};
use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::ptr::null_mut;
use tempfile::TempDir;
use uuid::Uuid;
use winapi::shared::winerror::{
    ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, ERROR_INVALID_HANDLE,
    WAIT_TIMEOUT,
};
use winapi::um::errhandlingapi::SetLastError;
use winapi::um::synchapi::{CreateMutexW, OpenMutexW, WaitForSingleObject};
use winapi::um::winbase::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, WAIT_OBJECT_0,
};
use winapi::um::winnt::{
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE, SYNCHRONIZE,
};

mod administrator;
mod installer;
mod protected;

pub(crate) use administrator::can_install_as_admin;
pub(crate) use protected::attempts_dir as protected_attempts_dir;

const FLAG: &str = "--oc2-update-supervisor";
const SUPERVISOR_FILE: &str = "update-supervisor.exe";
const SIGNATURE_FILE: &str = "update.signature";
const WAIT_MS: u32 = 30_000;
const UNAVAILABLE: &str = "update.stagedUnavailable";
const INSTALLER_BUSY: &str = "update.inProgress";
const INSTALLER_BUSY_EXIT: i32 = 2;
const INSTALLER_SLOT: &str = r"Global\OC2ModManager-Standalone-Installer";

pub(crate) struct Prepared {
    package: PreparedPackage,
    signature: String,
    source: PathBuf,
    destination: PathBuf,
    kind: &'static str,
    version: String,
}

enum PreparedPackage {
    Ordinary { work: TempDir, digest: String },
    Administrator(Vec<u8>),
}

impl Prepared {
    #[cfg(test)]
    pub(super) fn administrator_fixture(source: PathBuf) -> Self {
        Self {
            package: PreparedPackage::Administrator(Vec::new()),
            signature: String::new(),
            destination: source.clone(),
            source,
            kind: "exe",
            version: "99.1.0".into(),
        }
    }

    pub(crate) fn prepare(
        download: VerifiedInstallerDownload,
        info: &AppUpdateInfo,
    ) -> Result<Self, String> {
        let kind = UpdateTarget::current()?.kind;
        if kind != info.installer_type {
            return Err("update.manifestPlatformMismatch".into());
        }
        let admin_requested = info.install_mode == AppUpdateInstallMode::InstallAsAdministrator;
        let source = std::env::current_exe().map_err(stage_error)?;
        let destination = validated_destination(&source, kind).map_err(|error| {
            if admin_requested {
                administrator::preflight(error)
            } else {
                error
            }
        })?;
        let size = download.bytes.len();
        let package = match (info.install_mode, permission_route()) {
            (AppUpdateInstallMode::Deferred, Ok(PermissionRoute::User)) => {
                let work = tempfile::Builder::new()
                    .prefix("OC2ModManager-update-")
                    .tempdir()
                    .map_err(stage_error)?;
                let digest =
                    archive::sha256_reader(download.bytes.as_slice()).map_err(stage_error)?;
                let path = package_path(work.path(), kind)?;
                write_new(&path, &mut download.bytes.as_slice()).map_err(stage_error)?;
                let _locked = lock_verified(work.path(), &path, &digest).map_err(stage_error)?;
                if !installer::supports_background(&path, kind) {
                    return Err("update.deferredUnsupported".into());
                }
                PreparedPackage::Ordinary { work, digest }
            }
            (AppUpdateInstallMode::InstallAsAdministrator, Ok(PermissionRoute::Administrator)) => {
                administrator::validate_target(&source, &destination, kind)
                    .map_err(administrator::preflight)?;
                administrator::validate_package(&download.bytes, kind, &info.version)?;
                PreparedPackage::Administrator(download.bytes)
            }
            _ if admin_requested => return Err(administrator::unavailable()),
            _ => return Err("update.deferredUnsupported".into()),
        };
        log::info!(
            "更新已暂存：版本={}，格式={kind}，字节数={}，当前程序={}，安装目标={}",
            info.version,
            size,
            source.display(),
            destination.display()
        );
        Ok(Self {
            package,
            signature: info.installer_signature.clone(),
            source,
            destination,
            kind,
            version: info.version.clone(),
        })
    }

    pub(crate) fn source(&self) -> &Path {
        &self.source
    }

    pub(crate) fn destination(&self) -> &Path {
        &self.destination
    }

    pub(crate) fn start(&self, attempt: &Attempt) -> Result<(), String> {
        ensure_user_process()?;
        let PreparedPackage::Ordinary {
            work: staged_work,
            digest,
        } = &self.package
        else {
            return Err(UNAVAILABLE.into());
        };
        let current = std::env::current_exe().map_err(start_error)?;
        if current != self.source
            || validated_destination(&current, self.kind)? != self.destination
            || attempt.source != self.source
            || attempt.destination != self.destination
            || attempt.version != self.version
            || attempt.installer_type != self.kind
        {
            return Err(UNAVAILABLE.into());
        }
        let work = attempt.work_path();
        let _work_lock = lock_directory(work).map_err(start_error)?;
        let path = package_path(staged_work.path(), self.kind)?;
        let (_staged_dir, mut package) =
            lock_verified(staged_work.path(), &path, digest).map_err(|error| {
                log::warn!("暂存包复核失败：尝试={}，原因={error}", attempt.id);
                UNAVAILABLE
            })?;
        package.rewind().map_err(start_error)?;
        write_new(&package_path(work, self.kind)?, &mut package).map_err(start_error)?;
        write_new(&work.join(SIGNATURE_FILE), &mut self.signature.as_bytes())
            .map_err(start_error)?;

        let helper = work.join(SUPERVISOR_FILE);
        let mut source = lock_file(&current).map_err(start_error)?;
        let source_digest = archive::sha256_reader(&source).map_err(start_error)?;
        source.rewind().map_err(start_error)?;
        write_new(&helper, &mut source).map_err(start_error)?;
        let (_helper_dir, _helper_lock) =
            lock_verified(work, &helper, &source_digest).map_err(start_error)?;
        let owner = ManagerProcess::current().map_err(start_error)?;
        let events = HandoffEvents::named(attempt.id, true).map_err(start_error)?;
        let mut child = Command::new(&helper)
            .arg(FLAG)
            .arg(owner.pid.to_string())
            .arg(owner.created.to_string())
            .arg(attempt.id.to_string())
            .current_dir(work)
            .spawn()
            .map_err(start_error)?;
        if let Err(error) = events.commit_when_ready(child.as_raw_handle().cast(), WAIT_MS) {
            return Err(handoff_error(&mut child, error));
        }
        log::info!(
            "已交接 Windows 更新：尝试={}，旧进程={}，辅助进程={}，目标版本={}，重启={}",
            attempt.id,
            owner.pid,
            child.id(),
            attempt.version,
            attempt.restart
        );
        Ok(())
    }
}

pub(crate) fn run_if_requested() {
    let mut args = std::env::args_os().skip(1);
    let flag = match args.next().as_deref().and_then(OsStr::to_str) {
        Some(FLAG) => false,
        Some(administrator::FLAG) => true,
        _ => return,
    };
    let result = (|| {
        let owner =
            ManagerProcess::parse(args.next(), args.next()).map_err(|error| error.to_string())?;
        let id = parse_attempt_id(args.next(), args.next()).map_err(|error| error.to_string())?;
        if flag {
            administrator::run_supervisor(&owner, id)
        } else {
            run_supervisor(&owner, id)
        }
    })();
    if let Err(error) = &result {
        eprintln!("OC2 update supervisor: {error}");
    }
    let code = match result {
        Ok(()) => 0,
        Err(error) if error == INSTALLER_BUSY => INSTALLER_BUSY_EXIT,
        Err(_) => 1,
    };
    std::process::exit(code);
}

fn handoff_error(child: &mut std::process::Child, error: io::Error) -> String {
    if child
        .try_wait()
        .is_ok_and(|status| status.and_then(|status| status.code()) == Some(INSTALLER_BUSY_EXIT))
    {
        return INSTALLER_BUSY.into();
    }
    let _ = child.kill();
    let _ = child.wait();
    start_error(error)
}

fn parse_attempt_id(id: Option<OsString>, extra: Option<OsString>) -> io::Result<Uuid> {
    id.and_then(|id| Uuid::parse_str(id.to_str()?).ok())
        .filter(|id| id.get_version_num() == 4 && extra.is_none())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid update attempt"))
}

fn run_supervisor(owner: &ManagerProcess, id: Uuid) -> Result<(), String> {
    ensure_user_process()?;
    let attempt = Attempt::read(id)?;
    let work = attempt.work_path();
    let _work_lock = lock_directory(work).map_err(start_error)?;
    let attempt_lock = attempt.lock_work()?;
    let current = std::env::current_exe().map_err(start_error)?;
    if !same_file::is_same_file(&current, work.join(SUPERVISOR_FILE)).map_err(start_error)?
        || UpdateTarget::current()?.kind != attempt.installer_type
        || attempt.source_version != env!("CARGO_PKG_VERSION")
        || validated_destination(&attempt.source, &attempt.installer_type)? != attempt.destination
    {
        return Err(UNAVAILABLE.into());
    }
    let owner_handle = owner.open(&attempt.source).map_err(start_error)?;
    let _installer_slot = InstallerSlot::enter()?;
    let source_file = lock_file(&attempt.source).map_err(start_error)?;
    if !matches_installed_version(&attempt.source, &attempt.source_version) {
        return Err(UNAVAILABLE.into());
    }
    let source_digest = archive::sha256_reader(&source_file).map_err(start_error)?;
    if archive::sha256_reader(lock_file(&current).map_err(start_error)?).map_err(start_error)?
        != source_digest
    {
        return Err(UNAVAILABLE.into());
    }

    let path = package_path(work, &attempt.installer_type)?;
    let mut package = lock_file(&path).map_err(start_error)?;
    let signature = read_signature(work)?;
    let mut bytes = Vec::new();
    package.read_to_end(&mut bytes).map_err(start_error)?;
    verify_download(&bytes, &signature, &attempt.version)?;
    drop(bytes);
    if !installer::supports_background(&path, &attempt.installer_type) {
        return Err("update.deferredUnsupported".into());
    }

    if let Err(error) = HandoffEvents::named(id, false)
        .and_then(|events| events.yield_until_owner_exits(&owner_handle, WAIT_MS))
    {
        installer::write_diagnostic(
            &attempt,
            installer::WINDOWS_RESULT,
            &format!("outcome=Failed handoff_error={error}"),
        );
        attempt.record_outcome(Outcome::Failed)?;
        return Err(start_error(error));
    }
    drop(source_file);
    let outcome = finish_install(&attempt, path, owner.pid, package, attempt_lock, false);
    drop(owner_handle);
    if attempt.restart {
        let launched = match outcome {
            Outcome::Installed => restart_verified(&attempt.destination, &attempt.version, None),
            Outcome::Failed => restart_verified(
                &attempt.source,
                &attempt.source_version,
                Some(&source_digest),
            ),
            _ => return Ok(()),
        };
        if let Err(error) = launched {
            let receipt_error = (outcome == Outcome::Installed)
                .then(|| attempt.record_outcome(Outcome::RestartFailed).err())
                .flatten();
            installer::write_diagnostic(
                &attempt,
                "windows-restart-result.txt",
                &format!(
                    "outcome={outcome:?} launch_error={error} receipt_error={receipt_error:?}"
                ),
            );
            return Err(error);
        }
    }
    Ok(())
}

struct InstallerSlot {
    _handle: std::os::windows::io::OwnedHandle,
}

impl InstallerSlot {
    fn enter() -> Result<Self, String> {
        Self::open(OsStr::new(INSTALLER_SLOT))
    }

    fn open(name: &OsStr) -> Result<Self, String> {
        let name = wide(name);
        let (handle, status) = unsafe {
            SetLastError(0);
            let handle = CreateMutexW(null_mut(), 0, name.as_ptr());
            (handle, io::Error::last_os_error().raw_os_error())
        };
        let handle = owned_handle(handle);
        match status.map(|code| code as u32) {
            Some(ERROR_ALREADY_EXISTS | ERROR_ACCESS_DENIED | ERROR_INVALID_HANDLE) => {
                Err(INSTALLER_BUSY.into())
            }
            _ => handle
                .map(|handle| Self { _handle: handle })
                .map_err(start_error),
        }
    }

    fn visible(name: &OsStr) -> bool {
        let handle = unsafe { OpenMutexW(SYNCHRONIZE, 0, wide(name).as_ptr()) };
        match owned_handle(handle) {
            Ok(_) => true,
            Err(error) => error.raw_os_error() != Some(ERROR_FILE_NOT_FOUND as i32),
        }
    }
}

pub(in crate::app_update) fn installation_in_progress() -> bool {
    InstallerSlot::visible(OsStr::new(INSTALLER_SLOT))
}

fn finish_install(
    attempt: &Attempt,
    path: PathBuf,
    owner_pid: u32,
    package_lock: File,
    attempt_lock: File,
    administrator: bool,
) -> Outcome {
    let (outcome, details) = installer::apply_and_observe(attempt, &path, owner_pid, administrator);
    drop(package_lock);
    let receipt_error = attempt.record_outcome(outcome).err();
    if let Some(error) = &receipt_error {
        eprintln!("OC2 update supervisor: could not write installation receipt: {error}");
    }
    let cleanup_error = fs::remove_file(&path).err();
    installer::write_diagnostic(
        attempt,
        installer::WINDOWS_RESULT,
        &format!("{details} receipt_error={receipt_error:?} cleanup_error={cleanup_error:?}"),
    );
    drop(attempt_lock);
    outcome
}

fn restart_verified(path: &Path, version: &str, old_digest: Option<&str>) -> Result<(), String> {
    let locked = lock_file(path).map_err(|error| error.to_string())?;
    if !matches_installed_version(path, version)
        || old_digest
            .is_some_and(|digest| archive::sha256_reader(&locked).as_deref().ok() != Some(digest))
    {
        return Err(format!("Program does not match expected version {version}"));
    }
    let mut child = Command::new(path)
        .current_dir(installer_directory(path)?)
        .spawn()
        .map_err(|error| error.to_string())?;
    match unsafe { WaitForSingleObject(child.as_raw_handle().cast(), 1000) } {
        WAIT_TIMEOUT => Ok(()),
        WAIT_OBJECT_0 => Err(format!("Manager exited during startup: {:?}", child.wait())),
        _ => Err(io::Error::last_os_error().to_string()),
    }
}

fn matches_installed_version(path: &Path, expected: &str) -> bool {
    file_metadata::read_version_string(path, "ProductVersion").as_deref() == Some(expected)
}

fn validated_destination(source: &Path, kind: &str) -> Result<PathBuf, String> {
    if !source
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case(MANAGER_EXECUTABLE))
    {
        log::warn!("当前程序名与安装包不符，不能自动更新：{}", source.display());
        return Err("update.formatUnsupported".into());
    }
    let directory = installer_directory(source)?;
    let destination = update_executable(source)?;
    if !directory.is_absolute() || !installer_directory(&destination)?.is_absolute() {
        return Err("migrate.invalidInstallDirectory".into());
    }
    if kind == "msi" && destination != source {
        return Err("update.deferredUnsupported".into());
    }
    Ok(destination)
}

fn package_path(work: &Path, kind: &str) -> Result<PathBuf, String> {
    match kind {
        "exe" => Ok(work.join("update.exe")),
        "msi" => Ok(work.join("update.msi")),
        _ => Err("update.formatUnsupported".into()),
    }
}

fn write_new(path: &Path, bytes: &mut impl Read) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    io::copy(bytes, &mut file)?;
    file.flush()?;
    file.sync_all()
}

fn read_signature(work: &Path) -> Result<String, String> {
    let mut signature = String::new();
    lock_file(&work.join(SIGNATURE_FILE))
        .and_then(|file| file.take(16_385).read_to_string(&mut signature))
        .map_err(start_error)?;
    if signature.is_empty() || signature.len() > 16_384 {
        return Err(UNAVAILABLE.into());
    }
    Ok(signature)
}

fn lock_verified(work: &Path, path: &Path, digest: &str) -> io::Result<(File, File)> {
    let directory = lock_directory(work)?;
    let file = lock_file(path)?;
    if archive::sha256_reader(&file)? != digest {
        return Err(io::Error::other("staged installer digest changed"));
    }
    Ok((directory, file))
}

fn lock_directory(path: &Path) -> io::Result<File> {
    let directory = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    check_type(&directory, true)?;
    Ok(directory)
}

fn lock_file(path: &Path) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    check_type(&file, false)?;
    Ok(file)
}

fn check_type(file: &File, directory: bool) -> io::Result<()> {
    use std::os::windows::fs::MetadataExt;
    let metadata = file.metadata()?;
    let expected = if directory {
        metadata.is_dir()
    } else {
        metadata.is_file()
    };
    if !expected || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::other("unexpected staged path type"));
    }
    Ok(())
}

fn ensure_user_process() -> Result<(), String> {
    if crate::is_elevated() {
        return Err("update.deferredUnsupported".into());
    }
    Ok(())
}

fn stage_error(error: io::Error) -> String {
    log::error!("保存更新安装器失败：{error}");
    "update.stageFailed".into()
}

fn start_error(error: io::Error) -> String {
    log::error!("Windows 更新交接失败：{error}");
    "update.installFailedManual".into()
}

#[cfg(test)]
mod tests;
