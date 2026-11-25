use super::storage::{DataAction, ResetPaths, ResetPhase, ResetRecord, normalized};
use crate::display_path::display_path;
use std::{path::Path, sync::LazyLock};

static MESSAGES: LazyLock<serde_json::Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../src/locale/tools/dataResetNative.json"
    ))
    .expect("bundled data operation translations")
});

const SUPPORTED_LANGUAGES: [&str; 12] = [
    "en", "de", "fr", "it", "ko", "es-ES", "zh-CN", "zh-TW", "ru", "ja", "pl", "pt-BR",
];

pub(super) fn supported_language(value: &str) -> Option<&'static str> {
    let value = value.trim();
    SUPPORTED_LANGUAGES
        .into_iter()
        .find(|language| *language == value)
}

fn read_install_language(root: &Path) -> Option<String> {
    let value = std::fs::read_to_string(root.join("install-language.txt")).ok()?;
    supported_language(&value).map(str::to_owned)
}

pub(crate) fn install_language() -> Option<String> {
    crate::user_paths::default_root()
        .ok()
        .into_iter()
        .chain(crate::user_paths::root().ok())
        .find_map(|root| read_install_language(&root))
}

#[cfg(windows)]
fn system_language() -> Option<&'static str> {
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
        })
}

#[cfg(target_os = "macos")]
fn system_language() -> Option<&'static str> {
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
        });
        CFRelease(languages);
        result
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn system_language() -> Option<&'static str> {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .find_map(|key| std::env::var(key).ok().filter(|value| !value.is_empty()))
        .as_deref()
        .and_then(system_code)
}

fn system_code(value: &str) -> Option<&'static str> {
    let value = value.trim().split(['.', '@']).next()?;
    let mut parts = value.split(['-', '_']);
    let primary = parts.next()?;
    if primary.eq_ignore_ascii_case("zh") {
        let script_or_region = parts.next().unwrap_or_default();
        let traditional = ["Hant", "TW", "HK", "MO"]
            .iter()
            .any(|part| script_or_region.eq_ignore_ascii_case(part));
        return supported_language(if traditional { "zh-TW" } else { "zh-CN" });
    }
    SUPPORTED_LANGUAGES.into_iter().find(|language| {
        language
            .split('-')
            .next()
            .is_some_and(|base| base.eq_ignore_ascii_case(primary))
    })
}

fn language(requested: Option<&str>) -> String {
    requested
        .and_then(supported_language)
        .map(str::to_owned)
        .or_else(install_language)
        .or_else(|| system_language().map(str::to_owned))
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

pub(super) fn diagnostic_reason(error: &str) -> &str {
    let code = error.split('?').next().unwrap_or_default();
    if [
        "tools.",
        "config.",
        "modChanges.",
        "assetBundle.",
        "update.",
        "appUpdate.",
        "lock.",
        "recycle.",
    ]
    .iter()
    .any(|prefix| code.starts_with(prefix))
        && code.len() <= 80
        && code
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '.')
    {
        code
    } else {
        "unknown"
    }
}

pub(super) fn diagnostic_os_code(error: &str) -> Option<i32> {
    let (_, query) = error.split_once('?')?;
    let (_, reason) =
        url::form_urlencoded::parse(query.as_bytes()).find(|(key, _)| key == "reason")?;
    reason
        .rsplit_once("(os error ")?
        .1
        .strip_suffix(')')?
        .parse()
        .ok()
}

pub(super) fn diagnostics(record: &ResetRecord, current: &Path) -> String {
    let (action, source, target) = match &record.action {
        DataAction::Clear => ("清除", record.data_root.as_deref(), None),
        DataAction::Migrate { source, target } => {
            ("迁移", Some(source.as_path()), Some(target.as_path()))
        }
    };
    let outcome = match record.phase {
        ResetPhase::Pending => "操作已交接",
        ResetPhase::Completed => "操作已完成",
        ResetPhase::Failed => "操作失败",
        _ => "操作未结束",
    };
    let selected = match (source, target) {
        (_, Some(target)) if normalized(target) == normalized(current) => "目标",
        (Some(source), _) if normalized(source) == normalized(current) => "原位置",
        _ => "其他或未知",
    };
    let target_observation = match (
        &record.action,
        record.phase,
        record.error.as_deref().map(diagnostic_reason),
    ) {
        (DataAction::Migrate { .. }, ResetPhase::Pending, _) => "交接复核时不存在",
        (_, _, Some("tools.migrateDataTargetExists")) => "校验时已存在",
        _ => "无记录",
    };
    let os_code = record.error.as_deref().and_then(diagnostic_os_code);
    let log_path = crate::file_check::log_path;
    format!(
        "[data] 管理器数据{outcome}：请求={}，状态={:?}，失败前阶段={}，操作={action}，原数据根={:?}，目标数据根={:?}，当前数据根={:?}，指向={selected}，目标状态={target_observation}，原因={}，OS码={os_code:?}",
        record.token,
        record.phase,
        record
            .failed_from
            .map_or_else(|| "无记录".into(), |phase| format!("{phase:?}")),
        source.map(log_path),
        target.map(log_path),
        log_path(current),
        record.error.as_deref().map_or("无", diagnostic_reason),
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
