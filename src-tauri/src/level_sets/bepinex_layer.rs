//! BepInEx 表层布局识别、选择性安装与回滚。

use super::installed_versions::record_installed_mapset_versions;
use super::paths::LevelSetRoots;
use super::paths::disabled_root;
use super::paths::is_safe_dir_name;
use super::paths::is_safe_relative;
use super::paths::levels_root;
use super::plugin_assignment::assign_plugins;
use super::plugin_registry::LevelSetPluginOwner;
use super::plugin_registry::RegisteredLevelSetPlugin;
use super::plugin_registry::merge_registered_plugins;
use super::plugin_registry::read_registered_plugins;
use super::plugin_registry::write_registered_plugins;
use super::zip_info::InfoDirectory;
use super::zip_info::is_junk_entry;
use super::zip_info::parse_zip_info_directories;
use super::zip_inspect::ImportedLevelSet;
use super::zip_plugins::PreparedZipPlugin;
use super::zip_plugins::fallback_plugin_info;
use super::zip_plugins::prepare_zip_plugin;
use crate::asset_bundle::LevelSetMetadata;
use crate::metadata_read::MetadataKind;
use crate::metadata_read::MetadataRequest;
use crate::models::DllPluginInfo;
use crate::models::LevelSetLayout;
use crate::models::ZipLevelSetPlugin;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;

/// ZIP 内 `BepInEx/plugins/` 前缀（小写比较；条目名已归一化成 `/`）。
const BEPINEX_PLUGINS_PREFIX: &str = "bepinex/plugins/";

/// ZIP 内 DIYLevel 的 `levels/` 根前缀（小写比较）。
const BEPINEX_LEVELS_PREFIX: &str = "bepinex/plugins/oc2diylevel/levels/";

pub(super) fn is_bepinex_layer_entry(name: &str) -> bool {
    name.to_lowercase().starts_with(BEPINEX_PLUGINS_PREFIX)
}

/// ZIP 是否是 BepInEx 表层包：存在 `BepInEx/plugins/` 前缀的条目。
///
/// 大小写不敏感（作者在 Windows 上打包，`BepInEx` / `bepinex` 都可能写出来），
/// 条目名在上层已归一化成正斜杠并剔掉 `__MACOSX/` 垃圾。
pub(super) fn is_bepinex_layer_zip(entries: &[String]) -> bool {
    entries.iter().any(|entry| is_bepinex_layer_entry(entry))
}

/// `levels/` 根下的直接子目录名（原始大小写）；不是直接子目录时返回 `None`。
///
/// 只认直接子目录：更深层的东西（作者塞在集目录里的子目录、备份目录）按原样
/// 解压，不单独成卡 —— 一张卡必须对应一个 `info`。
pub(super) fn bepinex_level_set_dir_name(directory: &str) -> Option<&str> {
    let lower = directory.to_lowercase();
    let rest = lower.strip_prefix(BEPINEX_LEVELS_PREFIX)?;
    if rest.is_empty() || rest.contains('/') {
        return None;
    }
    directory.rsplit('/').next()
}

