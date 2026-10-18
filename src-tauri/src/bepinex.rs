use crate::models::{GameConfig, GameRuntime, Provider, WinhttpState};
use regex::Regex;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Package {
    Windows,
    Linux,
    MacOS,
}

impl Package {
    pub(crate) fn current(settings: &GameConfig) -> Self {
        Self::for_platform(std::env::consts::OS, settings.runtime)
    }

    fn for_platform(platform: &str, runtime: GameRuntime) -> Self {
        match (platform, runtime) {
            ("linux", GameRuntime::Native) => Self::Linux,
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
            Self::Linux => "Overcooked2.x86_64",
            Self::MacOS => "Overcooked2.app",
        }
    }

    pub(crate) fn url(self, provider: &Provider) -> &'static str {
        match (self, provider) {
            (Self::Windows, Provider::Gitee) => {
                "https://gitee.com/ch3ngyz/Overcooked-2-Installer-ResourceFiles/releases/download/bepinex/BepInEx_win_x86_5.4.23.5.zip"
            }
            (Self::Windows, Provider::Github) => {
                "https://github.com/BepInEx/BepInEx/releases/download/v5.4.23.5/BepInEx_win_x86_5.4.23.5.zip"
            }
            (Self::Linux, Provider::Gitee) => {
                "https://gitee.com/ch3ngyz/Overcooked-2-Installer-ResourceFiles/releases/download/bepinex/BepInEx_linux_x64_5.4.23.5.zip"
            }
            (Self::Linux, Provider::Github) => {
                "https://github.com/BepInEx/BepInEx/releases/download/v5.4.23.5/BepInEx_linux_x64_5.4.23.5.zip"
            }
            (Self::MacOS, Provider::Github) => {
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
            Self::Linux => {
                "sha256:e538560be65739f562519ab518a75f9c65b3f57f87457403ae7cde683c12dab7"
            }
            Self::MacOS => {
                "sha256:01c2ae782eb016dfd6c345a18dbd2dcafffb3d9d318449d6486689f426b4a323"
            }
        }
    }
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
        let script = fs::read_to_string(&script_path)
            .map_err(|_| "未找到原生 BepInEx 启动脚本，请先安装框架")?;
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
        (false, WinhttpState::Conflict) => {
            Err("winhttp.dll 与禁用文件同时存在，请先切换为框架已启用状态".into())
        }
        (_, WinhttpState::Missing) => Err("未找到框架加载器，请先安装框架".into()),
        _ => Ok(()),
    }
}

