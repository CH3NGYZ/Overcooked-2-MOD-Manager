use super::check;
use super::paths::{INVALID, io_error, validate_absolute, validate_relative};
use super::types::{Fingerprint, Journal};
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
            .map_err(io_error)?;
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
    validate_absolute(source).map_err(|error| check::context(stage, source, &error))?;
    validate_absolute(target).map_err(|error| check::context(stage, target, &error))?;
    let write_error = |error| file_check::report_io(stage, target, error, file_check::WRITE);
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
        if !replace && target.try_exists().map_err(write_error)? {
            return Err(file_check::report(stage, target, file_check::EXISTS));
        }
        fs::rename(source, target).map_err(write_error)?;
    }
    sync_parent(source).map_err(|error| check::context(stage, source, &error))?;
    sync_parent(target).map_err(|error| check::context(stage, target, &error))
}

pub(super) fn write_journal(directory: &Path, journal: &Journal) -> Result<(), String> {
    let temporary = directory.join(format!("journal-{}.tmp", Uuid::new_v4()));
    let bytes = serde_json::to_vec(journal).map_err(io_error)?;
    let write_error =
        |error| file_check::report_io(check::WRITE, &temporary, error, file_check::WRITE);
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
    let path = directory.join("journal.json");
    validate_absolute(&path)?;
    let metadata = fs::metadata(&path).map_err(io_error)?;
    if !metadata.is_file() || metadata.len() > 32 * 1024 * 1024 {
        return Err(INVALID.into());
    }
    serde_json::from_slice(&fs::read(path).map_err(io_error)?).map_err(|_| INVALID.into())
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
) -> Result<Fingerprint, String> {
    check::expect(source, source_stage, expected)?;
    validate_absolute(target).map_err(|error| check::context(write_stage, target, &error))?;
    let write_error = |error| file_check::report_io(write_stage, target, error, file_check::WRITE);
    let mut input = File::open(source)
        .map_err(|error| file_check::report_io(source_stage, source, error, file_check::READ))?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .map_err(write_error)?;
    std::io::copy(&mut input, &mut output)
        .map_err(|error| file_check::report_io(write_stage, target, error, file_check::COPY))?;
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
        .map_err(|error| file_check::report_io(check::TARGET, path, error, file_check::READ))?
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
            .map_err(|error| file_check::report_io(check::WRITE, path, error, file_check::WRITE))?;
    }
    fs::remove_file(path)
        .map_err(|error| file_check::report_io(check::WRITE, path, error, file_check::WRITE))?;
    sync_parent(path).map_err(|error| check::writing(path, &error))
}
