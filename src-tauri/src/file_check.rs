use std::{io, path::Path};

pub(crate) const MISSING: &str = "error.checkedFileMissing";
pub(crate) const CONTENT: &str = "error.checkedFileContent";
pub(crate) const TYPE: &str = "error.checkedFileType";
pub(crate) const READ: &str = "error.checkedFileRead";
pub(crate) const ACCESS: &str = "error.checkedFileAccess";
pub(crate) const BUSY: &str = "error.checkedFileBusy";
pub(crate) const EXISTS: &str = "error.checkedFileExists";
pub(crate) const UNSAFE: &str = "error.checkedFileUnsafe";
pub(crate) const LINK: &str = "error.checkedFileLink";
pub(crate) const PERMISSIONS: &str = "error.checkedFilePermissions";
pub(crate) const WRITE: &str = "error.checkedFileWrite";
pub(crate) const COPY: &str = "error.checkedFileCopy";

pub(crate) fn log_field(value: &str) -> String {
    let redacted = crate::http_diagnostics::redact_field(value);
    let mut chars = redacted.chars();
    let mut display = chars.by_ref().take(512).collect::<String>();
    if chars.next().is_some() {
        display.push('…');
    }
    display
}

pub(crate) fn log_path(path: &Path) -> String {
    log_field(&crate::display_path::display_path(path))
}

pub(crate) fn report(stage: &str, path: &Path, reason: &str) -> String {
    let path = crate::display_path::display_path(path);
    log::warn!(
        "文件操作检查未通过：步骤={stage}，路径={:?}，原因={reason}",
        log_field(&path)
    );
    crate::locale_tag::with_params(stage, &[("path", &path), ("reason", reason)])
}

fn io_reason(error: &io::Error, fallback: &'static str) -> &'static str {
    if cfg!(windows) && matches!(error.raw_os_error(), Some(32 | 33)) {
        return BUSY;
    }
    match error.kind() {
        io::ErrorKind::NotFound => MISSING,
        io::ErrorKind::AlreadyExists => EXISTS,
        io::ErrorKind::PermissionDenied => ACCESS,
        _ => fallback,
    }
}

pub(crate) fn report_io(
    stage: &str,
    path: &Path,
    error: io::Error,
    fallback: &'static str,
) -> String {
    let reason = io_reason(&error, fallback);
    log::warn!(
        "文件系统操作失败：步骤={stage}，路径={:?}，原因={reason}，OS码={:?}，分类={:?}",
        log_path(path),
        error.raw_os_error(),
        error.kind()
    );
    crate::locale_tag::with_params(
        stage,
        &[
            ("path", &crate::display_path::display_path(path)),
            ("reason", reason),
        ],
    )
}
