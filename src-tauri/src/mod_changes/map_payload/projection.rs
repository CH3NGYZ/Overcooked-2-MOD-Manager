//! Merge actual sandbox maps into the trusted baseline without uninstalling it.
use super::*;
use std::collections::HashSet;

pub(super) fn seed_snapshots(
    config: &crate::models::AppConfig,
    metadata: &[&crate::asset_bundle::LevelSetMetadata],
    primary: Option<MapSource<'_>>,
    manifests: &Path,
) -> Result<(), String> {
    let mut snapshots = archive::read_map_relation_snapshots(manifests);
    for metadata in metadata {
        let map =
            if let Some((_, source)) = primary.filter(|(main, _)| std::ptr::eq(*main, *metadata)) {
                Some(source)
            } else {
                let mut sources = config.map_repositories.iter().filter(|map| {
                    level_sets::level_set_identity_matches(
                        &metadata.name,
                        metadata.name_zh.as_deref(),
                        &metadata.author,
                        &map.level_set_name,
                        Some(&map.level_set_name_zh),
                        &map.author,
                    )
                });
                let map = sources.next();
                if sources.next().is_some() {
                    return Err("modChanges.ambiguousOwnership".into());
                }
                map
            };
        if let Some(map) = map {
            snapshots.entries.insert(
                map.id.clone(),
                crate::models::MapRelationSnapshot {
                    repository_id: map.id.clone(),
                    level_set_name: map.level_set_name.clone(),
                    level_set_name_zh: map.level_set_name_zh.clone(),
                    author: map.author.clone(),
                    group_i18n: map.group_i18n.clone(),
                    group: map.group.clone(),
                    relations: map.relations.clone(),
                    release_relations: map.release_relations.clone(),
                    installed_mapset_version: String::new(),
                },
            );
        }
    }
    archive::write_map_relation_snapshots(manifests, &snapshots)
}

pub(super) fn merge(
    session: &mut PlanSession,
    batch: &mut MapBatch,
    projected: Inventory,
    projected_files: BTreeMap<String, NodeFiles>,
    previous_registry: &[level_sets::RegisteredLevelSetPlugin],
    expected_maps: usize,
) -> Result<(), String> {
    let mut taken = session
        .trusted
        .files
        .values()
        .filter_map(|files| files.summary.map_dir_name.as_ref())
        .map(|name| name.to_lowercase())
        .collect::<HashSet<_>>();
    let mut imported = Vec::new();
    let mut previous_matches = BTreeSet::new();
    for mut node in projected
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Map)
        .cloned()
    {
        if !node.mutable {
            return Err("modChanges.ambiguousOwnership".into());
        }
        let mut files = projected_files
            .get(&node.id)
            .cloned()
            .ok_or("modChanges.targetChanged")?;
        let metadata = files
            .map_metadata
            .clone()
            .ok_or("metadataRead.taskFailed")?;
        if imported
            .iter()
            .any(|existing| same_identity(existing, &metadata))
        {
            return Err("modChanges.ambiguousOwnership".into());
        }
        let matches = session
            .trusted
            .files
            .iter()
            .filter(|(_, files)| {
                files
                    .map_metadata
                    .as_ref()
                    .is_some_and(|old| same_identity(old, &metadata))
            })
            .map(|(id, files)| (id.clone(), files.clone()))
            .collect::<Vec<_>>();
        if matches.len() > 1 {
            return Err("modChanges.ambiguousOwnership".into());
        }
        let stage_path = files.map_path.clone().ok_or("modChanges.targetChanged")?;
        let original_name = files
            .summary
            .map_dir_name
            .as_deref()
            .ok_or("modChanges.targetChanged")?;
        let (id, name, old_path) = if let Some((id, old)) = matches.first() {
            if !previous_matches.insert(id.clone()) {
                return Err("modChanges.ambiguousOwnership".into());
            }
            (
                id.clone(),
                old.summary
                    .map_dir_name
                    .clone()
                    .ok_or("modChanges.targetChanged")?,
                old.map_path.clone(),
            )
        } else {
            let name = level_sets::allocate_dir_name(original_name, &metadata, &mut taken, true);
            let mut id = format!("map:{name}");
            if session.trusted.files.contains_key(&id) {
                id = format!("map-import:{}:{name}", session.plan.plan_id);
            }
            (id, name, None)
        };
        let package = session
            .trusted
            .level_roots
            .package_root()
            .map(|root| relative(&session.trusted.game_root, root))
            .transpose()?
            .unwrap_or_else(|| crate::framework::DIYLEVEL_DIR.to_string());
        let target_prefix = format!("{package}/levels/{name}");
        let mut candidate = node.current.take().ok_or("modChanges.targetChanged")?;
        candidate.id = format!("map-import:{}:{id}", session.plan.plan_id);
        for plugin in &mut candidate.provides {
            remap_key(&mut plugin.member_key, &stage_path, &target_prefix)?;
        }
        for owned in &mut candidate.relations {
            remap_key(&mut owned.owner_member_key, &stage_path, &target_prefix)?;
        }
        if let Some(old) = session
            .trusted
            .inventory
            .nodes
            .iter()
            .find(|node| node.id == id)
            .and_then(|node| node.current.as_ref())
        {
            inherit_switches(old, &mut candidate)?;
        }
        files.summary.node_id = id.clone();
        files.summary.map_dir_name = Some(name.clone());
        files.summary.members = candidate
            .provides
            .iter()
            .filter_map(|plugin| {
                Some(MemberSummary {
                    key: plugin.member_key.clone()?,
                    dll_file: plugin.member_key.clone()?,
                    enabled: plugin.enabled,
                    guid: (!plugin.guid.is_empty()).then(|| plugin.guid.clone()),
                })
            })
            .collect();
        files.summary.dll_files = files
            .summary
            .members
            .iter()
            .map(|member| member.dll_file.clone())
            .collect();
        let staged = StagedMap {
            node_id: id.clone(),
            candidate_id: candidate.id.clone(),
            stage_path,
            member_prefix: target_prefix,
            target_dir: name,
            old_path,
        };
        insert_candidate(session, &id, candidate, files, NodeKind::Map)?;
        batch.node_ids.insert(id);
        batch.maps.push(staged);
        imported.push(metadata);
    }
    if imported.len() != expected_maps {
        return Err("modChanges.targetChanged".into());
    }
    let registry = level_sets::read_registered_plugins(&batch.manifests_root);
    for mut node in projected
        .nodes
        .into_iter()
        .filter(|node| node.kind != NodeKind::Map)
    {
        if !node.mutable {
            return Err("modChanges.ambiguousOwnership".into());
        }
        let mut files = projected_files
            .get(&node.id)
            .cloned()
            .ok_or("modChanges.targetChanged")?;
        let source = files
            .summary
            .members
            .first()
            .ok_or("modChanges.ambiguousOwnership")?
            .dll_file
            .clone();
        let canonical = crate::mods::canonical_dll_name(&source);
        let entry = registry
            .iter()
            .find(|entry| key(&entry.dll) == key(&canonical))
            .ok_or("modChanges.ambiguousOwnership")?;
        if imported
            .iter()
            .filter(|metadata| entry.owner.matches(metadata))
            .count()
            != 1
        {
            return Err("modChanges.ambiguousOwnership".into());
        }
        if previous_registry.iter().any(|old| {
            key(&old.dll) == key(&canonical)
                && !imported
                    .iter()
                    .any(|metadata| old.owner.matches(metadata) && entry.owner.matches(metadata))
        }) {
            return Err("modChanges.ambiguousOwnership".into());
        }
        let owners = session
            .trusted
            .files
            .iter()
            .filter(|(_, files)| {
                files.summary.members.iter().any(|member| {
                    key(&crate::mods::canonical_dll_name(&member.dll_file)) == key(&canonical)
                })
            })
            .collect::<Vec<_>>();
        if owners.len() > 1
            || owners
                .first()
                .is_some_and(|(id, _)| !id.starts_with("map-dll:"))
        {
            return Err(file_conflict(&node.id, &canonical));
        }
        let id = owners
            .first()
            .map(|(id, _)| (*id).clone())
            .unwrap_or_else(|| node.id.clone());
        let mut candidate = node.current.take().ok_or("modChanges.targetChanged")?;
        candidate.id = format!("map-import:{}:{id}", session.plan.plan_id);
        if let Some(old) = session
            .trusted
            .inventory
            .nodes
            .iter()
            .find(|node| node.id == id)
            .and_then(|node| node.current.as_ref())
        {
            inherit_switches(old, &mut candidate)?;
        }
        let physical = if batch.stage_root.join(&canonical).is_file() {
            canonical.clone()
        } else {
            crate::mods::disabled_dll_file(&canonical)
        };
        files.summary.node_id = id.clone();
        let external = StagedExternal {
            node_id: id.clone(),
            candidate_id: candidate.id.clone(),
            stage_path: physical,
            target: canonical,
        };
        insert_candidate(session, &id, candidate, files, NodeKind::Mod)?;
        batch.node_ids.insert(id);
        batch.external.push(external);
    }
    Ok(())
}

