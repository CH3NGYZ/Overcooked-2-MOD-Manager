//! Native identity and change time; length/last-write alone miss restored mtimes.
use std::{fs, io, path::Path};

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
}

#[cfg(unix)]
fn native_stamp(path: &Path) -> io::Result<NativeStamp> {
    use std::os::unix::fs::MetadataExt;
    let meta = fs::symlink_metadata(path)?;
    Ok(NativeStamp {
        device: meta.dev(),
        inode: meta.ino(),
        changed: (meta.ctime(), meta.ctime_nsec()),
        modified: (meta.mtime(), meta.mtime_nsec()),
        size: meta.size(),
        mode: meta.mode(),
        regular_file: meta.is_file(),
    })
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
}

#[cfg(windows)]
fn native_stamp(path: &Path) -> io::Result<NativeStamp> {
    use std::{
        mem::{MaybeUninit, size_of},
        os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
    };
    use winapi::um::{
        fileapi::{FILE_BASIC_INFO, FILE_ID_INFO},
        minwinbase::{FileBasicInfo, FileIdInfo},
        winbase::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, GetFileInformationByHandleEx,
        },
        winnt::FILE_READ_ATTRIBUTES,
    };
    // The handle doesn't follow a replacement reparse point and can inspect a
    // directory without reading any of the assets in it.
    let entry = fs::OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let handle = entry.as_raw_handle().cast();
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