/// 识别一个 BepInEx 表层包：`levels/` 根的直接子目录是关卡集，其余
/// `BepInEx/plugins/**` 的 DLL 是随包插件。
pub(super) fn inspect_bepinex_layer(
    zip_path: &Path,
    all_entries: &[String],
    by_directory: &BTreeMap<String, Vec<String>>,
    index_by_name: &HashMap<String, usize>,
    info_candidates: BTreeMap<String, Vec<(usize, String)>>,
    info_workers: usize,
    plugin_workers: usize,
    progress: &(dyn Fn(usize, usize, &str) + Sync),
) -> Result<Vec<ImportedLevelSet>, String> {
    // 同名（仅大小写不同）目录只取第一个：Windows 上它们是同一个目录
    let mut set_directories: BTreeMap<String, (String, Vec<(usize, String)>)> = BTreeMap::new();
    for (directory, candidates) in info_candidates {
        if bepinex_level_set_dir_name(&directory).is_none() {
            continue;
        }
        set_directories
            .entry(directory.to_lowercase())
            .or_insert((directory, candidates));
    }
    if set_directories.is_empty() {
        return Err("levelSet.zipWithoutInfo".into());
    }
    let info_tasks: Vec<InfoDirectory> = set_directories
        .into_values()
        .map(|(directory, candidates)| InfoDirectory {
            directory,
            candidates,
        })
        .collect();
    /*
     * 表层包里 `levels/<目录>` 是作者规划好的位置，杂项文件（备份 info、说明）
     * 不该让整包识别失败，所以用宽松模式：解析不出来的目录跳过。
     */
    let parsed_metadata =
        parse_zip_info_directories(zip_path, &info_tasks, info_workers, false, progress)?;

    // 目录名用 ZIP 里的真实目录名：表层包按原样解压，不替作者改名
    let mut parsed_sets: Vec<(String, LevelSetMetadata, String)> = Vec::new();
    for (directory, metadata) in parsed_metadata {
        let Some(dir_name) = directory.rsplit('/').next().map(str::to_string) else {
            continue;
        };
        parsed_sets.push((directory, metadata, dir_name));
    }
    if parsed_sets.is_empty() {
        return Err("levelSet.zipWithoutInfo".into());
    }

    /*
     * 随包插件 = `BepInEx/plugins/` 下、不在 `levels/` 根里的 DLL。
     *
     * 归属复用历史形态的两级规则（`assign_plugins`）：表层包的目录名与集名
     * 通常对得上，按名字打分能挑出正确的集；挑不出来时归排序第一张，结果确定。
     */
    let plugin_entries: Vec<String> = all_entries
        .iter()
        .filter(|entry| entry.to_lowercase().ends_with(".dll"))
        .filter(|entry| {
            let lower = entry.to_lowercase();
            is_bepinex_layer_entry(&lower) && !lower.starts_with(BEPINEX_LEVELS_PREFIX)
        })
        .cloned()
        .collect();
    let plugin_owner = assign_plugins(&plugin_entries, &parsed_sets);

    let mut scratch = (!plugin_owner.is_empty())
        .then(|| std::env::temp_dir().join(format!("oc2-level-plugins-{}", uuid::Uuid::new_v4())));
    if let Some(dir) = &scratch {
        if let Err(error) = fs::create_dir_all(dir) {
            log::warn!(
                "随包插件元数据：临时目录创建失败，降级为文件名兜底：目录={}，原因={}",
                dir.display(),
                error
            );
            scratch = None;
        }
    }
    let mut prepared_plugins: BTreeMap<String, Vec<PreparedZipPlugin>> = BTreeMap::new();
    {
        let file = fs::File::open(zip_path).map_err(|error| error.to_string())?;
        let mut archive = zip::ZipArchive::new(file).map_err(|error| {
            crate::locale_tag::with_params(
                "archive.zipReadFailed",
                &[("reason", &error.to_string())],
            )
        })?;
        for (directory, entries) in &plugin_owner {
            let prepared = entries
                .iter()
                .map(|entry| {
                    prepare_zip_plugin(
                        &mut archive,
                        index_by_name.get(entry).copied(),
                        entry,
                        scratch.as_deref(),
                    )
                })
                .collect::<Vec<_>>();
            prepared_plugins.insert(directory.clone(), prepared);
        }
    }
    let mut requests: Vec<MetadataRequest> = Vec::new();
    for prepared in prepared_plugins.values_mut() {
        for plugin in prepared.iter_mut() {
            let Some(temp) = plugin.temp.as_ref() else {
                continue;
            };
            plugin.request = Some(requests.len());
            requests.push(MetadataRequest {
                path: temp.clone(),
                display_name: plugin.file_name.clone(),
                kind: MetadataKind::ModDll,
            });
        }
    }
    let outcomes = crate::metadata_read::read_metadata_batch(&requests, plugin_workers, &|_, _| {});

    let mut imported = Vec::with_capacity(parsed_sets.len());
    for (directory, metadata, dir_name) in parsed_sets {
        let prefix = format!("{directory}/");
        let files: Vec<(String, String)> = by_directory
            .get(&directory)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|name| {
                let relative = name
                    .strip_prefix(&prefix)
                    .map(str::to_string)
                    .unwrap_or_else(|| name.clone());
                (name, relative)
            })
            .collect();
        let prepared = prepared_plugins.remove(&directory).unwrap_or_default();
        let plugins = prepared
            .into_iter()
            .map(|plugin| {
                let info = plugin
                    .request
                    .and_then(|index| outcomes.get(index))
                    .and_then(|outcome| outcome.mod_dll())
                    .cloned()
                    .map(DllPluginInfo::from)
                    .unwrap_or_else(|| fallback_plugin_info(&plugin.file_name));
                ZipLevelSetPlugin {
                    // 表层包的插件不搬位置：落盘路径就是 ZIP 内条目路径
                    install_path: Some(plugin.path.clone()),
                    path: plugin.path,
                    file_name: plugin.file_name,
                    info,
                }
            })
            .collect();
        imported.push(ImportedLevelSet {
            dir_name,
            layout: LevelSetLayout::Bepinex,
            metadata,
            files,
            plugins,
        });
    }
    if let Some(dir) = &scratch {
        let _ = fs::remove_dir_all(dir);
    }
    log::info!(
        "BepInEx 表层包识别完成：文件={}，关卡集={}，随包插件={}",
        zip_path.display(),
        imported.len(),
        imported.iter().map(|set| set.plugins.len()).sum::<usize>()
    );
    Ok(imported)
}