fn insert_candidate(
    session: &mut PlanSession,
    id: &str,
    candidate: Candidate,
    mut files: NodeFiles,
    kind: NodeKind,
) -> Result<(), String> {
    if session.request.roots.iter().any(|root| root.node_id == id) {
        return Err("modChanges.targetChanged".into());
    }
    let candidate_id = candidate.id.clone();
    if let Some(existing) = session
        .trusted
        .inventory
        .nodes
        .iter_mut()
        .find(|node| node.id == id)
    {
        if !existing.mutable {
            return Err("modChanges.ambiguousOwnership".into());
        }
        existing.candidates.push(candidate);
    } else {
        files.map_path = None;
        files.summary.enabled = false;
        files.summary.members.clear();
        files.summary.dll_files.clear();
        session.trusted.files.insert(id.into(), files);
        session.trusted.inventory.nodes.push(ChangeNode {
            id: id.into(),
            kind,
            current: None,
            enabled: false,
            mutable: true,
            candidates: vec![candidate],
            candidates_complete: true,
        });
    }
    session.request.roots.push(RootIntent {
        node_id: id.into(),
        candidate_id: Some(candidate_id),
        enabled: true,
        member_key: None,
    });
    Ok(())
}

fn remap_key(member: &mut Option<String>, from: &str, to: &str) -> Result<(), String> {
    if let Some(path) = member {
        if !within(path, from) {
            return Err("modChanges.ambiguousOwnership".into());
        }
        *path = key(&format!("{to}{}", &path[from.len()..]));
    }
    Ok(())
}

fn inherit_switches(previous: &Candidate, current: &mut Candidate) -> Result<(), String> {
    for member in &mut current.provides {
        let exact = previous
            .provides
            .iter()
            .find(|old| old.member_key == member.member_key);
        let by_guid = previous
            .provides
            .iter()
            .filter(|old| !member.guid.is_empty() && old.guid.eq_ignore_ascii_case(&member.guid))
            .collect::<Vec<_>>();
        if exact.is_none() && by_guid.len() > 1 {
            return Err("modChanges.ambiguousOwnership".into());
        }
        if let Some(old) = exact.or_else(|| by_guid.first().copied()) {
            member.enabled = old.enabled;
        }
    }
    Ok(())
}
