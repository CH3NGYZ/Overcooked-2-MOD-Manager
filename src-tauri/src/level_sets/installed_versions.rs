//! 主地图资产版本记录与旧快照兼容读取。

use super::identity::level_set_identity_matches;
use super::zip_inspect::ImportedLevelSet;
use crate::models::MapRelationSnapshots;
use std::path::Path;

const INSTALLED_PREFIX: &str = "installed:";

pub(crate) fn installed_repository_id<'a>(
    snapshots: &'a MapRelationSnapshots,
    name: &str,
    name_zh: Option<&str>,
    author: &str,
) -> Option<&'a str> {
    let mut ids = snapshots.entries.iter().filter_map(|(key, entry)| {
        (key.starts_with(INSTALLED_PREFIX)
            && !entry.repository_id.is_empty()
            && level_set_identity_matches(
                name,
                name_zh,
                author,
                &entry.level_set_name,
                Some(&entry.level_set_name_zh),
                &entry.author,
            )
            && entry
                .level_set_name_zh
                .trim()
                .eq_ignore_ascii_case(name_zh.unwrap_or_default().trim()))
        .then_some(entry.repository_id.as_str())
    });
    let first = ids.next()?;
    ids.all(|id| id == first).then_some(first)
}

/// 新快照有中文身份时优先精确匹配，避免旧版无中文名的记录抢走另一张同名图的版本。
/// 老快照仍按仓库 ID / 身份兜底，不因升级凭空抹掉修复版的安装记录。
pub fn installed_mapset_version<'a>(
    snapshots: &'a crate::models::MapRelationSnapshots,
    repository_id: Option<&str>,
    name: &str,
    name_zh: Option<&str>,
    author: &str,
) -> &'a str {
    let matches = |entry: &&crate::models::MapRelationSnapshot| {
        level_set_identity_matches(
            name,
            name_zh,
            author,
            &entry.level_set_name,
            Some(&entry.level_set_name_zh),
            &entry.author,
        )
    };
    let by_id = repository_id
        .and_then(|id| snapshots.entries.get(id))
        .filter(matches);
    let exact = |entry: &&crate::models::MapRelationSnapshot| {
        matches(entry)
            && entry
                .level_set_name_zh
                .trim()
                .eq_ignore_ascii_case(name_zh.unwrap_or_default().trim())
    };
    by_id
        .filter(exact)
        .or_else(|| snapshots.entries.values().find(exact))
        .or(by_id)
        .or_else(|| snapshots.entries.values().find(matches))
        .map(|entry| entry.installed_mapset_version.trim())
        .unwrap_or("")
}

/// 只有在线来源的主地图知道资产版本，其余随包地图只能按各自 info 判定。
/// 本地重装/随包覆盖时清除旧资产记录，避免沿用上一次安装的主包版本。
pub(super) fn record_installed_mapset_versions(
    manifests_dir: &Path,
    sets: &[&ImportedLevelSet],
    origin: Option<&crate::models::MapInstallOrigin>,
) {
    if sets.is_empty() {
        return;
    }
    let mut snapshots = crate::archive::read_map_relation_snapshots(manifests_dir);
    let previous = snapshots.entries.clone();
    for set in sets {
        let metadata = &set.metadata;
        let installed = origin.filter(|source| {
            level_set_identity_matches(
                &metadata.name,
                metadata.name_zh.as_deref(),
                &metadata.author,
                &source.level_set_name,
                Some(&source.level_set_name_zh),
                &source.author,
            ) && source
                .level_set_name_zh
                .trim()
                .eq_ignore_ascii_case(metadata.name_zh.as_deref().unwrap_or_default().trim())
        });
        let version = installed
            .map(|source| source.mapset_version.trim())
            .unwrap_or("");
        let repository_id = installed.and_then(|source| source.repository_id.as_deref());
        let mut matched = false;
        for (key, entry) in &mut snapshots.entries {
            if !level_set_identity_matches(
                &metadata.name,
                metadata.name_zh.as_deref(),
                &metadata.author,
                &entry.level_set_name,
                Some(&entry.level_set_name_zh),
                &entry.author,
            ) {
                continue;
            }
            // 不再保留已替换地图的来源偏好；仓库声明条目的 ID 与关系仍保持原样。
            if key.starts_with(INSTALLED_PREFIX) {
                entry.repository_id.clear();
            }
            // 老条目缺中文名时不能猜它属于哪个同名集：清掉可能串用的版本，
            // 另建含完整身份的新记录。
            if !entry
                .level_set_name_zh
                .trim()
                .eq_ignore_ascii_case(metadata.name_zh.as_deref().unwrap_or_default().trim())
            {
                entry.installed_mapset_version.clear();
                continue;
            }
            matched = true;
            entry.installed_mapset_version = version.to_string();
        }
        if repository_id.is_some() || (!matched && !version.is_empty()) {
            // 带上中文身份并转义分隔符，两个 GAODA/GAODA 不能共用安装记录。
            let key = format!(
                "{INSTALLED_PREFIX}{:?}",
                (
                    metadata.author.trim(),
                    metadata.name.trim(),
                    metadata.name_zh.as_deref().unwrap_or_default().trim(),
                )
            );
            snapshots.entries.insert(
                key,
                crate::models::MapRelationSnapshot {
                    repository_id: repository_id.unwrap_or_default().to_owned(),
                    level_set_name: metadata.name.clone(),
                    level_set_name_zh: metadata.name_zh.clone().unwrap_or_default(),
                    author: metadata.author.clone(),
                    group_i18n: Default::default(),
                    group: String::new(),
                    relations: Vec::new(),
                    release_relations: Vec::new(),
                    installed_mapset_version: version.to_string(),
                },
            );
        }
    }
    if snapshots.entries != previous
        && let Err(error) = crate::archive::write_map_relation_snapshots(manifests_dir, &snapshots)
    {
        log::warn!(
            "记录关卡集资产版本失败：目录={}，原因={}",
            manifests_dir.display(),
            error
        );
    }
}
