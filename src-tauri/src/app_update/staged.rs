use super::download::VerifiedInstallerDownload;
use crate::update_migration::{MANAGER_EXECUTABLE, update_executable};
use crate::{app_state::AppState, archive, file_metadata};
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::windows::{fs::OpenOptionsExt, process::CommandExt};
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use winapi::um::winbase::{FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT};
use winapi::um::winnt::{FILE_ATTRIBUTE_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE};

const UNAVAILABLE: &str = "update.stagedUnavailable";
const INSTALLER_FILE: &str = "update.exe";
const INSTALLER_RESULT: &str = "update-result.txt";

pub(crate) struct StagedInstaller {
    work: TempDir,
    digest: String,
    version: String,
    source: PathBuf,
    destination: PathBuf,
}

impl StagedInstaller {
    pub(crate) fn prepare(
        download: VerifiedInstallerDownload,
        version: String,
    ) -> Result<Self, String> {
        let source = std::env::current_exe().map_err(stage_error)?;
        let destination = validated_destination(&source)?;
        let work = tempfile::Builder::new()
            .prefix("OC2ModManager-update-")
            .tempdir()
            .map_err(stage_error)?;
        let digest = archive::sha256_reader(download.bytes.as_slice()).map_err(stage_error)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(work.path().join(INSTALLER_FILE))
            .map_err(stage_error)?;
        file.write_all(&download.bytes).map_err(stage_error)?;
        file.sync_all().map_err(stage_error)?;
        drop(file);
        let staged = Self {
            work,
            digest,
            version,
            source,
            destination,
        };
        let _guard = staged.lock_verified_file()?;
        if file_metadata::read_version_string(&staged.path(), "OC2DeferredUpdate").as_deref()
            != Some("1")
        {
            log::warn!("已验签的更新包不支持退出后安装：版本={}", staged.version);
            return Err("update.deferredUnsupported".into());
        }
        log::info!(
            "更新已暂存：版本={}，字节数={}，当前程序={}，安装目标={}",
            staged.version,
            download.bytes.len(),
            staged.source.display(),
            staged.destination.display()
        );
        Ok(staged)
    }

    fn path(&self) -> PathBuf {
        self.work.path().join(INSTALLER_FILE)
    }

    fn lock_verified_file(&self) -> Result<(File, File), String> {
        let check = || -> io::Result<(File, File)> {
            let directory = OpenOptions::new()
                .access_mode(0)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                .open(self.work.path())?;
            check_type(&directory, true)?;
            let file = OpenOptions::new()
                .read(true)
                .share_mode(FILE_SHARE_READ)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .open(self.path())?;
            check_type(&file, false)?;
            if archive::sha256_reader(&file)? != self.digest {
                return Err(io::Error::other("staged installer digest changed"));
            }
            Ok((directory, file))
        };
        check().map_err(|error| {
            log::warn!("暂存更新复核失败：版本={}，原因={error}", self.version);
            UNAVAILABLE.into()
        })
    }

    fn launch(&self, restart: bool) -> Result<(), String> {
        let current = std::env::current_exe().map_err(launch_error)?;
        let destination = validated_destination(&current)?;
        if current != self.source || destination != self.destination {
            log::warn!("更新暂存后安装位置发生变化：版本={}", self.version);
            return Err(UNAVAILABLE.into());
        }
        let _guard = self.lock_verified_file()?;
        let directory = super::installer_directory(&current)?;
        let args = super::deferred_nsis_args(directory, std::process::id(), restart);
        let child = std::process::Command::new(self.path())
            .raw_arg(args)
            .current_dir(self.work.path())
            .spawn()
            .map_err(launch_error)?;
        log::info!(
            "已交接退出后更新：版本={}，安装器进程={}，退出进程={}，安装成功后重启={restart}，安装结果={}",
            self.version,
            child.id(),
            std::process::id(),
            self.work.path().join(INSTALLER_RESULT).display(),
        );
        Ok(())
    }
}

fn check_type(file: &File, directory: bool) -> io::Result<()> {
    use std::os::windows::fs::MetadataExt;
    let metadata = file.metadata()?;
    let expected_type = if directory {
        metadata.is_dir()
    } else {
        metadata.is_file()
    };
    if !expected_type || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::other("unexpected staged path type"));
    }
    Ok(())
}

fn validated_destination(source: &Path) -> Result<PathBuf, String> {
    if !source
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case(MANAGER_EXECUTABLE))
    {
        log::warn!("当前程序名与安装包不符，不能自动更新：{}", source.display());
        return Err("update.formatUnsupported".into());
    }
    let directory = super::installer_directory(source)?;
    let destination = update_executable(source)?;
    if !directory.is_absolute() || !super::installer_directory(&destination)?.is_absolute() {
        return Err("migrate.invalidInstallDirectory".into());
    }
    Ok(destination)
}

fn stage_error(error: io::Error) -> String {
    log::error!("保存更新安装器失败：{error}");
    "update.stageFailed".into()
}

fn launch_error(error: io::Error) -> String {
    log::error!("启动退出后更新安装器失败：{error}");
    "update.installFailedManual".into()
}

pub(crate) fn handoff(state: &AppState, restart: bool) -> Result<(), String> {
    let mut slot = state
        .staged_app_update
        .lock()
        .map_err(|_| "lock.poisoned")?;
    let Some(staged) = slot.as_ref() else {
        return if restart {
            Err(UNAVAILABLE.into())
        } else {
            Ok(())
        };
    };
    crate::mod_changes::guard::ensure_recovered(state)?;
    match staged.launch(restart) {
        Ok(()) => {
            let _ = slot
                .take()
                .expect("successful handoff has an installer")
                .work
                .keep();
            Ok(())
        }
        Err(error) => {
            if error == UNAVAILABLE {
                *slot = None;
            }
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests;
