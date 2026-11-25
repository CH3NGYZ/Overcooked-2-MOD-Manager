use crate::display_path::display_path;
use crate::elevation::wide;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::Path;
use std::sync::LazyLock;
use tauri::{AppHandle, Manager};
use winapi::um::winuser::{IDYES, MB_DEFBUTTON2, MB_ICONWARNING, MB_YESNO, MessageBoxW};

static MESSAGES: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../src/locale/update/administratorNative.json"
    ))
    .expect("bundled administrator update translations")
});

fn message(locale: &str, version: &str, destination: &Path) -> String {
    MESSAGES
        .get(locale.trim())
        .unwrap_or_else(|| &MESSAGES["en"])
        .replace("{version}", version)
        .replace("{path}", &display_path(destination))
}

pub(crate) fn confirm(
    app: &AppHandle,
    locale: &str,
    version: &str,
    destination: &Path,
) -> Result<bool, String> {
    let window = app
        .get_webview_window("main")
        .ok_or("update.installAsAdministratorUnavailable")?;
    let owner = window.hwnd().map_err(|error| {
        log::warn!("无法打开管理员更新确认窗口：{error}");
        "update.installAsAdministratorUnavailable"
    })?;
    let body = message(locale, version, destination);
    let result = unsafe {
        MessageBoxW(
            owner.0.cast(),
            wide(OsStr::new(&body)).as_ptr(),
            wide(OsStr::new("OC2 MOD Manager")).as_ptr(),
            MB_YESNO | MB_DEFBUTTON2 | MB_ICONWARNING,
        )
    };
    if result == 0 {
        log::warn!(
            "无法显示管理员更新确认：{}",
            std::io::Error::last_os_error()
        );
        return Err("update.installAsAdministratorUnavailable".into());
    }
    Ok(result == IDYES)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_confirmation_includes_verified_version_and_path_for_all_languages() {
        let destination = Path::new(r"C:\Program Files\带空格\OC2ModManager.exe");
        for locale in [
            "en", "de", "fr", "it", "ko", "es-ES", "zh-CN", "zh-TW", "ru", "ja", "pl", "pt-BR",
        ] {
            assert!(MESSAGES.contains_key(locale));
            let text = message(locale, "9.8.7", destination);
            assert!(text.contains("9.8.7"), "{locale}");
            assert!(text.contains(&display_path(destination)), "{locale}");
            assert!(!text.contains(['{', '}']), "{locale}");
        }
        assert_eq!(
            message("invalid", "9.8.7", destination),
            message("en", "9.8.7", destination)
        );
    }
}
