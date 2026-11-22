use super::errors::CONFLICT;
use super::errors::INCOMPLETE;
use super::errors::INVALID_RECORD;
use super::errors::UNSAFE_PATH;
use super::errors::{
    VERIFY_FILES, WRITE_RESTORED_FILE, checked_error, checked_io_error, io_error, log_path,
};
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
    let matches = size == file.size && sha256 == file.sha256;
    if !matches {
        let expected_digest = if file.sha256.len() == 64
            && file.sha256.bytes().all(|value| value.is_ascii_hexdigit())
        {
            file.sha256.as_str()
        } else {
            "<invalid-sha256>"
        };
        log::warn!(
            "回收站文件指纹不符：步骤={}，路径={:?}，预期字节数={}，实际字节数={size}，预期SHA-256={expected_digest}，实际SHA-256={sha256}",
            stage.unwrap_or("回收或回滚文件校验"),
            log_path(path),
            file.size,
        );
    }
    Ok(matches)
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
    match fs::rename(source, target) {
        Ok(()) => {
            log::trace!("回收站文件移动完成：{}", file.relative);
            Ok(())
        }
        Err(rename_error) => copy_verified_and_remove(source, target, file, restore)
            .inspect_err(|error| {
                log::warn!(
                    "回收站改名和复制兜底均失败：恢复={restore}，源={:?}，目标={:?}，改名OS码={:?}，改名分类={:?}，复制兜底原因={:?}",
                    log_path(source),
                    log_path(target),
                    rename_error.raw_os_error(),
                    rename_error.kind(),
                    crate::locale_tag::diagnostic_tag(error),
                );
            }),
    }
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
            log::warn!(
                "回收站文件复制失败：恢复={restore}，源={:?}，目标={:?}，OS码={:?}，分类={:?}",
                log_path(source),
                log_path(target),
                error.raw_os_error(),
                error.kind(),
            );
            if let Some(stage) = write {
                file_check::report(stage, target, file_check::COPY)
            } else {
                super::errors::IO_ERROR.into()
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
                "回收站无法解除只读属性：路径={:?}，原因={:?}",
                log_path(source),
                crate::locale_tag::diagnostic_tag(&error)
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
            log::error!(
                "回收站复制失败的副本未能清理：源={:?}，副本={:?}，OS码={:?}，分类={:?}",
                log_path(source),
                log_path(target),
                error.raw_os_error(),
                error.kind(),
            );
        }
    }
    result
}

pub(super) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    validate_absolute_path(path)?;
    let temp = path.with_file_name(format!("{}.tmp", Uuid::new_v4()));
    let write_error = |step: &str, error: std::io::Error| {
        log::warn!(
            "回收站记录文件写入失败：步骤={step}，目标={:?}，临时文件={:?}，OS码={:?}，分类={:?}",
            log_path(path),
            log_path(&temp),
            error.raw_os_error(),
            error.kind(),
        );
        super::errors::IO_ERROR.to_owned()
    };
    let result = (|| {
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|error| write_error("创建临时文件", error))?;
        output
            .write_all(bytes)
            .map_err(|error| write_error("写入临时文件", error))?;
        output
            .sync_all()
            .map_err(|error| write_error("同步临时文件", error))?;
        drop(output);
        fs::rename(&temp, path).map_err(|error| write_error("提交临时文件", error))
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
                "回收站文件回滚失败：待移回副本={:?}，原位置={:?}，原因={:?}",
                log_path(&from.join(&file.relative)),
                log_path(&to.join(&file.relative)),
                crate::locale_tag::diagnostic_tag(&error)
            );
            complete = false;
        }
    }
    complete
}
