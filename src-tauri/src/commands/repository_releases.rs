use crate::app_state::AppState;
use crate::models::{ReleaseInfo, RepositoryDownloadMode};
use crate::release_cache::freshness::RELEASE_CACHE_TTL_MS;
use crate::release_cache::freshness::cache_is_fresh;
use crate::release_cache::freshness::now_millis;
use crate::releases;
use tauri::State;

/// 刷新单个仓库的 Release 列表。
///
/// `force = true` 跳过 5 分钟缓存强制请求 —— 用户手动点「刷新」时传，
/// 因为那个动作的语义就是「我要最新的」。批量刷新、自动刷新、切页回来
/// 都传 `false`（或缺省）走缓存。
#[tauri::command]
pub async fn refresh_repository(
    state: State<'_, AppState>,
    repository_id: String,
    force: Option<bool>,
) -> Result<Vec<ReleaseInfo>, String> {
    let force = force.unwrap_or(false);
    log::info!(
        "开始刷新仓库 Release：仓库={}，跳过缓存={}",
        repository_id,
        force
    );
    let config = state.config.load_or_default();
    let index = config
        .repositories
        .iter()
        .position(|repo| repo.id == repository_id)
        .ok_or("source.repositoryMissing")?;
    /*
     * 不可用的仓库不刷新：`enabled=false`（用户关闭，或被官方仓库源覆盖的
     * 社区副本）与「地区不适用」（例如中国大陆下的 international 仓库）
     * 都按「当前不存在」处理 —— 界面上根本没有这张卡片，
     * 刷新入口也就无从触发。
     */
    if !crate::config::repository_is_available(
        &config,
        &config.repositories[index],
        crate::config::current_user_region(),
    ) {
        return Err("source.repositoryUnavailableInRegion".into());
    }
    if config.repositories[index].download_mode == RepositoryDownloadMode::RepositoryFile {
        // 这条消息会直接展示在「刷新全部仓库源」的结果里，写成用户能看懂的话
        log::debug!(
            "跳过仓库文件模式的 Release 刷新：仓库={}",
            config.repositories[index].name
        );
        return Err(crate::locale_tag::with_params(
            "source.fileModeNoVersion",
            &[("repo", &config.repositories[index].name.to_string())],
        ));
    }
    let repo = config.repositories[index].clone();

    /*
     * 缓存命中就直接返回已有列表，一个网络请求都不发。
     *
     * 为什么放在这里、而不是在命令入口：上面几条「不可用 / 仓库文件模式」
     * 的拒绝要先执行 —— 那些是配置层面的「不该刷新」，与缓存无关。
     *
     * `last_error` 不清：上次失败留下的错误要保留到下一次真正成功为止，
     * 否则界面会出现「有缓存列表但没有错误」的假象，用户以为已经恢复。
     */
    if !force
        && !repo.cached_releases.is_empty()
        && cache_is_fresh(repo.releases_cached_at, now_millis(), RELEASE_CACHE_TTL_MS)
    {
        log::info!(
            "仓库 Release 命中缓存：仓库={}，数量={}，缓存时间={:?}",
            repo.name,
            repo.cached_releases.len(),
            repo.releases_cached_at
        );
        return Ok(repo.cached_releases.clone());
    }

    let client =
        releases::build_client(config.network.timeout_seconds).map_err(|e| e.to_string())?;
    let token = state.config.repository_api_token(&config, &repo);
    let api_proxies = crate::providers::api_proxies(&config, &repo.provider);
    log::debug!(
        "请求仓库 Release API：仓库={}，平台={:?}，代理数={}，最大 Release 数={}",
        repo.name,
        repo.provider,
        api_proxies.len(),
        repo.max_release_count
    );
    let fetched = releases::fetch_releases(
        &client,
        &repo,
        token.as_deref(),
        config.auth.official_api_only,
        api_proxies,
    )
    .await
    .map(|releases| releases::filter_channel(releases, &repo.channel))
    .map_err(|error| error.to_string());

    let _cache_guard = state.repository_cache.lock().await;
    let _operation_guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let mut latest_config = state.config.load_or_default();
    let latest_index = latest_config
        .repositories
        .iter()
        .position(|item| item.id == repository_id)
        .ok_or("source.repositoryRemoved")?;
    let latest_repo = &mut latest_config.repositories[latest_index];
    if latest_repo.username != repo.username
        || latest_repo.repository != repo.repository
        || latest_repo.provider != repo.provider
        || latest_repo.channel != repo.channel
        || latest_repo.download_mode != repo.download_mode
    {
        return Err("source.repositoryConfigChanged".into());
    }

    latest_repo.etag = None;
    match fetched {
        Ok(filtered) => {
            latest_repo.cached_releases = filtered.clone();
            // 只有真正拿到结果才刷新时间戳：失败后必须让下一次请求重新打 API
            latest_repo.releases_cached_at = Some(now_millis());
            latest_repo.last_error = None;
            state
                .config
                .save(&latest_config)
                .map_err(|error| error.to_string())?;
            log::info!(
                "仓库 Release 刷新完成：仓库={}，数量={}",
                repo.name,
                filtered.len()
            );
            Ok(filtered)
        }
        Err(error) => {
            log::error!("仓库 Release 刷新失败：仓库={}，原因={}", repo.name, error);
            latest_repo.cached_releases.clear();
            // 清掉时间戳，避免「空列表 + 新鲜时间」被误判成命中缓存
            latest_repo.releases_cached_at = None;
            latest_repo.last_error = Some(error.clone());
            state
                .config
                .save(&latest_config)
                .map_err(|save_error| save_error.to_string())?;
            Err(error)
        }
    }
}

#[tauri::command]
pub fn get_repository_releases(
    state: State<'_, AppState>,
    repository_id: String,
) -> Result<Vec<ReleaseInfo>, String> {
    let config = state.config.load_or_default();
    Ok(config
        .repositories
        .iter()
        .find(|repo| repo.id == repository_id)
        .ok_or("source.repositoryMissing")?
        .cached_releases
        .clone())
}
