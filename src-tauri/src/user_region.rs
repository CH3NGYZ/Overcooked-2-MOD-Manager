use crate::config::{REGION_CHINESE_MAINLAND, REGION_INTERNATIONAL};

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn current() -> &'static str {
    // Region is used on every configuration read. Query system preferences once
    // per process rather than spawning desktop commands in these hot paths.
    static REGION: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();
    REGION.get_or_init(|| {
        let detected = detect();
        let (region, source) = detected.unwrap_or((REGION_INTERNATIONAL, "unavailable"));
        log::info!(
            "系统地区检测：平台={}，来源={source}，地区={region}",
            std::env::consts::OS
        );
        if detected.is_none() {
            log::warn!("无法读取明确的系统国家/地区，使用国际渠道");
        }
        region
    })
}

fn country_region(country: &str) -> Option<&'static str> {
    let country = country.trim();
    if country.len() != 2 || !country.bytes().all(|value| value.is_ascii_alphabetic()) {
        return None;
    }
    Some(if country.eq_ignore_ascii_case("CN") {
        REGION_CHINESE_MAINLAND
    } else {
        REGION_INTERNATIONAL
    })
}

#[cfg(target_os = "macos")]
fn detect() -> Option<(&'static str, &'static str)> {
    use std::ffi::{CStr, c_char, c_void};

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFLocaleCopyCurrent() -> *const c_void;
        fn CFLocaleGetValue(locale: *const c_void, key: *const c_void) -> *const c_void;
        static kCFLocaleCountryCode: *const c_void;
        fn CFStringGetCString(
            string: *const c_void,
            buffer: *mut c_char,
            size: isize,
            encoding: u32,
        ) -> u8;
        fn CFRelease(value: *const c_void);
    }

    // CoreFoundation reads the user's Region preference even for applications
    // launched from Finder without LANG, and independently of AppleLanguages.
    unsafe {
        let locale = CFLocaleCopyCurrent();
        if locale.is_null() {
            return None;
        }
        let country = CFLocaleGetValue(locale, kCFLocaleCountryCode);
        let mut buffer = [0 as c_char; 16];
        let region = if !country.is_null()
            && CFStringGetCString(
                country,
                buffer.as_mut_ptr(),
                buffer.len() as isize,
                0x0800_0100,
            ) != 0
        {
            CStr::from_ptr(buffer.as_ptr())
                .to_str()
                .ok()
                .and_then(country_region)
        } else {
            None
        };
        // Country is borrowed from locale. Only release the copied locale.
        CFRelease(locale);
        region.map(|region| (region, "CoreFoundation country"))
    }
}

#[cfg(any(target_os = "linux", test))]
const FORMAT_VARIABLES: [&str; 5] = [
    "LC_ADDRESS",
    "LC_MONETARY",
    "LC_TIME",
    "LC_NUMERIC",
    "LC_MEASUREMENT",
];

#[cfg(any(target_os = "linux", test))]
fn locale_region(value: &str) -> Option<&'static str> {
    let value = value.trim().trim_matches(['\'', '"']);
    let identifier = value.split(['.', '@']).next()?;
    if identifier.eq_ignore_ascii_case("C") || identifier.eq_ignore_ascii_case("POSIX") {
        return Some(REGION_INTERNATIONAL);
    }
    let mut parts = identifier.split(['_', '-']);
    let language = parts.next()?;
    if !(2..=3).contains(&language.len())
        || !language.bytes().all(|value| value.is_ascii_alphabetic())
    {
        return None;
    }
    let mut territory = parts.next()?;
    if territory.len() == 4 && territory.bytes().all(|value| value.is_ascii_alphabetic()) {
        territory = parts.next()?;
    }
    if parts.next().is_some() {
        return None;
    }
    country_region(territory).or_else(|| {
        (territory.len() == 3 && territory.bytes().all(|value| value.is_ascii_digit()))
            .then_some(REGION_INTERNATIONAL)
    })
}

