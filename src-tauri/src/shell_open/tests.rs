use super::*;

/*
 * 这条只钉住「出口会剥掉内部路径的展示形态」：verbatim 前缀不进 Shell API、
 * 分隔符统一成反斜杠。真正的 Shell 调用是否打开成功只能在真机上点按钮验证，
 * 这里不假装验证它。
 */
#[test]
fn shell_path_strips_verbatim_prefixes_and_normalizes_separators() {
    let cases = [
        (
            r"\\?\C:\Games\厨房 MOD,包\Example DLL.dll",
            r"C:\Games\厨房 MOD,包\Example DLL.dll",
        ),
        (
            r"C:\Games\厨房 MOD,包\Example.dll.disabled",
            r"C:\Games\厨房 MOD,包\Example.dll.disabled",
        ),
        (
            r"\\?\UNC\server\共享 MOD\Example.dll",
            r"\\server\共享 MOD\Example.dll",
        ),
    ];
    for (source, expected) in cases {
        let actual = shell_path(Path::new(source));
        if cfg!(windows) {
            assert_eq!(actual, expected, "{source}");
        } else {
            // macOS 上 `\\?\` 是合法文件名字符，只剥前缀不改分隔符。
            assert!(!actual.starts_with(r"\\?\"), "{source}");
        }
    }
}

// `Path::join` 只补分隔符、不改写片段里的 `/`，混合串会让 shell 退回「此电脑」。
#[cfg(windows)]
#[test]
fn shell_path_normalizes_forward_slashes_from_joined_fragments() {
    let joined = Path::new(r"C:\Games\Overcooked! 2").join("BepInEx/logs");
    assert_eq!(shell_path(&joined), r"C:\Games\Overcooked! 2\BepInEx\logs");
}
