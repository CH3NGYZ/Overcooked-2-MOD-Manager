pub(super) const UNSAFE_PATH: &str = "recycle.unsafePath";

pub(super) const INVALID_RECORD: &str = "recycle.invalidRecord";

pub(super) const IO_ERROR: &str = "recycle.fileOperationFailed";

pub(super) const CONFLICT: &str = "recycle.restoreTargetExists";

pub(super) const INCOMPLETE: &str = "recycle.payloadModified";
pub(super) const VERIFY_FILES: &str = "recycle.verifyFiles";
pub(super) const WRITE_RESTORED_FILE: &str = "recycle.writeRestoredFile";

pub(super) const NO_FILES: &str = "recycle.noMovableFiles";

pub(super) use crate::file_check::log_path;

pub(super) fn log_failure(
    action: &str,
    id: &str,
    stage: &str,
    path: &std::path::Path,
    error: &str,
) {
    log::warn!(
        "回收站{action}失败：ID={:?}，阶段={stage}，路径={:?}，原因={:?}",
        crate::file_check::log_field(id),
        log_path(path),
        crate::locale_tag::diagnostic_tag(error),
    );
}

pub(super) fn io_error(error: impl std::fmt::Display) -> String {
    log::error!("回收站文件操作失败：{}", error);
    #[cfg(test)]
    eprintln!("回收站测试 I/O 诊断：{}", error);
    IO_ERROR.into()
}

pub(super) fn checked_error(
    stage: Option<&str>,
    path: &std::path::Path,
    reason: &str,
    fallback: &str,
) -> String {
    stage.map_or_else(
        || fallback.into(),
        |stage| crate::file_check::report(stage, path, reason),
    )
}

pub(super) fn checked_io_error(
    stage: Option<&str>,
    path: &std::path::Path,
    error: std::io::Error,
    fallback: &'static str,
) -> String {
    match stage {
        Some(stage) => crate::file_check::report_io(stage, path, error, fallback),
        None => {
            log::warn!(
                "回收站文件系统操作失败：路径={:?}，操作={fallback}，OS码={:?}，分类={:?}",
                log_path(path),
                error.raw_os_error(),
                error.kind(),
            );
            IO_ERROR.into()
        }
    }
}
