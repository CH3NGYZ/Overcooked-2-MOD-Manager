use super::catalog::SearchResults;
use super::catalog::source;
use super::catalog::source_id;
use super::ownership::identity;
use super::ownership::official_repository_keys;
use crate::archive::InstallManifest;
use crate::models::AppConfig;
use crate::models::ModRepositoryConfig;
use crate::models::Provider;
use std::collections::HashMap;
use std::collections::HashSet;

pub(super) fn merge_results(
    config: &mut AppConfig,
    provider: &Provider,
    results: SearchResults,
    manifests: &[InstallManifest],
) {
    let source_id = source_id(provider);
    let belongs_to_search =
        |repo: &ModRepositoryConfig| repo.source_id.as_deref() == Some(source_id);
    let official = official_repository_keys(config);
    let is_installed = |repo: &ModRepositoryConfig| {
        manifests.iter().any(|manifest| {
            manifest.repository_id == repo.id
                || manifest
                    .repository_binding
                    .as_ref()
                    .is_some_and(|binding| binding.matches_repository(repo))
        })
    };
    let mut discovery = results.discovery;
    let mut visible_ids = HashSet::new();
    let mut previous = config
        .repositories
        .iter()
        .filter(|repo| belongs_to_search(repo))
        .map(|repo| (repo.id.clone(), repo.clone()))
        .collect::<HashMap<_, _>>();
    config.repositories.retain(|repo| !belongs_to_search(repo));
    for mut repo in results.repositories {
        if let Some(old) = previous.remove(&repo.id) {
            repo.enabled = old.enabled;
        }
        let already_official = official.contains(&identity(&repo));
        if !already_official {
            visible_ids.insert(repo.id.clone());
        } else {
            /*
             * 被官方仓库源覆盖的社区仓库等同「不在安装页展示」：
             * 官方那份已经在列表里，社区这份再刷一遍只是重复请求同一个
             * Release。把 enabled 置假，让它与用户手动隐藏的仓库走同一条
             * 路径 —— 不显示、不参与刷新。
             *
             * 已安装的仍保留在配置里（下面那个 push），这样它的更新绑定
             * 不会丢；只是刷新交给官方仓库去做。
             */
            repo.enabled = false;
        }
        // Rediscovered records use catalogue defaults plus the old enabled flag;
        // installed manifests keep their binding, while official duplicates stay hidden.
        if !already_official || is_installed(&repo) {
            config.repositories.push(repo);
        }
    }
    let mut retained = previous
        .into_values()
        .filter(is_installed)
        .collect::<Vec<_>>();
    retained.sort_by(|left, right| left.id.cmp(&right.id));
    for repo in &mut retained {
        repo.cached_releases.clear();
        repo.last_error = None;
        repo.etag = None;
        // 被官方覆盖的历史社区仓库同样视为隐藏：不刷新，交给官方那份
        if official.contains(&identity(repo)) {
            repo.enabled = false;
        }
    }
    config.repositories.extend(retained);
    config
        .repository_sources
        .retain(|source| source.id != source_id);
    config.repository_sources.push(source(provider));
    discovery
        .repository_ids
        .retain(|id| visible_ids.contains(id));
    /*
     * 地图源仓库只按平台整体替换：它们没有「用户手动隐藏」这类本地状态，
     * 不需要像 MOD 仓库那样与旧条目逐个合并 —— 发现结果就是全部真相。
     */
    config
        .community_map_repositories
        .retain(|repository| repository.provider != *provider);
    config
        .community_map_repositories
        .extend(results.map_repositories);
    match provider {
        Provider::Github => config.community_discovery = Some(discovery),
        Provider::Gitee => config.gitee_community_discovery = Some(discovery),
        Provider::Gitcode => config.gitcode_community_discovery = Some(discovery),
    }
}

#[cfg(test)]
mod tests;
