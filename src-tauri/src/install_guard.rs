use std::path::{Component, Path};

const CHECK_FLAG: &str = "--validate-manager-install-dir";
const ERROR: &str = "config.installGuardBepInEx";

/// 校验管理器安装目录。
///
/// 禁止范围是**整个 `BepInEx` 树**：管理器备份与恢复框架时按整棵树快照，
/// 管理器自身程序若在树内，恢复会把正在运行的可执行文件覆盖掉；游戏平台
/// 校验、玩家手动清 BepInEx 也会顺手带走管理器。
///
/// 这条规则与数据根无关 —— 数据已经放在用户文档目录（macOS 是
/// `~/Library/Application Support`），不再跟着安装目录走。
pub(crate) fn validate_install_dir(directory: &Path) -> Result<(), String> {
    if directory.as_os_str().is_empty() || !directory.is_absolute() {
        return Err(ERROR.into());
    }
    if is_inside_bepinex(directory)? {
        return Err(ERROR.into());
    }
    Ok(())
}

/// 目录是否位于任意 `BepInEx` 目录内（含其本身）。
pub(crate) fn is_inside_bepinex(directory: &Path) -> Result<bool, String> {
    let absolute = std::path::absolute(directory).map_err(|_| ERROR.to_string())?;
    let resolved =
        crate::game::resolve_existing_ancestor(&absolute).map_err(|_| ERROR.to_string())?;
    Ok(inside_bepinex(&absolute) || inside_bepinex(&resolved))
}

/// 兼容旧调用点：判断是否位于 `BepInEx/plugins` 内。
pub(crate) fn is_plugins_directory(directory: &Path) -> Result<bool, String> {
    let absolute = std::path::absolute(directory).map_err(|_| ERROR.to_string())?;
    let resolved =
        crate::game::resolve_existing_ancestor(&absolute).map_err(|_| ERROR.to_string())?;
    Ok(inside_plugins(&absolute) || inside_plugins(&resolved))
}

/// 路径里出现过 `BepInEx` 目录（包括它自己）。
fn inside_bepinex(path: &Path) -> bool {
    path.components().any(|component| match component {
        Component::Normal(name) => name.to_string_lossy().eq_ignore_ascii_case("BepInEx"),
        _ => false,
    })
}

fn inside_plugins(path: &Path) -> bool {
    let mut previous_is_bepinex = false;
    for component in path.components() {
        if let Component::Normal(name) = component {
            let name = name.to_string_lossy();
            if previous_is_bepinex && name.eq_ignore_ascii_case("plugins") {
                return true;
            }
            previous_is_bepinex = name.eq_ignore_ascii_case("BepInEx");
        }
    }
    false
}

// Installers run the bundled executable from a temporary location solely for
// validation, before elevation, logging, configuration, or Tauri initialization.
pub(crate) fn run_if_requested() {
    let mut arguments = std::env::args_os().skip(1);
    if arguments.next().as_deref() != Some(std::ffi::OsStr::new(CHECK_FLAG)) {
        return;
    }
    let directory = arguments.next().unwrap_or_default();
    if validate_install_dir(Path::new(&directory)).is_ok() {
        std::process::exit(0);
    }
    let options = arguments.collect::<Vec<_>>();
    if options.iter().any(|argument| argument == "--show-error") {
        let language = options
            .windows(2)
            .find(|pair| pair[0] == "--language")
            .map(|pair| pair[1].to_string_lossy());
        show_error(language.as_deref().unwrap_or("1033"));
    }
    std::process::exit(1);
}

fn show_error(language: &str) {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let message = match language {
            "2052" => ERROR,
            "1028" => "config.installGuardBepInExTraditional",
            "1042" => {
                "관리자를 BepInEx 폴더 또는 그 하위 폴더에 설치할 수 없습니다. 게임 폴더 밖의 위치를 선택하세요."
            }
            _ => {
                "The manager cannot be installed in the BepInEx folder or any of its subfolders. Choose a location outside the game directory."
            }
        };
        let message = std::ffi::OsStr::new(message)
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let title = std::ffi::OsStr::new("OC2 MOD Manager")
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        unsafe {
            winapi::um::winuser::MessageBoxW(
                std::ptr::null_mut(),
                message.as_ptr(),
                title.as_ptr(),
                winapi::um::winuser::MB_OK | winapi::um::winuser::MB_ICONERROR,
            );
        }
    }
    #[cfg(not(windows))]
    let _ = language;
}

#[cfg(test)]
#[path = "install_guard/tests.rs"]
mod tests;
