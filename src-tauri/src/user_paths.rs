use std::fs;
use std::path::{Path, PathBuf};

/// 数据目录名。
const DATA_DIRECTORY: &str = "OC2ModManager";

/// 数据根定位失败时返回的 tag（`config` 域：配置目录不可用）。
const ROOT_UNAVAILABLE: &str = "config.directoryUnavailable";

/// 管理器自身可执行文件所在目录。
///
/// 只用于定位旧版本（1.3.x 及更早）留在安装目录里的数据，**不再作为数据根**。
/// 历史上把数据根写成 `install_dir()/OC2ModManager` 在 macOS 上是致命的：
/// `.app` 里的可执行文件恰好也叫 `OC2ModManager`（没有 `.exe` 扩展名），
/// 于是「数据根」等于可执行文件本身，`create_dir_all` 只会得到
/// `Not a directory`，Tauri 把这个错误变成启动期 panic，用户连日志都没有。
pub(crate) fn install_dir() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|_| "migrate.exeDirectoryUnknown")?;
    exe.parent()
        .filter(|parent| parent.is_absolute())
        .map(Path::to_path_buf)
        .ok_or_else(|| "migrate.exeDirectoryUnknown".into())
}

/// 数据根所在父目录。
///
/// Windows 取系统「文档」已知文件夹，macOS 取 `~/Library/Application Support`。
/// 两者都走系统 API 而不是拼 `%USERPROFILE%\Documents`：Windows 的文档可能被
/// 重定向到 OneDrive 或别的盘符，硬拼路径会写到一个用户根本找不到的地方。
fn data_parent() -> Result<PathBuf, String> {
    #[cfg(windows)]
    let parent =
        dirs::document_dir().or_else(|| dirs::home_dir().map(|home| home.join("Documents")));
    // macOS 的 data_dir 就是 ~/Library/Application Support；其余平台只用于
    // 编译与测试（项目不发布 Linux），沿用 XDG 数据目录保持同样的语义。
    #[cfg(not(windows))]
    let parent = dirs::data_dir();
    parent.ok_or_else(|| ROOT_UNAVAILABLE.to_string())
}

