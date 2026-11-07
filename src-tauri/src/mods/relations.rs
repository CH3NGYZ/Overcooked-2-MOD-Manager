use crate::models::MapRepositoryConfig;
use crate::models::ReleaseRelationOverride;
use crate::models::RepositoryConfig;
use crate::models::RepositoryRelation;

/// 从覆盖表里挑出对指定版本生效的那一张；没有命中返回 `None`。
///
/// 与前端 `modRelations.ts::relationsForRelease` 同语义：所有命中的区间里取
/// 区间下限最高（下限为空视为最低优先级）的那条，下限相同时取先出现的一条。
///
/// 为什么不合并：合并会让「新版降低要求」无法表达 —— 覆盖表的意图是
/// 「这个版本区间用的是另一套依赖」，作者改一张表就得到一套完整语义。
pub(crate) fn select_release_relations<'a>(
    overrides: &'a [ReleaseRelationOverride],
    release_version: Option<&str>,
) -> Option<&'a [RepositoryRelation]> {
    let current = release_version
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    if overrides.is_empty() {
        return None;
    }
    let compare = crate::releases::compare_release_versions;
    let mut best: Option<(&ReleaseRelationOverride, Option<String>)> = None;
    for entry in overrides {
        let Ok(range) = crate::version_range::VersionRange::parse(entry.version.trim()) else {
            // 校验阶段已经拦过；走到这里说明配置是内存里拼的，按不命中处理。
            log::warn!("忽略无法解析的版本区间：区间={}", entry.version);
            continue;
        };
        /*
         * 区间比较不出来的版本（tag 不是可解析版本号）一律不命中：用一套
         * 说不清依据的关系去固化依赖，比退回仓库级声明更危险。
         */
        if !range.matches(current, compare) {
            continue;
        }
        let lower = range.lower_bound(compare).map(str::to_string);
        let more_specific = match best.as_ref() {
            None => true,
            // 下限为空能匹配一切，不该压过更具体的规则
            Some((_, best_lower)) => match (best_lower.as_deref(), lower.as_deref()) {
                (None, Some(_)) => true,
                (Some(_), None) | (None, None) => false,
                (Some(best_min), Some(candidate)) => {
                    compare(candidate, best_min).is_some_and(|ordering| ordering.is_gt())
                }
            },
        };
        if more_specific {
            best = Some((entry, lower));
        }
    }
    best.map(|(entry, _)| entry.relations.as_slice())
}

/// 仓库对指定 Release 版本**生效**的关系表（`release_relations` 命中即整体替换）。
///
/// 版本为空、没有覆盖表、或没有命中时都用仓库级 `relations`。
pub fn effective_relations(
    repo: &RepositoryConfig,
    release_version: Option<&str>,
) -> Vec<RepositoryRelation> {
    match select_release_relations(&repo.release_relations, release_version) {
        Some(relations) => relations.to_vec(),
        None => repo.relations.clone(),
    }
}

/// 地图源对指定地图版本**生效**的关系表（`[[maps.release_relations]]`）。
///
/// 与 MOD 仓库同一条规则：同一个地图集的大版本可能依赖不同版本的 DIYLevel
/// 或 Web Runtime，一张固定表表达不了，命中的那张整体替换 `map.relations`。
pub fn effective_map_relations(
    map: &MapRepositoryConfig,
    release_version: Option<&str>,
) -> Vec<RepositoryRelation> {
    match select_release_relations(&map.release_relations, release_version) {
        Some(relations) => relations.to_vec(),
        None => map.relations.clone(),
    }
}
