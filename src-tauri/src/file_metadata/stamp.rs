//! Native identity and change time; length/last-write alone miss restored mtimes.
use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct FileIdentity {
    #[cfg(windows)]
    volume: u64,
    #[cfg(windows)]
    file: [u8; 16],
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(unix)]
    created: std::time::SystemTime,
}

impl FileIdentity {
    pub(crate) fn capture(path: &Path) -> io::Result<Option<Self>> {
        match Stamp::capture(path)? {
            Stamp::Missing => Ok(None),
            Stamp::Present(stamp) if stamp.is_file() => stamp.identity().map(Some),
            Stamp::Present(_) => Err(io::Error::from(io::ErrorKind::InvalidInput)),
        }
    }

    pub(crate) fn from_file(file: &fs::File) -> io::Result<Self> {
        stamp_file(file)?.identity()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Stamp {
    Missing,
    Present(NativeStamp),
}

impl Stamp {
    pub(crate) fn capture(path: &Path) -> io::Result<Self> {
        match native_stamp(path) {
            Ok(stamp) => Ok(Self::Present(stamp)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::Missing),
            Err(error) => Err(error),
        }
    }

    /// An unavailable identity (or a link to another entry) cannot validate
    /// cached bytes. The caller can still read and parse the file normally.
    pub(crate) fn for_file(path: &Path) -> Option<NativeStamp> {
        native_stamp(path).ok().filter(NativeStamp::is_file)
    }
}

#[cfg(unix)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeStamp {
    device: u64,
    inode: u64,
    created: Option<std::time::SystemTime>,
    changed: (i64, i64),
    modified: (i64, i64),
    size: u64,
    mode: u32,
    regular_file: bool,
}

#[cfg(unix)]
impl NativeStamp {
    fn is_file(&self) -> bool {
        self.regular_file
    }

    fn identity(&self) -> io::Result<FileIdentity> {
        Ok(FileIdentity {
            device: self.device,
            inode: self.inode,
            created: self.created.ok_or(io::ErrorKind::Unsupported)?,
        })
    }
}

#[cfg(unix)]
fn native_stamp(path: &Path) -> io::Result<NativeStamp> {
    Ok(stamp_metadata(fs::symlink_metadata(path)?))
}

#[cfg(unix)]
fn stamp_file(file: &fs::File) -> io::Result<NativeStamp> {
    Ok(stamp_metadata(file.metadata()?))
}

#[cfg(unix)]
fn stamp_metadata(meta: fs::Metadata) -> NativeStamp {
    use std::os::unix::fs::MetadataExt;
    NativeStamp {
        device: meta.dev(),
        inode: meta.ino(),
        created: meta.created().ok(),
        changed: (meta.ctime(), meta.ctime_nsec()),
        modified: (meta.mtime(), meta.mtime_nsec()),
        size: meta.size(),
        mode: meta.mode(),
        regular_file: meta.is_file(),
    }
}

#[cfg(windows)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeStamp {
    volume: u64,
    file: [u8; 16],
    changed: i64,
    modified: i64,
    attributes: u32,
}

#[cfg(windows)]
impl NativeStamp {
    fn is_file(&self) -> bool {
        use winapi::um::winnt::{FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT};
        self.attributes & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) == 0
    }

    fn identity(&self) -> io::Result<FileIdentity> {
        if self.file == [0; 16] {
            return Err(io::Error::from(io::ErrorKind::Unsupported));
        }
        Ok(FileIdentity {
            volume: self.volume,
            file: self.file,
        })
    }

    pub(super) fn checked_volume(&self) -> io::Result<u64> {
        self.identity().map(|identity| identity.volume)
    }
}

#[cfg(windows)]
pub(super) fn open_native_entry(path: &Path) -> io::Result<fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    use winapi::um::winbase::{FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT};
    use winapi::um::winnt::FILE_READ_ATTRIBUTES;
    // The handle doesn't follow a replacement reparse point and can inspect a
    // directory without reading any of the assets in it.
    fs::OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(windows)]
fn native_stamp(path: &Path) -> io::Result<NativeStamp> {
    stamp_file(&open_native_entry(path)?)
}

#[cfg(windows)]
pub(super) fn stamp_file(file: &fs::File) -> io::Result<NativeStamp> {
    use std::mem::{MaybeUninit, size_of};
    use std::os::windows::io::AsRawHandle;
    use winapi::um::fileapi::{FILE_BASIC_INFO, FILE_ID_INFO};
    use winapi::um::minwinbase::{FileBasicInfo, FileIdInfo};
    use winapi::um::winbase::GetFileInformationByHandleEx;
    let handle = file.as_raw_handle().cast();
    let mut basic = MaybeUninit::<FILE_BASIC_INFO>::uninit();
    let mut identity = MaybeUninit::<FILE_ID_INFO>::uninit();
    // The handle remains live and both buffers exactly match the selected info class.
    unsafe {
        if GetFileInformationByHandleEx(
            handle,
            FileBasicInfo,
            basic.as_mut_ptr().cast(),
            size_of::<FILE_BASIC_INFO>() as u32,
        ) == 0
            || GetFileInformationByHandleEx(
                handle,
                FileIdInfo,
                identity.as_mut_ptr().cast(),
                size_of::<FILE_ID_INFO>() as u32,
            ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let basic = basic.assume_init();
        let identity = identity.assume_init();
        let changed = *basic.ChangeTime.QuadPart();
        if changed == 0 {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "filesystem did not supply a change time",
            ));
        }
        Ok(NativeStamp {
            volume: identity.VolumeSerialNumber,
            file: identity.FileId.Identifier,
            changed,
            modified: *basic.LastWriteTime.QuadPart(),
            attributes: basic.FileAttributes,
        })
    }
}
