use super::paths::{INVALID, io_error, validate_absolute, validate_relative};
use super::types::{Fingerprint, Journal};
use super::{check, diagnostics};
use crate::file_check;
use std::{
    fs,
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
use uuid::Uuid;

pub(super) fn sync_parent(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        File::open(parent)
            .and_then(|file| file.sync_all())
            .map_err(|error| super::paths::io_at(parent, "sync_parent", error))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

pub(crate) fn rename(source: &Path, target: &Path, replace: bool) -> Result<(), String> {
    rename_checked(source, target, replace, check::WRITE)
}

pub(super) fn rename_checked(
    source: &Path,
    target: &Path,
    replace: bool,
    stage: &str,
) -> Result<(), String> {
    rename_durable(source, target, replace, stage).map_err(|failure| match failure {
        RenameFailure::Unchanged(error) | RenameFailure::Sync(error) => error,
    })
}

pub(super) enum RenameFailure {
    Unchanged(String),
    Sync(String),
}

pub(super) fn rename_durable(
    source: &Path,
    target: &Path,
    replace: bool,
    stage: &str,
) -> Result<(), RenameFailure> {
    for path in [source, target] {
        validate_absolute(path)
            .map_err(|error| RenameFailure::Unchanged(check::context(stage, path, &error)))?;
    }
    let write_error = |error: std::io::Error| {
        log::warn!(
            target: diagnostics::TARGET,
            "整链事务改名失败：{}步骤={stage}，源={:?}，目标={:?}，允许替换={replace}，OS码={:?}，分类={:?}",
            diagnostics::Location, diagnostics::path(source), diagnostics::path(target), error.raw_os_error(), error.kind()
        );
        RenameFailure::Unchanged(file_check::report_io(
            stage,
            target,
            error,
            file_check::WRITE,
        ))
    };
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use winapi::um::winbase::{MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW};
        let source_wide: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let target_wide: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
        let flags = MOVEFILE_WRITE_THROUGH
            | if replace {
                MOVEFILE_REPLACE_EXISTING
            } else {
                0
            };
        if unsafe { MoveFileExW(source_wide.as_ptr(), target_wide.as_ptr(), flags) } == 0 {
            return Err(write_error(std::io::Error::last_os_error()));
        }
    }
    #[cfg(not(windows))]
    {
        if replace {
            fs::rename(source, target).map_err(write_error)?;
        } else {
            rename_exclusive(source, target).map_err(write_error)?;
        }
    }
    let sync = |path| {
        sync_parent(path).map_err(|error| {
            log::warn!(
                target: diagnostics::TARGET,
                "整链事务已改名但未能确认刷盘：{}步骤={stage}，源={:?}，目标={:?}，失败位置={:?}",
                diagnostics::Location, diagnostics::path(source), diagnostics::path(target), diagnostics::path(path)
            );
            RenameFailure::Sync(check::context(stage, path, &error))
        })
    };
    sync(source)?;
    sync(target)
}

#[cfg(target_os = "macos")]
fn rename_exclusive(source: &Path, target: &Path) -> std::io::Result<()> {
    use std::{ffi::CString, io, os::unix::ffi::OsStrExt};
    let source = CString::new(source.as_os_str().as_bytes())?;
    let target = CString::new(target.as_os_str().as_bytes())?;
    if unsafe { libc::renamex_np(source.as_ptr(), target.as_ptr(), libc::RENAME_EXCL) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(any(windows, target_os = "macos")))]
fn rename_exclusive(_: &Path, _: &Path) -> std::io::Result<()> {
    Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
}

pub(super) fn write_journal(directory: &Path, journal: &Journal) -> Result<(), String> {
    let temporary = directory.join(format!("journal-{}.tmp", Uuid::new_v4()));
    let bytes = serde_json::to_vec(journal).map_err(io_error)?;
    let write_error = |error| diagnostic_io(check::WRITE, &temporary, error, file_check::WRITE);
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(write_error)?;
    output.write_all(&bytes).map_err(write_error)?;
    output.sync_all().map_err(write_error)?;
    drop(output);
    rename(&temporary, &directory.join("journal.json"), true)
}

pub(super) fn read_journal(directory: &Path) -> Result<Journal, String> {
    const MAX_JOURNAL_BYTES: u64 = 32 * 1024 * 1024;
    let path = directory.join("journal.json");
    validate_absolute(&path).inspect_err(|error| {
        log::warn!(
            target: diagnostics::TARGET,
            "整链事务记录路径无效：{}路径={:?}，原因={:?}",
            diagnostics::Location, diagnostics::path(&path), crate::locale_tag::diagnostic_tag(error),
        );
    })?;
    let metadata = fs::metadata(&path)
        .map_err(|error| super::paths::io_at(&path, "read_journal_metadata", error))?;
    if !metadata.is_file() || metadata.len() > MAX_JOURNAL_BYTES {
        log::warn!(
            target: diagnostics::TARGET,
            "整链事务记录类型或大小不符合要求：{}路径={:?}，是普通文件={}，大小={}，上限={MAX_JOURNAL_BYTES}",
            diagnostics::Location, diagnostics::path(&path), metadata.is_file(), metadata.len(),
        );
        return Err(INVALID.into());
    }
    let bytes =
        fs::read(&path).map_err(|error| super::paths::io_at(&path, "read_journal", error))?;
    serde_json::from_slice(&bytes).map_err(|error| {
        log::warn!(
            target: diagnostics::TARGET,
            "整链事务记录解析失败：{}路径={:?}，类型={:?}，行={}，列={}",
            diagnostics::Location, diagnostics::path(&path), error.classify(), error.line(), error.column(),
        );
        INVALID.into()
    })
}

pub(super) fn private_path(
    directory: &Path,
    relative: &str,
    stage: &str,
) -> Result<PathBuf, String> {
    let path = directory.join(relative);
    validate_relative(relative)
        .and_then(|()| validate_absolute(&path))
        .map_err(|error| check::context(stage, &path, &error))?;
    Ok(path)
}

pub(super) fn copy_verified(
    source: &Path,
    target: &Path,
    expected: &Fingerprint,
    source_stage: &str,
    write_stage: &str,
    created: impl FnOnce(&File) -> Result<(), String>,
) -> Result<Fingerprint, String> {
    copy_file(source, target, expected, source_stage, write_stage, created).inspect_err(|error| {
        log::warn!(
            target: diagnostics::TARGET,
            "整链事务复制失败：{}源={:?}，目标={:?}，源步骤={source_stage}，写步骤={write_stage}，期望={}，错误={:?}",
            diagnostics::Location, diagnostics::path(source), diagnostics::path(target),
            diagnostics::Stamp(expected), crate::locale_tag::diagnostic_tag(error)
        );
    })
}

fn copy_file(
    source: &Path,
    target: &Path,
    expected: &Fingerprint,
    source_stage: &str,
    write_stage: &str,
    created: impl FnOnce(&File) -> Result<(), String>,
) -> Result<Fingerprint, String> {
    check::expect(source, source_stage, expected)?;
    validate_absolute(target).map_err(|error| check::context(write_stage, target, &error))?;
    let write_error = |error| diagnostic_io(write_stage, target, error, file_check::WRITE);
    let mut input = File::open(source)
        .map_err(|error| diagnostic_io(source_stage, source, error, file_check::READ))?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .map_err(write_error)?;
    created(&output)?;
    std::io::copy(&mut input, &mut output)
        .map_err(|error| diagnostic_io(write_stage, target, error, file_check::COPY))?;
    output.sync_all().map_err(write_error)?;
    drop(output);
    let target_stage = if write_stage == check::WRITE_BACKUP {
        check::BACKUP
    } else {
        check::STAGED
    };
    let actual = check::capture(target, target_stage)?;
    check::compare(target, target_stage, &actual, expected, true)?;
    check::expect(source, source_stage, expected)?;
    sync_parent(target).map_err(|error| check::context(write_stage, target, &error))?;
    Ok(actual)
}

pub(super) fn diagnostic_io(
    stage: &str,
    path: &Path,
    error: std::io::Error,
    fallback: &'static str,
) -> String {
    super::paths::log_io(path, stage, &error);
    file_check::report_io(stage, path, error, fallback)
}

pub(super) fn remove_file_checked(
    path: &Path,
    expected: &Fingerprint,
    require_single_link: bool,
) -> Result<(), String> {
    check::expect(path, check::TARGET, expected)?;
    if expected == &Fingerprint::Missing {
        return Ok(());
    }
    if require_single_link {
        super::paths::guard_single_link(path)?;
    }
    // The write-ahead record retains the original permissions if clearing readonly crashes.
    let mut permissions = fs::metadata(path)
        .map_err(|error| diagnostic_io(check::TARGET, path, error, file_check::READ))?
        .permissions();
    if permissions.readonly() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            permissions.set_mode(permissions.mode() | 0o200);
        }
        #[cfg(not(unix))]
        permissions.set_readonly(false);
        fs::set_permissions(path, permissions)
            .map_err(|error| diagnostic_io(check::WRITE, path, error, file_check::WRITE))?;
    }
    fs::remove_file(path)
        .map_err(|error| diagnostic_io(check::WRITE, path, error, file_check::WRITE))?;
    sync_parent(path).map_err(|error| check::writing(path, &error))
}
