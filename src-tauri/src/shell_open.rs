//! 系统 Shell 交互：打开目录 / 文件 / URL，在文件管理器中定位一个项。
//!
//! 全部走 Windows Shell API，**不硬编码 `explorer.exe`**：
//!
//!   * `ShellExecuteW` 是 shell 关联的正规入口 —— 目录走
//!     `Directory\shell\open`，URL 与 `steam://` 这类自定义协议走协议关联。
//!     用户把这些默认处理器改到别处时能被遵循；`explorer.exe <target>` 则是
//!     绕过关联直接把参数丢给资源管理器，既无视用户选择，URL 还要靠 explorer
//!     当中转。
//!   * `SHOpenFolderAndSelectItems` 是 `explorer.exe /select,` 的 shell 层等价
//!     实现，不经过 explorer 的命令行解析，因此没有 verbatim 前缀、混合分隔符
//!     与 `/select,"..."` 引号规则的坑（这三类都真实踩过）。
//!
//! macOS 仍用系统自带的 `open`（定位用 `-R`）：那边没有对应的多实现问题。
//!
//! **边界**：Windows 没有"默认文件管理器"这个概念，第三方文件管理器无法成为
//! 系统默认。换用 shell API 能保证「用户改过注册表关联时被遵循」与「不再
//! 依赖 explorer 的命令行行为」，但不能保证在任何第三方管理器里定位成功 ——
//! 那取决于对方是否接管了 shell 命名空间。

use std::path::Path;

use crate::display_path::display_path;

/// 交给 Windows Shell 的路径串：普通盘符 / UNC 路径。
///
/// `display_path` 已经剥掉 `\\?\` verbatim 前缀并把分隔符统一成平台原生，
/// 这两件事正是 Shell API 需要的；这层薄封装只表达「给 shell 用」的语义边界，
/// 避免以后有人给它加面向用户的格式化（缩略、省略号之类）。
pub(crate) fn shell_path(path: &Path) -> String {
    display_path(path)
}

/// 用系统默认处理器打开一个目录或文件。
pub(crate) fn open_path(path: &Path) -> Result<(), String> {
    open_target(&shell_path(path))
}

/// 用系统默认协议处理器打开一个 URL（含 `steam://` 这类自定义协议）。
pub(crate) fn open_url(url: &str) -> Result<(), String> {
    open_target(url)
}

