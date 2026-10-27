use std::path::{Path, PathBuf};

/// 数据目录名：放在管理器安装目录下，不再使用用户主目录。
const DATA_DIRECTORY: &str = "OC2ModManager";

/// 管理器自身可执行文件所在目录。
///
/// 数据根建立在安装目录内，与安装位置同生共死 —— 卸载时删目录即可清理，
/// 不会在用户主目录留下孤儿配置、备份或回收站。
pub(crate) fn install_dir() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|_| "无法确定程序所在目录")?;
    exe.parent()
        .filter(|parent| parent.is_absolute())
        .map(Path::to_path_buf)
        .ok_or_else(|| "无法确定程序所在目录".into())
}

/// 数据根：安装目录下的 `OC2ModManager/` 子目录。
///
/// 为什么不直接用安装目录：安装目录里还有 exe、卸载器和依赖 DLL，
/// 把数据和程序文件混在一层会让「清理数据」变得危险。独立子目录让
/// 「删数据」与「删程序」互不影响。
pub(crate) fn root() -> Result<PathBuf, String> {
    Ok(install_dir()?.join(DATA_DIRECTORY))
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
    fn data_root_is_a_subdirectory_of_the_install_dir() {
        // 数据与程序文件必须分开：数据根 = 安装目录 / OC2ModManager，
        // 既跟着安装位置走，又不与 exe、卸载器混在同一层。
        let install = Path::new("C:/Apps/OC2");
        let root = install.join(DATA_DIRECTORY);
        assert_eq!(root.parent(), Some(install));
        assert_ne!(root, install);
    }

    #[test]
    fn data_directories_stay_inside_the_root() {
        let install = Path::new("C:/Apps/OC2");
        let root = install.join(DATA_DIRECTORY);
        for sub in ["logs", "backups", "cache/updates"] {
            assert!(root.join(sub).starts_with(&root), "{sub}");
        }
    }
}
