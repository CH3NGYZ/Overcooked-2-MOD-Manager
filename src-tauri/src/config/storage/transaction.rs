//! 同目录临时文件、旧配置恢复与 Windows 短时共享冲突处理。

use crate::file_check::log_path;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

pub(super) fn write_config(root: &Path, target: &Path, text: &[u8]) -> io::Result<()> {
    // NamedTempFile owns cleanup even if a partial write or the backup rename
    // fails. Close its handle before Windows tries to rename the temporary file.
    let mut temporary = tempfile::Builder::new()
        .prefix("config.")
        .suffix(".tmp")
        .tempfile_in(root)
        .inspect_err(|error| {
            log::error!(
                "[config] 创建配置临时文件失败：目录={:?}，分类={:?}，OS码={:?}",
                log_path(root),
                error.kind(),
                error.raw_os_error(),
            )
        })?;
    temporary.write_all(text).inspect_err(|error| {
        log::error!(
            "[config] 写入配置临时文件失败：文件={:?}，分类={:?}，OS码={:?}",
            log_path(temporary.path()),
            error.kind(),
            error.raw_os_error(),
        )
    })?;
    let temporary = temporary.into_temp_path();
    let previous = root.join("config.previous.toml");
    let result = replace_with_backup(&temporary, target, &previous);
    let temporary_path = log_path(&temporary);
    if let Err(error) = temporary.close()
        && error.kind() != io::ErrorKind::NotFound
    {
        log::warn!(
            "[config] 清理本次配置临时文件失败：文件={temporary_path:?}，分类={:?}，OS码={:?}",
            error.kind(),
            error.raw_os_error(),
        );
    }
    result
}

fn replace_with_backup(temporary: &Path, target: &Path, previous: &Path) -> io::Result<()> {
    let backed_up = target.try_exists().inspect_err(|error| {
        log::error!(
            "[config] 保存前无法检查主配置位置：文件={:?}，原因={error}",
            log_path(target)
        );
    })?;
    if backed_up {
        if let Err(error) = retry_sharing_violation(|| fs::remove_file(previous))
            && error.kind() != io::ErrorKind::NotFound
        {
            // Do not replace recovery material that could not be removed.
            log::error!(
                "[config] 删除旧配置备份失败，保留原配置和备份：主文件={:?}，备份={:?}，原因={error}",
                log_path(target),
                log_path(previous)
            );
            return Err(error);
        }
        retry_sharing_violation(|| fs::rename(target, previous)).inspect_err(|error| {
            log::error!(
                "[config] 备份旧配置失败：主文件={:?}，备份={:?}，原因={error}",
                log_path(target),
                log_path(previous)
            );
        })?;
    }
    if let Err(error) = retry_sharing_violation(|| fs::rename(temporary, target)) {
        log::error!(
            "[config] 发布新配置失败：临时={:?}，目标={:?}，备份={:?}，本次已移走旧文件={backed_up}，分类={:?}，OS码={:?}",
            log_path(temporary),
            log_path(target),
            log_path(previous),
            error.kind(),
            error.raw_os_error(),
        );
        // Only restore the original moved by this transaction. A pre-existing
        // previous file or an externally created target must stay recoverable.
        if backed_up {
            match target.try_exists() {
                Ok(false) => {
                    if let Err(rollback_error) =
                        retry_sharing_violation(|| fs::rename(previous, target))
                    {
                        log::error!(
                            "[config] 替换及恢复旧配置均失败，保留备份：主文件={:?}，备份={:?}，替换原因={error}，恢复原因={rollback_error}",
                            log_path(target),
                            log_path(previous)
                        );
                    } else {
                        log::warn!(
                            "[config] 替换配置失败，已恢复旧配置：主文件={:?}，原因={error}",
                            log_path(target)
                        );
                    }
                }
                Ok(true) => log::warn!(
                    "[config] 替换配置失败，目标已存在，保留旧配置备份：主文件={:?}，备份={:?}，原因={error}",
                    log_path(target),
                    log_path(previous)
                ),
                Err(inspection_error) => {
                    log::error!(
                        "[config] 替换配置失败且无法检查恢复目标，保留旧配置备份：主文件={:?}，备份={:?}，替换原因={error}，检查原因={inspection_error}",
                        log_path(target),
                        log_path(previous)
                    );
                }
            }
        } else {
            log::error!(
                "[config] 替换配置失败，本次未移动旧配置，不恢复既有备份：主文件={:?}，备份={:?}，原因={error}",
                log_path(target),
                log_path(previous)
            );
        }
        return Err(error);
    }
    if let Err(error) = retry_sharing_violation(|| fs::remove_file(previous))
        && error.kind() != io::ErrorKind::NotFound
    {
        log::warn!(
            "[config] 配置保存成功但旧备份清理失败：备份={:?}，原因={error}",
            log_path(previous)
        );
    }
    Ok(())
}

pub(super) fn retry_sharing_violation<T>(
    mut operation: impl FnMut() -> io::Result<T>,
) -> io::Result<T> {
    let mut retries = 0;
    loop {
        match operation() {
            Err(error) if cfg!(windows) && error.raw_os_error() == Some(32) && retries < 4 => {
                retries += 1;
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            result => return result,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_temporary_file_restores_the_original_moved_by_this_transaction() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("config.toml");
        let previous = root.path().join("config.previous.toml");
        fs::write(&target, b"original configuration").unwrap();
        // A removed temporary file forces publication to fail after backup.
        let error = replace_with_backup(
            &root.path().join("absent-temporary-file"),
            &target,
            &previous,
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert_eq!(fs::read(&target).unwrap(), b"original configuration");
        assert!(!previous.exists());
    }

    #[test]
    fn publication_failure_without_own_backup_preserves_existing_recovery_material() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("config.toml");
        let previous = root.path().join("config.previous.toml");
        fs::write(&previous, b"pre-existing recovery material").unwrap();
        let error = replace_with_backup(
            &root.path().join("absent-temporary-file"),
            &target,
            &previous,
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert_eq!(
            fs::read(&previous).unwrap(),
            b"pre-existing recovery material"
        );
        assert!(
            !target.exists(),
            "a failed transaction moved recovery material it did not create"
        );
    }
}