/// 覆盖已有文件时最多回滚多少字节：地图包不该带超大文件，超过就拒绝覆盖，
/// 免得为了回滚把几百 MB 读进内存。
const MAX_OVERWRITE_ROLLBACK_BYTES: u64 = 16 * 1024 * 1024;

/// 地图写入选中 DIYLevel 的 levels，随包插件保留 ZIP 内位置并登记归属。
///
/// 与历史形态的差异：
/// - 不拍平、不改集目录名：ZIP 的历史 `OC2DIYLevel/levels/` 子树重定向到
///   当前最高版本 DLL 的兄弟 levels，避免装进已经不再使用的旧目录。
/// - 插件不搬进集目录：它们留在 `BepInEx/plugins/**`，绑定关系写进登记表，
///   MOD 扫描据此避让（见 `mods::scan_mods_with_manifests`）。
///
/// 解压范围按选择收窄：`levels/` 下未选中的集目录与属于未选中集的插件都不
/// 落盘 —— 否则用户只勾了一张图，另一张也进了游戏目录，返回的 `installed`
/// 还与界面选择对不上。
pub(super) fn install_bepinex_layer_sets(
    all_sets: &[ImportedLevelSet],
    targets: &[&ImportedLevelSet],
    zip_path: &Path,
    roots: &LevelSetRoots,
    manifests_dir: &Path,
    origin: Option<&crate::models::MapInstallOrigin>,
) -> Result<Vec<String>, String> {
    let game_root = roots.game_root();
    let levels_dir = levels_root(roots)?;
    let disabled_dir = roots.safe_path(&disabled_root(game_root))?;
    for set in targets {
        if !is_safe_dir_name(&set.dir_name) {
            return Err("levelSet.invalidName".into());
        }
        /*
         * 表层包不换名：目录名是作者写死在 ZIP 里的。已存在就拒绝，先让用户
         * 卸载旧副本（同身份冲突已由 `find_install_conflicts` 报给前端）。
         */
        if levels_dir.join(&set.dir_name).exists() || disabled_dir.join(&set.dir_name).exists() {
            return Err(crate::locale_tag::with_params(
                "levelSet.targetExists",
                &[("name", &set.dir_name)],
            ));
        }
    }

    let selected_dirs: HashSet<String> = targets
        .iter()
        .map(|set| set.dir_name.to_lowercase())
        .collect();
    // 全部集的插件归属（含未选中的）：解压时据此跳过未选中集的插件
    let mut owned_plugins: HashMap<String, bool> = HashMap::new();
    for set in all_sets {
        let selected = selected_dirs.contains(&set.dir_name.to_lowercase());
        for plugin in &set.plugins {
            let Some(path) = plugin.install_path.as_ref() else {
                continue;
            };
            owned_plugins.insert(path.to_lowercase(), selected);
        }
    }

    /*
     * 先写登记表再解压：解压中途失败时 DLL 已经落盘却没有登记，它们会出现在
     * MOD 列表里（用户能单独禁用一张地图的插件）。反过来先登记、失败再回滚
     * 登记表，最坏只是表里多几条指向不存在文件的记录 —— 扫描会跳过它们。
     */
    let previous = read_registered_plugins(manifests_dir);
    let mut added = Vec::new();
    for set in targets {
        let owner = LevelSetPluginOwner::from_metadata(&set.metadata);
        for plugin in &set.plugins {
            let Some(dll) = plugin.install_path.as_ref() else {
                continue;
            };
            added.push(RegisteredLevelSetPlugin {
                dll: dll.clone(),
                owner: owner.clone(),
            });
        }
    }
    let added_count = added.len();
    if added_count > 0 {
        crate::game::ensure_game_stopped(game_root)?;
    }
    write_registered_plugins(
        manifests_dir,
        &merge_registered_plugins(previous.clone(), added),
    )?;

    if let Err(reason) = extract_bepinex_layer(zip_path, roots, &selected_dirs, &owned_plugins) {
        if let Err(error) = write_registered_plugins(manifests_dir, &previous) {
            log::warn!(
                "随包插件登记表回滚失败，可能留下指向不存在文件的记录：原因={}",
                error
            );
        }
        return Err(reason);
    }
    log::info!(
        "BepInEx 表层包导入完成：文件={}，关卡集={}，登记插件={}",
        zip_path.display(),
        targets.len(),
        added_count
    );
    record_installed_mapset_versions(manifests_dir, targets, origin);
    Ok(targets.iter().map(|set| set.dir_name.clone()).collect())
}

