mod detection;
pub(crate) mod install;
pub(crate) use detection::detect_bepinex;

#[cfg(test)]
mod loader_tests;

use crate::models::{GameConfig, GameRuntime, Provider, WinhttpState};
use regex::Regex;
use std::fs;
use std::path::Path;

/// 内置 BepInEx 包版本（安装清单里记录用，与各包 URL 中的版本号一致）。
pub(crate) const PACKAGE_VERSION: &str = "5.4.23.5";

/// BepInEx 包种类：Windows 版与 macOS 原生版。
///
/// 只保留两个：本项目不再发布 Linux 版本。原来还有一个 `Linux` 变体，
/// 对应 `Overcooked2.x86_64` 与 `BepInEx_linux_x64_*.zip`；移除发布目标后
/// 那些字段再也没有调用方，留着只会让每条 match 多一个死分支。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Package {
    Windows,
    MacOS,
}

impl Package {
    pub(crate) fn current(settings: &GameConfig) -> Self {
        Self::for_platform(std::env::consts::OS, settings.runtime)
    }

    fn for_platform(platform: &str, runtime: GameRuntime) -> Self {
        match (platform, runtime) {
            ("macos", GameRuntime::Native) => Self::MacOS,
            _ => Self::Windows,
        }
    }

    pub(crate) fn native(self) -> bool {
        self != Self::Windows
    }

    pub(crate) fn executable(self) -> &'static str {
        match self {
            Self::Windows => "Overcooked2.exe",
            Self::MacOS => "Overcooked2.app",
        }
    }

    pub(crate) fn url(self, provider: &Provider) -> &'static str {
        match (self, provider) {
            (Self::Windows, Provider::Gitee) => {
                "https://gitee.com/ch3ngyz/Overcooked-2-Installer-ResourceFiles/releases/download/bepinex/BepInEx_win_x86_5.4.23.5.zip"
            }
            (Self::Windows, Provider::Github | Provider::Gitcode) => {
                "https://github.com/BepInEx/BepInEx/releases/download/v5.4.23.5/BepInEx_win_x86_5.4.23.5.zip"
            }
            (Self::MacOS, Provider::Github | Provider::Gitcode) => {
                "https://github.com/BepInEx/BepInEx/releases/download/v5.4.23.5/BepInEx_macos_universal_5.4.23.5.zip"
            }
            (Self::MacOS, Provider::Gitee) => {
                "https://gitee.com/ch3ngyz/Overcooked-2-Installer-ResourceFiles/releases/download/bepinex/BepInEx_macos_universal_5.4.23.5.zip"
            }
        }
    }

    pub(crate) fn digest(self) -> &'static str {
        match self {
            Self::Windows => {
                "sha256:37651c79e40d6f909572a4f461ac25350bb3ef8fe7fbd29f1aa8791a33b84c82"
            }
            Self::MacOS => {
                "sha256:01c2ae782eb016dfd6c345a18dbd2dcafffb3d9d318449d6486689f426b4a323"
            }
        }
    }
}

/// 框架是否**真正**安装：`BepInEx/core` 下存在核心程序集。
///
/// 不能用「`BepInEx/` 目录存在」判断 —— 安装任何一个 MOD 都会创建
/// `BepInEx/plugins/`。只判断目录会把这个空壳当成已安装，界面于是显示
/// 启用开关和卸载按钮，而开关又会因为找不到加载器直接报错。
pub(crate) fn installed(game: &Path) -> bool {
    ["BepInEx.dll", "BepInEx.Core.dll"]
        .iter()
        .any(|name| game.join("BepInEx/core").join(name).is_file())
}

pub(crate) fn state(game: &Path, settings: &GameConfig) -> WinhttpState {
    let package = Package::current(settings);
    if package.native() {
        return native_state(game, package);
    }
    match (
        game.join("winhttp.dll").is_file(),
        game.join("winhttp.dll.disabled").is_file(),
    ) {
        (true, false) => WinhttpState::ModEnabled,
        (false, true) => WinhttpState::ModDisabled,
        (true, true) => WinhttpState::Conflict,
        _ => WinhttpState::Missing,
    }
}

pub(crate) fn set_enabled(game: &Path, settings: &GameConfig, enabled: bool) -> Result<(), String> {
    if Package::current(settings).native() {
        let script_path = crate::game::safe_game_path(game, "run_bepinex.sh")?;
        let script =
            fs::read_to_string(&script_path).map_err(|_| "framework.nativeScriptMissing")?;
        let changed = set_script_enabled(&script, enabled)?;
        return fs::write(script_path, changed).map_err(|e| e.to_string());
    }
    let active = crate::game::safe_game_path(game, "winhttp.dll")?;
    let inactive = crate::game::safe_game_path(game, "winhttp.dll.disabled")?;
    match (enabled, state(game, settings)) {
        (true, WinhttpState::ModDisabled) => {
            crate::archive::clear_readonly(&inactive)?;
            fs::rename(inactive, active).map_err(|e| e.to_string())
        }
        (true, WinhttpState::Conflict) => {
            crate::archive::clear_readonly(&inactive)?;
            fs::remove_file(inactive).map_err(|e| e.to_string())
        }
        (false, WinhttpState::ModEnabled) => {
            crate::archive::clear_readonly(&active)?;
            fs::rename(active, inactive).map_err(|e| e.to_string())
        }
        (false, WinhttpState::Conflict) => Err("framework.loaderConflict".into()),
        (_, WinhttpState::Missing) => Err("framework.loaderMissing".into()),
        _ => Ok(()),
    }
}

