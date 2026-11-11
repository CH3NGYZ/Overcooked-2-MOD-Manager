//! 关卡集启停、定位与回收；地图操作不按游戏运行状态门控。

use super::info_cache::read_info_metadata;
use super::paths::LevelSetRoots;
use super::paths::collect_files;
use super::paths::disabled_root;
use super::paths::is_safe_dir_name;
use super::paths::levels_root;
use super::paths::remove_empty_directories;
use super::paths::remove_empty_parents;
use super::plugin_registry::RegisteredLevelSetPlugin;
use super::plugin_registry::forget_registered_plugins;
use super::plugin_registry::read_registered_plugins;
use super::scan::locate_info_file;
use super::scan::scan_level_sets;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

/// 启用或停用一个关卡集：在 `levels/` 与 `BepInEx/cache/modmanager_disabled_maps/` 之间移动整个目录。
///
/// 返回值是移动后的新路径。目标位置已有同名目录时**拒绝**并返回错误 ——
/// 覆盖会静默丢掉用户已有的那个关卡集，而这两个目录都是用户数据。
pub fn set_level_set_enabled(
    roots: &LevelSetRoots,
    dir_name: &str,
    enabled: bool,
) -> Result<PathBuf, String> {
    // 目录名必须是单一路径片段：允许 `..` 或分隔符就能把任意目录搬进游戏目录
    if !is_safe_dir_name(dir_name) {
        log::warn!("拒绝不安全的关卡集目录名：{}", dir_name);
        return Err("levelSet.invalidName".into());
    }
    let game_root = roots.game_root();
    let levels_dir = levels_root(roots)?;
    let disabled_dir = roots.safe_path(&disabled_root(game_root))?;
    let (from_root, to_root) = if enabled {
        (disabled_dir, levels_dir)
    } else {
        (levels_dir, disabled_dir)
    };
    let source = roots.safe_path(&from_root.join(dir_name))?;
    let destination = roots.safe_path(&to_root.join(dir_name))?;
    if !source.starts_with(&from_root) || !destination.starts_with(&to_root) {
        return Err("archive.pathEscape".into());
    }
    if !source.is_dir() {
        log::warn!(
            "关卡集不在预期位置：目录={}，期望={}",
            dir_name,
            source.display()
        );
        return Err("levelSet.notFound".into());
    }
    if destination.exists() {
        log::warn!(
            "目标位置已有同名关卡集，拒绝覆盖：目录={}，目标={}",
            dir_name,
            destination.display()
        );
        return Err(crate::locale_tag::with_params(
            "levelSet.targetExists",
            &[("name", dir_name)],
        ));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::rename(&source, &destination).map_err(|error| {
        log::error!(
            "移动关卡集失败：目录={}，从={}，到={}，原因={}",
            dir_name,
            source.display(),
            destination.display(),
            error
        );
        crate::locale_tag::with_params("levelSet.moveFailed", &[("reason", &error.to_string())])
    })?;
    log::info!(
        "关卡集已{}：目录={}，新路径={}",
        if enabled { "启用" } else { "停用" },
        dir_name,
        destination.display()
    );
    Ok(destination)
}

/// 批量启用 / 停用全部关卡集，返回**实际**移动过的目录名。
///
/// 逐条调用 `set_level_set_enabled` 时每次都重新读目录，地图一多就是重复
/// 遍历；这里先扫一次盘、只处理需要变的目录。中途失败按**逆序回滚**已经
/// 移动的目录，保证列表不会停在半开半关的状态（与 MOD 的批量开关同语义）。
///
/// 空列表表示「本来就已经是目标状态」，调用方据此给出「无需切换」提示。
pub fn set_all_level_sets_enabled(
    roots: &LevelSetRoots,
    enabled: bool,
) -> Result<Vec<String>, String> {
    let targets: Vec<String> = scan_level_sets(roots)
        .into_iter()
        .filter(|set| set.enabled != enabled)
        .map(|set| set.dir_name)
        .collect();
    let mut changed: Vec<String> = Vec::new();
    for dir_name in &targets {
        match set_level_set_enabled(roots, dir_name, enabled) {
            Ok(_) => changed.push(dir_name.clone()),
            Err(error) => {
                log::error!(
                    "批量切换关卡集失败，开始回滚：目录={}，原因={}",
                    dir_name,
                    error
                );
                for previous in changed.iter().rev() {
                    if let Err(rollback) = set_level_set_enabled(roots, previous, !enabled) {
                        log::error!("批量切换回滚失败：目录={}，原因={}", previous, rollback);
                    }
                }
                return Err(error);
            }
        }
    }
    Ok(changed)
}

/// 定位一个关卡集目录，并确认它属于选中的 DIYLevel 根或停用根。
///
/// `enabled` 决定去 `levels/` 还是停用根找：同名目录可能只存在于其中一边，
/// 猜错就会操作到另一个关卡集（打开错目录、删错文件）。
///
/// 「打开目录」与「删除」都先走这里，路径校验只写一遍 —— 两个入口不会出现
/// 一个严一个松，也不会因为某个入口将来漏改而让前端参数变成任意路径。
pub fn locate_level_set_directory(
    roots: &LevelSetRoots,
    dir_name: &str,
    enabled: bool,
) -> Result<PathBuf, String> {
    if !is_safe_dir_name(dir_name) {
        log::warn!("拒绝不安全的关卡集目录名：{}", dir_name);
        return Err("levelSet.invalidName".into());
    }
    /*
     * 先确认这一侧的根目录本身在游戏内，再拼关卡集目录：`safe_game_path` 会
     * 解析最近的已存在祖先，目录联接因此不能把目标指到游戏目录之外。
     * 两侧各有自己的根（启用侧在 plugins 里、停用侧在 cache 里），归属检查
     * 必须对着各自的根做 —— 拿 DIYLevel 的根去校验停用侧会永远不通过。
     */
    let base = if enabled {
        levels_root(roots)?
    } else {
        roots.safe_path(&disabled_root(roots.game_root()))?
    };
    let path = roots.safe_path(&base.join(dir_name))?;
    if !path.starts_with(&base) || !path.is_dir() {
        log::warn!(
            "关卡集目录不存在或不在 DIYLevel 下：目录={}，期望={}",
            dir_name,
            path.display()
        );
        return Err("levelSet.notFound".into());
    }
    Ok(path)
}

/// 删除一个关卡集：跟随回收开关，要么整目录移进回收站，要么直接删除。
///
/// ## 为什么默认进回收站
///
/// 关卡集里常是作者精心调过的场景文件，删掉没有版本控制可回退。回收站按
/// **文件**记账（恢复能逐路径还原、失败能逐文件回滚），所以这里先把目录展开
/// 成文件列表再交给 `recycle_bin::stage` —— 与「安装时回收旧版本」走同一条
/// 代码路径，回收站记录格式不会分叉。
///
/// ## 为什么空目录不进回收站
///
/// `stage` 会为它产生一条 0 文件的记录，用户点恢复什么也拿不回来。
/// 直接移除更诚实。
pub fn remove_level_set(
    recycle_root: &Path,
    manifests_dir: &Path,
    roots: &LevelSetRoots,
    dir_name: &str,
    enabled: bool,
    recycle: bool,
) -> Result<(), String> {
    let game_root = roots.game_root();
    let set_dir = locate_level_set_directory(roots, dir_name, enabled)?;
    /*
     * BepInEx 表层包的随包插件不在集目录里：按身份从登记表里取出属于它的
     * DLL，卸载时一起回收并清掉登记 —— 否则插件留在 `BepInEx/plugins` 里
     * 继续被加载，用户以为「地图删干净了」。
     */
    let metadata = locate_info_file(&set_dir).and_then(|info| read_info_metadata(&info).ok());
    let registered = read_registered_plugins(manifests_dir);
    let owned: Vec<RegisteredLevelSetPlugin> = match &metadata {
        Some(metadata) => registered
            .iter()
            .filter(|entry| entry.owner.matches(metadata))
            .cloned()
            .collect(),
        // 读不出身份就不猜：只删集目录，登记表保持原样（过期记录扫描会跳过）
        None => Vec::new(),
    };
    let registered_files: Vec<String> = owned
        .iter()
        .filter(|entry| game_root.join(&entry.dll).is_file())
        .map(|entry| entry.dll.clone())
        .collect();
    let plugins_root = game_root.join("BepInEx/plugins");
    // Pure map replacement is allowed during play. Bundled DLLs are MOD
    // changes, including unregistered DLLs inside the map directory.
    let mut files = collect_files(game_root, &set_dir)?;
    if !registered_files.is_empty()
        || files
            .iter()
            .any(|file| file.to_ascii_lowercase().ends_with(".dll"))
    {
        crate::game::ensure_game_stopped(game_root)?;
    }

    if !recycle {
        // 用户关掉了「删除 MOD 时优先放入回收站」：路径归属已校验，直接删
        fs::remove_dir_all(&set_dir).map_err(|error| {
            crate::locale_tag::with_params(
                "levelSet.deleteFailed",
                &[("reason", &error.to_string())],
            )
        })?;
        for dll in &registered_files {
            let path = game_root.join(dll);
            if let Err(error) = fs::remove_file(&path) {
                log::warn!(
                    "随包插件删除失败，文件可能已被占用：文件={}，原因={}",
                    path.display(),
                    error
                );
                continue;
            }
            remove_empty_parents(&path, &plugins_root);
        }
        forget_registered_plugins(manifests_dir, &registered, &owned, dir_name);
        log::info!("关卡集已删除（未进回收站）：目录={}", dir_name);
        return Ok(());
    }
    files.extend(registered_files.iter().cloned());
    if files.is_empty() {
        fs::remove_dir_all(&set_dir).map_err(|error| {
            crate::locale_tag::with_params(
                "levelSet.deleteFailed",
                &[("reason", &error.to_string())],
            )
        })?;
        forget_registered_plugins(manifests_dir, &registered, &owned, dir_name);
        log::info!("关卡集目录为空，已直接移除：目录={}", dir_name);
        return Ok(());
    }
    // 回收记录用集名显示（界面语言下的名字）；info 读不出来时退回目录名
    let name = metadata
        .as_ref()
        .map(|metadata| metadata.name.clone())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| dir_name.to_string());
    crate::recycle_bin::stage(
        recycle_root,
        manifests_dir,
        game_root,
        name,
        None,
        None,
        Vec::new(),
        files,
        None,
        &mut || Ok(()),
    )?;
    /*
     * 回收只搬文件，目录壳会留在原地。不清掉它，扫描会把这个空目录当成
     * 一个「缺 info」的坏关卡集继续显示在列表里。
     */
    remove_empty_directories(&set_dir);
    for dll in &registered_files {
        remove_empty_parents(&game_root.join(dll), &plugins_root);
    }
    forget_registered_plugins(manifests_dir, &registered, &owned, dir_name);
    log::info!("关卡集已移入回收站：目录={}", dir_name);
    Ok(())
}
