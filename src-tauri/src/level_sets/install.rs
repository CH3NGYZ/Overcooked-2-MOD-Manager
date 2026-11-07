//! 已识别地图的落盘、冲突保护与布局分流。

use super::bepinex_layer::install_bepinex_layer_sets;
use super::identity::allocate_dir_name;
use super::identity::level_set_identity_matches;
use super::info_cache::read_info_metadata;
use super::installed_versions::record_installed_mapset_versions;
use super::paths::collect_dir_names;
use super::paths::disabled_root;
use super::paths::is_safe_relative;
use super::paths::levels_root;
use super::scan::locate_info_file;
use super::zip_info::is_info_file;
use super::zip_inspect::ImportedLevelSet;
#[cfg(test)]
use super::zip_inspect::inspect_zip;
use crate::asset_bundle::LevelSetMetadata;
use crate::models::LevelSetLayout;
use std::fs;
use std::io::Read;
use std::path::Path;

/// 找到与 `dir_name` 同名的已装关卡集并读出它的身份（先 `levels/` 再 `disabled/`）。
///
/// 两个根都要看：同名目录只在 `disabled/` 时，只读 `levels/` 会得到「读不出
/// 身份」，于是「不同身份可以并存」的换名规则永远不会生效 —— 用户的另一张
/// 同名地图会被一句 `levelSet.targetExists` 挡在门外（且没有任何旧副本可卸）。
pub(super) fn existing_level_set_metadata(
    levels_dir: &Path,
    disabled_dir: &Path,
    dir_name: &str,
) -> Option<LevelSetMetadata> {
    for root in [levels_dir, disabled_dir] {
        let path = root.join(dir_name);
        if !path.is_dir() {
            continue;
        }
        // 这里只需要身份（集名/中文名/作者），随包插件与 requires 都不必读
        return locate_info_file(&path).and_then(|info| read_info_metadata(&info).ok());
    }
    None
}

/// 把选定的关卡集从 ZIP 写入 `levels/`。
///
/// 只写入 `selected` 里列出的条目。目标目录已有**同身份**关卡集时拒绝：安装
/// 前必须先卸载旧副本（`find_install_conflicts` 报告、前端确认后逐个
/// `delete_level_set`），这里不做隐式覆盖；同名但身份不同的目录会换名并存，
/// 读不出身份的目录同样拒绝，不猜「用户想删哪个」。
#[cfg(test)]
pub fn install_from_zip(
    zip_path: &Path,
    game_root: &Path,
    manifests_dir: &Path,
    selected: &[String],
) -> Result<Vec<String>, String> {
    let sets = inspect_zip(zip_path)?;
    install_parsed_sets(&sets, zip_path, game_root, manifests_dir, selected, None)
}

