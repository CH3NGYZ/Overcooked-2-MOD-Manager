use super::paths::normalize_path;
#[cfg(unix)]
use crate::bepinex::Package;
use crate::models::{GameConfig, GameInfo};
use regex::Regex;
use std::fs;
#[cfg(unix)]
use std::path::Path;
use std::path::PathBuf;

/// Read only the small registry/environment hints. The state observer uses
/// these to notice a newly installed Steam client without reparsing its VDF.
pub(crate) fn steam_root_paths(settings: &GameConfig) -> Vec<PathBuf> {
    #[cfg(unix)]
    if !Package::current(settings).native() {
        return crate::wine::steam_roots();
    }
    #[cfg(windows)]
    let _ = settings;
    let mut roots = Vec::new();

    #[cfg(windows)]
    {
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;
        if let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey("Software\\Valve\\Steam") {
            for value in ["SteamPath", "InstallPath"] {
                if let Ok(path) = key.get_value::<String, _>(value) {
                    roots.push(PathBuf::from(path));
                }
            }
        }
        if let Some(program_files) = std::env::var_os("PROGRAMFILES(X86)") {
            roots.push(PathBuf::from(program_files).join("Steam"));
        }
        if let Some(program_files) = std::env::var_os("PROGRAMFILES") {
            roots.push(PathBuf::from(program_files).join("Steam"));
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(home_path) = dirs::home_dir() {
            roots.push(home_path.join("Library/Application Support/Steam"));
        }
    }

    roots
}

pub(crate) fn steam_libraries_from_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut libraries = Vec::new();
    for root in roots {
        let root = normalize_path(root);
        if !libraries.contains(&root) {
            libraries.push(root.clone());
        }
        let vdf = root.join("steamapps/libraryfolders.vdf");
        if let Ok(text) = fs::read_to_string(vdf) {
            if let Ok(regex) = Regex::new(r#""path"\s+"([^"]+)""#) {
                for capture in regex.captures_iter(&text) {
                    if let Some(value) = capture.get(1) {
                        #[cfg(unix)]
                        let path = if crate::wine::prefix_for_path(&root).is_some() {
                            crate::wine::host_library_path(&root, value.as_str())
                        } else {
                            let path = PathBuf::from(value.as_str());
                            path.is_absolute().then_some(path)
                        };
                        #[cfg(not(unix))]
                        let path = Some(PathBuf::from(value.as_str().replace("\\\\", "\\")));
                        let Some(path) = path.map(normalize_path) else {
                            continue;
                        };
                        if !libraries.contains(&path) {
                            libraries.push(path);
                        }
                    }
                }
            }
        }
    }
    log::trace!("Steam 库探测完成：候选数量={}", libraries.len());
    libraries
}

pub(crate) fn steam_launch_command(settings: &GameConfig, info: &GameInfo) -> Result<(), String> {
    #[cfg(unix)]
    if !Package::current(settings).native() {
        let prefix = info.wine_prefix.as_deref().ok_or("wine.prefixUnknown")?;
        return crate::wine::steam_command(Path::new(prefix))?
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string());
    }
    /*
     * Windows 与 macOS 原生走平台自己的启动方式。`settings` / `info`
     * 在这条路径上都不用，显式标记避免未使用警告 —— 它们的取值只在
     * macOS 的 Wine 分支里需要。
     */
    #[cfg(windows)]
    let _ = (settings, info);
    native_steam_launch(std::env::consts::OS)
}

/// 启动厨房时请求的 Steam URL。
const STEAM_RUN_URL: &str = "steam://rungameid/728880";

/// 原生模式的启动方式。
///
/// 抽成纯函数是为了让「Windows 走协议关联、macOS 指名 Steam bundle」这条规则
/// 能被测试钉住 —— 真正执行的两个入口一个走 ShellExecute、一个起子进程，
/// 单测里都断言不了。
#[derive(Debug, PartialEq, Eq)]
enum SteamStrategy {
    /// 交给系统默认协议处理器（Windows：`ShellExecuteW`）。
    ///
    /// 不要退回 `explorer.exe steam://...`：那是借资源管理器当中转，绕过了
    /// 协议关联，用户把 Steam 关联改到别处时就会失效。
    ProtocolHandler,
    /// 用 `open -b <bundle>` 指名应用：CrossOver 会导出同名 Steam app，
    /// 只按协议打开有几率起错那个。
    OpenBundle(&'static str),
}

fn steam_launch_strategy(platform: &str) -> Result<SteamStrategy, String> {
    match platform {
        "windows" => Ok(SteamStrategy::ProtocolHandler),
        "macos" => Ok(SteamStrategy::OpenBundle("com.valvesoftware.steam")),
        _ => Err("game.unsupportedPlatform".into()),
    }
}

/// 原生模式下启动厨房：只有 Windows 与 macOS 两条路径。
fn native_steam_launch(platform: &str) -> Result<(), String> {
    match steam_launch_strategy(platform)? {
        SteamStrategy::ProtocolHandler => crate::shell_open::open_url(STEAM_RUN_URL),
        SteamStrategy::OpenBundle(bundle) => std::process::Command::new("open")
            .args(["-b", bundle, STEAM_RUN_URL])
            .stdin(std::process::Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string()),
    }
}

#[cfg(test)]
mod tests;
