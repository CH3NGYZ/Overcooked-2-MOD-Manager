use super::storage::{DataAction, ResetPaths, ResetPhase, ResetRecord, normalized};
use crate::display_path::display_path;
use std::{path::Path, sync::LazyLock};

static MESSAGES: LazyLock<serde_json::Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../src/locale/tools/dataResetNative.json"
    ))
    .expect("bundled data operation translations")
});

pub(crate) fn supported_language(value: &str) -> Option<&str> {
    match value.trim() {
        value @ ("zh-CN" | "zh-TW" | "en" | "ko" | "ja") => Some(value),
        _ => None,
    }
}

pub(crate) fn install_language() -> Option<String> {
    crate::user_paths::default_root()
        .ok()
        .into_iter()
        .chain(crate::user_paths::root().ok())
        .find_map(|root| {
            let value = std::fs::read_to_string(root.join("install-language.txt")).ok()?;
            supported_language(&value).map(str::to_owned)
        })
}

#[cfg(windows)]
fn system_language() -> Option<String> {
    use winapi::um::winnls::{
        GetUserDefaultLocaleName, GetUserDefaultUILanguage, LCIDToLocaleName,
    };

    let mut display = [0_u16; 85];
    let mut regional = [0_u16; 85];
    let (display_length, regional_length) = unsafe {
        (
            LCIDToLocaleName(
                u32::from(GetUserDefaultUILanguage()),
                display.as_mut_ptr(),
                display.len() as i32,
                0,
            ),
            GetUserDefaultLocaleName(regional.as_mut_ptr(), regional.len() as i32),
        )
    };
    [(display, display_length), (regional, regional_length)]
        .into_iter()
        .filter(|(_, length)| *length > 1)
        .find_map(|(buffer, length)| {
            system_code(&String::from_utf16_lossy(&buffer[..length as usize - 1]))
                .map(str::to_owned)
        })
}

#[cfg(target_os = "macos")]
fn system_language() -> Option<String> {
    use std::ffi::{CStr, c_char, c_void};

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFLocaleCopyPreferredLanguages() -> *const c_void;
        fn CFArrayGetCount(array: *const c_void) -> isize;
        fn CFArrayGetValueAtIndex(array: *const c_void, index: isize) -> *const c_void;
        fn CFStringGetCString(
            string: *const c_void,
            buffer: *mut c_char,
            size: isize,
            encoding: u32,
        ) -> u8;
        fn CFRelease(value: *const c_void);
    }

    unsafe {
        let languages = CFLocaleCopyPreferredLanguages();
        if languages.is_null() {
            return None;
        }
        let result = (0..CFArrayGetCount(languages)).find_map(|index| {
            let value = CFArrayGetValueAtIndex(languages, index);
            let mut buffer = [0 as c_char; 85];
            if CFStringGetCString(
                value,
                buffer.as_mut_ptr(),
                buffer.len() as isize,
                0x0800_0100,
            ) == 0
            {
                return None;
            }
            CStr::from_ptr(buffer.as_ptr())
                .to_str()
                .ok()
                .and_then(system_code)
                .map(str::to_owned)
        });
        CFRelease(languages);
        result
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn system_language() -> Option<String> {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .find_map(|key| std::env::var(key).ok().filter(|value| !value.is_empty()))
}

fn system_code(value: &str) -> Option<&'static str> {
    let value = value
        .split('.')
        .next()?
        .replace('_', "-")
        .to_ascii_lowercase();
    match value.split('-').next()? {
        "zh" if value
            .split('-')
            .any(|part| matches!(part, "tw" | "hk" | "mo" | "hant")) =>
        {
            Some("zh-TW")
        }
        "zh" => Some("zh-CN"),
        "en" => Some("en"),
        "ko" => Some("ko"),
        "ja" => Some("ja"),
        _ => None,
    }
}

fn language(requested: Option<&str>) -> String {
    requested
        .and_then(supported_language)
        .map(str::to_owned)
        .or_else(install_language)
        .or_else(|| {
            system_language()
                .as_deref()
                .and_then(system_code)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "zh-CN".into())
}

fn message(key: &str, language: &str) -> &'static str {
    MESSAGES[key][language]
        .as_str()
        .expect("complete data operation translations")
}