/// 把已经解析好的关卡集写进 `levels/`（`zip_path` 只用于回读文件内容）。
///
/// 识别与写盘拆开是为了多选导入只解析一次：`import_level_sets` 以前对每个
/// 选中的关卡集各重新整包识别一遍 —— N 张合集就是 N 倍识别时间。
pub fn install_parsed_sets(
    sets: &[ImportedLevelSet],
    zip_path: &Path,
    game_root: &Path,
    manifests_dir: &Path,
    selected: &[String],
    origin: Option<&crate::models::MapInstallOrigin>,
) -> Result<Vec<String>, String> {
    let targets: Vec<&ImportedLevelSet> = sets
        .iter()
        .filter(|set| selected.iter().any(|name| name == &set.dir_name))
        .collect();
    if targets.is_empty() {
        return Err("levelSet.nothingSelected".into());
    }

    /*
     * 表层包与历史形态是两条完全不同的落盘路径：前者按 ZIP 内路径原样解压
     * 并登记插件归属，后者拍平成 `levels/<集名>/`。混选不该出现（同一个 ZIP
     * 只有一种形态），真出现时按表层包处理会漏掉拍平语义，直接拒绝更诚实。
     */
    let bepinex_layer = targets
        .iter()
        .any(|set| set.layout == LevelSetLayout::Bepinex);
    if bepinex_layer {
        if targets
            .iter()
            .any(|set| set.layout != LevelSetLayout::Bepinex)
        {
            return Err("levelSet.nothingSelected".into());
        }
        return install_bepinex_layer_sets(
            sets,
            &targets,
            zip_path,
            game_root,
            manifests_dir,
            origin,
        );
    }

    let levels_dir = levels_root(game_root);
    let disabled_dir = disabled_root(game_root);

    /*
     * 目标目录名下已经有东西时：
     *
     * - **身份相同** → 拒绝（`levelSet.targetExists`）。安装前必须先卸载旧副本：
     *   `find_install_conflicts` 已经把它报给前端确认过，走到这里说明调用方
     *   跳过了那一步（或并发装了同一张图），不能悄悄覆盖用户的地图。
     * - **身份不同** → 换一个目录名并存。两个不同地图共用同一个英文集名与
     *   作者是真实存在的（实测「胡闹糕中」与「胡闹达学」都是 GAODA/GAODA），
     *   整批拒绝只会让用户既装不上新的、也不知道该删谁。
     * - `disabled/` 下有同名目录也算占用：那是用户停用的数据，直接覆盖会丢。
     *   所以命中 `taken` 时要**两个根都去读**，只看 `levels/` 会把「只在停用
     *   根里同名」误判成「读不出身份」，不同身份的地图也会被拒绝。
     */
    let mut resolved: Vec<(String, &ImportedLevelSet)> = Vec::new();
    let mut taken = collect_dir_names(&levels_dir);
    taken.extend(collect_dir_names(&disabled_dir));
    for set in &targets {
        let mut dir_name = set.dir_name.clone();
        if taken.contains(&dir_name.to_lowercase()) {
            let existing = existing_level_set_metadata(&levels_dir, &disabled_dir, &dir_name);
            /*
             * 读不出 info 的目录（空目录、损坏、手动塞进来的东西）不算「另一张
             * 地图」，而是「无法判断」—— 这种情况不替用户换名绕过，按原规则
             * 拒绝，让他先处理掉那个目录。换名只用于**确证身份不同**的并存。
             */
            let Some(existing_meta) = existing.as_ref() else {
                return Err(crate::locale_tag::with_params(
                    "levelSet.targetExists",
                    &[("name", &dir_name)],
                ));
            };
            let same_identity = level_set_identity_matches(
                &existing_meta.name,
                existing_meta.name_zh.as_deref(),
                &existing_meta.author,
                &set.metadata.name,
                set.metadata.name_zh.as_deref(),
                &set.metadata.author,
            );
            if same_identity {
                return Err(crate::locale_tag::with_params(
                    "levelSet.targetExists",
                    &[("name", &dir_name)],
                ));
            }
            dir_name = allocate_dir_name(&dir_name, &set.metadata, &mut taken, false);
            log::info!(
                "导入的关卡集与已有目录不同身份，改用新目录名：原目录={}，新目录={}",
                set.dir_name,
                dir_name
            );
        } else {
            taken.insert(dir_name.to_lowercase());
        }
        resolved.push((dir_name, *set));
    }

    let mut installed = Vec::new();
    for (dir_name, set) in &resolved {
        write_level_set(zip_path, game_root, dir_name, set)?;
        installed.push(dir_name.clone());
    }
    let installed_sets: Vec<&ImportedLevelSet> = resolved.iter().map(|(_, set)| *set).collect();
    record_installed_mapset_versions(manifests_dir, &installed_sets, origin);
    Ok(installed)
}

/// 把单个关卡集从 ZIP 写入 `levels/`。
fn write_level_set(
    zip_path: &Path,
    game_root: &Path,
    dir_name: &str,
    set: &ImportedLevelSet,
) -> Result<(), String> {
    let destination = levels_root(game_root).join(dir_name);
    fs::create_dir_all(&destination).map_err(|error| error.to_string())?;

    let file = fs::File::open(zip_path).map_err(|error| error.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| error.to_string())?;
    /*
     * 先建「条目名 → 索引」表，再用 `by_index` 取内容。
     *
     * `by_name` 每次都从头线性扫描中央目录；一个含 11 个文件的 ZIP 上不明显，
     * 但多关卡集（几十个文件）会明显变慢，而这是用户点一次「安装」就要等的。
     */
    let mut index_of = std::collections::HashMap::new();
    for index in 0..archive.len() {
        if let Ok(entry) = archive.by_index_raw(index) {
            index_of.insert(entry.name().replace('\\', "/"), index);
        }
    }

    for (entry_name, relative) in &set.files {
        if !is_safe_relative(relative) {
            log::warn!(
                "拒绝不安全的 ZIP 条目：条目={}，相对路径={}",
                entry_name,
                relative
            );
            // 已创建的目录要清掉，避免留下半个关卡集
            let _ = fs::remove_dir_all(&destination);
            return Err("archive.pathEscape".into());
        }
        /*
         * `info_<集名>` 落盘成固定名 `info`：DLL 只认这个。`s_*` 原样保留 ——
         * 游戏按 `sceneName` 找场景，改名就找不到。
         */
        let disk_name = if is_info_file(relative) {
            "info".to_string()
        } else {
            relative.clone()
        };
        let target = destination.join(&disk_name);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let Some(index) = index_of.get(entry_name) else {
            let _ = fs::remove_dir_all(&destination);
            return Err(crate::locale_tag::with_params(
                "levelSet.zipEntryMissing",
                &[("entry", entry_name)],
            ));
        };
        let mut entry = archive
            .by_index(*index)
            .map_err(|error| error.to_string())?;
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        fs::write(&target, &bytes).map_err(|error| {
            crate::locale_tag::with_params(
                "levelSet.writeFailed",
                &[("reason", &error.to_string())],
            )
        })?;
    }
    Ok(())
}
