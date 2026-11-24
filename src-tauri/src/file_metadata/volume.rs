use super::stamp::{open_native_entry, stamp_file};
use std::fs;
use std::io::{self, ErrorKind};
use std::os::windows::{fs::MetadataExt, io::AsRawHandle};
use std::path::Path;
use std::ptr::null_mut;
use winapi::shared::winerror::{
    ERROR_INVALID_FUNCTION, ERROR_INVALID_PARAMETER, ERROR_NOT_SUPPORTED,
};
use winapi::um::fileapi::GetVolumeInformationByHandleW;
use winapi::um::winnt::{
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_PERSISTENT_ACLS, FILE_READ_ONLY_VOLUME,
};

#[derive(Debug)]
pub(crate) enum WriteVolumeError {
    Unsupported(String),
    ReadOnly,
    Unverified(io::Error),
}

impl From<io::Error> for WriteVolumeError {
    fn from(error: io::Error) -> Self {
        Self::Unverified(error)
    }
}

pub(crate) fn write_volume(directory: &Path) -> Result<u64, WriteVolumeError> {
    let entry = directory
        .ancestors()
        .find_map(|path| match fs::symlink_metadata(path) {
            Err(error) if error.kind() == ErrorKind::NotFound => None,
            Err(error) => Some(Err(error)),
            Ok(_) => Some(open_native_entry(path)),
        })
        .ok_or_else(|| io::Error::from(ErrorKind::NotFound))??;
    let metadata = entry.metadata()?;
    if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::from(ErrorKind::InvalidInput).into());
    }
    let mut flags = 0;
    let mut system = [0; 32];
    let read = unsafe {
        GetVolumeInformationByHandleW(
            entry.as_raw_handle().cast(),
            null_mut(),
            0,
            null_mut(),
            null_mut(),
            &mut flags,
            system.as_mut_ptr(),
            system.len() as u32,
        )
    };
    if read == 0 {
        return Err(io::Error::last_os_error().into());
    }
    if flags & FILE_READ_ONLY_VOLUME != 0 {
        return Err(WriteVolumeError::ReadOnly);
    }
    let filesystem = || {
        String::from_utf16_lossy(
            &system[..system
                .iter()
                .position(|&unit| unit == 0)
                .unwrap_or(system.len())],
        )
    };
    if flags & FILE_PERSISTENT_ACLS == 0 {
        return Err(WriteVolumeError::Unsupported(filesystem()));
    }
    stamp_file(&entry)
        .and_then(|stamp| stamp.checked_volume())
        .map_err(|error| {
            if error.kind() == ErrorKind::Unsupported
                || error.raw_os_error().is_some_and(|code| {
                    matches!(
                        code as u32,
                        ERROR_INVALID_FUNCTION | ERROR_INVALID_PARAMETER | ERROR_NOT_SUPPORTED
                    )
                })
            {
                WriteVolumeError::Unsupported(filesystem())
            } else {
                error.into()
            }
        })
}
