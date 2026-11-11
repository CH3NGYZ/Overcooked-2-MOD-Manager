//! DLL 路径、普通插件枚举与启停改名；扫描、普通 MOD 与 DIYLevel 共用。
use crate::{archive, game, mods};
use std::collections::{HashMap, HashSet};
use std::{
    fs,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

/// 插件目录的游戏相对路径：扫描与批量开关必须落在同一处。
pub(super) const PLUGINS_DIR: &str = "BepInEx/plugins";

pub(super) fn is_restore_staging_component(name: &std::ffi::OsStr) -> bool {
    name.to_str()
        .is_some_and(|text| text.contains(".before-restore-"))
}

/// 先排除明确属于恢复暂存或登记表的 DLL；地图归属在共用元数据批读后确认。
/// levels / disabled 可以是普通 MOD 的目录名，不能仅凭名字全局排除。
pub(super) fn is_ordinary_plugin(relative: &Path, registered: &HashSet<String>) -> bool {
    if relative
        .components()
        .any(|component| is_restore_staging_component(component.as_os_str()))
    {
        return false;
    }
    let dll_file = format!(
        "{PLUGINS_DIR}/{}",
        relative.to_string_lossy().replace('\\', "/")
    );
    !registered.contains(&canonical_dll_name(&dll_file).to_lowercase())
}

/// 只有已读 GUID 确认的 DIYLevel 包，或仍有明确 info 的地图集才拥有地图子树。
/// 这里只判文件归属，不选择版本；扫描和批量开关使用同一份批读结果。
pub(super) fn is_map_plugin(
    path: &Path,
    plugins: &Path,
    diy_level_packages: &HashSet<PathBuf>,
    map_info: &mut HashMap<PathBuf, bool>,
) -> bool {
    let mut child = path;
    for directory in path
        .ancestors()
        .skip(1)
        .take_while(|directory| *directory != plugins)
    {
        if directory.file_name().is_some_and(|name| {
            name.eq_ignore_ascii_case("levels") || name.eq_ignore_ascii_case("disabled")
        }) {
            if directory
                .parent()
                .is_some_and(|package| diy_level_packages.contains(package))
            {
                return true;
            }
            // 框架 DLL 已移除时，沿用固定 info / 唯一 info_* 的候选规则，
            // 仅检查目录结构，不再解析 bundle；每个目录一批只枚举一次。
            let has_info = map_info.entry(child.to_path_buf()).or_insert_with(|| {
                child.join("info").is_file()
                    || fs::read_dir(child).is_ok_and(|entries| {
                        entries
                            .flatten()
                            .filter(|entry| {
                                entry.file_type().is_ok_and(|kind| kind.is_file())
                                    && entry.file_name().to_string_lossy().starts_with("info_")
                            })
                            .take(2)
                            .count()
                            == 1
                    })
            });
            if *has_info {
                return true;
            }
        }
        child = directory;
    }
    false
}

pub(super) fn is_mod_file(path: &Path) -> bool {
    let name = path.to_string_lossy();
    name.to_ascii_lowercase().ends_with(".dll")
        || name.to_ascii_lowercase().ends_with(".dll.disabled")
}

pub(super) fn is_disabled_mod_file(path: &Path) -> bool {
    path.to_string_lossy()
        .to_ascii_lowercase()
        .ends_with(".dll.disabled")
}

/**
 * 收集 `BepInEx/plugins` 下属于**普通 MOD** 的插件 DLL（含 `.dll.disabled`），
 * 返回游戏相对路径。
 *
 * 「关卡集目录里的 DLL 是地图的随包插件」这条规则必须与
 * `scan_mods_with_manifests` 一致：它们跟着整个集目录一起开关（`levels/` ↔
 * 停用根整目录搬家），再被当成 MOD 单独改 `.dll.disabled` 会与地图开关打架 ——
 * 早期批量开关就是这样把地图随包插件一起 rename 掉的（日志里的「变更文件数」
 * 比 MOD 数量多出几个）。
 *
 * 相对路径一律以 `game::safe_game_path` 解析出的目录本身为基准剥离：它返回
 * `canonicalize` 之后的路径（Windows 上是 `\\?\E:\...`），而 `GameInfo.path`
 * 交给前端的是剥掉 verbatim 前缀的展示形态。把两者混着做 `strip_prefix` 会
 * 静默失配 —— 批量开关曾因此一个文件都收集不到，报「没有找到任何 MOD 插件 DLL」。
 *
 * BepInEx 表层包的随包插件也在这个目录里（作者规划好的位置），但归属是地图集，
 * 由 `manifests/level-set-plugins.json` 登记。批量开关必须跳过它们，否则会把
 * 一张地图的插件改成 `.dll.disabled` —— 与地图开关打架，而且卸载地图时登记的
 * 路径已经不存在了。
 */
pub fn plugin_dll_files(
    game_root: &Path,
    manifest_dir: Option<&Path>,
) -> Result<Vec<String>, String> {
    let plugins = crate::game::safe_game_path(game_root, PLUGINS_DIR)?;
    let registered: HashSet<String> = manifest_dir
        .map(crate::level_sets::registered_plugin_paths)
        .unwrap_or_default()
        .into_iter()
        .collect();
    let mut paths = Vec::new();
    for entry in WalkDir::new(&plugins)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        if !is_mod_file(entry.path()) {
            continue;
        }
        let Ok(relative) = entry.path().strip_prefix(&plugins) else {
            continue;
        };
        if !is_ordinary_plugin(relative, &registered) {
            continue;
        }
        paths.push(entry.into_path());
    }
    // 批量开关也必须先确认真实 DIYLevel 包，避免把地图 DLL 当成独立 MOD。
    // 复用扫描的唯一元数据批读，保留同时存在的 .dll / .dll.disabled 两个路径。
    let workers = crate::concurrency::workers_from(crate::concurrency::AUTO_CONCURRENCY);
    let mut files = super::scan::read_plugin_metadata(&plugins, paths, workers)
        .into_iter()
        .filter_map(|(path, _)| {
            let relative = path.strip_prefix(&plugins).ok()?;
            Some(format!(
                "{PLUGINS_DIR}/{}",
                relative.to_string_lossy().replace('\\', "/")
            ))
        })
        .collect::<Vec<_>>();
    files.sort_by_key(|file| file.to_ascii_lowercase());
    Ok(files)
}

