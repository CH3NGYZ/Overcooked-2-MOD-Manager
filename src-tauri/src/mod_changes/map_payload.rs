//! Map ZIPs use the existing installer inside a private game-shaped sandbox.
//! Only the resulting verified model and explicit transaction endpoints leave it.
use super::{
    inventory::*, model::*, mutations::file_conflict, session::PlanSession, transaction::*,
};
use crate::{app_state::AppState, archive, level_sets, models::MapInstallOrigin};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

mod output;
mod projection;
pub(super) use output::mutations;

type MapSource<'metadata, 'source> = (
    &'metadata crate::asset_bundle::LevelSetMetadata,
    &'source crate::models::MapRepositoryConfig,
);

pub(crate) struct MapBatch {
    pub stage_root: PathBuf,
    pub manifests_root: PathBuf,
    pub source_path: PathBuf,
    pub source_fingerprint: Fingerprint,
    pub node_ids: BTreeSet<String>,
    maps: Vec<StagedMap>,
    external: Vec<StagedExternal>,
    extra_files: Vec<String>,
}

impl MapBatch {
    pub(super) fn has_external_plugin(&self, id: &str) -> bool {
        self.external.iter().any(|plugin| plugin.node_id == id)
    }

    pub(crate) fn registry_paths(&self, id: &str, target: &str) -> Vec<(String, String)> {
        self.maps
            .iter()
            .filter(|map| map.node_id == id)
            .flat_map(|map| {
                [
                    (map.stage_path.clone(), target.to_owned()),
                    (map.member_prefix.clone(), target.to_owned()),
                ]
            })
            .collect()
    }
}

struct StagedMap {
    node_id: String,
    candidate_id: String,
    stage_path: String,
    member_prefix: String,
    target_dir: String,
    old_path: Option<String>,
}

struct StagedExternal {
    node_id: String,
    candidate_id: String,
    stage_path: String,
    target: String,
}

pub(crate) fn attach(state: &AppState, session: &mut PlanSession) -> Result<(), String> {
    let mut request = session
        .wire_request
        .map_import
        .clone()
        .ok_or("modChanges.targetChanged")?;
    let source = PathBuf::from(&request.zip_path);
    let source_fingerprint = Fingerprint::capture(&source)?;
    if !matches!(source_fingerprint, Fingerprint::File { .. }) {
        return Err("levelSet.zipWithoutInfo".into());
    }
    archive::validate_zip(&source)?;
    let config = &session.trusted.config;
    let sets = level_sets::inspect_zip_with_progress(
        &source,
        crate::concurrency::workers_from(config.map_info_concurrency),
        crate::concurrency::workers_from(config.mod_metadata_concurrency),
        &|_, _, _| {},
    )?;
    let selected = request.selected.iter().collect::<BTreeSet<_>>();
    if selected.is_empty()
        || selected.len() != request.selected.len()
        || selected.iter().any(|name| {
            sets.iter()
                .filter(|set| set.dir_name.as_str() == name.as_str())
                .count()
                != 1
        })
    {
        return Err("levelSet.nothingSelected".into());
    }
    let chosen = sets
        .iter()
        .filter(|set| selected.contains(&set.dir_name))
        .collect::<Vec<_>>();
    let metadata = chosen.iter().map(|set| &set.metadata).collect::<Vec<_>>();
    let primary = validate_origin(&metadata, request.origin.as_ref(), config)?;
    if let (Some((_, map)), Some(origin)) = (primary, request.origin.as_mut()) {
        // An old caller did not send an ID; resolve it only if the source was unique.
        origin.repository_id = Some(map.id.clone());
    }
    let stage_root = create_sandbox(state, session, &session.staging.join("map-game"))?;
    let manifests_root = create_sandbox(state, session, &session.staging.join("map-manifests"))?;
    copy_metadata(&state.config.manifests_dir(), &manifests_root)?;
    projection::seed_snapshots(config, &metadata, primary, &manifests_root)?;
    let roots = level_sets::LevelSetRoots::new(
        stage_root.clone(),
        Some(stage_root.join(crate::framework::DIYLEVEL_DIR)),
    );
    level_sets::install_parsed_sets(
        &sets,
        &source,
        &roots,
        &manifests_root,
        &request.selected,
        request.origin.as_ref(),
    )?;
    verify_staged_source(&manifests_root, primary)?;
    if Fingerprint::capture(&source)? != source_fingerprint {
        return Err("modChanges.payloadChanged".into());
    }
    let mut inventory = Inventory::default();
    let mut files = BTreeMap::new();
    super::inventory_maps::append_at(
        &manifests_root,
        config,
        &stage_root,
        &roots,
        &mut inventory,
        &mut files,
    )?;
    let mut batch = MapBatch {
        stage_root,
        manifests_root,
        source_path: source,
        source_fingerprint,
        node_ids: BTreeSet::new(),
        maps: vec![],
        external: vec![],
        extra_files: vec![],
    };
    let registered = level_sets::read_registered_plugins(&state.config.manifests_dir());
    let primary = primary.map(|(metadata, source)| (metadata, source.clone()));
    projection::merge(
        session,
        &mut batch,
        inventory,
        files,
        &registered,
        chosen.len(),
        primary
            .as_ref()
            .map(|(metadata, source)| (*metadata, source)),
    )?;
    batch.extra_files = loose_files(&batch)?;
    session.map_payload = Some(batch);
    Ok(())
}