/// 解压 BepInEx 表层包（`levels/` 下只落选中的集，插件按归属过滤）。
///
/// 失败时把本次创建的文件删掉、覆盖的文件写回原字节：安装要么完整、要么
/// 什么都没发生，留下半个 BepInEx 目录比报错更糟。
fn extract_bepinex_layer(
    zip_path: &Path,
    roots: &LevelSetRoots,
    selected_dirs: &HashSet<String>,
    owned_plugins: &HashMap<String, bool>,
) -> Result<(), String> {
    let file = fs::File::open(zip_path).map_err(|error| error.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| {
        crate::locale_tag::with_params("archive.zipReadFailed", &[("reason", &error.to_string())])
    })?;
    let mut created: Vec<PathBuf> = Vec::new();
    let mut created_dirs: Vec<PathBuf> = Vec::new();
    let mut overwritten: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    let result = extract_bepinex_layer_entries(
        &mut archive,
        roots,
        selected_dirs,
        owned_plugins,
        &mut created,
        &mut created_dirs,
        &mut overwritten,
    );
    if let Err(reason) = result {
        for path in created.iter().rev() {
            let _ = fs::remove_file(path);
        }
        /*
         * 只删本次新建的目录：`BepInEx/plugins` 这类原有目录即使被搬空也
         * 不能动 —— 回滚的目标是「恢复原样」，不是清理游戏目录。
         */
        for directory in created_dirs.iter().rev() {
            let _ = fs::remove_dir(directory);
        }
        for (path, bytes) in overwritten.iter().rev() {
            if let Err(error) = fs::write(path, bytes) {
                log::warn!(
                    "解压回滚失败，文件保持新内容：文件={}，原因={}",
                    path.display(),
                    error
                );
            }
        }
        log::warn!(
            "BepInEx 表层包解压失败，已回滚：文件={}，原因={}",
            zip_path.display(),
            reason
        );
        return Err(reason);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn extract_bepinex_layer_entries(
    archive: &mut zip::ZipArchive<fs::File>,
    roots: &LevelSetRoots,
    selected_dirs: &HashSet<String>,
    owned_plugins: &HashMap<String, bool>,
    created: &mut Vec<PathBuf>,
    created_dirs: &mut Vec<PathBuf>,
    overwritten: &mut Vec<(PathBuf, Vec<u8>)>,
) -> Result<(), String> {
    let game_root = roots.game_root();
    let levels_dir = levels_root(roots)?;
    let mut targets = HashSet::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| {
            crate::locale_tag::with_params(
                "archive.zipEntryReadFailed",
                &[("reason", &error.to_string())],
            )
        })?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().replace('\\', "/");
        if is_junk_entry(&name) {
            continue;
        }
        if !is_safe_relative(&name) {
            log::warn!("拒绝不安全的 ZIP 条目：条目={}", name);
            return Err("archive.pathEscape".into());
        }
        let lower = name.to_lowercase();
        if lower.starts_with(BEPINEX_LEVELS_PREFIX) {
            let rest = &lower[BEPINEX_LEVELS_PREFIX.len()..];
            let first = rest.split('/').next().unwrap_or_default();
            if !selected_dirs.contains(first) {
                continue;
            }
        }
        if lower.ends_with(".dll") && owned_plugins.get(&lower) == Some(&false) {
            continue;
        }
        // Layer archives can also contain framework/config files or DLLs
        // without parsed plugin metadata. Guard the actual write scope.
        if !lower.starts_with(BEPINEX_LEVELS_PREFIX) || lower.ends_with(".dll") {
            crate::game::ensure_game_stopped(game_root)?;
        }
        let target = if lower.starts_with(BEPINEX_LEVELS_PREFIX) {
            roots.safe_path(&levels_dir.join(&name[BEPINEX_LEVELS_PREFIX.len()..]))?
        } else {
            roots.safe_path(&game_root.join(&name))?
        };
        // 重定向后也不能让不同 ZIP 条目写到同一个目标。
        let target_key = target.to_string_lossy().to_lowercase();
        if !targets.insert(target_key) {
            return Err("archive.pathEscape".into());
        }
        if let Some(parent) = target.parent() {
            /*
             * 记下本次新建的目录链：失败回滚时只删这些，原有目录（哪怕是空的
             * `BepInEx/plugins`）保持原样。
             */
            let mut missing: Vec<PathBuf> = Vec::new();
            let mut cursor = Some(parent);
            while let Some(directory) = cursor {
                if directory.exists() {
                    break;
                }
                missing.push(directory.to_path_buf());
                cursor = directory.parent();
            }
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            created_dirs.extend(missing.into_iter().rev());
        }
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if target.exists() {
            let metadata = fs::metadata(&target).map_err(|error| error.to_string())?;
            if metadata.len() > MAX_OVERWRITE_ROLLBACK_BYTES {
                return Err(crate::locale_tag::with_params(
                    "levelSet.targetExists",
                    &[("name", &name)],
                ));
            }
            let previous = fs::read(&target).map_err(|error| error.to_string())?;
            overwritten.push((target.clone(), previous));
        } else {
            created.push(target.clone());
        }
        fs::write(&target, &bytes).map_err(|error| {
            crate::locale_tag::with_params(
                "levelSet.writeFailed",
                &[("reason", &error.to_string())],
            )
        })?;
    }
    Ok(())
}