pub(crate) fn prepare_native(game: &Path, package: Package) -> Result<(), String> {
    let script_path = crate::game::safe_game_path(game, "run_bepinex.sh")?;
    let script =
        fs::read_to_string(&script_path).map_err(|_| "原生 BepInEx 包缺少 run_bepinex.sh")?;
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
        .map_err(|_| "原生 BepInEx 包缺少 run_bepinex.sh".to_string())?;
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
        return Err("无法识别 macOS BepInEx 启动脚本，无法修复 Steam Overlay".into());
    }
    if script.contains(&patched) {
        return Ok(script);
    }
    let original = format!("{MARKER}{LOADER}");
    if !script.contains(&original) {
        return Err("无法识别 macOS BepInEx 启动脚本，无法修复 Steam Overlay".into());
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
        return Err("无法识别原生 BepInEx 启动脚本，请重新安装框架".into());
    }
    Ok(pattern
        .replace(&script, |_: &regex::Captures<'_>| {
            format!("{name}=\"{value}\"")
        })
        .into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_selection_keeps_native_and_wine_separate_on_each_platform() {
        for (os, runtime, expected, executable) in [
            (
                "windows",
                GameRuntime::Native,
                Package::Windows,
                "Overcooked2.exe",
            ),
            (
                "windows",
                GameRuntime::Wine,
                Package::Windows,
                "Overcooked2.exe",
            ),
            (
                "linux",
                GameRuntime::Wine,
                Package::Windows,
                "Overcooked2.exe",
            ),
            (
                "linux",
                GameRuntime::Native,
                Package::Linux,
                "Overcooked2.x86_64",
            ),
            (
                "macos",
                GameRuntime::Wine,
                Package::Windows,
                "Overcooked2.exe",
            ),
            (
                "macos",
                GameRuntime::Native,
                Package::MacOS,
                "Overcooked2.app",
            ),
        ] {
            let package = Package::for_platform(os, runtime);
            assert_eq!(package, expected);
            assert_eq!(package.executable(), executable);
            assert!(package.url(&Provider::Github).contains(match package {
                Package::Windows => "BepInEx_win_x86_",
                Package::Linux => "BepInEx_linux_x64_",
                Package::MacOS => "BepInEx_macos_universal_",
            }));
        }
        for package in [Package::Windows, Package::Linux, Package::MacOS] {
            assert_eq!(
                package.url(&Provider::Github).rsplit('/').next(),
                package.url(&Provider::Gitee).rsplit('/').next(),
            );
            assert_eq!(package.digest().len(), 71);
        }
    }

    #[test]
    fn native_scripts_configure_the_executable_and_toggle_without_renaming_loaders() {
        let script = "#!/bin/sh\nexecutable_name=\"\"\nenabled=\"1\"\n  enabled=\"$2\"\nexec \"$executable_name\" \"$@\"\n";
        for package in [Package::Linux, Package::MacOS] {
            let configured =
                replace_assignment(script, "executable_name", package.executable()).unwrap();
            assert!(configured.contains(&format!("executable_name=\"{}\"", package.executable())));
            let disabled = set_script_enabled(&configured, false).unwrap();
            assert_eq!(script_enabled(&disabled), Some(false));
            assert!(disabled.contains("  enabled=\"$2\""));
            assert_eq!(
                script_enabled(&set_script_enabled(&disabled, true).unwrap()),
                Some(true)
            );
        }
        assert!(
            replace_assignment(
                "executable_name=\"a\"\nexecutable_name=\"b\"",
                "executable_name",
                "c"
            )
            .is_err()
        );
        assert!(set_script_enabled("unrecognized script", true).is_err());
    }

    #[test]
    fn macos_overlay_repair_restores_steam_libraries_after_doorstop_setup() {
        let script = "#!/bin/sh\nexecutable_name=\"\"\nenabled=\"1\"\nexport DYLD_LIBRARY_PATH=\"${doorstop_directory}:${DYLD_LIBRARY_PATH}\"\nif [ -z \"$DYLD_INSERT_LIBRARIES\" ]; then\n    export DYLD_INSERT_LIBRARIES=\"${doorstop_name}\"\nelse\n    export DYLD_INSERT_LIBRARIES=\"${doorstop_name}:${DYLD_INSERT_LIBRARIES}\"\nfi\n";
        let repaired = repair_macos_overlay_script(script).unwrap();
        let steam = repaired.find("$STEAM_DYLD_INSERT_LIBRARIES").unwrap();
        let doorstop = repaired
            .find("${doorstop_name}:${DYLD_INSERT_LIBRARIES}")
            .unwrap();
        assert!(repaired.contains(
            "[ -z \"$DYLD_INSERT_LIBRARIES\" ] && [ -n \"$STEAM_DYLD_INSERT_LIBRARIES\" ]"
        ));
        assert!(steam < doorstop);
        assert_eq!(repair_macos_overlay_script(&repaired).unwrap(), repaired);
        assert!(repair_macos_overlay_script("unrecognized").is_err());
        assert!(repair_macos_overlay_script("# STEAM_DYLD_INSERT_LIBRARIES\n").is_err());
        let comment = format!("# STEAM_DYLD_INSERT_LIBRARIES is not a repair\n{script}");
        assert!(
            repair_macos_overlay_script(&comment)
                .unwrap()
                .contains("export DYLD_INSERT_LIBRARIES=\"$STEAM_DYLD_INSERT_LIBRARIES\"")
        );
        assert_eq!(
            repair_macos_overlay_script(&script.replace('\n', "\r\n")).unwrap(),
            repaired
        );
        assert!(repair_macos_overlay_script(&format!("{script}{script}")).is_err());
        assert!(
            repair_macos_overlay_script(&script.replace(
                "${doorstop_name}:${DYLD_INSERT_LIBRARIES}",
                "${doorstop_name}"
            ))
            .is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn macos_overlay_loader_order_preserves_existing_dyld_or_restores_steam() {
        let script = "export DYLD_LIBRARY_PATH=\"${doorstop_directory}:${DYLD_LIBRARY_PATH}\"\nif [ -z \"$DYLD_INSERT_LIBRARIES\" ]; then\n    export DYLD_INSERT_LIBRARIES=\"${doorstop_name}\"\nelse\n    export DYLD_INSERT_LIBRARIES=\"${doorstop_name}:${DYLD_INSERT_LIBRARIES}\"\nfi\n";
        let repaired = repair_macos_overlay_script(script).unwrap();
        for (dyld, steam, expected) in [
            ("", "", "libdoorstop.dylib"),
            (
                "",
                "/Steam/loader.dylib:/Steam/overlay.dylib",
                "libdoorstop.dylib:/Steam/loader.dylib:/Steam/overlay.dylib",
            ),
            (
                "/custom/library.dylib",
                "/Steam/overlay.dylib",
                "libdoorstop.dylib:/custom/library.dylib",
            ),
            (
                "/custom/library.dylib",
                "",
                "libdoorstop.dylib:/custom/library.dylib",
            ),
        ] {
            // Assign inside the shell to model the environment the script sees after SIP.
            let output = std::process::Command::new("/bin/sh")
                .arg("-c")
                .arg(format!("doorstop_name=libdoorstop.dylib\nDYLD_INSERT_LIBRARIES=\"$1\"\nSTEAM_DYLD_INSERT_LIBRARIES=\"$2\"\n{repaired}printf '%s' \"$DYLD_INSERT_LIBRARIES\""))
                .args(["overlay-test", dyld, steam])
                .output()
                .unwrap();
            assert!(output.status.success());
            assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
        }
    }

    #[test]
    fn older_doorstop_scripts_and_crlf_are_supported() {
        let script = "#!/bin/sh\r\nexecutable_name=\"\";\r\nexport DOORSTOP_ENABLE=TRUE\r\n";
        let changed = replace_assignment(script, "executable_name", "Overcooked2.app").unwrap();
        let changed = set_script_enabled(&changed, false).unwrap();
        assert!(!changed.contains('\r'));
        assert_eq!(script_enabled(&changed), Some(false));
    }

    #[test]
    fn native_installer_prepares_a_real_script_and_loader_state() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/verification/native-bepinex")
            .join(uuid::Uuid::new_v4().to_string());
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("run_bepinex.sh"),
            "#!/bin/sh\nexecutable_name=\"\"\nenabled=\"1\"\nexport DYLD_LIBRARY_PATH=\"${doorstop_directory}:${DYLD_LIBRARY_PATH}\"\nif [ -z \"$DYLD_INSERT_LIBRARIES\" ]; then\n    export DYLD_INSERT_LIBRARIES=\"${doorstop_name}\"\nelse\n    export DYLD_INSERT_LIBRARIES=\"${doorstop_name}:${DYLD_INSERT_LIBRARIES}\"\nfi\n",
        )
        .unwrap();
        fs::write(root.join("libdoorstop.so"), b"linux fixture").unwrap();
        fs::write(root.join("libdoorstop.dylib"), b"mac fixture").unwrap();
        for package in [Package::Linux, Package::MacOS] {
            prepare_native(&root, package).unwrap();
            assert_eq!(native_state(&root, package), WinhttpState::ModEnabled);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_ne!(
                    fs::metadata(root.join("run_bepinex.sh"))
                        .unwrap()
                        .permissions()
                        .mode()
                        & 0o100,
                    0
                );
            }
        }
        let script_path = root.join("run_bepinex.sh");
        let disabled =
            set_script_enabled(&fs::read_to_string(&script_path).unwrap(), false).unwrap();
        fs::write(&script_path, &disabled).unwrap();
        repair_macos_overlay(&root).unwrap();
        repair_macos_overlay(&root).unwrap();
        assert_eq!(fs::read_to_string(&script_path).unwrap(), disabled);
        assert_eq!(
            native_state(&root, Package::MacOS),
            WinhttpState::ModDisabled
        );
        fs::write(&script_path, "unrecognized").unwrap();
        assert!(repair_macos_overlay(&root).is_err());
        assert_eq!(fs::read_to_string(&script_path).unwrap(), "unrecognized");
        fs::remove_file(root.join("run_bepinex.sh")).unwrap();
        assert!(prepare_native(&root, Package::Linux).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
