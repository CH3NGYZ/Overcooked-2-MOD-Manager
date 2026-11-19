use super::paths::{INVALID, MODIFIED, io_error, validate_absolute, validate_relative};
use super::types::{Fingerprint, Journal};
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
    validate_absolute(source)?;
    validate_absolute(target)?;
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use winapi::um::winbase::{MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW};
        let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
        let flags = MOVEFILE_WRITE_THROUGH
            | if replace {
                MOVEFILE_REPLACE_EXISTING
            } else {
                0
            };
        if unsafe { MoveFileExW(source.as_ptr(), target.as_ptr(), flags) } == 0 {
            return Err(io_error(std::io::Error::last_os_error()));
        }
    }
    #[cfg(not(windows))]
    {
        if !replace && target.try_exists().map_err(io_error)? {
            return Err(super::paths::CONFLICT.into());
        }
        fs::rename(source, target).map_err(io_error)?;
    }
    sync_parent(source)?;
    sync_parent(target)
}

pub(super) fn write_journal(directory: &Path, journal: &Journal) -> Result<(), String> {
    let temporary = directory.join(format!("journal-{}.tmp", Uuid::new_v4()));
    let bytes = serde_json::to_vec(journal).map_err(io_error)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(io_error)?;
    output.write_all(&bytes).map_err(io_error)?;
    output.sync_all().map_err(io_error)?;
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

pub(super) fn private_path(directory: &Path, relative: &str) -> Result<PathBuf, String> {
    validate_relative(relative)?;
    let path = directory.join(relative);
    validate_absolute(&path)?;
    Ok(path)
}

pub(super) fn copy_verified(
    source: &Path,
    target: &Path,
    expected: &Fingerprint,
) -> Result<Fingerprint, String> {
    validate_absolute(source)?;
    validate_absolute(target)?;
    if Fingerprint::capture(source)? != *expected {
        return Err(MODIFIED.into());
    }
    let mut input = File::open(source).map_err(io_error)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .map_err(io_error)?;
    std::io::copy(&mut input, &mut output).map_err(io_error)?;
    output.sync_all().map_err(io_error)?;
    drop(output);
    let actual = Fingerprint::capture(target)?;
    if !actual.same_content(expected) || Fingerprint::capture(source)? != *expected {
        return Err(MODIFIED.into());
    }
    sync_parent(target)?;
    Ok(actual)
}

pub(super) fn remove_file_checked(
    path: &Path,
    expected: &Fingerprint,
    require_single_link: bool,
) -> Result<(), String> {
    if Fingerprint::capture(path)? != *expected {
        return Err(MODIFIED.into());
    }
    if expected == &Fingerprint::Missing {
        return Ok(());
    }
    if require_single_link {
        super::paths::guard_single_link(path)?;
    }
    // The write-ahead record retains the original permissions if clearing readonly crashes.
    let mut permissions = fs::metadata(path).map_err(io_error)?.permissions();
    if permissions.readonly() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            permissions.set_mode(permissions.mode() | 0o200);
        }
        #[cfg(not(unix))]
        permissions.set_readonly(false);
        fs::set_permissions(path, permissions).map_err(io_error)?;
    }
    fs::remove_file(path).map_err(io_error)?;
    sync_parent(path)
}
