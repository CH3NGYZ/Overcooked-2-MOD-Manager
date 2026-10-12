use std::path::{Path, PathBuf};

const STEAM_DIRECTORIES: &[&str] = &[
    "drive_c/Program Files (x86)/Steam",
    "drive_c/Program Files/Steam",
    "drive_c/Steam",
];

pub(crate) fn prefix_for_path(path: &Path) -> Option<PathBuf> {
    path.ancestors().find_map(|ancestor| {
        ancestor
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("drive_c"))
            .then(|| ancestor.parent().map(Path::to_path_buf))
            .flatten()
    })
}

/// VDF paths belong to the Windows Steam client, not to the host filesystem.
pub(crate) fn host_library_path(steam_root: &Path, value: &str) -> Option<PathBuf> {
    let value = value.replace("\\\\", "\\");
    let bytes = value.as_bytes();
    if bytes.len() < 3 || bytes[1] != b':' || !bytes[0].is_ascii_alphabetic() {
        return None;
    }
    if !matches!(bytes[2], b'\\' | b'/') {
        return None;
    }
    let prefix = prefix_for_path(steam_root)?;
    let drive = (bytes[0] as char).to_ascii_lowercase();
    let base = if drive == 'c' {
        prefix.join("drive_c")
    } else {
        let device = prefix.join(format!("dosdevices/{drive}:"));
        std::fs::canonicalize(device).ok()?
    };
    let mut result = base;
    for component in value[3..].split(['\\', '/']) {
        if component.is_empty() || component == "." {
            continue;
        }
        if component == ".." || component.contains(':') {
            return None;
        }
        result.push(component);
    }
    Some(result)
}

fn steam_roots_in_prefix(prefix: &Path) -> Vec<PathBuf> {
    STEAM_DIRECTORIES
        .iter()
        .map(|path| prefix.join(path))
        .filter(|path| path.join("steam.exe").is_file())
        .collect()
}

#[cfg(unix)]
pub(crate) fn steam_roots() -> Vec<PathBuf> {
    let mut prefixes = Vec::new();
    if let Some(prefix) = std::env::var_os("WINEPREFIX").map(PathBuf::from)
        && prefix.is_absolute()
    {
        prefixes.push(prefix);
    }
    if let Some(home) = dirs::home_dir() {
        prefixes.push(home.join(".wine"));
        #[cfg(target_os = "macos")]
        {
            let container = home.join("Library/Application Support/CrossOver/Bottles");
            if let Ok(entries) = std::fs::read_dir(container) {
                prefixes.extend(entries.flatten().map(|entry| entry.path()));
            }
        }
    }
    let mut roots = Vec::new();
    for prefix in prefixes {
        for root in steam_roots_in_prefix(&prefix) {
            if !roots.contains(&root) {
                roots.push(root);
            }
        }
    }
    roots
}

fn dll_overrides(previous: &str) -> String {
    let mut overrides = Vec::new();
    for setting in previous.split(';').filter(|value| !value.trim().is_empty()) {
        if let Some((libraries, mode)) = setting.split_once('=') {
            let libraries = libraries
                .split(',')
                .filter(|library| !library.trim().eq_ignore_ascii_case("winhttp"))
                .collect::<Vec<_>>();
            if !libraries.is_empty() {
                overrides.push(format!("{}={mode}", libraries.join(",")));
            }
        } else {
            overrides.push(setting.to_string());
        }
    }
    overrides.push("winhttp=n,b".into());
    overrides.join(";")
}

#[cfg(unix)]
fn wine_command(prefix: &Path) -> Result<std::process::Command, String> {
    if !prefix.is_absolute() || !prefix.join("drive_c").is_dir() {
        return Err("无法确定游戏使用的 Wine 容器，请重新选择厨房目录".into());
    }
    #[cfg(target_os = "linux")]
    let mut command = std::process::Command::new("wine");
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut apps = vec![PathBuf::from("/Applications/CrossOver.app")];
        if let Some(home) = dirs::home_dir() {
            apps.push(home.join("Applications/CrossOver.app"));
        }
        let wine = apps
            .into_iter()
            .map(|app| app.join("Contents/SharedSupport/CrossOver/bin/wine"))
            .find(|path| path.is_file())
            .ok_or("请先安装 CrossOver，再启动 Wine 版厨房")?;
        let mut command = std::process::Command::new(wine);
        command
            .arg("--bottle")
            .arg(prefix.file_name().ok_or("Wine 容器路径无效")?);
        command.env(
            "CX_BOTTLE_PATH",
            prefix.parent().ok_or("Wine 容器路径无效")?,
        );
        command
    };
    command
        .env("WINEPREFIX", prefix)
        .env(
            "WINEDLLOVERRIDES",
            dll_overrides(&std::env::var("WINEDLLOVERRIDES").unwrap_or_default()),
        )
        .stdin(std::process::Stdio::null());
    Ok(command)
}