pub(crate) fn prepare_native(game: &Path, package: Package) -> Result<(), String> {
    let script_path = crate::game::safe_game_path(game, "run_bepinex.sh")?;
    let script =
        fs::read_to_string(&script_path).map_err(|_| "framework.nativePackageMissingScript")?;
    let script = replace_assignment(&script, "executable_name", package.executable())?;
    let script = if package == Package::MacOS {
        repair_macos_overlay_script(&script)?
    } else {
        script
    };
    fs::write(&script_path, set_script_enabled(&script, true)?).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&script_path)
            .map_err(|e| e.to_string())?
            .permissions();
        permissions.set_mode(permissions.mode() | 0o100);
        fs::set_permissions(&script_path, permissions).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(crate) fn repair_macos_overlay(game: &Path) -> Result<(), String> {
    let script_path = crate::game::safe_game_path(game, "run_bepinex.sh")?;
    let script = fs::read_to_string(&script_path)
        .map_err(|_| "framework.nativePackageMissingScript".to_string())?;
    let script = repair_macos_overlay_script(&script)?;
    fs::write(&script_path, script).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&script_path)
            .map_err(|e| e.to_string())?
            .permissions();
        permissions.set_mode(permissions.mode() | 0o100);
        fs::set_permissions(&script_path, permissions).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn repair_macos_overlay_script(script: &str) -> Result<String, String> {
    let script = script.replace("\r\n", "\n");
    const MARKER: &str =
        "export DYLD_LIBRARY_PATH=\"${doorstop_directory}:${DYLD_LIBRARY_PATH}\"\n";
    const PATCH: &str = "# Steam passes its loader list separately because /bin/sh may have DYLD variables stripped.\nif [ -z \"$DYLD_INSERT_LIBRARIES\" ] && [ -n \"$STEAM_DYLD_INSERT_LIBRARIES\" ]; then\n    export DYLD_INSERT_LIBRARIES=\"$STEAM_DYLD_INSERT_LIBRARIES\"\nfi\n";
    const LOADER: &str = "if [ -z \"$DYLD_INSERT_LIBRARIES\" ]; then\n    export DYLD_INSERT_LIBRARIES=\"${doorstop_name}\"\nelse\n    export DYLD_INSERT_LIBRARIES=\"${doorstop_name}:${DYLD_INSERT_LIBRARIES}\"\nfi\n";
    let patched = format!("{MARKER}{PATCH}{LOADER}");
    if script.matches(MARKER).count() != 1 {
        return Err("framework.overlayScriptUnrecognized".into());
    }
    if script.contains(&patched) {
        return Ok(script);
    }
    let original = format!("{MARKER}{LOADER}");
    if !script.contains(&original) {
        return Err("framework.overlayScriptUnrecognized".into());
    }
    Ok(script.replacen(&original, &patched, 1))
}

fn native_state(game: &Path, package: Package) -> WinhttpState {
    let extension = if package == Package::MacOS {
        "dylib"
    } else {
        "so"
    };
    let has_loader = [
        format!("libdoorstop.{extension}"),
        format!("doorstop_libs/libdoorstop_x64.{extension}"),
    ]
    .iter()
    .any(|path| game.join(path).is_file());
    if !has_loader {
        return WinhttpState::Missing;
    }
    match fs::read_to_string(game.join("run_bepinex.sh"))
        .ok()
        .and_then(|script| script_enabled(&script))
    {
        Some(true) => WinhttpState::ModEnabled,
        Some(false) => WinhttpState::ModDisabled,
        None => WinhttpState::Missing,
    }
}

fn script_enabled(script: &str) -> Option<bool> {
    script.lines().find_map(|line| {
        let value = line
            .strip_prefix("enabled=")
            .or_else(|| line.strip_prefix("export DOORSTOP_ENABLE="))?;
        match value.trim().trim_end_matches(';').trim_matches(['"', '\'']) {
            "1" | "TRUE" | "true" => Some(true),
            "0" | "FALSE" | "false" => Some(false),
            _ => None,
        }
    })
}

fn set_script_enabled(script: &str, enabled: bool) -> Result<String, String> {
    if script.lines().any(|line| line.starts_with("enabled=")) {
        replace_assignment(script, "enabled", if enabled { "1" } else { "0" })
    } else {
        replace_assignment(
            script,
            "export DOORSTOP_ENABLE",
            if enabled { "TRUE" } else { "FALSE" },
        )
    }
}

fn replace_assignment(script: &str, name: &str, value: &str) -> Result<String, String> {
    let script = script.replace("\r\n", "\n");
    let pattern =
        Regex::new(&format!(r"(?m)^{}=[^\n]*$", regex::escape(name))).map_err(|e| e.to_string())?;
    if pattern.find_iter(&script).count() != 1 {
        return Err("framework.nativeScriptUnrecognized".into());
    }
    Ok(pattern
        .replace(&script, |_: &regex::Captures<'_>| {
            format!("{name}=\"{value}\"")
        })
        .into_owned())
}

#[cfg(test)]
#[path = "bepinex/tests.rs"]
mod tests;
