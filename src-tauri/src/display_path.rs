//! 展示用路径归一化：所有送给前端或写进日志的绝对路径都从这里出口。
//!
//! 游戏目录与关卡集目录来自 `fs::canonicalize`，Windows 上会带 `\\?\` /
//! `\\?\UNC\` 前缀。这个前缀对进程内部的文件操作合法，但直接显示给用户、
//! 粘贴进资源管理器或别人给的路径里都不认，所以展示前必须剥掉。规则只维护
//! 这一份：函数是纯字符串变换（除了 `to_string_lossy` 的降级），不读盘、
//! 不写盘、不做存在性判断 —— 写盘、比较、`canonicalize` 等 I/O 路径不要
//! 经过它，否则「统一展示形态」会悄悄改变文件操作语义。

use std::path::Path;

/// 把内部路径转成展示字符串（全仓库唯一出口）。
///
/// - Windows 去掉 verbatim 前缀 `\\?\`；UNC 的 `\\?\UNC\server\share`
///   还原成 `\\server\share`；
/// - 分隔符统一成平台原生：Windows 上的 `/` 一律换成 `\`（见
///   `normalize_separators` 里为什么 macOS 不反向转）；
/// - 去掉多余的结尾分隔符，但保留根（`/`、`C:\`）；
/// - 非 UTF-8 由 `to_string_lossy` 安全降级（替换成 U+FFFD），不 panic；
/// - 空路径返回空串，重复调用结果不变。
pub(crate) fn display_path(path: impl AsRef<Path>) -> String {
    let text = path.as_ref().to_string_lossy();
    let text = strip_verbatim_prefix(&text);
    let text = normalize_separators(&text);
    trim_trailing_separators(&text)
}

/// 把混进路径里的 `/` 统一成 `\`（仅 Windows）。
///
/// 同一个绝对路径的各个片段来源不同：`game_root` 来自 `canonicalize`，分隔符
/// 已经是原生的 `\`；而常量目录（`BepInEx/plugins/OC2DIYLevel`）按跨平台习惯
/// 写成 `/`，`PathBuf::join` 只负责拼接、不会改写片段里的分隔符，拼出来就是
/// `E:\...\Overcooked! 2\BepInEx/plugins/OC2DIYLevel\levels` 这种半截歪的样子。
///
/// macOS 不反向转：那边的 `/` 本来就是唯一分隔符，而 `\` 是**合法文件名字符**，
/// 把 `\` 换成 `/` 会改坏真实路径。
#[cfg(windows)]
fn normalize_separators(value: &str) -> String {
    if value.contains('/') {
        value.replace('/', r"\")
    } else {
        value.to_string()
    }
}

#[cfg(not(windows))]
fn normalize_separators(value: &str) -> String {
    value.to_string()
}

/// `\\?\` 规则；其余字符串原样返回。
fn strip_verbatim_prefix(value: &str) -> String {
    let Some(rest) = value.strip_prefix(r"\\?\") else {
        return value.to_string();
    };
    // `\\?\UNC\server\share` 与 `\\?\C:\...` 是两种不同形态：前者要还原成
    // 普通 UNC 写法，后者只剥前缀。UNC 标记按大小写不敏感匹配，但切片只在
    // 4 个 ASCII 字符都匹配时才发生，不会切在非 ASCII 边界上。
    if rest
        .get(..4)
        .is_some_and(|head| head.eq_ignore_ascii_case("UNC\\"))
    {
        return format!(r"\\{}", &rest[4..]);
    }
    rest.to_string()
}

/// 去掉结尾多余的 `/` 与 `\`；纯根路径保留一个原生分隔符。
fn trim_trailing_separators(value: &str) -> String {
    let trimmed = value.trim_end_matches(['/', '\\']);
    if trimmed.len() == value.len() {
        return value.to_string();
    }
    if trimmed.is_empty() {
        // 只有分隔符（POSIX 的 `/`、Windows 的 `\`）：保留一个，别变成空串。
        return value.chars().next().map(String::from).unwrap_or_default();
    }
    if trimmed.ends_with(':') {
        // `C:\` 去掉尾分隔符会变成「C:」（驱动器相对路径），必须补回来。
        let separator = value[trimmed.len()..].chars().next().unwrap_or('\\');
        return format!("{trimmed}{separator}");
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
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
            let mac =
                "/Users/someone/Library/Application Support/Steam/steamapps/common/Overcooked! 2";
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
}
