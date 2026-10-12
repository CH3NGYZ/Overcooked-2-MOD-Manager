use std::path::{Component, Path};

const CHECK_FLAG: &str = "--validate-manager-install-dir";
const ERROR: &str = "管理器不能安装到 BepInEx/plugins 或其子目录，请选择游戏目录外的位置。";

pub(crate) fn validate_install_dir(directory: &Path) -> Result<(), String> {
    if directory.as_os_str().is_empty() || !directory.is_absolute() {
        return Err(ERROR.into());
    }
    if is_plugins_directory(directory)? {
        return Err(ERROR.into());
    }
    Ok(())
}

pub(crate) fn is_plugins_directory(directory: &Path) -> Result<bool, String> {
    let absolute = std::path::absolute(directory).map_err(|_| ERROR.to_string())?;
    let resolved =
        crate::game::resolve_existing_ancestor(&absolute).map_err(|_| ERROR.to_string())?;
    Ok(inside_plugins(&absolute) || inside_plugins(&resolved))
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
            "1028" => "管理器不能安裝到 BepInEx/plugins 或其子目錄，請選擇遊戲目錄外的位置。",
            "1042" => {
                "관리자를 BepInEx/plugins 또는 그 하위 폴더에 설치할 수 없습니다. 게임 폴더 밖의 위치를 선택하세요."
            }
            _ => {
                "The manager cannot be installed in BepInEx/plugins or any of its subfolders. Choose a location outside the game directory."
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
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn rejects_plugins_and_descendants_with_case_variants() {
        let root = std::env::temp_dir();
        for suffix in [
            "BepInEx/plugins",
            "bepinex/PLUGINS/OC2ModManager",
            "BepInEx/plugins/one/two",
            "BepInEx/plugins/./OC2ModManager",
        ] {
            assert!(
                validate_install_dir(&root.join(suffix)).is_err(),
                "{suffix}"
            );
        }
        assert!(validate_install_dir(Path::new("")).is_err());
        assert!(validate_install_dir(Path::new("relative/path")).is_err());
    }

    #[test]
    fn allows_normal_locations_and_similarly_named_directories() {
        let root = std::env::temp_dir();
        for suffix in [
            "OC2ModManager",
            "BepInEx/plugins-other/Manager",
            "OtherBepInEx/plugins/Manager",
        ] {
            assert!(validate_install_dir(&root.join(suffix)).is_ok(), "{suffix}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn rejects_existing_junction_with_missing_destination_suffix() {
        let root = std::env::temp_dir().join(format!("oc2-install-guard-{}", uuid::Uuid::new_v4()));
        let plugins = root.join("game/BepInEx/plugins");
        let alias = root.join("alias");
        fs::create_dir_all(&plugins).unwrap();
        let junction = std::process::Command::new("cmd")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(&alias)
            .arg(plugins.to_string_lossy().replace('/', "\\"))
            .output()
            .unwrap();
        assert!(
            junction.status.success(),
            "{}",
            String::from_utf8_lossy(&junction.stderr)
        );
        assert!(validate_install_dir(&alias.join("new/Manager")).is_err());
        fs::remove_dir(&alias).unwrap();
        fs::remove_dir_all(&root).unwrap();
    }
}
