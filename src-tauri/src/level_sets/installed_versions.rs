//! 主地图资产版本记录与旧快照兼容读取。

use super::identity::installed_map_identity_matches;
use super::zip_inspect::ImportedLevelSet;
use crate::models::{MapRelationSnapshot, MapRelationSnapshots};
use std::path::Path;

pub(crate) const INSTALLED_PREFIX: &str = "installed:";
pub(crate) const LOCAL_IMPORT_PREFIX: &str = "local:";

fn identity_suffix(name: &str, name_zh: Option<&str>, author: &str) -> String {
    format!(
        "{:?}",
        (
            author.trim().to_ascii_lowercase(),
            name.trim().to_ascii_lowercase(),
            name_zh.unwrap_or_default().trim().to_ascii_lowercase(),
        )
    )
}

pub(crate) fn is_local_import(
    snapshots: &MapRelationSnapshots,
    name: &str,
    name_zh: Option<&str>,
    author: &str,
) -> bool {
    !name.trim().is_empty()
        && snapshots.entries.contains_key(&format!(
            "{LOCAL_IMPORT_PREFIX}{}",
            identity_suffix(name, name_zh, author)
        ))
}

pub(crate) fn installed_repository_id<'a>(
    snapshots: &'a MapRelationSnapshots,
    name: &str,
    name_zh: Option<&str>,
    author: &str,
) -> Option<&'a str> {
    let mut ids = snapshots.entries.iter().filter_map(|(key, entry)| {
        (key.starts_with(INSTALLED_PREFIX)
            && !entry.repository_id.is_empty()
            && exact_identity(entry, name, name_zh, author))
        .then_some(entry.repository_id.as_str())
    });
    let first = ids.next()?;
    ids.all(|id| id == first).then_some(first)
}

fn exact_identity(
    entry: &MapRelationSnapshot,
    name: &str,
    name_zh: Option<&str>,
    author: &str,
) -> bool {
    installed_map_identity_matches(
        name,
        name_zh,
        author,
        &entry.level_set_name,
        Some(&entry.level_set_name_zh),
        &entry.author,
    )
}

fn recorded_identity(key: &str) -> bool {
    key.starts_with(INSTALLED_PREFIX) || key.starts_with(LOCAL_IMPORT_PREFIX)
}

fn has_nameless_install(snapshots: &MapRelationSnapshots, name: &str, author: &str) -> bool {
    snapshots
        .entries
        .iter()
        .any(|(key, entry)| recorded_identity(key) && exact_identity(entry, name, None, author))
}

/// 新快照有中文身份时优先精确匹配，避免旧版无中文名的记录抢走另一张同名图的版本。
/// 老快照仍按仓库 ID / 身份兜底，不因升级凭空抹掉修复版的安装记录。
pub fn installed_mapset_version<'a>(
    snapshots: &'a MapRelationSnapshots,
    repository_id: Option<&str>,
    name: &str,
    name_zh: Option<&str>,
    author: &str,
) -> &'a str {
    if is_local_import(snapshots, name, name_zh, author) {
        return "";
    }
    let exact = |entry: &&MapRelationSnapshot| exact_identity(entry, name, name_zh, author);
    let by_id = repository_id.and_then(|id| snapshots.entries.get(id));
    by_id
        .filter(|entry| exact(entry) && !entry.installed_mapset_version.trim().is_empty())
        .or_else(|| {
            snapshots
                .entries
                .values()
                .find(|entry| exact(entry) && !entry.installed_mapset_version.trim().is_empty())
        })
        .or_else(|| snapshots.entries.values().find(exact))
        .or_else(|| {
            if has_nameless_install(snapshots, name, author) {
                return None;
            }
            let legacy = |key: &str, entry: &MapRelationSnapshot| {
                !recorded_identity(key) && exact_identity(entry, name, None, author)
            };
            repository_id
                .and_then(|id| by_id.filter(|entry| legacy(id, entry)))
                .or_else(|| {
                    snapshots
                        .entries
                        .iter()
                        .find_map(|(key, entry)| legacy(key, entry).then_some(entry))
                })
        })
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
            installed_map_identity_matches(
                &metadata.name,
                metadata.name_zh.as_deref(),
                &metadata.author,
                &source.level_set_name,
                Some(&source.level_set_name_zh),
                &source.author,
            )
        });
        let version = installed
            .map(|source| source.mapset_version.trim())
            .unwrap_or("");
        let repository_id = installed.and_then(|source| source.repository_id.as_deref());
        let preserve_nameless_version =
            has_nameless_install(&snapshots, &metadata.name, &metadata.author);
        let mut matched = false;
        for (key, entry) in &mut snapshots.entries {
            if key.starts_with(LOCAL_IMPORT_PREFIX) {
                continue;
            }
            if exact_identity(
                entry,
                &metadata.name,
                metadata.name_zh.as_deref(),
                &metadata.author,
            ) {
                if key.starts_with(INSTALLED_PREFIX) {
                    entry.repository_id.clear();
                }
                matched = true;
                entry.installed_mapset_version = version.to_string();
            } else if !preserve_nameless_version
                && !key.starts_with(INSTALLED_PREFIX)
                && exact_identity(entry, &metadata.name, None, &metadata.author)
            {
                entry.installed_mapset_version.clear();
            }
        }
        let identity = identity_suffix(
            &metadata.name,
            metadata.name_zh.as_deref(),
            &metadata.author,
        );
        let local_key = format!("{LOCAL_IMPORT_PREFIX}{identity}");
        if installed.is_some() {
            snapshots.entries.remove(&local_key);
        }
        if origin.is_none() || repository_id.is_some() || (!matched && !version.is_empty()) {
            let key = if origin.is_none() {
                local_key
            } else {
                format!("{INSTALLED_PREFIX}{identity}")
            };
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