#[cfg(unix)]
pub(crate) fn steam_command(prefix: &Path) -> Result<std::process::Command, String> {
    let steam = steam_roots_in_prefix(prefix)
        .into_iter()
        .next()
        .ok_or("Wine 容器中未找到 Windows Steam，请先完成安装")?
        .join("steam.exe");
    let mut command = wine_command(prefix)?;
    command.arg(steam).args(["-applaunch", "728880"]);
    Ok(command)
}

#[cfg(any(unix, test))]
fn winhttp_registry_command(mut command: std::process::Command) -> std::process::Command {
    command
        .args([
            "reg.exe",
            "add",
            r"HKEY_CURRENT_USER\Software\Wine\DllOverrides",
            "/v",
            "winhttp",
            "/t",
            "REG_SZ",
            "/d",
            "native,builtin",
            "/f",
        ])
        .env("WINEDEBUG", "-all")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    command
}

#[cfg(unix)]
pub(crate) fn configure_winhttp(prefix: &Path) -> Result<(), String> {
    // A launch-time environment override cannot affect an already running Steam.
    // Persist this one library in the actual game's prefix, through Wine itself.
    let mut child = winhttp_registry_command(wine_command(prefix)?)
        .spawn()
        .map_err(|error| {
            log::warn!("Wine winhttp 配置进程启动失败：{error}");
            "无法自动配置 Wine winhttp，请在目标容器的 winecfg 中手动设置".to_string()
        })?;
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => {
                log::info!("已在游戏 Wine 容器中设置 winhttp 原装优先覆盖");
                return Ok(());
            }
            Ok(Some(_)) => {
                return Err("无法自动配置 Wine winhttp，请在目标容器的 winecfg 中手动设置".into());
            }
            Ok(None) if started.elapsed() < std::time::Duration::from_secs(20) => {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Wine winhttp 自动配置超时，请在目标容器的 winecfg 中手动设置".into());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_steam_paths_resolve_inside_their_own_prefix() {
        let prefix = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/verification/wine-paths");
        let steam = prefix.join("drive_c/Program Files (x86)/Steam");
        for path in [
            r"C:\\Program Files (x86)\\Steam",
            r"c:\Program Files (x86)\Steam",
        ] {
            assert_eq!(host_library_path(&steam, path), Some(steam.clone()));
        }
        assert_eq!(
            prefix_for_path(&steam.join("steamapps/common/Overcooked! 2")),
            Some(prefix)
        );
        for path in [
            r"C:Steam",
            r"C:\..\outside",
            r"C:\folder:stream",
            "relative",
            "/host/Steam",
        ] {
            assert!(host_library_path(&steam, path).is_none(), "{path}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn secondary_drive_mappings_use_wine_dosdevices_not_host_drive_letters() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/verification/wine-drives")
            .join(uuid::Uuid::new_v4().to_string());
        let prefix = root.join("prefix");
        let library = root.join("library");
        std::fs::create_dir_all(prefix.join("dosdevices")).unwrap();
        std::fs::create_dir_all(&library).unwrap();
        std::os::unix::fs::symlink(&library, prefix.join("dosdevices/d:")).unwrap();
        let steam = prefix.join("drive_c/Steam");
        assert_eq!(
            host_library_path(&steam, r"D:\SteamLibrary"),
            Some(library.join("SteamLibrary"))
        );
        assert!(host_library_path(&steam, r"E:\SteamLibrary").is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn wine_steam_discovery_does_not_accept_native_steam_or_empty_prefixes() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/verification/wine-steam")
            .join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(root.join("drive_c/Steam")).unwrap();
        assert!(steam_roots_in_prefix(&root).is_empty());
        std::fs::write(root.join("drive_c/Steam/steam.exe"), b"fixture").unwrap();
        assert_eq!(
            steam_roots_in_prefix(&root),
            vec![root.join("drive_c/Steam")]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn wine_winhttp_override_preserves_unrelated_libraries() {
        assert_eq!(dll_overrides(""), "winhttp=n,b");
        assert_eq!(
            dll_overrides("d3d11=n;WinHTTP,dxgi=b;version=n,b"),
            "d3d11=n;dxgi=b;version=n,b;winhttp=n,b"
        );
    }

    #[test]
    fn persistent_override_uses_wine_registry_without_shell_interpolation() {
        let mut command = std::process::Command::new("wine");
        let prefix =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("target/verification/Prefix with spaces");
        command.env("WINEPREFIX", &prefix);
        let command = winhttp_registry_command(command);
        let args = command.get_args().collect::<Vec<_>>();
        assert_eq!(args[0], "reg.exe");
        assert_eq!(args[2], r"HKEY_CURRENT_USER\Software\Wine\DllOverrides");
        assert_eq!(args[4], "winhttp");
        assert_eq!(args[8], "native,builtin");
        assert!(
            command
                .get_envs()
                .any(|(key, value)| { key == "WINEPREFIX" && value == Some(prefix.as_os_str()) })
        );
    }
}
