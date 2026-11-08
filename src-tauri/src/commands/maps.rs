use crate::app_state::AppState;
use crate::map_sources::view::map_to_info;
use crate::models::{MapInfo, Provider, ReleaseInfo};
use crate::release_cache::freshness::RELEASE_CACHE_TTL_MS;
use crate::release_cache::freshness::cache_is_fresh;
use crate::release_cache::freshness::now_millis;
use crate::releases;
use tauri::State;

/// 列出当前地区可用的地图（含 Release 缓存）。
///
/// 只读：不发起网络请求。缓存为空时界面显示「待检查」，用户点刷新才抓。
///
/// 这里**不再扫描 BepInEx/plugins**：以前为了算 `unmet_frameworks` 会调用
/// `installed_plugin_versions`，而它内部是全量 MOD 扫描（读所有 DLL 的
/// BepInEx 元数据 + 安装清单）。地图页因此要等一次与地图无关的扫描。
/// 依赖判定改由前端用 Dashboard 已扫好的 MOD 列表计算，两边共用同一份
/// 关系判定实现，结论不会分叉。
#[tauri::command]
pub fn get_maps(state: State<'_, AppState>) -> Result<Vec<MapInfo>, String> {
    let config = state.config.load_or_default();
    let maps = crate::map_sources::available_maps(&config, crate::config::current_user_region());
    log::debug!("地图列表读取完成：数量={}", maps.len());
    Ok(maps.iter().map(map_to_info).collect())
}

/// 刷新地图源树，并保留同 ID 条目的 Release 缓存与失败状态，声明字段取新值。
///
/// 非 builtin_only 的源树加载失败时尝试内置条目；内置为空则返回原错误。
/// 这里不区分网络错误与格式错误，不能套用 MOD 仓库源的回退条件。
#[tauri::command]
pub async fn refresh_map_sources(
    state: State<'_, AppState>,
    pass_id: Option<String>,
) -> Result<Vec<MapInfo>, String> {
    let config = state.config.load_or_default();
    let builtin_only = config.builtin_map_sources_only;
    let region = crate::config::current_user_region().to_string();
    log::info!("开始刷新地图源：内置={}，地区={}", builtin_only, region);
    /*
     * 源文档可以放在 Release 资产里，那时要先向同一个仓库的 API 取 size/sha256
     * 才能校验。客户端与令牌在这里建一次、整棵源树共用；`pass_id` 是这次用户
     * 动作的批次身份，让同一批里同一个仓库只请求一次列表 —— 索引里挂十几个
     * 第三方源、其中几个指着同一个仓库时，这个去重就是匿名限流（60 次/小时）
     * 下的生死线。
     */
    let client = releases::build_client(config.network.timeout_seconds)
        .map_err(|error| error.to_string())?;
    let github_token = state.config.api_token(&config, &Provider::Github);
    let gitee_token = state.config.api_token(&config, &Provider::Gitee);
    let gitcode_token = state.config.api_token(&config, &Provider::Gitcode);
    let loaded = {
        let ctx = crate::map_sources::MapSourceContext {
            cache: &state.release_cache,
            client: &client,
            config: &config,
            github_token: github_token.as_deref(),
            gitee_token: gitee_token.as_deref(),
            gitcode_token: gitcode_token.as_deref(),
            pass_id: pass_id.as_deref(),
            force: pass_id.is_some(),
            // 取不到 API 时沿用上次的条目，见 `MapSourceContext::previous`
            previous: &config.map_repositories,
        };
        crate::map_sources::load_map_tree(&region, builtin_only, &ctx).await
    };
    // 来源标签描述根索引；即使根为内置，release-tags 与社区条目仍可能联网。
    let (maps, source) = match loaded {
        Ok(maps) => (maps, if builtin_only { "bundled" } else { "remote" }),
        Err(error) => {
            /*
             * 加载失败时试探内置快照，但不修改入口配置，下次刷新仍走原入口。
             * 快照为空时不能用空成功结果覆盖在用条目。
             */
            if builtin_only {
                return Err(error.to_string());
            }
            log::warn!(
                "地图源刷新失败，回退内置快照：原因={}",
                crate::repository_sources::error_category(&error)
            );
            let bundled = crate::map_sources::bundled_map_tree();
            if bundled.is_empty() {
                return Err(error.to_string());
            }
            (bundled, "bundled-fallback")
        }
    };

    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let mut config = state.config.load_or_default();
    let changed = crate::map_sources::replace_maps_keeping_runtime_state(&mut config, maps);
    if changed {
        state
            .config
            .save(&config)
            .map_err(|error| error.to_string())?;
    }
    log::info!(
        "地图源刷新完成：来源={}，地图={}，配置已更新={}",
        source,
        config.map_repositories.len(),
        changed
    );
    Ok(crate::map_sources::available_maps(&config, &region)
        .iter()
        .map(map_to_info)
        .collect())
}

