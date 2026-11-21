//! Resolve map endpoints only after every dependency asset has actual member paths.
use super::*;
use crate::mod_changes::application_progress::{ItemKey, OwnedMutation};

pub(in crate::mod_changes) fn mutations(
    roots: &TransactionRoots,
    session: &PlanSession,
) -> Result<Vec<OwnedMutation>, String> {
    let batch = session
        .map_payload
        .as_ref()
        .ok_or("modChanges.notPrepared")?;
    if Fingerprint::capture(&batch.source_path)? != batch.source_fingerprint {
        return Err("modChanges.payloadChanged".into());
    }
    let solved = session.solved.as_ref().ok_or("modChanges.notPrepared")?;
    let package =
        super::super::mutations::final_package(solved)?.ok_or("framework.diyLevelMissing")?;
    let mut result = Vec::new();
    let mut targets = BTreeSet::new();
    let replacements = batch
        .maps
        .iter()
        .map(|map| crate::mod_changes::level_trees::Replacement {
            node_id: map.node_id.clone(),
            source: batch.stage_root.join(&map.stage_path),
            target_dir: map.target_dir.clone(),
            old_path: map.old_path.clone(),
            member_prefix: map.member_prefix.clone(),
        })
        .collect::<Vec<_>>();
    let migration = crate::mod_changes::level_trees::compose(session, roots, &replacements)?;
    if let Some(migration) = &migration {
        for owned in &migration.mutations {
            if let Mutation::Tree(change) = &owned.mutation {
                reserve(&mut targets, &change.target, "map-migration")?;
            }
        }
    }
    let map_paths = crate::mod_changes::level_trees::final_map_paths(session, roots)?;
    crate::mod_changes::level_trees::rewrite_registry(
        &batch
            .manifests_root
            .join(level_sets::LEVEL_SET_PLUGIN_REGISTRY),
        migration.as_ref().map(|m| m.from.as_str()).unwrap_or(""),
        migration.as_ref().map(|m| m.to.as_str()).unwrap_or(""),
        &map_paths,
    )?;
    for map in &batch.maps {
        let node = selected(solved, &map.node_id, &map.candidate_id)?;
        let current = node.current.as_ref().ok_or("modChanges.targetChanged")?;
        crate::mod_changes::level_trees::set_members(
            &batch.stage_root.join(&map.stage_path),
            &map.member_prefix,
            current,
        )?;
        let target = if node.enabled {
            format!("{package}/levels/{}", map.target_dir)
        } else {
            relative(
                &roots.game_root,
                &level_sets::disabled_root(&roots.game_root).join(&map.target_dir),
            )?
        };
        if migration
            .as_ref()
            .is_some_and(|composition| composition.handled_maps.contains(&map.node_id))
        {
            continue;
        }
        if let Some(old) = &map.old_path {
            if key(old) != key(&target) {
                push_tree(&mut result, &mut targets, roots, old, None, &map.node_id)?;
            }
        }
        let destination = game(&target).resolve(roots)?;
        if Fingerprint::capture(&destination)? != Fingerprint::Missing
            && map
                .old_path
                .as_deref()
                .is_none_or(|old| key(old) != key(&target))
        {
            return Err("modChanges.pathCollision".into());
        }
        let source = game(&map.stage_path).resolve(&TransactionRoots {
            game_root: batch.stage_root.clone(),
            data_root: roots.data_root.clone(),
        })?;
        push_tree(
            &mut result,
            &mut targets,
            roots,
            &target,
            Some(source),
            &map.node_id,
        )?;
    }
    if let Some(migration) = migration {
        result.extend(migration.mutations);
    }
    for external in &batch.external {
        let node = selected(solved, &external.node_id, &external.candidate_id)?;
        let current = node.current.as_ref().ok_or("modChanges.targetChanged")?;
        if current.provides.len() != 1 {
            return Err("modChanges.ambiguousOwnership".into());
        }
        let member = &current.provides[0];
        let canonical = crate::mods::canonical_dll_name(&external.target);
        let disabled = crate::mods::disabled_dll_file(&canonical);
        let active = game(&canonical).resolve(roots)?;
        let inactive = game(&disabled).resolve(roots)?;
        let active_before = Fingerprint::capture(&active)?;
        let inactive_before = Fingerprint::capture(&inactive)?;
        let has_active = active_before != Fingerprint::Missing;
        let has_inactive = inactive_before != Fingerprint::Missing;
        if has_active && has_inactive {
            for (path, before) in [(&canonical, &active_before), (&disabled, &inactive_before)] {
                if matches!(before, Fingerprint::Directory { .. }) {
                    return Err(InstallBlock::Directory.report(&external.node_id, path));
                }
            }
            return Err(InstallBlock::BothStates.report(&external.node_id, &canonical));
        }
        let owns_existing = session
            .trusted
            .files
            .get(&external.node_id)
            .is_some_and(|files| {
                files.summary.members.iter().any(|member| {
                    key(&crate::mods::canonical_dll_name(&member.dll_file)) == key(&canonical)
                })
            });
        if (has_active || has_inactive) && !owns_existing {
            let (path, before) = if has_active {
                (&canonical, &active_before)
            } else {
                (&disabled, &inactive_before)
            };
            return Err(occupied_target(before).report(&external.node_id, path));
        }
        let target = if node.enabled && member.enabled {
            canonical.clone()
        } else {
            disabled.clone()
        };
        let old = if has_active {
            Some(&canonical)
        } else if has_inactive {
            Some(&disabled)
        } else {
            None
        };
        if let Some(old) = old {
            if key(old) != key(&target) {
                push_file(
                    &mut result,
                    &mut targets,
                    roots,
                    game(old),
                    None,
                    Some(&external.node_id),
                )?;
            }
        }
        let source = batch.stage_root.join(&external.stage_path);
        push_file(
            &mut result,
            &mut targets,
            roots,
            game(&target),
            Some(source),
            Some(&external.node_id),
        )?;
    }
    for relative in &batch.extra_files {
        let target = game(relative);
        // Unowned existing config/framework files cannot be overwritten as a map side effect.
        let existing = Fingerprint::capture(&target.resolve(roots)?)?;
        if existing != Fingerprint::Missing {
            return Err(occupied_target(&existing).report("map-import", relative));
        }
        push_file(
            &mut result,
            &mut targets,
            roots,
            target,
            Some(batch.stage_root.join(relative)),
            None,
        )?;
    }
    for name in [level_sets::LEVEL_SET_PLUGIN_REGISTRY, "map-relations.json"] {
        let source = batch.manifests_root.join(name);
        if source.is_file() {
            push_file(
                &mut result,
                &mut targets,
                roots,
                TransactionPath {
                    root: RootKind::Data,
                    relative: format!("manifests/{name}"),
                },
                Some(source),
                None,
            )?;
        }
    }
    Ok(result)
}

