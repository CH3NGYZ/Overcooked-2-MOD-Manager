use crate::models::AppConfig;
use crate::models::MapRepositoryConfig;
use std::collections::BTreeMap;

/// 用新加载的地图列表替换配置里的地图，**保留同 ID 条目的 Release 缓存**。
///
/// 每次刷新都是「源文档 → 全新条目」，直接替换会让 5 分钟缓存失效，
/// 于是每次刷新都把整批地图的 API 重打一遍（MOD 侧踩过完全相同的坑）。
///
/// 只保留缓存与失败原因，其余字段以源文档为准：
///
/// - `enabled` 必须听文档的 —— 它是作者的声明（替换或下线某张图）。管理器
///   目前没有「用户手动隐藏地图」这个操作，保留旧值只会让作者下线的图复活。
/// - 名字、资产规则、依赖版本改了就该立刻生效。
///
/// 返回是否有变化，供调用方决定要不要写盘。**「变化」必须包括只改展示字段的
/// 情况**（分组、名字、备注、资产规则……）：ID 由「源 ID + author +
/// level_set_name」派生，作者把分组从 `RW` 改成作者名时 ID 不变 —— 早先这里
/// 只比 ID 和长度，于是线上改了分组，配置里永远是旧值，刷新多少次都不生效。
/// 比较走序列化结果，以后加字段也不会漏掉。
pub(crate) fn replace_maps_keeping_runtime_state(
    config: &mut AppConfig,
    fresh: Vec<MapRepositoryConfig>,
) -> bool {
    let (merged, changed) =
        merge_maps_keeping_runtime_state(std::mem::take(&mut config.map_repositories), fresh);
    config.map_repositories = merged;
    changed
}

/// `replace_maps_keeping_runtime_state` 的纯逻辑部分：不碰配置，便于直接测。
pub(super) fn merge_maps_keeping_runtime_state(
    previous: Vec<MapRepositoryConfig>,
    fresh: Vec<MapRepositoryConfig>,
) -> (Vec<MapRepositoryConfig>, bool) {
    let mut merged = Vec::with_capacity(fresh.len());
    for mut map in fresh {
        if map.releases_cached_at.is_none()
            && map.last_error.is_none()
            && let Some(old) = previous.iter().find(|old| old.id == map.id)
            && same_release_binding(old, &map)
        {
            map.cached_releases = old.cached_releases.clone();
            map.releases_cached_at = old.releases_cached_at;
            map.last_error = old.last_error.clone();
        }
        merged.push(map);
    }
    // 运行态字段刚刚从旧条目接回，所以整份比较等价于「声明字段有没有变」
    let changed = match (
        serde_json::to_string(&previous),
        serde_json::to_string(&merged),
    ) {
        (Ok(old), Ok(new)) => old != new,
        // 序列化失败时保守地算作有变化：宁可多写一次配置，也不能让作者改过的
        // 声明被静默丢掉
        _ => true,
    };
    (merged, changed)
}

/// Stable map IDs describe installation identity, not a hosting repository.
/// A source migration or projection rule change must invalidate cached assets.
pub(crate) fn same_release_binding(
    left: &MapRepositoryConfig,
    right: &MapRepositoryConfig,
) -> bool {
    left.release_derived == right.release_derived
        && left.provider == right.provider
        && left
            .username
            .trim()
            .eq_ignore_ascii_case(right.username.trim())
        && left
            .repository
            .trim()
            .eq_ignore_ascii_case(right.repository.trim())
        && left.channel == right.channel
        && super::identity::map_tag(left) == super::identity::map_tag(right)
        && left.max_release_count == right.max_release_count
        && left.asset_versions == right.asset_versions
        && left.latest_mapset_version == right.latest_mapset_version
        && left.latest_info_version == right.latest_info_version
        && left.asset_rules.len() == right.asset_rules.len()
        && left
            .asset_rules
            .iter()
            .zip(&right.asset_rules)
            .all(|(a, b)| a.rule_type == b.rule_type && a.value == b.value)
}

/// 一个源在界面上的身份：ID + 昵称（单语言镜像与多语言表）。
///
/// 地图源树不落盘，所以这三样必须跟着每个地图条目走 —— 见
/// `MapRepositoryConfig::source_name`。一份结构而不是三个散参数，是因为
/// 它们总是一起出现（`attach` 一次写全），拆开容易漏写其中一个。
#[derive(Debug, Clone, Default)]
pub(super) struct SourceLabel {
    pub(super) network_plan: crate::models::NetworkPlanSelection,
    pub(super) id: String,
    pub(super) name: String,
    pub(super) name_i18n: BTreeMap<String, String>,
    pub(super) note_i18n: BTreeMap<String, String>,
    pub(super) regions: Vec<String>,
}

impl SourceLabel {
    pub(super) fn child(&self, declaration: &super::document::MapSourceDeclaration) -> Self {
        let mut regions = self.regions.clone();
        let region = crate::config::normalize_repository_region(declaration.region.as_deref());
        if !region.is_empty() && !regions.contains(&region) {
            regions.push(region);
        }
        Self {
            network_plan: declaration.network_plan.clone(),
            id: declaration.id.trim().into(),
            name: declaration.nickname.clone(),
            name_i18n: declaration.nickname_i18n.clone(),
            note_i18n: declaration.note_i18n.clone(),
            regions,
        }
    }

    pub(super) fn matches_region(&self, region: &str) -> bool {
        self.regions
            .iter()
            .all(|value| crate::config::region_matches(Some(value), region))
    }

    /// 把源归属与源名写进条目。
    pub(super) fn attach(&self, map: &mut MapRepositoryConfig) {
        map.network_plan = map.network_plan.inherit(&self.network_plan);
        map.source_id = Some(self.id.clone());
        map.source_name = self.name.clone();
        map.source_name_i18n = self.name_i18n.clone();
        map.source_note_i18n = self.note_i18n.clone();
        map.source_regions = Some(self.regions.clone());
    }
}

/// 当前地区可用的地图（启用 + 地区匹配）。
///
/// 与仓库侧 `mod_repository_is_available` 同一条语义：`enabled=false` 与
/// 「地区不适用」都表示「当前用户看不见这张地图」，前端不该为它发请求。
pub fn map_is_available(map: &MapRepositoryConfig, current_region: &str) -> bool {
    if !map.enabled
        || map
            .source_id
            .as_deref()
            .is_some_and(crate::community::is_source)
    {
        return false;
    }
    crate::config::region_matches(map.region.as_deref(), current_region)
        && map
            .source_regions
            .as_deref()
            .unwrap_or_default()
            .iter()
            .all(|region| crate::config::region_matches(Some(region), current_region))
}

/// 从配置里取出当前地区可用的全部地图。
pub fn available_maps(config: &AppConfig, current_region: &str) -> Vec<MapRepositoryConfig> {
    config
        .map_repositories
        .iter()
        .filter(|map| map_is_available(map, current_region))
        .cloned()
        .collect()
}