fn path_line(key: &str, language: &str, path: &Path) -> String {
    message(key, language).replace("{path}", &display_path(path))
}

fn reason(error: &str, language: &str) -> Option<String> {
    let (key, params) = error.split_once('?').unwrap_or((error, ""));
    match key {
        "tools.migrateDataTargetExists" => {
            let (_, path) = url::form_urlencoded::parse(params.as_bytes())
                .find(|(key, value)| key == "path" && !value.is_empty())?;
            Some(message("migrateDataTargetExists", language).replace("{path}", path.as_ref()))
        }
        "tools.migrateDataUnsafe" | "tools.clearAllConfigUnsafe" => {
            Some(message("dataOperationUnsafe", language).into())
        }
        "tools.clearAllConfigBusy" => Some(message("clearAllConfigBusy", language).into()),
        "config.directoryUnavailable" => {
            Some(message("dataOperationRootUnavailable", language).into())
        }
        _ => None,
    }
}

fn format_failure(record: &ResetRecord, current: Option<&Path>, language: &str) -> String {
    let mut lines = vec![message("dataOperationFailed", language).to_owned()];
    let stage = match record.failed_from {
        Some(ResetPhase::Pending) => "dataOperationPending",
        Some(ResetPhase::Copying) => "dataOperationCopying",
        Some(ResetPhase::Verified) => "dataOperationVerified",
        Some(
            ResetPhase::Deleting
            | ResetPhase::Switched
            | ResetPhase::Cleaning
            | ResetPhase::Completed,
        ) => "dataOperationCleaning",
        _ => "dataOperationStageUnknown",
    };
    lines.push(message(stage, language).into());
    if let DataAction::Migrate { source, target } = &record.action {
        lines.push(path_line("dataOperationSource", language, source));
        lines.push(path_line("dataOperationTarget", language, target));
    } else if let Some(root) = &record.data_root {
        lines.push(path_line("dataOperationSource", language, root));
    }
    lines.push(current.map_or_else(
        || message("dataOperationCurrentUnknown", language).into(),
        |root| path_line("dataOperationCurrent", language, root),
    ));
    if let Some(reason) = record
        .error
        .as_deref()
        .and_then(|error| reason(error, language))
    {
        lines.push(reason);
    }
    let next = match (&record.action, current) {
        (DataAction::Migrate { target, .. }, Some(root))
            if normalized(target) == normalized(root) =>
        {
            "dataOperationSwitched"
        }
        (DataAction::Migrate { source, .. }, Some(root))
            if normalized(source) == normalized(root) =>
        {
            "dataOperationRetry"
        }
        _ => "dataOperationPreserve",
    };
    lines.push(message(next, language).into());
    lines.join("\n")
}

pub(super) fn diagnostics(record: &ResetRecord) -> String {
    let current = crate::user_paths::root().ok();
    format!(
        "管理器数据操作失败：请求={}，阶段={:?}，操作={:?}，原数据根={:?}，当前数据根={:?}，原因={}",
        record.token,
        record.failed_from,
        record.action,
        record.data_root,
        current,
        record.error.as_deref().unwrap_or("unknown"),
    )
}

pub(super) fn show_once(paths: &ResetPaths, record: &ResetRecord) {
    match paths.claim_failure_notice(record.token) {
        Ok(Some(record)) => {
            let current = crate::user_paths::root().ok();
            let message = format_failure(
                &record,
                current.as_deref(),
                &language(record.locale.as_deref()),
            );
            super::handoff::show_error(&message);
        }
        Ok(None) => {}
        Err(error) => eprintln!("Failed to claim data operation notice: {error}"),
    }
}

pub(super) fn show_unrecorded(error: &str, requested: Option<&str>) {
    eprintln!("Manager data operation did not finish: {error}");
    let language = language(requested);
    let mut lines = vec![message("dataOperationFailed", &language).to_owned()];
    if let Some(detail) = reason(error, &language) {
        lines.push(detail);
    }
    lines.push(message("dataOperationPreserve", &language).into());
    super::handoff::show_error(&lines.join("\n"));
}

#[cfg(test)]
mod tests;
