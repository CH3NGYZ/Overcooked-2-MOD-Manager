use super::*;
use std::path::Path;

#[test]
fn strips_windows_verbatim_drive_prefixes() {
    assert_eq!(
        display_path(Path::new(
            r"\\?\E:\SteamLibrary\steamapps\common\Overcooked! 2"
        )),
        r"E:\SteamLibrary\steamapps\common\Overcooked! 2"
    );
    // 已经是普通写法时原样返回，重复调用不会越剥越短。
    assert_eq!(
        display_path(r"E:\Games\Overcooked! 2"),
        r"E:\Games\Overcooked! 2"
    );
    assert_eq!(display_path(Path::new(r"\\?\C:\")), r"C:\");
}

#[test]
fn restores_verbatim_unc_to_plain_unc() {
    assert_eq!(
        display_path(Path::new(r"\\?\UNC\server\share\levels")),
        r"\\server\share\levels"
    );
    assert_eq!(
        display_path(Path::new(r"\\?\unc\server\share")),
        r"\\server\share"
    );
    // 普通 UNC 不该被误当成 verbatim 前缀。
    assert_eq!(
        display_path(Path::new(r"\\server\share\levels")),
        r"\\server\share\levels"
    );
}

#[test]
fn keeps_posix_paths_and_native_separators() {
    // POSIX 路径只可能在非 Windows 上出现：Windows 上 `/` 会被归一成 `\`。
    #[cfg(not(windows))]
    {
        let mac = "/Users/someone/Library/Application Support/Steam/steamapps/common/Overcooked! 2";
        assert_eq!(display_path(Path::new(mac)), mac);
    }
    // Windows 风格路径保留 `\`，不悄悄换成 `/`。
    assert_eq!(
        display_path(Path::new(r"E:\Games\Overcooked! 2\BepInEx")),
        r"E:\Games\Overcooked! 2\BepInEx"
    );
}

/// 拼接时片段自带 `/`（`DIYLEVEL_DIR` 这类跨平台常量）会留下半截歪的路径。
#[cfg(windows)]
#[test]
fn unifies_forward_slashes_on_windows() {
    assert_eq!(
        display_path(Path::new(
            r"E:\Games\Overcooked! 2\BepInEx/plugins/OC2DIYLevel\levels/Hogwaffles"
        )),
        r"E:\Games\Overcooked! 2\BepInEx\plugins\OC2DIYLevel\levels\Hogwaffles"
    );
    // 先剥 verbatim 前缀再统一，两者互不干扰。
    assert_eq!(
        display_path(Path::new(r"\\?\E:\Games\OC2/BepInEx")),
        r"E:\Games\OC2\BepInEx"
    );
    // 只有斜杠的写法也归一，且不会把 `C:\` 的尾分隔符吃掉。
    assert_eq!(display_path(Path::new("C:/Games/OC2")), r"C:\Games\OC2");
    assert_eq!(display_path(Path::new("C:/")), r"C:\");
}

/// macOS 上 `\` 是合法文件名字符，不能当分隔符改写。
#[cfg(not(windows))]
#[test]
fn leaves_backslashes_alone_off_windows() {
    let path = r"/Users/me/OC2/BepInEx\weird/name";
    assert_eq!(display_path(Path::new(path)), path);
}

#[test]
fn trims_redundant_trailing_separators_but_keeps_roots() {
    // POSIX 断言只在非 Windows 上成立（Windows 上 `/` 会先被归一成 `\`）。
    #[cfg(not(windows))]
    {
        assert_eq!(display_path(Path::new("/Users/me/game/")), "/Users/me/game");
        assert_eq!(
            display_path(Path::new("/Users/me/game///")),
            "/Users/me/game"
        );
        assert_eq!(display_path(Path::new("/")), "/");
        assert_eq!(display_path(Path::new("/Users/me/game")), "/Users/me/game");
    }
    #[cfg(windows)]
    {
        assert_eq!(display_path(Path::new(r"E:\Games\OC2\")), r"E:\Games\OC2");
        assert_eq!(display_path(Path::new(r"E:\Games\OC2\\\")), r"E:\Games\OC2");
        assert_eq!(display_path(Path::new(r"\")), r"\");
    }
}

#[test]
fn empty_paths_stay_empty() {
    assert_eq!(display_path(""), "");
    assert_eq!(display_path(Path::new("")), "");
}

#[cfg(unix)]
#[test]
fn non_utf8_paths_degrade_instead_of_panicking() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let path = Path::new(OsStr::from_bytes(b"/tmp/\xffOC2"));
    let display = display_path(path);
    assert!(display.starts_with("/tmp/"));
    assert!(display.contains('\u{FFFD}'));
}

#[cfg(windows)]
#[test]
fn non_utf8_paths_degrade_instead_of_panicking() {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    // 未配对的代理项不是合法 UTF-16 文本，`to_string_lossy` 换成 U+FFFD。
    let path = std::path::PathBuf::from(OsString::from_wide(&[0xD800, b'x' as u16]));
    let display = display_path(&path);
    assert!(!display.is_empty());
    assert!(display.contains('\u{FFFD}'));
}
