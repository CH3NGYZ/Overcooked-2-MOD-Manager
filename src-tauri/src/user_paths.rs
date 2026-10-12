use std::path::{Path, PathBuf};

const DATA_DIRECTORY: &str = "OC2ModManager";

pub(crate) fn root() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("无法确定当前用户目录")?;
    root_in_home(&home)
}

fn root_in_home(home: &Path) -> Result<PathBuf, String> {
    if !home.is_absolute() {
        return Err("无法确定当前用户目录".into());
    }
    Ok(home.join(DATA_DIRECTORY))
}

pub(crate) fn logs_dir() -> Result<PathBuf, String> {
    Ok(root()?.join("logs"))
}

pub(crate) fn backups_dir() -> Result<PathBuf, String> {
    Ok(root()?.join("backups"))
}

pub(crate) fn updates_dir() -> Result<PathBuf, String> {
    Ok(root()?.join("cache/updates"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_root_is_inside_the_home_not_platform_appdata() {
        #[cfg(windows)]
        let home = Path::new("C:/Users/Current User");
        #[cfg(not(windows))]
        let home = Path::new("/home/current user");
        assert_eq!(root_in_home(home).unwrap(), home.join("OC2ModManager"));
    }

    #[cfg(unix)]
    #[test]
    fn supports_root_and_macos_homes() {
        for home in ["/root", "/Users/current user"] {
            assert_eq!(
                root_in_home(Path::new(home)).unwrap(),
                Path::new(home).join("OC2ModManager")
            );
        }
    }

    #[test]
    fn missing_home_never_falls_back_to_the_install_directory() {
        for home in ["", "relative-home"] {
            assert!(root_in_home(Path::new(home)).is_err());
        }
    }
}
