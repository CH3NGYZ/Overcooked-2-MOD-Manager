use crate::models::AppConfig;
use std::collections::HashSet;

/// Discard persisted source content before requesting any fresh documents.
/// Keep the selected source headings and all unrelated configuration.
pub(crate) fn clear_source_cache(config: &mut AppConfig, source_ids: &[String]) -> (usize, usize) {
    let roots = source_ids.iter().cloned().collect::<HashSet<_>>();
    let mut descendants = roots.clone();
    loop {
        let before = descendants.len();
        for source in &config.repository_sources {
            if source
                .parent_id
                .as_ref()
                .is_some_and(|id| descendants.contains(id))
            {
                descendants.insert(source.id.clone());
            }
        }
        if descendants.len() == before {
            break;
        }
    }
    let repository_count = config.repositories.len();
    /*
     * 清空这些源的仓库，但把 Release 缓存**暂存下来**。
     *
     * 早先的做法是整条 `retain` 掉仓库，于是列表重新拉回来、仓库重建之后
     * `cached_releases` 与 `releases_cached_at` 都是空的 —— 5 分钟缓存
     * 永远不可能命中，每次刷新（含每次启动）都得把整棵仓库树的 API 重打
     * 一遍。实测 20+ 个仓库、每次几十个请求，还固定撞上 Gitee 的速率限制。
     *
     * 缓存按 `repository_id` 存：ID 已经被 `source_repository_ids` 保住，
     * 重建时由 import_repository_source_into_config 按原 ID 逐项认领，
     * 不能清整张表，否则尚未重建的其他源会丢缓存。未认领项仍随配置保存。
     */
    for repo in &config.repositories {
        let Some(source_id) = repo.source_id.as_ref() else {
            continue;
        };
        if !descendants.contains(source_id) {
            continue;
        }
        let key = crate::config::source_repository_key(source_id, &crate::config::repo_key(repo));
        config.source_repository_ids.insert(key, repo.id.clone());
        // 只搬「有内容」的缓存：空列表搬过去也没有意义，反而让判断变复杂
        if !repo.cached_releases.is_empty() {
            config.staged_release_cache.insert(
                repo.id.clone(),
                crate::models::StagedReleaseCache {
                    releases: repo.cached_releases.clone(),
                    cached_at: repo.releases_cached_at,
                    last_error: repo.last_error.clone(),
                },
            );
        }
    }
    config.repositories.retain(|repo| {
        repo.source_id
            .as_ref()
            .is_none_or(|id| !descendants.contains(id))
    });
    let source_count = config.repository_sources.len();
    config
        .repository_sources
        .retain(|source| roots.contains(&source.id) || !descendants.contains(&source.id));
    (
        repository_count - config.repositories.len(),
        source_count - config.repository_sources.len(),
    )
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod release_cache_staging_tests;

#[cfg(test)]
mod staged_cache_roundtrip_tests;