#[cfg(any(target_os = "linux", test))]
fn linux_region(
    environment: impl Fn(&str) -> Option<String>,
    desktop: impl FnOnce() -> Option<String>,
    system: impl FnOnce() -> Option<&'static str>,
) -> Option<(&'static str, &'static str)> {
    if let Some(region) = environment("LC_ALL").as_deref().and_then(locale_region) {
        return Some((region, "LC_ALL"));
    }
    for key in FORMAT_VARIABLES {
        if let Some(region) = environment(key).as_deref().and_then(locale_region) {
            return Some((region, key));
        }
    }
    if let Some(region) = desktop().as_deref().and_then(locale_region) {
        return Some((region, "desktop formats"));
    }
    if let Some(region) = environment("LANG").as_deref().and_then(locale_region) {
        return Some((region, "LANG"));
    }
    system().map(|region| (region, "system locale file"))
}

#[cfg(any(target_os = "linux", test))]
fn locale_setting(document: &str, section: Option<&str>) -> Option<String> {
    let mut values = std::collections::HashMap::new();
    let mut selected = section.is_none();
    for line in document.lines().map(str::trim) {
        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|line| line.strip_suffix(']'))
        {
            selected = section == Some(name);
            continue;
        }
        if !selected || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some((key, value)) = line.strip_prefix("export ").unwrap_or(line).split_once('=') {
            let value = value.split('#').next().unwrap_or_default().trim();
            values.insert(key.trim(), value.trim_matches(['\'', '"']));
        }
    }
    for key in std::iter::once("LC_ALL")
        .chain(FORMAT_VARIABLES)
        .chain(std::iter::once("LANG"))
    {
        if let Some(value) = values
            .get(key)
            .filter(|value| locale_region(value).is_some())
        {
            return Some((*value).into());
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn detect() -> Option<(&'static str, &'static str)> {
    linux_region(
        |name| std::env::var(name).ok(),
        desktop_locale,
        || {
            ["/etc/locale.conf", "/etc/default/locale"]
                .iter()
                .find_map(|path| {
                    let document = std::fs::read_to_string(path).ok()?;
                    locale_setting(&document, None)
                        .as_deref()
                        .and_then(locale_region)
                })
        },
    )
}

#[cfg(target_os = "linux")]
fn desktop_locale() -> Option<String> {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_ascii_lowercase();
    if desktop.contains("kde") {
        let root = std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| dirs::home_dir().map(|home| home.join(".config")))?;
        let document = std::fs::read_to_string(root.join("plasma-localerc")).ok()?;
        return locale_setting(&document, Some("Formats"));
    }
    if desktop.contains("gnome") || desktop.contains("unity") {
        return gnome_locale();
    }
    None
}

#[cfg(target_os = "linux")]
fn gnome_locale() -> Option<String> {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    let mut child = Command::new("/usr/bin/gsettings")
        .args(["get", "org.gnome.system.locale", "region"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + Duration::from_millis(500);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return None;
                }
                let output = child.wait_with_output().ok()?;
                return (output.stdout.len() <= 256)
                    .then(|| String::from_utf8(output.stdout).ok())
                    .flatten();
            }
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn territory_is_independent_of_interface_language() {
        for value in [
            "zh_CN.UTF-8",
            "en_CN.UTF-8",
            "zh-Hans-CN",
            "en-CN",
            "zh_Hans_CN@calendar=gregorian",
            "'zh_CN.UTF-8'",
            "zh_cn.utf8",
        ] {
            assert_eq!(
                locale_region(value),
                Some(REGION_CHINESE_MAINLAND),
                "{value}"
            );
        }
        for value in [
            "zh_TW.UTF-8",
            "zh_HK",
            "zh_MO",
            "zh_SG",
            "en_US",
            "zh-Hant-TW",
            "en-001",
            "C",
            "C.UTF-8",
            "POSIX",
        ] {
            assert_eq!(locale_region(value), Some(REGION_INTERNATIONAL), "{value}");
        }
        for value in [
            "",
            "zh",
            "zh-Hans",
            "CN",
            "invalid_CN",
            "en_CNN",
            "en_CN_extra",
            "../../zh_CN",
            "$(zh_CN)",
        ] {
            assert_eq!(locale_region(value), None, "{value}");
        }
        assert_eq!(country_region("CN"), Some(REGION_CHINESE_MAINLAND));
        assert_eq!(country_region("cn"), Some(REGION_CHINESE_MAINLAND));
        assert_eq!(country_region("TW"), Some(REGION_INTERNATIONAL));
        assert_eq!(country_region(""), None);
    }

    fn from_environment(
        values: &[(&str, &str)],
        desktop: Option<&str>,
        system: Option<&'static str>,
    ) -> Option<(&'static str, &'static str)> {
        linux_region(
            |key| {
                values
                    .iter()
                    .find(|(name, _)| *name == key)
                    .map(|(_, value)| (*value).into())
            },
            || desktop.map(str::to_owned),
            || system,
        )
    }

    #[test]
    fn linux_uses_locale_overrides_and_formats_before_language() {
        assert_eq!(
            from_environment(
                &[
                    ("LC_ALL", "en_US"),
                    ("LC_MONETARY", "zh_CN"),
                    ("LANG", "zh_CN")
                ],
                Some("zh_CN"),
                None
            ),
            Some((REGION_INTERNATIONAL, "LC_ALL"))
        );
        assert_eq!(
            from_environment(&[("LC_ALL", "C.UTF-8"), ("LANG", "zh_CN")], None, None),
            Some((REGION_INTERNATIONAL, "LC_ALL"))
        );
        assert_eq!(
            from_environment(
                &[("LC_ALL", ""), ("LC_TIME", "en_CN"), ("LANG", "en_US")],
                None,
                None
            ),
            Some((REGION_CHINESE_MAINLAND, "LC_TIME"))
        );
        assert_eq!(
            from_environment(
                &[("LC_MONETARY", "en_US"), ("LANG", "zh_CN")],
                Some("zh_CN"),
                None
            ),
            Some((REGION_INTERNATIONAL, "LC_MONETARY"))
        );
        assert_eq!(
            from_environment(&[("LANG", "en_US")], Some("'en_CN.UTF-8'"), None),
            Some((REGION_CHINESE_MAINLAND, "desktop formats"))
        );
        assert_eq!(
            from_environment(&[("LANG", "zh_CN.UTF-8")], Some("''"), None),
            Some((REGION_CHINESE_MAINLAND, "LANG"))
        );
        assert_eq!(
            from_environment(&[("LANG", "zh_Hans")], None, Some(REGION_CHINESE_MAINLAND)),
            Some((REGION_CHINESE_MAINLAND, "system locale file"))
        );
        assert_eq!(from_environment(&[], None, None), None);
    }

    #[test]
    fn linux_avoids_desktop_commands_when_environment_is_explicit() {
        assert_eq!(
            linux_region(
                |key| (key == "LC_ALL").then(|| "en_CN".into()),
                || panic!("Desktop lookup is unnecessary"),
                || panic!("File lookup is unnecessary")
            ),
            Some((REGION_CHINESE_MAINLAND, "LC_ALL"))
        );
        assert_eq!(
            linux_region(
                |key| (key == "LANG").then(|| "en_US".into()),
                || None,
                || panic!("File lookup is unnecessary")
            ),
            Some((REGION_INTERNATIONAL, "LANG"))
        );
    }

    #[test]
    fn linux_reads_only_locale_assignments_and_the_formats_section() {
        assert_eq!(
            locale_setting(
                "# LANG=en_US\nLANG=\"en_US.UTF-8\"\nexport LC_TIME='en_CN.UTF-8' # format\n",
                None
            )
            .as_deref(),
            Some("en_CN.UTF-8")
        );
        assert_eq!(locale_setting("[Translations]\nLANGUAGE=zh_CN\n[Formats]\nLANG=en_CN.UTF-8\n[Other]\nLANG=en_US\n", Some("Formats")).as_deref(), Some("en_CN.UTF-8"));
        assert_eq!(
            locale_setting("[Translations]\nLANG=zh_CN\n", Some("Formats")),
            None
        );
        assert_eq!(locale_setting("LANG=$(echo zh_CN)\n", None), None);
    }
}
