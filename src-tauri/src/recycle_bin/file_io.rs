use super::errors::CONFLICT;
use super::errors::INCOMPLETE;
use super::errors::INVALID_RECORD;
use super::errors::UNSAFE_PATH;
use super::errors::{VERIFY_FILES, WRITE_RESTORED_FILE, checked_error, checked_io_error, io_error};
use super::paths::checked_path;
use super::paths::is_link;
use super::paths::{validate_absolute_path, validate_absolute_path_for};
use super::records::StagedFile;
use crate::{archive, file_check};
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

pub(super) fn fingerprint(path: &Path, stage: Option<&str>) -> Result<(u64, String), String> {
    let read_error = |error| checked_io_error(stage, path, error, file_check::READ);
    let metadata = fs::symlink_metadata(path).map_err(read_error)?;
    if is_link(&metadata) || !metadata.is_file() {
        let reason = if is_link(&metadata) {
            file_check::UNSAFE
        } else {
            file_check::TYPE
        };
        return Err(checked_error(stage, path, reason, UNSAFE_PATH));
    }
    let mut input = File::open(path).map_err(read_error)?;
    let mut digest = Sha256::new();
    let mut total = 0;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let length = input.read(&mut buffer).map_err(read_error)?;
        if length == 0 {
            break;
        }
        digest.update(&buffer[..length]);
        total += length as u64;
    }
    Ok((total, format!("{:x}", digest.finalize())))
}

pub(super) fn matches_fingerprint(
    path: &Path,
    file: &StagedFile,
    stage: Option<&str>,
) -> Result<bool, String> {
    let (size, sha256) = fingerprint(path, stage)?;
    Ok(size == file.size && sha256 == file.sha256)
}

// Cross-volume moves fall back to a verified copy. The source is removed only
// after all bytes were flushed and match the original fingerprint.
pub(super) fn move_file(
    source: &Path,
    target: &Path,
    file: &StagedFile,
    restore: bool,
) -> Result<(), String> {
    let verify = restore.then_some(VERIFY_FILES);
    let write = restore.then_some(WRITE_RESTORED_FILE);
    validate_absolute_path_for(source, verify)?;
    validate_absolute_path_for(target, write)?;
    if target
        .try_exists()
        .map_err(|error| checked_io_error(write, target, error, file_check::READ))?
    {
        return Err(checked_error(write, target, file_check::EXISTS, CONFLICT));
    }
    if !matches_fingerprint(source, file, verify)? {
        return Err(checked_error(
            verify,
            source,
            file_check::CONTENT,
            INCOMPLETE,
        ));
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| checked_io_error(write, parent, error, file_check::WRITE))?;
    }
    if fs::rename(source, target).is_ok() {
        log::trace!("回收站文件移动完成：{}", file.relative);
        return Ok(());
    }
    copy_verified_and_remove(source, target, file, restore)
}

pub(super) fn copy_verified_and_remove(
    source: &Path,
    target: &Path,
    file: &StagedFile,
    restore: bool,
) -> Result<(), String> {
    log::debug!("回收站跨目录复制校验：{}", file.relative);
    let mut created_copy = false;
    let verify = restore.then_some(VERIFY_FILES);
    let write = restore.then_some(WRITE_RESTORED_FILE);
    let target_error = |error| checked_io_error(write, target, error, file_check::WRITE);
    let result = (|| {
        let mut input = File::open(source)
            .map_err(|error| checked_io_error(verify, source, error, file_check::READ))?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)
            .map_err(target_error)?;
        created_copy = true;
        std::io::copy(&mut input, &mut output).map_err(|error| {
            if let Some(stage) = write {
                log::warn!(
                    "回收站恢复复制失败：源={}，目标={}，原因={error}",
                    source.display(),
                    target.display()
                );
                file_check::report(stage, target, file_check::COPY)
            } else {
                io_error(error)
            }
        })?;
        output.sync_all().map_err(target_error)?;
        drop(output);
        if !matches_fingerprint(target, file, write)? {
            return Err(checked_error(
                write,
                target,
                file_check::CONTENT,
                INCOMPLETE,
            ));
        }
        if !matches_fingerprint(source, file, verify)? {
            return Err(checked_error(
                verify,
                source,
                file_check::CONTENT,
                INCOMPLETE,
            ));
        }
        archive::clear_readonly(source).map_err(|error| {
            log::warn!(
                "回收站无法解除只读属性：路径={}，原因={error}",
                source.display()
            );
            checked_error(
                write,
                source,
                file_check::PERMISSIONS,
                super::errors::IO_ERROR,
            )
        })?;
        fs::remove_file(source)
            .map_err(|error| checked_io_error(write, source, error, file_check::WRITE))
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
            checked_path(to, &file.relative)
                .and_then(|target| move_file(&source, &target, file, false))
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
