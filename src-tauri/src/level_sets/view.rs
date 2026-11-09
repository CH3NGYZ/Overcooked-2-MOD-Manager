use crate::display_path::display_path;
use crate::models::{AppConfig, LevelSetInfo, MapRelationSnapshots};
use std::collections::BTreeMap;

/// 把内部扫描结果转成 IPC 模型。
pub(crate) fn level_set_to_info(set: crate::level_sets::LevelSetOnDisk) -> LevelSetInfo {
    let metadata = set.metadata;
    /*
     * `requires.txt` 是本地安装包自带的外部 runtime 要求（纯版本号）。
     * 先在这里填成兜底依赖：`bind_map_relation_snapshot` 拿到线上声明时会覆盖它
     * —— 线上优先，只有线上一条都没有时才用这份。没有 `requires.txt` 就保持
     * 为空，界面也就不会显示关系卡片。
     */
    let fallback_relations: Vec<_> = set
        .required_runtime
        .as_deref()
        .and_then(crate::level_sets::runtime_relation)
        .into_iter()
        .collect();
    LevelSetInfo {
        dir_name: set.dir_name,
        enabled: set.enabled,
        path: display_path(&set.path),
        name: metadata
            .as_ref()
            .map(|meta| meta.name.clone())
            .unwrap_or_default(),
        name_zh: metadata.as_ref().and_then(|meta| meta.name_zh.clone()),
        author: metadata
            .as_ref()
            .map(|meta| meta.author.clone())
            .unwrap_or_default(),
        version: metadata
            .as_ref()
            .map(|meta| meta.version.clone())
            .unwrap_or_default(),
        uid: metadata
            .as_ref()
            .map(|meta| meta.uid.clone())
            .unwrap_or_default(),
        base_uid: metadata.as_ref().and_then(|meta| meta.base_uid.clone()),
        level_count: metadata.as_ref().map(|meta| meta.levels.len()).unwrap_or(0),
        levels: metadata.map(|meta| meta.levels).unwrap_or_default(),
        error: set.error,
        // 绑定、依赖快照与资产版本都由 `bind_map_relation_snapshot` 补：它需要
        // 配置与快照，这里只负责把磁盘扫描结果转成模型
        map_id: None,
        installed_mapset_version: String::new(),
        // 分组与依赖一样由 `bind_map_relation_snapshot` 补：它需要配置与快照，
        // 这里只负责把磁盘扫描结果转成模型；都查不到时前端回退 info 的 author
        map_group_i18n: BTreeMap::new(),
        map_group: None,
        relations: fallback_relations,
        // 随包插件就是集目录里的 DLL（扫描时读回），与绑定/依赖无关
        plugins: set.plugins,
    }
}

/// 给关卡集补上「绑定的地图仓库 id」、分组与依赖快照。
///
/// 身份匹配走 `level_sets::level_set_identity_matches`（`levelSetName +
/// levelSetNameZH + author`，忽略大小写与首尾空白，空名不匹配；地图源没写
/// 中文名时退回前两段）；分组取「绑定地图的当前声明 → 快照」，都查不到就留空
/// 交给前端回退 info 里的作者名；关系优先用地图依赖快照（安装时/上次
/// 同步固化的），没有快照才退回仓库当前声明 —— 地图源被删之后依赖表仍在，
/// 源还在时以线上声明为准。
pub(crate) fn bind_map_relation_snapshot(
    info: &mut LevelSetInfo,
    config: &AppConfig,
    snapshots: &MapRelationSnapshots,
) {
    let bound = config.map_repositories.iter().find(|map| {
        crate::level_sets::level_set_identity_matches(
            &info.name,
            info.name_zh.as_deref(),
            &info.author,
            &map.level_set_name,
            Some(map.level_set_name_zh.as_str()).filter(|value| !value.is_empty()),
            &map.author,
        )
    });
    /*
     * 分组来源：绑定地图的当前声明 → 快照（先按地图 id，再按身份找）。
     *
     * 地图源被删掉之后配置里已经没有这条，但快照还记着「这张图归到哪个
     * 作者」—— 管理页的折叠分组与详情里的作者中英两名都靠它。两处都没有
     * 就留空，前端回退 info 里的本地 `author`。
     */
    let snapshot = bound
        .and_then(|map| snapshots.entries.get(&map.id))
        .or_else(|| {
            snapshots.entries.values().find(|entry| {
                crate::level_sets::level_set_identity_matches(
                    &info.name,
                    info.name_zh.as_deref(),
                    &info.author,
                    &entry.level_set_name,
                    Some(entry.level_set_name_zh.as_str()),
                    &entry.author,
                )
            })
        });
    let mut group_i18n = bound.map(|map| map.group_i18n.clone()).unwrap_or_default();
    let mut group = bound
        .map(|map| map.group.trim().to_string())
        .unwrap_or_default();
    if let Some(entry) = snapshot
        && group_i18n.is_empty()
        && group.is_empty()
    {
        group_i18n = entry.group_i18n.clone();
        group = entry.group.trim().to_string();
    }
    if !group_i18n.is_empty() {
        info.map_group_i18n = group_i18n;
    }
    if !group.is_empty() {
        info.map_group = Some(group);
    }
    /*
     * 资产版本只有快照里有（安装时记的），未绑定的地图也要填：地图源被删之后
     * 界面仍要靠它判「装的是不是最新资产」，留空就只能退回比 info 版本。
     */
    info.installed_mapset_version = crate::level_sets::installed_mapset_version(
        snapshots,
        bound.map(|map| map.id.as_str()),
        &info.name,
        info.name_zh.as_deref(),
        &info.author,
    )
    .to_string();
    let Some(map) = bound else {
        return;
    };
    info.map_id = Some(map.id.clone());
    /*
     * 声明表与按版本覆盖表都优先取快照（安装/上次同步固化的那份），没有快照
     * 才用仓库当前声明。随后按**本地装着的这个版本**选覆盖表 —— 已装 1.x 的
     * 关卡集不该按线上 2.x 的依赖去判定。
     */
    let (declared, overrides) = match snapshots.entries.get(&map.id) {
        Some(entry) => (entry.relations.clone(), entry.release_relations.clone()),
        None => (map.relations.clone(), map.release_relations.clone()),
    };
    let installed_version =
        Some(info.version.as_str()).filter(|version| !version.trim().is_empty());
    let relations = match crate::mods::select_release_relations(&overrides, installed_version) {
        Some(selected) => selected.to_vec(),
        None => declared,
    };
    /*
     * 线上一条声明都没有时，保留 `level_set_to_info` 用 `requires.txt` 填的兜底：
     * 本地包自带的外部 runtime 要求，不该因为「绑定了地图源、但那个源没写
     * relations」而消失。线上写了就一律以线上为准。
     */
    if !relations.is_empty() {
        info.relations = relations;
    }
}