/// 刷新一个地图仓库的 Release（5 分钟缓存，与 MOD 仓库同一套 TTL）。
#[tauri::command]
pub async fn refresh_map_releases(
    state: State<'_, AppState>,
    map_id: String,
    force: Option<bool>,
    pass_id: Option<String>,
) -> Result<Vec<ReleaseInfo>, String> {
    let force = force.unwrap_or(false);
    let config = state.config.load_or_default();
    let Some(map) = config
        .map_repositories
        .iter()
        .find(|entry| entry.id == map_id)
        .cloned()
    else {
        return Err("mapSource.mapMissing".into());
    };
    if !crate::map_sources::map_is_available(&map, crate::config::current_user_region()) {
        return Err("source.repositoryUnavailableInRegion".into());
    }

    // 缓存命中直接返回，一个请求都不发（与 MOD 仓库同一条规则）
    let now = now_millis();
    if !force
        && !map.cached_releases.is_empty()
        && cache_is_fresh(map.releases_cached_at, now, RELEASE_CACHE_TTL_MS)
    {
        log::info!(
            "地图 Release 命中缓存：地图={}，数量={}",
            map.name,
            map.cached_releases.len()
        );
        return Ok(map.cached_releases.clone());
    }

    let client = releases::build_client(config.network.timeout_seconds)
        .map_err(|error| error.to_string())?;
    let github_token = state.config.api_token(&config, &Provider::Github);
    let gitee_token = state.config.api_token(&config, &Provider::Gitee);
    let gitcode_token = state.config.api_token(&config, &Provider::Gitcode);
    let fetched = {
        let ctx = crate::map_sources::MapSourceContext {
            cache: &state.release_cache,
            client: &client,
            config: &config,
            github_token: github_token.as_deref(),
            gitee_token: gitee_token.as_deref(),
            gitcode_token: gitcode_token.as_deref(),
            pass_id: pass_id.as_deref(),
            force,
            // 取不到 API 时沿用上次的条目，见 `MapSourceContext::previous`
            previous: &config.map_repositories,
        };
        crate::map_sources::fetch_map_releases(&ctx, &map).await
    };

    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let mut latest = state.config.load_or_default();
    let Some(target) = latest
        .map_repositories
        .iter_mut()
        .find(|entry| entry.id == map_id)
    else {
        return Err("mapSource.mapMissing".into());
    };
    if !crate::map_sources::same_release_binding(target, &map) {
        return Err("source.repositoryConfigChanged".into());
    }
    match fetched {
        Ok(releases) => {
            target.cached_releases = releases.clone();
            target.releases_cached_at = Some(now_millis());
            target.last_error = None;
            state
                .config
                .save(&latest)
                .map_err(|error| error.to_string())?;
            Ok(releases)
        }
        Err(error) => {
            log::error!("地图 Release 刷新失败：地图={}，原因={}", map.name, error);
            target.cached_releases.clear();
            target.releases_cached_at = None;
            target.last_error = Some(error.clone());
            state
                .config
                .save(&latest)
                .map_err(|save| save.to_string())?;
            Err(error)
        }
    }
}
