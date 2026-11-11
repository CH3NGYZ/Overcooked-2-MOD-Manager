//! 安装前同身份冲突报告，不执行卸载。

use super::identity::level_set_display_name;
use super::identity::same_level_set;
use super::paths::LevelSetRoots;
use super::scan::LevelSetOnDisk;
use super::scan::scan_level_sets_inner;
use super::zip_inspect::ImportedLevelSet;
use crate::models::InstallConflict;
use std::collections::HashSet;

/// 已安装的关卡集（启用根 + 停用根各扫一遍），供安装前的身份冲突判定用。
///
/// 与地图页扫描的差别是不要进度、解析不出
/// `info` 的目录直接跳过（读不出身份就无法判定冲突，交给 `install_from_zip`
/// 按目录名规则处理）。目录名排序，冲突行顺序才稳定。
fn installed_level_sets(roots: &LevelSetRoots, workers: usize) -> Vec<LevelSetOnDisk> {
    // 冲突判定只看身份（info），不需要随包插件，所以不传登记表
    scan_level_sets_inner(roots, None, workers, &|_, _, _| {})
        .into_iter()
        .filter(|set| set.metadata.is_some())
        .collect()
}

/// 同上，按调用方解析好的并行度读已装副本的 `info`。
#[cfg(test)]
mod test_support;
#[cfg(test)]
pub use test_support::find_install_conflicts;

pub fn find_install_conflicts_with_workers(
    roots: &LevelSetRoots,
    sets: &[ImportedLevelSet],
    workers: usize,
) -> Vec<InstallConflict> {
    conflicts_with_installed(&installed_level_sets(roots, workers), sets)
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