/// 数据根：`<文档|Application Support>/OC2ModManager`。
///
/// 数据与程序彻底分开：
///   · 管理器更新（macOS 是整包替换 `.app`）不会带走配置、清单、备份；
///   · 卸载程序不会顺手删掉用户的回收站与备份；
///   · 便携版、安装版、调试版共用同一份数据，不再各存一套。
pub(crate) fn root() -> Result<PathBuf, String> {
    let root = data_parent()?.join(DATA_DIRECTORY);
    // 数据根必须是一个目录路径，不能恰好等于可执行文件：一旦重合，
    // 之后每次文件操作都只会返回 `Not a directory` / `File exists`，
    // 而且 panic 日志也写不进去 —— 正是 macOS 那次崩溃的现场。
    if let Ok(executable) = std::env::current_exe() {
        if root == executable {
            return Err(ROOT_UNAVAILABLE.to_string());
        }
    }
    Ok(root)
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

/// 旧版本数据根：安装目录下的 `OC2ModManager/`。
fn legacy_root() -> Result<PathBuf, String> {
    Ok(install_dir()?.join(DATA_DIRECTORY))
}

/// 旧数据迁移的结果。
///
/// 迁移发生在日志插件初始化之前（要先搬走旧目录，再在新数据根里建日志目录），
/// 所以这里只返回结果，由调用方在日志可用后补记。
pub(crate) enum LegacyMigration {
    /// 没有旧数据，或新位置早就在用。
    None,
    /// 已搬到新位置。
    Moved { legacy: PathBuf, target: PathBuf },
    /// 搬迁中途失败：程序仍用新位置启动，旧目录原样保留。
    Failed {
        legacy: PathBuf,
        target: PathBuf,
        error: std::io::Error,
    },
}

/// 把旧版本留在安装目录里的数据搬到新数据根，启动时调用一次。
///
/// 迁移失败不能让管理器起不来：最坏情况是用户重新配置一遍，而不是打不开程序。
pub(crate) fn migrate_legacy_root() -> LegacyMigration {
    let (Ok(legacy), Ok(target)) = (legacy_root(), root()) else {
        return LegacyMigration::None;
    };
    match migrate_between(&legacy, &target) {
        Ok(false) => LegacyMigration::None,
        Ok(true) => LegacyMigration::Moved { legacy, target },
        Err(error) => LegacyMigration::Failed {
            legacy,
            target,
            error,
        },
    }
}

/// 把 `legacy` 的顶层条目搬到 `target`，返回是否真的搬了东西。
///
/// 只搬目标里不存在的条目：安装器可能已经在目标写过 `install-language.txt`，
/// 那份比旧值新，不能被覆盖。
fn migrate_between(legacy: &Path, target: &Path) -> std::io::Result<bool> {
    if legacy == target || !legacy.is_dir() {
        return Ok(false);
    }
    // 目标已经写过配置：这台机器早就在用新版本，任何旧目录都只是残留。
    if target.join("config.toml").is_file() {
        return Ok(false);
    }
    fs::create_dir_all(target)?;
    let mut moved = false;
    for entry in fs::read_dir(legacy)? {
        let entry = entry?;
        let destination = target.join(entry.file_name());
        if destination.exists() {
            continue;
        }
        move_entry(&entry.path(), &destination)?;
        moved = true;
    }
    if moved {
        // 旧目录搬空后顺手删掉：安装目录里留一个空的同名目录，
        // 用户会以为数据还在那儿。还有残留（被跳过的条目）时删除会失败，忽略即可。
        let _ = fs::remove_dir(legacy);
    }
    Ok(moved)
}

/// 移动一个条目：同一卷用 `rename`（瞬时），跨卷退化为复制后删除。
fn move_entry(source: &Path, target: &Path) -> std::io::Result<()> {
    if fs::rename(source, target).is_ok() {
        return Ok(());
    }
    copy_entry(source, target)?;
    remove_entry(source)
}

fn copy_entry(source: &Path, target: &Path) -> std::io::Result<()> {
    if fs::symlink_metadata(source)?.is_dir() {
        fs::create_dir_all(target)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_entry(&entry.path(), &target.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(source, target).map(|_| ())
    }
}

fn remove_entry(path: &Path) -> std::io::Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("oc2-user-paths-{label}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    /// 数据根必须在可执行文件之外：macOS 的可执行文件与数据目录同名，
    /// 这份断言守的就是那次 `Not a directory` 启动崩溃。
    #[test]
    fn data_root_is_a_directory_beside_the_user_data_parent() {
        let root = root().unwrap();
        assert!(root.is_absolute());
        assert_eq!(root.file_name().unwrap(), DATA_DIRECTORY);
        let executable = std::env::current_exe().unwrap();
        assert_ne!(root, executable);
        assert!(!root.starts_with(executable.parent().unwrap()));
    }

    #[test]
    fn data_directories_stay_inside_the_root() {
        let root = Path::new("/data/oc2").join(DATA_DIRECTORY);
        for sub in ["logs", "backups", "cache/updates"] {
            assert!(root.join(sub).starts_with(&root), "{sub}");
        }
    }

    /// 旧版本把数据放在安装目录里，升级后必须搬过来，否则用户的配置、
    /// 安装清单和回收站会凭空消失。
    #[test]
    fn migrates_legacy_data_into_an_empty_target() {
        let base = temp_dir("migrate-empty");
        let legacy = base.join("install").join(DATA_DIRECTORY);
        let target = base.join("documents").join(DATA_DIRECTORY);
        fs::create_dir_all(legacy.join("logs")).unwrap();
        fs::write(legacy.join("config.toml"), "ui_zoom = 1.5").unwrap();
        fs::write(legacy.join("logs/rust.log"), "started").unwrap();

        assert!(migrate_between(&legacy, &target).unwrap());
        assert_eq!(
            fs::read_to_string(target.join("config.toml")).unwrap(),
            "ui_zoom = 1.5"
        );
        assert!(target.join("logs/rust.log").is_file());
        assert!(!legacy.exists(), "搬空的旧目录应被删掉");
        fs::remove_dir_all(base).unwrap();
    }

    /// 安装器会先在新数据根写安装语言标记，迁移不能把它盖掉。
    #[test]
    fn keeps_target_entries_and_still_moves_the_rest() {
        let base = temp_dir("migrate-merge");
        let legacy = base.join("install").join(DATA_DIRECTORY);
        let target = base.join("documents").join(DATA_DIRECTORY);
        fs::create_dir_all(&legacy).unwrap();
        fs::create_dir_all(&target).unwrap();
        fs::write(legacy.join("install-language.txt"), "en").unwrap();
        fs::write(legacy.join("config.toml"), "ui_zoom = 1.0").unwrap();
        fs::write(target.join("install-language.txt"), "zh-CN").unwrap();

        assert!(migrate_between(&legacy, &target).unwrap());
        assert_eq!(
            fs::read_to_string(target.join("install-language.txt")).unwrap(),
            "zh-CN"
        );
        assert!(target.join("config.toml").is_file());
        // 被跳过的条目还留在旧目录里，所以旧目录不能被删。
        assert!(legacy.join("install-language.txt").is_file());
        fs::remove_dir_all(base).unwrap();
    }

    /// 新位置已经写过配置 → 旧目录只是残留，绝不覆盖正在用的数据。
    #[test]
    fn skips_when_the_target_is_already_initialized() {
        let base = temp_dir("migrate-initialized");
        let legacy = base.join("install").join(DATA_DIRECTORY);
        let target = base.join("documents").join(DATA_DIRECTORY);
        fs::create_dir_all(&legacy).unwrap();
        fs::create_dir_all(&target).unwrap();
        fs::write(legacy.join("config.toml"), "old").unwrap();
        fs::write(target.join("config.toml"), "current").unwrap();

        assert!(!migrate_between(&legacy, &target).unwrap());
        assert_eq!(
            fs::read_to_string(target.join("config.toml")).unwrap(),
            "current"
        );
        assert!(legacy.join("config.toml").is_file());
        fs::remove_dir_all(base).unwrap();
    }

    /// macOS 的旧路径上是一个**文件**（可执行文件本身），不能被当成目录搬走。
    #[test]
    fn ignores_a_legacy_path_that_is_a_file() {
        let base = temp_dir("migrate-file");
        let legacy = base.join("install").join(DATA_DIRECTORY);
        let target = base.join("documents").join(DATA_DIRECTORY);
        fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        fs::write(&legacy, b"executable").unwrap();

        assert!(!migrate_between(&legacy, &target).unwrap());
        assert!(!target.exists());
        fs::remove_dir_all(base).unwrap();
    }
}