fn create_sandbox(state: &AppState, session: &PlanSession, path: &Path) -> Result<PathBuf, String> {
    let data = state
        .config
        .config_path()
        .parent()
        .ok_or("archive.pathEscape")?
        .to_path_buf();
    if !path.is_absolute()
        || !path.starts_with(&data)
        || !path.starts_with(&session.staging)
        || path.starts_with(&session.trusted.game_root)
        || Fingerprint::capture(path)? != Fingerprint::Missing
    {
        return Err("archive.pathEscape".into());
    }
    // The scanner returns canonical paths, including the Windows verbatim
    // prefix. Normalize the approved sandbox before using it as a scan root.
    // Check physical scope too: the same path can have different spellings.
    let resolve = crate::game::resolve_existing_ancestor;
    let data = resolve(&data)?;
    let staging = resolve(&session.staging)?;
    let game = resolve(&session.trusted.game_root)?;
    let sandbox = resolve(path)?;
    if staging == data
        || !staging.starts_with(&data)
        || sandbox == staging
        || !sandbox.starts_with(&staging)
        || sandbox.starts_with(&game)
        || game.starts_with(&sandbox)
    {
        return Err("archive.pathEscape".into());
    }
    fs::create_dir(&sandbox).map_err(|error| error.to_string())?;
    Ok(sandbox)
}