fn open_target(target: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use std::ptr::null_mut;
        use winapi::um::shellapi::ShellExecuteW;
        use winapi::um::winuser::SW_SHOWNORMAL;

        let wide = |value: &str| -> Vec<u16> {
            std::ffi::OsStr::new(value)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect()
        };
        let verb = wide("open");
        let file = wide(target);
        let result = unsafe {
            ShellExecuteW(
                null_mut(),
                verb.as_ptr(),
                file.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        // 返回值不是 HRESULT：> 32 才是成功，32 及以下是 ShellExecute 的错误码
        // （0 内存不足、2/3 找不到、5 拒绝访问、31 没有关联程序）。
        let code = result as usize;
        if code <= 32 {
            return Err(crate::locale_tag::with_params(
                "error.shell.openFailed",
                &[("reason", &code.to_string())],
            ));
        }
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(target)
            .stdin(std::process::Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = target;
        Err("game.unsupportedPlatform".into())
    }
}

/// 在系统文件管理器中定位一个文件或目录（打开父目录并选中它）。
///
/// 传目录时与传文件语义一致：都是打开它的父目录并选中它本身 ——
/// 这正是 `explorer.exe /select,` 原来的行为，调用方不用区分。
pub(crate) fn reveal_in_file_manager(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        reveal_on_windows(path)
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .stdin(std::process::Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = path;
        Err("game.unsupportedPlatform".into())
    }
}

#[cfg(windows)]
fn reveal_on_windows(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;

    use winapi::shared::minwindef::{DWORD, UINT};
    use winapi::shared::winerror::{HRESULT, S_FALSE, S_OK};
    use winapi::um::combaseapi::CoUninitialize;
    use winapi::um::objbase::CoInitialize;
    use winapi::um::shlobj::SHOpenFolderAndSelectItems;
    use winapi::um::shtypes::{ITEMIDLIST, PCIDLIST_ABSOLUTE, PCUITEMID_CHILD_ARRAY};

    /*
     * winapi 0.3 的 `shlobj` 只导出 `SHOpenFolderAndSelectItems`，没有
     * `ILCreateFromPathW` / `ILFree`。两者是 shell32.dll 的稳定导出（Vista 起），
     * 按真实签名补声明即可，不必为此引入 windows-rs。
     */
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn ILCreateFromPathW(psz_path: *const u16) -> *mut ITEMIDLIST;
        fn ILFree(pidl: *mut ITEMIDLIST);
    }

    let wide = |value: &std::ffi::OsStr| -> Vec<u16> {
        value.encode_wide().chain(std::iter::once(0)).collect()
    };
    let failure = || {
        crate::locale_tag::with_params("error.shell.revealFailed", &[("path", &shell_path(path))])
    };

    /*
     * `SHOpenFolderAndSelectItems` 要求当前线程初始化过 COM。已经初始化时
     * `CoInitialize` 返回 S_FALSE；线程被别的组件按 MTA 初始化过时返回
     * RPC_E_CHANGED_MODE —— 后者照样能调 shell API，但不需要也不该配对
     * `CoUninitialize`（引用计数是别人的）。
     */
    let initialized = unsafe { CoInitialize(std::ptr::null_mut()) };
    let release = initialized == S_OK || initialized == S_FALSE;
    let finish = |result: Result<(), String>| {
        if release {
            unsafe { CoUninitialize() };
        }
        result
    };

    /*
     * 调用方传进来的是 canonicalize 后的路径，Windows 上是 `\\?\` verbatim 形态，
     * 而 shell 的项标识列表不认这个前缀。与 `open_path` 一样先过 `shell_path`：
     * 剥前缀、统一分隔符，否则定位会直接失败（不是「选中了别的目录」那么轻）。
     */
    let target_path = shell_path(path);
    let target = wide(std::ffi::OsStr::new(&target_path));
    let pidl = unsafe { ILCreateFromPathW(target.as_ptr()) };
    if pidl.is_null() {
        /*
         * 拿不到 PIDL 说明 shell 不认这个项（未挂载的卷、已失效的网络位置、
         * 路径形态不对）。父目录通常还打得开，先送用户到那里。
         */
        log::warn!("定位文件失败：ILCreateFromPathW 返回空，路径={target_path}");
        if let Some(parent) = path.parent() {
            return finish(open_path(parent));
        }
        return finish(Err(failure()));
    }

    // cidl = 0 且 apidl = NULL：pidl 指向的是完整项，shell 打开它的父目录并选中它。
    let result: HRESULT = unsafe {
        SHOpenFolderAndSelectItems(
            pidl as PCIDLIST_ABSOLUTE,
            0 as UINT,
            std::ptr::null() as PCUITEMID_CHILD_ARRAY,
            0 as DWORD,
        )
    };
    unsafe { ILFree(pidl) };

    if result == 0 {
        return finish(Ok(()));
    }
    /*
     * 选中做不到时退回打开父目录，至少把用户送到正确位置。已知会回
     * `HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND)` 的情形：网络路径、未挂载的卷；
     * 其余失败码同样只影响「选中」这一件事，没必要让用户连目录都打不开。
     */
    log::warn!("定位文件失败：SHOpenFolderAndSelectItems 返回 0x{result:08X}，路径={target_path}");
    if let Some(parent) = path.parent() {
        return finish(open_path(parent));
    }
    finish(Err(failure()))
}

#[cfg(test)]
mod tests {
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
}
