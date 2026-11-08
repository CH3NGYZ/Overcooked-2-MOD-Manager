//! macOS 的系统地区检测。
//!
//! Windows 走 `config::current_user_region` 里的 `GetUserGeoID`，不经过本模块。
//! Linux 已随发布目标移除：原先按 `LC_ALL` / 桌面 locale / `locale.conf`
//! 逐级解析地域的整套逻辑一并删除，这里只保留 macOS 需要的 CoreFoundation 读取。

use crate::config::{REGION_CHINESE_MAINLAND, REGION_INTERNATIONAL};

#[cfg(target_os = "macos")]
pub(crate) fn current() -> &'static str {
    // Config reads use a snapshot; the explicit post-tutorial recheck refreshes it.
    *region_cache()
        .read()
        .unwrap_or_else(|error| error.into_inner())
}

#[cfg(target_os = "macos")]
fn region_cache() -> &'static std::sync::RwLock<&'static str> {
    static REGION: std::sync::OnceLock<std::sync::RwLock<&'static str>> =
        std::sync::OnceLock::new();
    REGION.get_or_init(|| std::sync::RwLock::new(detect_region()))
}

#[cfg(target_os = "macos")]
fn detect_region() -> &'static str {
    detect()
        .map(|(region, _)| region)
        .unwrap_or(REGION_INTERNATIONAL)
}

/// Explicit startup recheck also updates the snapshot used by later config reads.
#[cfg(target_os = "macos")]
pub(crate) fn refresh() -> &'static str {
    let region = detect_region();
    *region_cache()
        .write()
        .unwrap_or_else(|error| error.into_inner()) = region;
    region
}

/// 两位国家代码 → 地区。`CN` 是中国大陆，其余有效代码都是国际地区。
///
/// 在 Windows 上只以 `cfg(test)` 参与编译，供单元测试复用。
#[cfg(any(target_os = "macos", test))]
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

#[cfg(test)]
mod tests;