fn copy_metadata(from: &Path, to: &Path) -> Result<(), String> {
    for name in [level_sets::LEVEL_SET_PLUGIN_REGISTRY, "map-relations.json"] {
        let source = from.join(name);
        match Fingerprint::capture(&source)? {
            Fingerprint::Missing => continue,
            Fingerprint::File { .. } => (),
            _ => return Err("modChanges.ambiguousOwnership".into()),
        }
        let bytes = fs::read(&source).map_err(|error| error.to_string())?;
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|_| "modChanges.ambiguousOwnership")?;
        let valid = if name == level_sets::LEVEL_SET_PLUGIN_REGISTRY {
            value
                .get("version")
                .and_then(|value| value.as_u64())
                .is_some_and(|version| version <= 1)
                && value
                    .get("entries")
                    .is_some_and(serde_json::Value::is_array)
        } else {
            value
                .get("schema_version")
                .and_then(|value| value.as_u64())
                .is_none_or(|version| version <= 1)
                && value
                    .get("entries")
                    .is_some_and(serde_json::Value::is_object)
                && serde_json::from_slice::<crate::models::MapRelationSnapshots>(&bytes).is_ok()
        };
        if !valid {
            return Err("modChanges.ambiguousOwnership".into());
        }
        fs::write(to.join(name), bytes).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn validate_origin<'metadata, 'source>(
    metadata: &[&'metadata crate::asset_bundle::LevelSetMetadata],
    origin: Option<&MapInstallOrigin>,
    config: &'source crate::models::AppConfig,
) -> Result<Option<MapSource<'metadata, 'source>>, String> {
    let Some(origin) = origin else {
        return Ok(None);
    };
    let matches = |name: &str, zh: Option<&str>, author: &str| {
        level_sets::level_set_identity_matches(
            name,
            zh,
            author,
            &origin.level_set_name,
            Some(&origin.level_set_name_zh),
            &origin.author,
        )
    };
    let mut chosen = metadata.iter().copied().filter(|m| {
        matches(&m.name, m.name_zh.as_deref(), &m.author)
            && m.name_zh
                .as_deref()
                .unwrap_or_default()
                .trim()
                .eq_ignore_ascii_case(origin.level_set_name_zh.trim())
    });
    let primary = chosen
        .next()
        .filter(|_| chosen.next().is_none())
        .ok_or_else(|| {
            log::warn!("地图更新拒绝：ZIP 里没有唯一的主地图");
            "modChanges.targetChanged".to_owned()
        })?;
    let source = if let Some(id) = origin.repository_id.as_deref() {
        level_sets::map_repository_by_id(&config.map_repositories, id)
    } else {
        let mut sources = config
            .map_repositories
            .iter()
            .filter(|m| matches(&m.level_set_name, Some(&m.level_set_name_zh), &m.author));
        sources
            .next()
            .filter(|_| sources.next().is_none())
            .and_then(|map| level_sets::map_repository_by_id(&config.map_repositories, &map.id))
    };
    let Some(source) = source.filter(|map| {
        let mut matching = metadata.iter().copied().filter(|m| {
            level_sets::level_set_identity_matches(
                &m.name,
                m.name_zh.as_deref(),
                &m.author,
                &map.level_set_name,
                Some(&map.level_set_name_zh),
                &map.author,
            )
        });
        matches(
            &map.level_set_name,
            Some(&map.level_set_name_zh),
            &map.author,
        ) && matching
            .next()
            .is_some_and(|candidate| std::ptr::eq(candidate, primary))
            && matching.next().is_none()
    }) else {
        log::warn!("地图更新拒绝：已选地图源不存在、重复或与 ZIP 主地图身份不一致");
        return Err("modChanges.targetChanged".into());
    };
    Ok(Some((primary, source)))
}

fn verify_staged_source(
    manifests: &Path,
    primary: Option<MapSource<'_, '_>>,
) -> Result<(), String> {
    if let Some((metadata, source)) = primary {
        let snapshots = archive::read_map_relation_snapshots(manifests);
        let persisted = level_sets::installed_repository_id(
            &snapshots,
            &metadata.name,
            metadata.name_zh.as_deref(),
            &metadata.author,
        );
        // Existing importers log snapshot write errors. A planner must not use
        // a different channel's relations if the chosen source was not recorded.
        if persisted != Some(source.id.as_str()) {
            log::warn!("地图更新拒绝：暂存未保存已选地图源");
            return Err("modChanges.targetChanged".into());
        }
    }
    Ok(())
}

fn loose_files(batch: &MapBatch) -> Result<Vec<String>, String> {
    let mut result = Vec::new();
    for entry in walkdir::WalkDir::new(&batch.stage_root).follow_links(false) {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry.file_type().is_symlink() {
            return Err("archive.pathEscape".into());
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let path = relative(&batch.stage_root, entry.path())?;
        if batch.maps.iter().any(|map| within(&path, &map.stage_path))
            || batch
                .external
                .iter()
                .any(|external| key(&path) == key(&external.stage_path))
        {
            continue;
        }
        // Every loader input must participate in the same evaluated graph.
        if path.to_ascii_lowercase().ends_with(".dll")
            || path.to_ascii_lowercase().ends_with(".dll.disabled")
        {
            log::warn!("地图批次拒绝未归属 DLL：文件={path}");
            return Err("modChanges.ambiguousOwnership".into());
        }
        result.push(path);
    }
    result.sort();
    Ok(result)
}

fn within(path: &str, parent: &str) -> bool {
    let path = key(path);
    let parent = key(parent);
    path == parent
        || path
            .strip_prefix(&parent)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn same_identity(
    a: &crate::asset_bundle::LevelSetMetadata,
    b: &crate::asset_bundle::LevelSetMetadata,
) -> bool {
    level_sets::level_set_identity_matches(
        &a.name,
        a.name_zh.as_deref(),
        &a.author,
        &b.name,
        b.name_zh.as_deref(),
        &b.author,
    )
}

#[cfg(test)]
mod tests;
