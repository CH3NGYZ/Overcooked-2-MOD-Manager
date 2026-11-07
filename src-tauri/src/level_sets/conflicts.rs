//! 安装前同身份冲突报告，不执行卸载。

use super::identity::level_set_display_name;
use super::identity::same_level_set;
use super::scan::LevelSetOnDisk;
use super::scan::scan_level_sets_inner;
use super::zip_inspect::ImportedLevelSet;
use crate::models::InstallConflict;
use std::collections::HashSet;
use std::path::Path;

/// 已安装的关卡集（启用根 + 停用根各扫一遍），供安装前的身份冲突判定用。
///
/// 与 [`scan_level_sets`] 的差别只有三点：不迁移旧停用根、不要进度、解析不出
/// `info` 的目录直接跳过（读不出身份就无法判定冲突，交给 `install_from_zip`
/// 按目录名规则处理）。目录名排序，冲突行顺序才稳定。
fn installed_level_sets(game_root: &Path, workers: usize) -> Vec<LevelSetOnDisk> {
    // 冲突判定只看身份（info），不需要随包插件，所以不传登记表
    scan_level_sets_inner(game_root, None, workers, false, &|_, _, _| {})
        .into_iter()
        .filter(|set| set.metadata.is_some())
        .collect()
}

/// 找出本次要导入的关卡集与本地已装副本的身份冲突。
///
/// 只报告、不卸载：地图的卸载要走 [`delete_level_set`]（带回收站记录），由前端
/// 用户确认后逐个调，安装事务里不掺卸载。同一张图不管版本相同、升级还是降级
/// 都算同一个东西；已有重复副本（启用 + 停用各一份）全部列出。
///
/// `sets` 是同一批（同一个 ZIP / 同一个 Release）里识别出的关卡集。目录名撞车
/// 但身份不同不算冲突 —— 那是允许并存的另一张图，`install_from_zip` 会为它换名。
#[cfg(test)]
pub fn find_install_conflicts(game_root: &Path, sets: &[ImportedLevelSet]) -> Vec<InstallConflict> {
    find_install_conflicts_with_workers(
        game_root,
        sets,
        crate::metadata_read::resolve_concurrency(
            crate::metadata_read::DEFAULT_MAP_INFO_CONCURRENCY,
            crate::metadata_read::cores(),
        ),
    )
}

/// 同上，按调用方解析好的并行度读已装副本的 `info`。
pub fn find_install_conflicts_with_workers(
    game_root: &Path,
    sets: &[ImportedLevelSet],
    workers: usize,
) -> Vec<InstallConflict> {
    conflicts_with_installed(&installed_level_sets(game_root, workers), sets)
}

/// 纯函数部分：已装列表 × 待装列表 → 冲突行。
///
/// 与文件系统无关，单测直接构造两边即可（真实 `info` 很难为每个分支凑齐）。
pub(super) fn conflicts_with_installed(
    installed: &[LevelSetOnDisk],
    sets: &[ImportedLevelSet],
) -> Vec<InstallConflict> {
    let mut reported: HashSet<String> = HashSet::new();
    let mut conflicts = Vec::new();
    for set in sets {
        for candidate in installed {
            let Some(metadata) = candidate.metadata.as_ref() else {
                continue;
            };
            // 身份匹配要双向试：中文名只有一侧有时，另一侧的比较会判不中
            if !same_level_set(metadata, &set.metadata) && !same_level_set(&set.metadata, metadata)
            {
                continue;
            }
            // 同一份已装副本只报一条：一个 ZIP 里有两张同身份的图时不该显示两遍
            if !reported.insert(candidate.dir_name.to_lowercase()) {
                continue;
            }
            conflicts.push(InstallConflict {
                name: level_set_display_name(metadata),
                version: metadata.version.trim().to_string(),
                handle: candidate.dir_name.clone(),
                enabled: candidate.enabled,
                same_version: metadata.version.trim() == set.metadata.version.trim(),
            });
        }
    }
    conflicts
}
