//! 地图目录约定、路径校验与文件枚举。

use crate::framework::DIYLEVEL_DIR;
use std::fs;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

/// 启用中的关卡集目录名。
pub(super) const LEVELS_DIR: &str = "levels";

/// 停用关卡集的根（游戏相对路径）。
///
/// 刻意**不在 `plugins/` 树里**：BepInEx 递归扫 `plugins/` 下所有 `.dll`，放
/// 在那里停用的只是地图，作者随包的修复插件还会被加载。也不要用
/// `levels/.disabled` 这种嵌套路径 —— DLL 会把 `.disabled` 当成一个关卡集去
/// 读 `info`，找不到就刷错误。
pub(super) const DISABLED_ROOT: &str = "BepInEx/cache/modmanager_disabled_maps";

/// 旧的停用根（`plugins/OC2DIYLevel/disabled`），只用于一次性迁移。
pub(super) const LEGACY_DISABLED_ROOT: &str = "BepInEx/plugins/OC2DIYLevel/disabled";

/// 启用中的关卡集根：`BepInEx/plugins/OC2DIYLevel/levels`。
pub(crate) fn levels_root(game_root: &Path) -> PathBuf {
    game_root.join(DIYLEVEL_DIR).join(LEVELS_DIR)
}

/// 停用中的关卡集根：`BepInEx/cache/modmanager_disabled_maps`。
pub fn disabled_root(game_root: &Path) -> PathBuf {
    game_root.join(DISABLED_ROOT)
}

/// 旧停用根（在 `plugins/` 里）：只用于迁移，以及 MOD 扫描时把它排除掉。
pub(crate) fn legacy_disabled_root(game_root: &Path) -> PathBuf {
    game_root.join(LEGACY_DISABLED_ROOT)
}

/// 把旧停用根（在 `plugins/` 里）整批搬到新位置。
///
/// 只在旧目录存在时动作：整体 rename 失败（目标已存在、跨卷等）就退化成逐项
/// 搬迁，同名目录留在原地并记日志 —— 宁可少搬一个，也不覆盖用户的数据。
/// 搬空后删掉旧目录壳，避免下次启动又扫到一遍。
pub fn migrate_legacy_disabled_root(game_root: &Path) {
    let legacy = legacy_disabled_root(game_root);
    if !legacy.is_dir() {
        return;
    }
    let target = disabled_root(game_root);
    if fs::create_dir_all(&target).is_err() {
        log::warn!("创建停用根失败，暂不迁移旧目录：{}", target.display());
        return;
    }
    if !target
        .read_dir()
        .map(|mut it| it.next().is_some())
        .unwrap_or(false)
    {
        match fs::rename(&legacy, &target) {
            Ok(()) => {
                log::info!(
                    "旧停用根已整体迁移：从={}，到={}",
                    legacy.display(),
                    target.display()
                );
                return;
            }
            Err(error) => log::warn!(
                "旧停用根整体迁移失败，改为逐项搬迁：从={}，原因={}",
                legacy.display(),
                error
            ),
        }
    }
    let Ok(entries) = fs::read_dir(&legacy) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if !kind.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let destination = target.join(&name);
        if destination.exists() {
            log::warn!(
                "旧停用根里的目录与新位置同名，保留在旧位置：目录={}",
                name.to_string_lossy()
            );
            continue;
        }
        match fs::rename(entry.path(), &destination) {
            Ok(()) => log::info!("旧停用根逐项迁移：目录={}", name.to_string_lossy()),
            Err(error) => log::warn!(
                "旧停用根逐项迁移失败：目录={}，原因={}",
                name.to_string_lossy(),
                error
            ),
        }
    }
    // 只在真的空掉时删壳；还有搬不动的条目就留着现场
    let _ = fs::remove_dir(&legacy);
}

/// 目录名是否是**单一路径片段**（不含分隔符、`..`、盘符、根）。
///
/// 关卡集目录名来自扫描结果或前端传参。允许 `..` 或分隔符就能把游戏目录之外
/// 的目录搬进来 / 删掉，所以所有「按目录名定位」的地方都先过这里 ——
/// 集中一处判定，不会出现某个入口忘了校验。
///
/// 判定用 `components()` 而不是查字符串里的 `/`、`\`：Windows 同时认两种分隔符，
/// 而且 `C:`、`.`、`..` 这些写法必须先拆成组件才看得清。这里要求**恰好一个**
/// `Normal` 组件，`a/b` 这种多段写法也会被拒。
pub(super) fn is_safe_dir_name(dir_name: &str) -> bool {
    if dir_name.trim().is_empty() {
        return false;
    }
    let mut components = Path::new(dir_name).components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

/// 把一个关卡集目录展开成「游戏相对路径」列表。
///
/// 回收站按文件记账（这样恢复能逐路径还原、失败能逐文件回滚），所以目录要先
/// 展开。只收文件，不收目录本身 —— 空目录由回收后的清理逻辑处理。
pub(super) fn collect_files(game_root: &Path, set_dir: &Path) -> Result<Vec<String>, String> {
    let canonical_root = game_root
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(set_dir).follow_links(false) {
        let entry = entry.map_err(|error| error.to_string())?;
        if !entry.file_type().is_file() {
            continue;
        }
        let absolute = entry
            .path()
            .canonicalize()
            .map_err(|error| error.to_string())?;
        // 目录联接可能把真实文件指到游戏目录之外，逐文件确认归属
        let relative = absolute
            .strip_prefix(&canonical_root)
            .map_err(|_| "archive.pathEscape".to_string())?;
        files.push(relative.to_string_lossy().replace('\\', "/"));
    }
    Ok(files)
}

/// 自底向上删除空目录；非空目录保留。
///
/// 回收只搬文件、不搬目录，所以删完 / 回收完要清掉空壳。**只删空目录**：
/// 里面还有没被搬走的文件（目录联接、被占用而搬不动的文件）时保留现场 ——
/// 宁可留一个空壳让用户自己看，也不能静默丢掉文件。
pub(super) fn remove_empty_directories(path: &Path) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            remove_empty_directories(&entry.path());
        }
    }
    // 非空时 remove_dir 会失败，正是我们要的「保留现场」
    let _ = fs::remove_dir(path);
}

/// 自底向上删空目录，直到 `stop`（不含）。非空目录立刻停手。
pub(super) fn remove_empty_parents(path: &Path, stop: &Path) {
    let mut current = path.parent();
    while let Some(directory) = current {
        if directory == stop || !directory.starts_with(stop) {
            break;
        }
        if fs::remove_dir(directory).is_err() {
            break;
        }
        current = directory.parent();
    }
}

/// 相对路径是否安全（非绝对、无 `..`、无 Windows 盘符与数据流冒号）。
pub(super) fn is_safe_relative(relative: &str) -> bool {
    if relative.is_empty() || relative.contains(':') {
        return false;
    }
    let path = Path::new(relative);
    !path.is_absolute()
        && !path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::RootDir))
}

/// 收集一个目录下所有子目录名（转小写），用于避开已占用的目录名。
pub(super) fn collect_dir_names(directory: &Path) -> std::collections::HashSet<String> {
    let mut names = std::collections::HashSet::new();
    let Ok(entries) = fs::read_dir(directory) else {
        return names;
    };
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            names.insert(entry.file_name().to_string_lossy().to_lowercase());
        }
    }
    names
}
