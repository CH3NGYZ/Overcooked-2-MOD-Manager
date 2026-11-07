pub(super) const UNSAFE_PATH: &str = "recycle.unsafePath";

pub(super) const INVALID_RECORD: &str = "recycle.invalidRecord";

pub(super) const IO_ERROR: &str = "recycle.fileOperationFailed";

pub(super) const CONFLICT: &str = "recycle.restoreTargetExists";

pub(super) const INCOMPLETE: &str = "recycle.payloadModified";

pub(super) const NO_FILES: &str = "recycle.noMovableFiles";

pub(super) fn io_error(error: impl std::fmt::Display) -> String {
    log::error!("回收站文件操作失败：{}", error);
    #[cfg(test)]
    eprintln!("回收站测试 I/O 诊断：{}", error);
    IO_ERROR.into()
}