fn selected<'a>(
    inventory: &'a Inventory,
    id: &str,
    candidate: &str,
) -> Result<&'a ChangeNode, String> {
    let node = inventory
        .nodes
        .iter()
        .find(|node| node.id == id)
        .ok_or("modChanges.targetChanged")?;
    if node
        .current
        .as_ref()
        .is_none_or(|current| current.id != candidate || current.verification_pending)
    {
        return Err("modChanges.targetChanged".into());
    }
    Ok(node)
}

fn reserve(
    targets: &mut BTreeSet<String>,
    path: &TransactionPath,
    owner: &str,
) -> Result<(), String> {
    let prefix = format!("{:?}:", path.root);
    let target = format!("{prefix}{}", key(&path.relative));
    if targets
        .iter()
        .any(|old| within(&target, old) || within(old, &target))
    {
        return Err(InstallBlock::PathCollision.report(owner, &path.relative));
    }
    targets.insert(target);
    Ok(())
}

pub(super) fn push_tree(
    result: &mut Vec<OwnedMutation>,
    targets: &mut BTreeSet<String>,
    roots: &TransactionRoots,
    path: &str,
    source: Option<PathBuf>,
    owner: &str,
) -> Result<(), String> {
    let target = game(path);
    reserve(targets, &target, owner)?;
    let expected = Fingerprint::capture(&target.resolve(roots)?)?;
    if !matches!(
        expected,
        Fingerprint::Missing | Fingerprint::Directory { .. }
    ) {
        return Err("modChanges.pathCollision".into());
    }
    if expected == Fingerprint::Missing && source.is_none() {
        return Err("modChanges.targetChanged".into());
    }
    result.push(OwnedMutation::for_node(
        Mutation::Tree(TreeChange {
            target,
            source,
            expected,
        }),
        owner,
    ));
    Ok(())
}

fn push_file(
    result: &mut Vec<OwnedMutation>,
    targets: &mut BTreeSet<String>,
    roots: &TransactionRoots,
    target: TransactionPath,
    source: Option<PathBuf>,
    owner: Option<&str>,
) -> Result<(), String> {
    reserve(targets, &target, owner.unwrap_or("map-import"))?;
    let expected = Fingerprint::capture(&target.resolve(roots)?)?;
    if matches!(expected, Fingerprint::Directory { .. }) {
        return Err(InstallBlock::Directory.report(owner.unwrap_or("map-import"), &target.relative));
    }
    result.push(OwnedMutation::new(
        Mutation::File(FileChange {
            target,
            source: source.map(StagedFile::capture).transpose()?,
            expected,
            require_single_link: false,
        }),
        owner.map(|node| ItemKey::new(node, None)),
    ));
    Ok(())
}

fn game(path: &str) -> TransactionPath {
    TransactionPath {
        root: RootKind::Game,
        relative: path.into(),
    }
}
