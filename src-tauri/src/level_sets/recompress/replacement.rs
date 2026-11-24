use crate::file_metadata::FileIdentity;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek};
use std::path::Path;

pub(super) const MAX_BUNDLE_BYTES: u64 = 512 * 1024 * 1024;

pub(super) fn open_existing(path: &Path, writable: bool) -> io::Result<(File, FileIdentity)> {
    let expected = FileIdentity::capture(path)?.ok_or(io::ErrorKind::NotFound)?;
    if writable && fs::symlink_metadata(path)?.permissions().readonly() {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let mut options = OpenOptions::new();
    options.read(true).write(writable);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use winapi::um::winbase::FILE_FLAG_OPEN_REPARSE_POINT;
        use winapi::um::winnt::FILE_SHARE_READ;
        options
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .share_mode(FILE_SHARE_READ);
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW_ANY | libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    if FileIdentity::from_file(&file)? != expected || !is_current(path, &file, &expected) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok((file, expected))
}

pub(super) fn is_current(path: &Path, file: &File, identity: &FileIdentity) -> bool {
    file.metadata().is_ok_and(|metadata| metadata.is_file())
        && crate::mod_changes::transaction::has_single_link(file)
        && FileIdentity::capture(path).ok().flatten().as_ref() == Some(identity)
}

pub(super) fn hash(path: &Path) -> io::Result<Vec<u8>> {
    digest(&mut File::open(path)?)
}

pub(super) fn digest(file: &mut File) -> io::Result<Vec<u8>> {
    file.rewind()?;
    let mut digest = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let count = file.read(&mut buf)?;
        if count == 0 {
            break;
        }
        digest.update(&buf[..count]);
    }
    Ok(digest.finalize().to_vec())
}

pub(super) fn verified_backup(path: &Path, expected: &[u8]) -> io::Result<Vec<u8>> {
    let (file, _) = open_existing(path, false)?;
    let mut bytes = Vec::new();
    file.take(MAX_BUNDLE_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_BUNDLE_BYTES || Sha256::digest(&bytes).as_slice() != expected {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "rollback backup hash mismatch",
        ));
    }
    Ok(bytes)
}

pub(super) fn overwrite(file: &mut File, mut bytes: impl Read, expected: &[u8]) -> io::Result<()> {
    file.rewind()?;
    file.set_len(0)?;
    io::copy(&mut bytes, file)?;
    file.sync_all()?;
    if digest(file)? != expected {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "bundle write hash mismatch",
        ));
    }
    Ok(())
}

pub(super) fn replace(
    target_path: (&Path, &Path),
    output: &Path,
    backup: &Path,
    source_hash: &[u8],
    output_hash: &[u8],
    should_continue: &dyn Fn() -> bool,
) -> Result<bool, String> {
    let (_, path) = target_path;
    if !should_continue() || super::recovery::blocks_conversion(path) {
        return Ok(false);
    }
    let (mut target, identity) = open_existing(path, true).map_err(|error| error.to_string())?;
    let input = File::open(output).map_err(|error| error.to_string())?;
    if digest(&mut target).map_err(|error| error.to_string())? != source_hash
        || !is_current(path, &target, &identity)
    {
        return Err("source changed before replacement".into());
    }
    if !should_continue() {
        return Ok(false);
    }
    let result = overwrite(&mut target, input, output_hash)
        .map_err(|error| error.to_string())
        .and_then(|()| {
            is_current(path, &target, &identity)
                .then_some(())
                .ok_or_else(|| super::recovery::SOURCE_CHANGED.to_string())
        });
    if let Err(error) = result {
        let rollback: Result<(), String> = (|| {
            let bytes = verified_backup(backup, source_hash).map_err(|error| error.to_string())?;
            if !is_current(path, &target, &identity) {
                return Err(super::recovery::SOURCE_CHANGED.into());
            }
            overwrite(&mut target, bytes.as_slice(), source_hash)
                .map_err(|error| error.to_string())?;
            if !is_current(path, &target, &identity) {
                return Err(super::recovery::SOURCE_CHANGED.into());
            }
            Ok(())
        })();
        if let Err(restore_error) = &rollback {
            let failed_hash = digest(&mut target).ok();
            super::recovery::register(
                target_path,
                backup,
                identity,
                source_hash,
                failed_hash,
                restore_error,
            );
        }
        return Err(format!(
            "{error}; rollback={rollback:?}; backup={}",
            backup.display()
        ));
    }
    Ok(true)
}
