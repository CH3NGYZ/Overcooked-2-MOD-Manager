use crate::app_state::AppState;
use crate::archive;
use crate::models::{DependencySyncResult, MapRelationSnapshot, MapRelationSnapshots};
use crate::mods;
use std::collections::BTreeMap;
use tauri::State;

/// 把线上依赖声明同步进本地清单（MOD 安装清单 + 地图依赖清单）。
///
/// 依赖声明只存在于仓库源文档里，源下线/文件被删之后本地就查不到了；
/// 所以安装时固化一份，刷新仓库后再按线上声明刷新 —— 在线与本地不一致时以在线为准。
///
/// 只读本地文件、不扫描游戏目录、不发网络请求，所以只取短的 `operation` 锁。
#[tauri::command]
pub fn sync_dependency_snapshots(
    state: State<'_, AppState>,
) -> Result<DependencySyncResult, String> {
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let manifest_dir = state.config.manifests_dir();

    let mut mods_updated = 0usize;
    for mut manifest in archive::read_manifests(&manifest_dir) {
        // `manual-` 开头是本地文件/在线直链安装：没有仓库，也就没有线上声明
        if manifest.repository_id.starts_with("manual-") {
            continue;
        }
        /*
         * 仓库找不到时原样跳过、不清空：源还没刷新出来、地区/平台不适用、
         * 自定义源被删都只是「当前查不到」，快照仍是已知的最新事实。
         */
        let Some(repo) = config
            .repositories
            .iter()
            .find(|repo| repo.id == manifest.repository_id)
        else {
            continue;
        };
        let relations = mods::effective_relations(repo, Some(&manifest.release_tag));
        if relations == manifest.relations {
            continue;
        }
        manifest.relations = relations;
        match archive::write_manifest(&manifest_dir, &manifest) {
            Ok(()) => mods_updated += 1,
            Err(error) => log::warn!(
                "同步 MOD 依赖快照失败：仓库={}，原因={}",
                manifest.repository_id,
                error
            ),
        }
    }

    /*
     * 地图快照整体重写，但配置里已经没有的 id **只保留分组**：地图源被删之后
     * 这张图确实不再有线上依赖声明，留着旧 relations 等于给用户看一份幽灵
     * 依赖；可「它归到哪个作者」是安装时的事实，管理页的折叠分组与详情里的
     * 作者名还要靠它。没有分组的旧条目没有保留价值，直接丢掉。
     */
    let previous = archive::read_map_relation_snapshots(&manifest_dir);
    /*
     * 资产版本搬回重建后的条目：它是安装那一刻的事实，线上声明里没有这一项。
     * 不搬的话每次刷新地图源都会把「本地装的是 3.4.1-fix」抹掉，界面又退回
     * 只比 info 版本 —— 而那正是分不出 `-fix` 的情况。
     */
    let mut entries: BTreeMap<String, MapRelationSnapshot> = config
        .map_repositories
        .iter()
        .filter(|map| map.enabled)
        .map(|map| {
            let installed_mapset_version = crate::level_sets::installed_mapset_version(
                &previous,
                Some(&map.id),
                &map.level_set_name,
                Some(&map.level_set_name_zh),
                &map.author,
            )
            .to_string();
            (
                map.id.clone(),
                MapRelationSnapshot {
                    repository_id: map.id.clone(),
                    level_set_name: map.level_set_name.clone(),
                    level_set_name_zh: map.level_set_name_zh.clone(),
                    author: map.author.clone(),
                    group_i18n: map.group_i18n.clone(),
                    group: map.group.clone(),
                    relations: map.relations.clone(),
                    release_relations: map.release_relations.clone(),
                    installed_mapset_version,
                },
            )
        })
        .collect();
    for (id, entry) in &previous.entries {
        // 资产版本与分组同理：源被删掉之后这条记录仍然有用，不能一起清理掉
        if entries.contains_key(id)
            || (entry.group_i18n.is_empty()
                && entry.group.trim().is_empty()
                && entry.installed_mapset_version.is_empty())
        {
            continue;
        }
        entries.insert(
            id.clone(),
            MapRelationSnapshot {
                relations: Vec::new(),
                release_relations: Vec::new(),
                ..entry.clone()
            },
        );
    }
    let changed = entries
        .iter()
        .filter(|(id, entry)| previous.entries.get(*id) != Some(*entry))
        .count()
        + previous
            .entries
            .keys()
            .filter(|id| !entries.contains_key(*id))
            .count();
    let mut maps_updated = 0usize;
    if changed > 0 {
        let snapshots = MapRelationSnapshots {
            schema_version: crate::models::default_snapshot_schema_version(),
            entries,
        };
        match archive::write_map_relation_snapshots(&manifest_dir, &snapshots) {
            Ok(()) => maps_updated = changed,
            Err(error) => log::warn!("同步地图依赖快照失败：原因={}", error),
        }
    }
    log::info!(
        "依赖快照同步完成：MOD={}，地图={}",
        mods_updated,
        maps_updated
    );
    Ok(DependencySyncResult {
        mods_updated,
        maps_updated,
    })
}
