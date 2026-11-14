use std::path::{Component, Path};

const CHECK_FLAG: &str = "--validate-manager-install-dir";
const ERROR: &str = "config.installGuardBepInEx";

#[derive(Debug)]
pub(crate) enum InstallDirectoryError {
    Forbidden,
    Unavailable(String),
}

impl InstallDirectoryError {
    fn exit_code(&self) -> i32 {
        match self {
            Self::Forbidden => 1,
            Self::Unavailable(_) => 2,
        }
    }
}

impl std::fmt::Display for InstallDirectoryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Forbidden => formatter.write_str("invalid directory or directory inside BepInEx"),
            Self::Unavailable(error) => {
                write!(formatter, "cannot resolve installation directory: {error}")
            }
        }
    }
}

impl From<InstallDirectoryError> for String {
    fn from(_: InstallDirectoryError) -> Self {
        ERROR.into()
    }
}

/// 校验管理器安装目录。
///
/// 禁止范围是**整个 `BepInEx` 树**：管理器备份与恢复框架时按整棵树快照，
/// 管理器自身程序若在树内，恢复会把正在运行的可执行文件覆盖掉；游戏平台
/// 校验、玩家手动清 BepInEx 也会顺手带走管理器。
///
/// 这条规则与数据根无关 —— 数据已经放在用户文档目录（macOS 是
/// `~/Library/Application Support`），不再跟着安装目录走。
pub(crate) fn validate_install_dir(directory: &Path) -> Result<(), InstallDirectoryError> {
    if directory.as_os_str().is_empty() || !directory.is_absolute() {
        return Err(InstallDirectoryError::Forbidden);
    }
    if is_inside_bepinex(directory)? {
        return Err(InstallDirectoryError::Forbidden);
    }
    Ok(())
}

/// 目录是否位于任意 `BepInEx` 目录内（含其本身）。
pub(crate) fn is_inside_bepinex(directory: &Path) -> Result<bool, InstallDirectoryError> {
    let absolute = std::path::absolute(directory)
        .map_err(|error| InstallDirectoryError::Unavailable(error.to_string()))?;
    let resolved = crate::game::resolve_existing_ancestor(&absolute)
        .map_err(InstallDirectoryError::Unavailable)?;
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
    let error = match validate_install_dir(Path::new(&directory)) {
        Ok(()) => std::process::exit(0),
        Err(error) => error,
    };
    eprintln!("Install directory validation failed: {error}");
    let options = arguments.collect::<Vec<_>>();
    if options.iter().any(|argument| argument == "--show-error") {
        let language = options
            .windows(2)
            .find(|pair| pair[0] == "--language")
            .map(|pair| pair[1].to_string_lossy());
        show_error(language.as_deref().unwrap_or("1033"), &error);
    }
    std::process::exit(error.exit_code());
}

#[cfg(any(windows, test))]
fn error_message(language: &str, error: &InstallDirectoryError) -> &'static str {
    match (language, error) {
        ("2052", InstallDirectoryError::Forbidden) => {
            "管理器不能安装到 BepInEx 文件夹或其子目录，请选择游戏目录外的位置。"
        }
        ("1028", InstallDirectoryError::Forbidden) => {
            "管理器不能安裝到 BepInEx 資料夾或其子目錄，請選擇遊戲目錄外的位置。"
        }
        ("1042", InstallDirectoryError::Forbidden) => {
            "관리자를 BepInEx 폴더 또는 그 하위 폴더에 설치할 수 없습니다. 게임 폴더 밖의 위치를 선택하세요."
        }
        ("1041", InstallDirectoryError::Forbidden) => {
            "マネージャーを BepInEx フォルダーまたはそのサブフォルダーにはインストールできません。ゲームフォルダー外の場所を選択してください。"
        }
        (_, InstallDirectoryError::Forbidden) => {
            "The manager cannot be installed in the BepInEx folder or any of its subfolders. Choose a location outside the game directory."
        }
        ("2052", _) => "无法完成安装目录校验，请查看安装详情中的错误原因后重试。",
        ("1028", _) => "無法完成安裝目錄驗證，請查看安裝詳細資料中的錯誤原因後重試。",
        ("1042", _) => {
            "설치 폴더 확인을 완료하지 못했습니다. 설치 세부 정보에서 오류를 확인한 후 다시 시도하세요."
        }
        ("1041", _) => {
            "インストール先の確認を完了できませんでした。インストールの詳細でエラーを確認してから再試行してください。"
        }
        _ => {
            "Installation directory validation could not be completed. Check the error in the installation details and try again."
        }
    }
}

fn show_error(language: &str, error: &InstallDirectoryError) {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let message = std::ffi::OsStr::new(error_message(language, error))
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
    let _ = (language, error);
}

#[cfg(test)]
#[path = "install_guard/tests.rs"]
mod tests;
