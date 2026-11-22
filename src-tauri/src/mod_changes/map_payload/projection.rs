//! Merge actual sandbox maps into the trusted baseline without uninstalling it.
use super::*;
use std::collections::HashSet;

pub(super) fn seed_snapshots(
    config: &crate::models::AppConfig,
    metadata: &[&crate::asset_bundle::LevelSetMetadata],
    primary: Option<MapSource<'_, '_>>,
    manifests: &Path,
    operation: &str,
) -> Result<Vec<String>, String> {
    let mut snapshots = archive::read_map_relation_snapshots(manifests);
    let mut notices = Vec::new();
    for metadata in metadata {
        let explicit = primary.filter(|(main, _)| std::ptr::eq(*main, *metadata));
        if let Some((_, map)) = explicit {
            log::info!(
                "规划地图来源：操作={operation}，地图身份={}，选中来源={}",
                diagnostic(&(
                    metadata.name.as_str(),
                    metadata.name_zh.as_deref(),
                    metadata.author.as_str(),
                    metadata.version.as_str()
                )),
                diagnostic(&map.id)
            );
        }
        let matched = || {
            level_sets::source_relations::matching_maps(
                config,
                &metadata.name,
                metadata.name_zh.as_deref(),
                &metadata.author,
            )
        };
        if explicit.is_none() {
            let resolution =
                level_sets::source_relations::resolve(matched(), &metadata.version, &[]);
            log::info!(
                "规划地图来源：操作={operation}，地图身份={}，未指定来源，候选来源={}，适用声明={}，未覆盖此版本的来源={}，声明冲突={}，共同依赖={}",
                diagnostic(&(
                    metadata.name.as_str(),
                    metadata.name_zh.as_deref(),
                    metadata.author.as_str(),
                    metadata.version.as_str()
                )),
                diagnostic(&resolution.source_labels),
                resolution.applicable_sources,
                diagnostic(&resolution.uncovered_sources),
                resolution.conflict,
                diagnostic(&resolution.relations)
            );
            let display = metadata
                .name_zh
                .as_deref()
                .filter(|name| !name.trim().is_empty())
                .unwrap_or(&metadata.name);
            if resolution.conflict {
                notices.push(crate::locale_tag::with_params(
                    "levelSet.localMapSourceConflict",
                    &[
                        ("map", display),
                        ("sources", &resolution.source_labels.join(", ")),
                    ],
                ));
            }
            if resolution.applicable_sources == 0 && !resolution.uncovered_sources.is_empty() {
                notices.push(crate::locale_tag::with_params(
                    "levelSet.localMapSourceVersionUnconfirmed",
                    &[
                        ("map", display),
                        ("version", &metadata.version),
                        ("sources", &resolution.uncovered_sources.join("\n")),
                    ],
                ));
            }
        }
        let sources = explicit
            .map(|(_, map)| vec![map])
            .unwrap_or_else(|| matched().collect());
        for map in sources {
            let installed_mapset_version = snapshots
                .entries
                .get(&map.id)
                .filter(|entry| {
                    level_sets::installed_map_identity_matches(
                        &entry.level_set_name,
                        Some(&entry.level_set_name_zh),
                        &entry.author,
                        &map.level_set_name,
                        Some(&map.level_set_name_zh),
                        &map.author,
                    )
                })
                .map(|entry| entry.installed_mapset_version.clone())
                .unwrap_or_default();
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
                    installed_mapset_version,
                },
            );
        }
    }
    archive::write_map_relation_snapshots(manifests, &snapshots)?;
    Ok(notices)
}

fn diagnostic(value: &impl std::fmt::Debug) -> String {
    crate::http_diagnostics::redact_text(&format!("{value:?}"))
}

