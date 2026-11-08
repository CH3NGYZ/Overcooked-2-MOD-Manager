use crate::models::{MapInfo, RequiredFramework};

/// 把内部地图配置转成 IPC 模型。
///
/// ## 为什么这里不做依赖判定
///
/// 判定「哪个框架版本没满足」需要本地已装插件的 GUID → 版本，而拿到它必须
/// 全量扫描 `BepInEx/plugins`（每个 DLL 读 BepInEx 元数据 + 安装清单）。地图页
/// 打开时再扫一遍纯属重复劳动：Dashboard 早就扫好了同一份数据，前端拿它就能
/// 用同一份关系判定实现算出结论。
///
/// 因此 `required_frameworks` 只给出**声明**（`installed_version` 恒为 `None`），
/// `unmet_frameworks` 恒为空。字段保留是为了兼容尚未迁移的旧前端代码 ——
/// 删字段会让它在 `map.unmet_frameworks ?? []` 之外的访问路径上崩掉。
pub(crate) fn map_to_info(map: &crate::models::MapRepositoryConfig) -> MapInfo {
    /*
     * 覆盖表按「这张卡现在代表哪个版本」选：作者钉死的 `latest_mapset_version`
     * 优先，其次是最新一次拉到的 Release tag；两个都没有就只看固定声明。
     */
    let release_version = map.latest_mapset_version.as_deref().or_else(|| {
        map.cached_releases
            .first()
            .map(|release| release.tag_name.as_str())
    });
    let declared: Vec<RequiredFramework> =
        crate::mods::effective_map_relations(map, release_version)
            .iter()
            .map(|relation| RequiredFramework {
                guid: relation.guid.clone(),
                version: relation.version.clone(),
                installed_version: None,
            })
            .collect();
    MapInfo {
        release_derived: map.release_derived,
        id: map.id.clone(),
        name: map.name.clone(),
        name_i18n: map.name_i18n.clone(),
        group: map.group.clone(),
        group_i18n: map.group_i18n.clone(),
        provider: map.provider.clone(),
        username: map.username.clone(),
        repository: map.repository.clone(),
        level_set_name: map.level_set_name.clone(),
        level_set_name_zh: map.level_set_name_zh.clone(),
        author: map.author.clone(),
        // 详情面板要标出「安装时会选哪个资产」，与后端自动匹配共用同一份规则
        asset_rules: map.asset_rules.clone(),
        note: map.note.clone(),
        note_i18n: map.note_i18n.clone(),
        enabled: map.enabled,
        builtin: map.builtin,
        // 源归属与源名：界面据此排「源 → 分类 → 地图」三层，
        // 后端不持久化源树，这两样只存在于条目上（见 `map_sources`）
        source_id: map.source_id.clone(),
        source_name: map.source_name.clone(),
        source_name_i18n: map.source_name_i18n.clone(),
        /*
         * 作者钉死的版本号：有值时前端只比等值（本地更高也提示更新），
         * 因为版本号是作者手写的自由文本，`0.7 > 0.69` 这种大小关系
         * 不代表作者认为 0.7 更新。判定实现只有前端的 `mapUpdateState` 一处。
         */
        latest_mapset_version: map.latest_mapset_version.clone(),
        latest_info_version: map.latest_info_version.clone(),
        latest_tag: map
            .cached_releases
            .first()
            .map(|release| release.tag_name.clone()),
        latest_release: map.cached_releases.first().cloned(),
        // 详情抽屉要列出可安装的版本（含回滚到旧版），只给最新一条不够
        cached_releases: map.cached_releases.clone(),
        cached_release_count: map.cached_releases.len(),
        cached_at: map.releases_cached_at,
        last_error: map.last_error.clone(),
        relations: map.relations.clone(),
        release_relations: map.release_relations.clone(),
        required_frameworks: declared,
        unmet_frameworks: Vec::new(),
    }
}
