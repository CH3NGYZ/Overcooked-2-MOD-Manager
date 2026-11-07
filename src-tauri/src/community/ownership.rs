use super::catalog::is_repository;
use super::catalog::is_source;
use super::catalog::source_id;
use crate::models::AppConfig;
use crate::models::CommunityDiscovery;
use crate::models::RepositoryConfig;
use std::collections::HashMap;
use std::collections::HashSet;

pub(super) type RepositoryIdentity = (&'static str, String, String);

pub(super) fn identity(repo: &RepositoryConfig) -> RepositoryIdentity {
    (
        source_id(&repo.provider),
        repo.username.to_ascii_lowercase(),
        repo.repository.to_ascii_lowercase(),
    )
}

pub(super) fn official_repository_keys(config: &AppConfig) -> HashSet<RepositoryIdentity> {
    let mut children: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut pending = vec![crate::repository_sources::DEFAULT_INDEX_ID];
    for source in &config.repository_sources {
        if is_source(&source.id) {
            continue;
        }
        if let Some(parent) = source.parent_id.as_deref() {
            children.entry(parent).or_default().push(&source.id);
        } else if source.builtin {
            pending.push(&source.id);
        }
    }
    let mut official_sources = HashSet::new();
    while let Some(id) = pending.pop() {
        if official_sources.insert(id)
            && let Some(ids) = children.get(id)
        {
            pending.extend(ids);
        }
    }
    // Source ownership is independent of the builtin edit/delete protection flag.
    let mut keys = config
        .repositories
        .iter()
        .filter(|repo| {
            !is_repository(repo)
                && (repo.builtin
                    || repo
                        .source_id
                        .as_deref()
                        .is_some_and(|id| official_sources.contains(id)))
        })
        .map(identity)
        .collect::<HashSet<_>>();
    /*
     * 配置里只保存「当前地区可见」的官方仓库：地区过滤会把不适用的仓库
     * 从 config 中剔除。若只按 config 判断，那些被地区过滤掉的官方仓库
     * 就会重新出现在社区搜索结果里。内置快照是没有地区过滤的完整清单，
     * 因此把它的全部仓库身份也并入回避集合。
     */
    keys.extend(bundled_official_repository_keys());
    keys
}

/// 内置仓库快照里的全部仓库身份（不受当前地区过滤影响）。
fn bundled_official_repository_keys() -> HashSet<RepositoryIdentity> {
    crate::repository_sources::bundled_repository_configs()
        .into_iter()
        .map(|repo| {
            (
                source_id(&repo.provider),
                repo.username.to_ascii_lowercase(),
                repo.repository.to_ascii_lowercase(),
            )
        })
        .collect()
}

pub(crate) fn visible_discoveries(
    config: &AppConfig,
) -> (Option<CommunityDiscovery>, Option<CommunityDiscovery>) {
    let official = official_repository_keys(config);
    let visible_ids = config
        .repositories
        .iter()
        .filter(|repo| is_repository(repo) && !official.contains(&identity(repo)))
        .map(|repo| &repo.id)
        .collect::<HashSet<_>>();
    // Filter only the public snapshot; saved records still support installed Tag/channel bindings.
    let visible = |discovery: &Option<CommunityDiscovery>| {
        discovery.clone().map(|mut result| {
            result.repository_ids.retain(|id| visible_ids.contains(id));
            result
        })
    };
    (
        visible(&config.community_discovery),
        visible(&config.gitee_community_discovery),
    )
}

#[cfg(test)]
mod tests;
