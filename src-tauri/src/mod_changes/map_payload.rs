//! Map ZIPs use the existing installer inside a private game-shaped sandbox.
//! Only the resulting verified model and explicit transaction endpoints leave it.
use super::{inventory::*, model::*, session::PlanSession, transaction::*};
use crate::{app_state::AppState, archive, level_sets, models::MapInstallOrigin};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

mod output;
mod projection;

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
    let request = session
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
    validate_origin(
        &chosen.iter().map(|set| &set.metadata).collect::<Vec<_>>(),
        request.origin.as_ref(),
        config,
    )?;
    let stage_root = session.staging.join("map-game");
    let manifests_root = session.staging.join("map-manifests");
    create_sandbox(state, session, &stage_root)?;
    create_sandbox(state, session, &manifests_root)?;
    copy_metadata(&state.config.manifests_dir(), &manifests_root)?;
    projection::seed_snapshots(
        config,
        &chosen.iter().map(|set| &set.metadata).collect::<Vec<_>>(),
        &manifests_root,
    )?;
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
    projection::merge(
        session,
        &mut batch,
        inventory,
        files,
        &registered,
        chosen.len(),
    )?;
    batch.extra_files = loose_files(&batch)?;
    session.map_payload = Some(batch);
    Ok(())
}

pub(crate) fn mutations(state: &AppState, session: &PlanSession) -> Result<Vec<Mutation>, String> {
    output::build(state, session)
}

fn create_sandbox(state: &AppState, session: &PlanSession, path: &Path) -> Result<(), String> {
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
    fs::create_dir(path).map_err(|error| error.to_string())
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

fn validate_origin(
    metadata: &[&crate::asset_bundle::LevelSetMetadata],
    origin: Option<&MapInstallOrigin>,
    config: &crate::models::AppConfig,
) -> Result<(), String> {
    let Some(origin) = origin else {
        return Ok(());
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
    if metadata
        .iter()
        .filter(|m| matches(&m.name, m.name_zh.as_deref(), &m.author))
        .count()
        != 1
        || config
            .map_repositories
            .iter()
            .filter(|m| matches(&m.level_set_name, Some(&m.level_set_name_zh), &m.author))
            .count()
            != 1
    {
        return Err("modChanges.targetChanged".into());
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
