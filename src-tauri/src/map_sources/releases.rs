use super::assets::synthesize_releases;
use super::context::MapSourceContext;
use super::identity::map_tag;
use crate::models::MapRepositoryConfig;
use crate::models::ReleaseAsset;
use crate::models::ReleaseInfo;

/// 刷新一个地图仓库的 Release 列表（tag 模式下返回合成的版本列表）。
///
/// 与 MOD 共用缓存，缓存按仓库和 API 路线收敛。tag 模式下把该 tag 的资产逐个拆成版本。
pub(crate) async fn fetch_map_release_data(
    ctx: &MapSourceContext<'_>,
    map: &MapRepositoryConfig,
) -> Result<MapRepositoryConfig, String> {
    let probe = crate::releases::RepositoryRequest::from(map);
    let releases = crate::releases::cached_releases(
        ctx.cache,
        ctx.client,
        &probe,
        &ctx.config.region,
        ctx.token(&probe.provider),
        ctx.pass_id,
        ctx.force,
    )
    .await?;
    if map.release_derived {
        let declaration = serde_json::from_value(serde_json::json!({
            "id": map.source_id.as_deref().unwrap_or_default(),
            "mode": "release-tags",
            "provider": map.provider,
            "username": map.username,
            "repository": map.repository,
            "network_plan": map.network_plan,
        }))
        .map_err(|error| error.to_string())?;
        let label = super::state::SourceLabel {
            note_i18n: map.source_note_i18n.clone(),
            regions: map.source_regions.clone().unwrap_or_default(),
            id: map.source_id.clone().unwrap_or_default(),
            name: map.source_name.clone(),
            name_i18n: map.source_name_i18n.clone(),
            network_plan: map.network_plan.clone(),
        };
        // This location is only a diagnostic/validation base; no TOML is read.
        let origin =
            crate::repository_sources::SourceLocation::parse(super::tree::BUILTIN_MAP_INDEX)
                .map_err(|error| error.to_string())?;
        let release = releases
            .iter()
            .find(|release| Some(release.tag_name.trim()) == map_tag(map))
            .ok_or_else(|| "mapSource.mapMissing".to_string())?;
        let refreshed = super::release_tags::project_release_tag_maps(
            &declaration,
            &label,
            map.builtin,
            &origin,
            std::slice::from_ref(release),
            None,
        )
        .into_iter()
        .find(|entry| entry.id == map.id)
        .ok_or_else(|| "mapSource.mapMissing".to_string())?;
        if let Some(error) = &refreshed.last_error {
            return Err(error.clone());
        }
        return Ok(refreshed);
    }
    let mut refreshed = map.clone();
    refreshed.cached_releases = project_map_releases(map, &releases)?;
    refreshed.releases_cached_at = Some(crate::release_cache::freshness::now_millis());
    refreshed.last_error = None;
    Ok(refreshed)
}

/// One projection for fresh source declarations and individual map refreshes.
pub(super) fn project_map_releases(
    map: &MapRepositoryConfig,
    releases: &[ReleaseInfo],
) -> Result<Vec<ReleaseInfo>, String> {
    match map_tag(map) {
        /*
         * tag 模式不走 `filter_channel`：渠道过滤的是 **Release**（稳定/预览），
         * 而这里一个 Release 被拆成了「每个资产一个版本」—— 版本稳不稳定由
         * 作者在条目里声明，不再由 Release 的 prerelease 标记决定。
         * 硬过滤只会让写了 tag 的条目在 stable 渠道下整条消失。
         */
        Some(tag) => synthesize_releases(map, tag, releases),
        None => Ok(crate::releases::filter_channel(
            releases.to_vec(),
            &map.channel,
        )),
    }
}

/// 暂存地图 Release 时挑出要下载的那个资产。
///
/// 指定资产名时精确匹配（详情抽屉把选中的文件传下来）；未指定时分两种形态：
/// - tag 模式：`cached_releases` 是 `synthesize_releases` 拆出来的合成版本，
///   每条只带一个已经按版本表或版本规则选中的资产，直接用它。
/// - 普通模式：与 MOD 安装共用 `select_install_asset` 的 `asset_rules` 匹配。
///
/// 版本表接受 `3.4.1-fix` 这类自由文本版本，而 `asset_rules` 的版本正则通常
/// 只写 `[0-9.]+`。tag 模式下再按规则过一遍会把唯一资产筛掉，卡片一键安装
/// 报 `source.noMatchingAsset`，而抽屉因为传了资产名不受影响。
pub(crate) fn select_map_stage_asset<'a>(
    map: &MapRepositoryConfig,
    release: &'a ReleaseInfo,
    asset_name: Option<&str>,
) -> Option<&'a ReleaseAsset> {
    if asset_name.is_none() && map_tag(map).is_some() {
        return release.assets.first();
    }
    let probe = crate::releases::RepositoryRequest::from(map);
    crate::releases::select_install_asset(&probe, &map.asset_rules, release, asset_name)
}
