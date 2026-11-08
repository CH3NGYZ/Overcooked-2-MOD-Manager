use super::assets::synthesize_releases;
use super::context::MapSourceContext;
use super::identity::map_tag;
use crate::models::MapRepositoryConfig;
use crate::models::ReleaseAsset;
use crate::models::ReleaseInfo;

/// 把地图仓库投影成 `fetch_releases` 需要的最小 `RepositoryConfig`。
///
/// ## 为什么用投影而不是改 `fetch_releases` 的签名
///
/// `fetch_releases` 是 MOD 与地图共用的**唯一** Release 抓取入口：限流闸门、
/// 直连/代理目标顺序、403 提前停止、渠道过滤、Gitee 特例都在里面。为了地图
/// 再造一条抓取路径，等于把这些规则复制一遍 —— 迟早两边行为不一致。
///
/// 投影只填「取 Release 真正会读」的字段；其余用默认值。任何新增的
/// Release 相关字段都要同时出现在这里，否则地图侧会拿到默认行为。
pub(crate) fn release_probe(map: &MapRepositoryConfig) -> crate::models::RepositoryConfig {
    // max_release_count only limits the displayed asset versions; API pagination is shared.
    // 借 `repository_from_export` 而不是手写字段：它掌握全部默认值，
    // 手写一份会在新增字段时静默落空。
    /*
     * 用 serde 构造 `RepositoryExport`：它字段很多且多数有默认值，
     * 手写结构体会在新增字段时编译失败（好事），但也要同步维护一份字段表。
     * 走 JSON 只写这次真正关心的几项，其余取模型里的 serde 默认值。
     */
    let export: crate::models::RepositoryExport = serde_json::from_value(serde_json::json!({
        "name": map.name,
        "name_i18n": map.name_i18n,
        "group": map.group,
        "group_i18n": map.group_i18n,
        "type": "map",
        "network_plan": map.network_plan,
        "provider": map.provider,
        "username": map.username,
        "repository": map.repository,
        "enabled": map.enabled,
        "download_mode": "release",
        "channel": map.channel,
        "max_release_count": map.max_release_count,
        "asset_rules": map.asset_rules,
        "install_target": "",
    }))
    .expect("地图投影的字段形状固定，构造失败说明模型改了默认值");
    crate::config::repository_from_export(&export, map.id.clone(), map.builtin)
}

/// 刷新一个地图仓库的 Release 列表（tag 模式下返回合成的版本列表）。
///
/// 与仓库侧同一条路径：`fetch_releases`。区别只有两处 —— 请求按仓库收敛
/// （同一批里同一个仓库只打一次 API），以及 tag 模式下不「取最新那一条」，
/// 而是把那个 tag 的资产逐个拆成版本。
pub(crate) async fn fetch_map_releases(
    ctx: &MapSourceContext<'_>,
    map: &MapRepositoryConfig,
) -> Result<Vec<ReleaseInfo>, String> {
    let probe = release_probe(map);
    let key =
        crate::releases::repository_cache_key(map.provider.clone(), &map.username, &map.repository);
    let token = ctx.token(&map.provider).map(str::to_string);
    let official_api_only = ctx.config.auth.official_api_only;
    let api_proxies = ctx.api_proxies(&map.provider).to_vec();
    let releases = ctx
        .cache
        .list(key, ctx.pass_id, ctx.force, || async move {
            crate::releases::fetch_releases(
                ctx.client,
                &probe,
                token.as_deref(),
                official_api_only,
                &api_proxies,
            )
            .await
            .map_err(|error| error.to_string())
        })
        .await?;
    project_map_releases(map, releases.as_ref())
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
    let probe = release_probe(map);
    crate::releases::select_install_asset(&probe, release, asset_name)
}