pub(super) fn merge(
    session: &mut PlanSession,
    roots: &TransactionRoots,
    batch: &mut MapBatch,
    projected: Inventory,
    projected_files: BTreeMap<String, NodeFiles>,
    previous_registry: &[level_sets::RegisteredLevelSetPlugin],
    expected_maps: usize,
    primary: Option<MapSource<'_, '_>>,
) -> Result<(), String> {
    let mut taken = session
        .trusted
        .files
        .values()
        .filter_map(|files| files.summary.map_dir_name.as_ref())
        .map(|name| name.to_lowercase())
        .collect::<HashSet<_>>();
    let mut imported: Vec<crate::asset_bundle::LevelSetMetadata> = Vec::new();
    let mut previous_matches = BTreeSet::new();
    for mut node in projected
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Map)
        .cloned()
    {
        let mut files = projected_files
            .get(&node.id)
            .cloned()
            .ok_or("modChanges.targetChanged")?;
        let metadata = files
            .map_metadata
            .clone()
            .ok_or("metadataRead.taskFailed")?;
        let stage_path = files.map_path.clone().ok_or("modChanges.targetChanged")?;
        if !node.mutable {
            return Err(InstallBlock::Unrecognized.report(&node.id, &stage_path));
        }
        if imported
            .iter()
            .any(|existing| level_sets::same_level_set(existing, &metadata))
        {
            return Err(InstallBlock::PathCollision.report(&node.id, &stage_path));
        }
        let matches = session
            .trusted
            .files
            .iter()
            .filter(|(_, files)| {
                files
                    .map_metadata
                    .as_ref()
                    .is_some_and(|old| level_sets::same_level_set(old, &metadata))
            })
            .map(|(id, files)| (id.clone(), files.clone()))
            .collect::<Vec<_>>();
        if matches.len() > 1 {
            let paths = matches
                .iter()
                .filter_map(|(_, old)| old.map_path.as_deref())
                .collect::<Vec<_>>()
                .join("\n");
            let display = metadata
                .name_zh
                .as_deref()
                .filter(|name| !name.trim().is_empty())
                .unwrap_or(&metadata.name);
            return Err(super::super::mutations::report_install_block(
                "levelSet.mapDuplicateInstallTargets",
                &node.id,
                &[("map", display), ("paths", &paths)],
            ));
        }
        let original_name = files
            .summary
            .map_dir_name
            .as_deref()
            .ok_or("modChanges.targetChanged")?;
        let (id, name, old_path, enabled) = if let Some((id, old)) = matches.first() {
            if !previous_matches.insert(id.clone()) {
                return Err(InstallBlock::PathCollision
                    .report(id, old.map_path.as_deref().unwrap_or(&stage_path)));
            }
            (
                id.clone(),
                old.summary
                    .map_dir_name
                    .clone()
                    .ok_or("modChanges.targetChanged")?,
                old.map_path.clone(),
                old.summary.enabled,
            )
        } else {
            let name = level_sets::allocate_dir_name(original_name, &metadata, &mut taken, true);
            let mut id = format!("map:{name}");
            if session.trusted.files.contains_key(&id) {
                id = format!("map-import:{}:{name}", session.plan.plan_id);
            }
            (id, name, None, true)
        };
        let package = session
            .trusted
            .level_roots
            .package_root()
            .map(|root| relative(&session.trusted.game_root, root))
            .transpose()?
            .unwrap_or_else(|| crate::framework::DIYLEVEL_DIR.to_string());
        let target_prefix = super::super::level_trees::map_destination(
            &session.trusted.game_root,
            &name,
            enabled,
            Some(&package),
        )?;
        let mut candidate = node.current.take().ok_or("modChanges.targetChanged")?;
        candidate.id = format!("map-import:{}:{id}", session.plan.plan_id);
        let mut roles = BTreeMap::new();
        for member in &files.summary.members {
            if let Some(role) = member.role {
                let mut target_key = Some(member.key.clone());
                remap_key(&mut target_key, &stage_path, &target_prefix)?;
                roles.extend(target_key.map(|key| (key, role)));
            }
        }
        for plugin in &mut candidate.provides {
            remap_key(&mut plugin.member_key, &stage_path, &target_prefix)?;
        }
        for owned in &mut candidate.relations {
            remap_key(&mut owned.owner_member_key, &stage_path, &target_prefix)?;
        }
        log::info!(
            "规划地图安装：操作={}，地图={}，版本={}，节点={}，启用={enabled}，旧位置={}，目标位置={}，依赖={}",
            session.operation_id(),
            diagnostic(&metadata.name),
            diagnostic(&metadata.version),
            diagnostic(&id),
            diagnostic(&old_path),
            diagnostic(&target_prefix),
            diagnostic(&candidate.relations)
        );
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
        if let Some((_, map)) =
            primary.filter(|(main, _)| level_sets::same_level_set(main, &metadata))
        {
            // Only the ZIP's verified main map may use the online card title.
            files.summary.label = if map.name.trim().is_empty() {
                format!("{}/{}", map.username, map.repository)
            } else {
                map.name.clone()
            };
            files.summary.name_i18n.clone_from(&map.name_i18n);
            files.summary.map_name_zh = None;
            files.summary.map_id = Some(map.id.clone());
        }
        files.summary.members = candidate
            .provides
            .iter()
            .filter_map(|plugin| {
                let key = plugin.member_key.as_ref()?;
                Some(MemberSummary {
                    key: key.clone(),
                    dll_file: key.clone(),
                    enabled: plugin.enabled,
                    guid: (!plugin.guid.is_empty()).then(|| plugin.guid.clone()),
                    role: roles.get(key).copied(),
                })
            })
            .collect();
        batch.member_roles.insert(candidate.id.clone(), roles);
        files.summary.dll_files = files
            .summary
            .members
            .iter()
            .map(|member| member.dll_file.clone())
            .collect();
        let staged = StagedMap {
            node_id: id.clone(),
            candidate_id: candidate.id.clone(),
            enabled,
            stage_path,
            member_prefix: target_prefix,
            target_dir: name,
            old_path,
        };
        insert_candidate(session, &id, candidate, files, NodeKind::Map, enabled)?;
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
        let mut files = projected_files
            .get(&node.id)
            .cloned()
            .ok_or("modChanges.targetChanged")?;
        let source = files
            .summary
            .members
            .first()
            .ok_or_else(|| {
                InstallBlock::MapRecordsUnreadable.report(
                    &node.id,
                    &format!("manifests/{}", level_sets::LEVEL_SET_PLUGIN_REGISTRY),
                )
            })?
            .dll_file
            .clone();
        let canonical = crate::mods::canonical_dll_name(&source);
        if !node.mutable {
            return Err(InstallBlock::Unrecognized.report(&node.id, &canonical));
        }
        let entry = registry
            .iter()
            .find(|entry| key(&entry.dll) == key(&canonical))
            .ok_or_else(|| {
                InstallBlock::MapRecordsUnreadable.report(
                    &node.id,
                    &format!("manifests/{}", level_sets::LEVEL_SET_PLUGIN_REGISTRY),
                )
            })?;
        let mut matching_maps = batch
            .maps
            .iter()
            .zip(&imported)
            .filter(|(_, metadata)| entry.owner.matches_install(metadata));
        let Some((map, _)) = matching_maps.next() else {
            return Err(InstallBlock::Unrecognized.report(&node.id, &canonical));
        };
        if matching_maps.next().is_some() {
            return Err(InstallBlock::Unrecognized.report(&node.id, &canonical));
        }
        let map_enabled = map.enabled;
        if previous_registry.iter().any(|old| {
            key(&old.dll) == key(&canonical)
                && !imported.iter().any(|metadata| {
                    old.owner.matches_install(metadata) && entry.owner.matches_install(metadata)
                })
        }) {
            return Err(InstallBlock::MapProtected.report(&node.id, &canonical));
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
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        if owners.len() > 1 {
            return Err(InstallBlock::PathOwned.report(&node.id, &canonical));
        }
        let mut candidate = node.current.take().ok_or("modChanges.targetChanged")?;
        let mut claimed = None;
        let id = node.id.clone();
        if let Some(owner) = owners
            .first()
            .filter(|owner| !owner.starts_with("map-dll:"))
        {
            if !batch.downloaded
                || session
                    .request
                    .roots
                    .iter()
                    .any(|root| &root.node_id == owner)
                || session.request.repair_dependencies_of.as_ref() == Some(owner)
            {
                return Err(InstallBlock::PathOwned.report(&id, &canonical));
            }
            let [plugin] = candidate.provides.as_slice() else {
                return Err(InstallBlock::Unrecognized.report(&id, &canonical));
            };
            claimed = Some(crate::mod_changes::mutations::prove_loose_dll(
                session,
                roots.clone(),
                owner,
                &canonical,
                plugin,
            )?);
            let old = session
                .trusted
                .inventory
                .nodes
                .iter_mut()
                .find(|old| &old.id == owner)
                .filter(|old| old.mutable)
                .ok_or_else(|| InstallBlock::PathOwned.report(&id, &canonical))?;
            old.id.clone_from(&id);
            old.candidates.clear();
            old.candidates_complete = true;
            let mut prior = session
                .trusted
                .files
                .remove(owner)
                .ok_or("modChanges.targetChanged")?;
            prior.summary.node_id.clone_from(&id);
            prior.summary.map_id.clone_from(&files.summary.map_id);
            prior.summary.repository_id = None;
            prior.summary.candidates.clear();
            session.trusted.files.insert(id.clone(), prior);
            session.trusted.toggle_unbound_nodes.remove(owner);
        }
        candidate.id = format!("map-import:{}:{id}", session.plan.plan_id);
        batch.member_roles.insert(
            candidate.id.clone(),
            files
                .summary
                .members
                .iter()
                .filter_map(|member| Some((member.key.clone(), member.role?)))
                .collect(),
        );
        let mut enabled = map_enabled;
        if let Some(old) = session
            .trusted
            .inventory
            .nodes
            .iter()
            .find(|node| node.id == id)
        {
            if let Some(current) = old.current.as_ref() {
                enabled = old.enabled;
                inherit_switches(current, &mut candidate)?;
            }
        } else {
            for member in &mut candidate.provides {
                member.enabled = enabled;
            }
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
            claimed,
            detached_active: !map_enabled
                && enabled
                && candidate.provides.iter().any(|member| member.enabled),
        };
        insert_candidate(session, &id, candidate, files, NodeKind::Mod, enabled)?;
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
    enabled: bool,
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
            let path = session
                .trusted
                .files
                .get(id)
                .and_then(|old| old.map_path.as_deref())
                .or_else(|| files.summary.dll_files.first().map(String::as_str))
                .or(files.map_path.as_deref())
                .unwrap_or(id);
            return Err(InstallBlock::Unrecognized.report(id, path));
        }
        if kind == NodeKind::Map {
            let summary = &mut session
                .trusted
                .files
                .get_mut(id)
                .ok_or("modChanges.targetChanged")?
                .summary;
            summary.label = files.summary.label;
            summary.name_i18n = files.summary.name_i18n;
            summary.map_name_zh = files.summary.map_name_zh;
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
        enabled,
        member_key: None,
    });
    Ok(())
}

fn remap_key(member: &mut Option<String>, from: &str, to: &str) -> Result<(), String> {
    if let Some(path) = member {
        if !within(path, from) {
            return Err(InstallBlock::OutsidePackage.report("map-import", path));
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
            return Err(super::super::mutations::package_identity_conflict(
                "map-import",
                &member.guid,
                by_guid,
            ));
        }
        if let Some(old) = exact.or_else(|| by_guid.first().copied()) {
            member.enabled = old.enabled;
        }
    }
    Ok(())
}
