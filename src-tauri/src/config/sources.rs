//! 官方根源同步与旧源树清理。

use super::availability::current_user_region;
use crate::models::*;

/// 官方根源的地址：打开「仅用内置仓库源」时直接指向打包资源。
///
/// `mods.toml` 是内置索引的路径，`SourceLocation` 会把它解析成
/// 嵌入资源 —— 也就是回退时用的那一份。所以这个开关不需要第二套加载
/// 逻辑，只是把源头换成同一棵树。
pub(crate) fn builtin_repository_sources_for_region(
    region: &str,
    builtin_only: bool,
) -> Vec<RepositorySource> {
    // Only the entry point is shipped with the application. Its children are
    // file-relative names inside that index, so they resolve to bundled
    // resources as well; the live index points them at remote raw URLs.
    vec![RepositorySource {
        id: crate::repository_sources::DEFAULT_INDEX_ID.into(),
        nickname: "官方仓库源".into(),
        nickname_i18n: [
            ("zh-CN".into(), "官方仓库源".into()),
            ("zh-TW".into(), "官方倉庫來源".into()),
            ("en".into(), "Official repositories".into()),
            ("ko".into(), "공식 저장소 소스".into()),
            ("ja".into(), "公式リポジトリソース".into()),
        ]
        .into(),
        url: if builtin_only {
            crate::repository_sources::BUILTIN_INDEX.into()
        } else {
            crate::repository_sources::default_index_url(region).into()
        },
        region: None,
        builtin: true,
        parent_id: None,
    }]
}

pub(super) fn synchronize_builtin_repository_sources(config: &mut AppConfig) -> bool {
    merge_builtin_repository_sources(
        config,
        builtin_repository_sources_for_region(current_user_region(), config.builtin_sources_only),
    )
}

pub(super) fn merge_builtin_repository_sources(
    config: &mut AppConfig,
    builtin_sources: Vec<RepositorySource>,
) -> bool {
    if builtin_sources.is_empty() {
        return false;
    }
    let previous = config.repository_sources.clone();
    let builtin_ids = builtin_sources
        .iter()
        .map(|source| source.id.clone())
        .collect::<std::collections::HashSet<_>>();
    let mut synchronized = builtin_sources;
    let previous_ids = previous
        .iter()
        .map(|source| source.id.clone())
        .collect::<std::collections::HashSet<_>>();
    for source in previous {
        // Only default root identities are fixed. Descendants are owned by
        // their remote documents and must survive ordinary config reads.
        // This also removes the obsolete embedded roots and their trees.
        if source.builtin && source.parent_id.is_none() {
            continue;
        }
        if builtin_ids.contains(&source.id)
            || synchronized.iter().any(|existing| {
                crate::repository_sources::source_address_key(
                    &existing.url,
                    existing.region.as_deref(),
                ) == crate::repository_sources::source_address_key(
                    &source.url,
                    source.region.as_deref(),
                )
            })
        {
            continue;
        }
        synchronized.push(source);
    }
    loop {
        let before = synchronized.len();
        let known_ids = synchronized
            .iter()
            .map(|source| source.id.clone())
            .collect::<std::collections::HashSet<_>>();
        synchronized.retain(|source| {
            source
                .parent_id
                .as_deref()
                .is_none_or(|parent_id| known_ids.contains(parent_id))
        });
        if synchronized.len() == before {
            break;
        }
    }
    let mut changed = config.repository_sources != synchronized;
    config.repository_sources = synchronized;

    let known_ids = config
        .repository_sources
        .iter()
        .map(|source| &source.id)
        .collect::<std::collections::HashSet<_>>();
    let before = config.repositories.len();
    // Old source-less fallback cards and obsolete regional source trees are
    // replaced by the ordinary source loader on the next read.
    config.repositories.retain(|repository| {
        !(repository.builtin && repository.source_id.is_none())
            && repository
                .source_id
                .as_ref()
                .is_none_or(|id| !previous_ids.contains(id) || known_ids.contains(id))
    });
    changed |= config.repositories.len() != before;
    changed
}