pub fn canonical_dll_file(path: &Path, game: &Path) -> String {
    let relative = path
        .strip_prefix(game)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    canonical_dll_name(&relative)
}

pub fn canonical_dll_name(dll_file: &str) -> String {
    if dll_file.to_ascii_lowercase().ends_with(".disabled") {
        dll_file[..dll_file.len() - ".disabled".len()].to_string()
    } else {
        dll_file.to_string()
    }
}

pub fn disabled_dll_file(dll_file: &str) -> String {
    format!("{dll_file}.disabled")
}

pub(crate) fn set_local_mod_file_state(
    game_root: &Path,
    relative: &str,
    enabled: bool,
) -> Result<bool, String> {
    if !Path::new(&relative)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("dll"))
    {
        return Err("mods.notInPlugins".into());
    }
    let plugins_root = game::safe_game_path(&game_root, "BepInEx/plugins")?;
    let enabled_path = game::safe_game_path(&game_root, &relative)?;
    let disabled_path = game::safe_game_path(&game_root, &mods::disabled_dll_file(&relative))?;
    if !enabled_path.starts_with(&plugins_root) || !disabled_path.starts_with(&plugins_root) {
        return Err("mods.notInPlugins".into());
    }
    let (source, target) = if enabled {
        (&disabled_path, &enabled_path)
    } else {
        (&enabled_path, &disabled_path)
    };
    let source_exists = source.is_file();
    let target_exists = target.is_file();
    match (source_exists, target_exists) {
        (false, true) => return Ok(false),
        (false, false) => {
            return Err(if enabled {
                "mods.disabledDllMissing"
            } else {
                "mods.enabledDllMissing"
            }
            .into());
        }
        (true, true) => return Err("mods.toggleTargetExists".into()),
        (true, false) => {}
    }
    archive::clear_readonly(source)?;
    fs::rename(source, target).map_err(|error| error.to_string())?;
    Ok(true)
}

pub(crate) fn toggle_local_mod_file(
    game_root: &Path,
    relative: &str,
    enabled: bool,
) -> Result<(), String> {
    set_local_mod_file_state(game_root, relative, enabled).map(|_| ())
}
