use super::errors::CONFLICT;
use super::errors::INCOMPLETE;
use super::errors::INVALID_RECORD;
use super::errors::UNSAFE_PATH;
use super::errors::io_error;
use super::paths::checked_path;
use super::paths::is_link;
use super::paths::validate_absolute_path;
use super::records::StagedFile;
use crate::archive;
use sha2::Digest;
use sha2::Sha256;
use std::fs;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use uuid::Uuid;

pub(super) const MAX_RECORD_BYTES: u64 = 8 * 1024 * 1024;

pub(super) fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
    if is_link(&metadata) || !metadata.is_file() || metadata.len() > MAX_RECORD_BYTES {
        return Err(INVALID_RECORD.into());
    }
    fs::read(path).map_err(io_error)
}

pub(super) fn fingerprint(path: &Path) -> Result<(u64, String), String> {
    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
    if is_link(&metadata) || !metadata.is_file() {
        return Err(UNSAFE_PATH.into());
    }
    let mut input = File::open(path).map_err(io_error)?;
    let mut digest = Sha256::new();
    let mut total = 0;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let length = input.read(&mut buffer).map_err(io_error)?;
        if length == 0 {
            break;
        }
        digest.update(&buffer[..length]);
        total += length as u64;
    }
    Ok((total, format!("{:x}", digest.finalize())))
}

pub(super) fn matches_fingerprint(path: &Path, file: &StagedFile) -> Result<bool, String> {
    let (size, sha256) = fingerprint(path)?;
    Ok(size == file.size && sha256 == file.sha256)
}

// Cross-volume moves fall back to a verified copy. The source is removed only
// after all bytes were flushed and match the original fingerprint.
pub(super) fn move_file(source: &Path, target: &Path, file: &StagedFile) -> Result<(), String> {
    validate_absolute_path(source)?;
    validate_absolute_path(target)?;
    if target.try_exists().map_err(io_error)? {
        return Err(CONFLICT.into());
    }
    if !matches_fingerprint(source, file)? {
        return Err(INCOMPLETE.into());
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    if fs::rename(source, target).is_ok() {
        log::trace!("回收站文件移动完成：{}", file.relative);
        return Ok(());
    }
    copy_verified_and_remove(source, target, file)
}

pub(super) fn copy_verified_and_remove(
    source: &Path,
    target: &Path,
    file: &StagedFile,
) -> Result<(), String> {
    log::debug!("回收站跨目录复制校验：{}", file.relative);
    let mut created_copy = false;
    let result = (|| {
        let mut input = File::open(source).map_err(io_error)?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)
            .map_err(io_error)?;
        created_copy = true;
        std::io::copy(&mut input, &mut output).map_err(io_error)?;
        output.sync_all().map_err(io_error)?;
        drop(output);
        if !matches_fingerprint(target, file)? || !matches_fingerprint(source, file)? {
            return Err(INCOMPLETE.into());
        }
        archive::clear_readonly(source).map_err(io_error)?;
        fs::remove_file(source).map_err(io_error)
    })();
    if result.is_err() && created_copy && source.exists() && target.exists() {
        if let Err(error) = fs::remove_file(target) {
            log::error!("回收站复制失败的副本保留：{}", error);
        }
    }
    result
}

pub(super) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    validate_absolute_path(path)?;
    let temp = path.with_file_name(format!("{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(io_error)?;
        output.write_all(bytes).map_err(io_error)?;
        output.sync_all().map_err(io_error)?;
        drop(output);
        fs::rename(&temp, path).map_err(io_error)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

pub(super) fn rollback_moves(from: &Path, to: &Path, moved: &[StagedFile]) -> bool {
    let mut complete = true;
    for file in moved.iter().rev() {
        let result = checked_path(from, &file.relative).and_then(|source| {
            checked_path(to, &file.relative).and_then(|target| move_file(&source, &target, file))
        });
        if let Err(error) = result {
            log::error!(
                "回收站文件回滚失败，保留副本：文件={}，原因={}",
                file.relative,
                error
            );
            complete = false;
        }
    }
    complete
}
