//! Whole map-tree movement when the selected DIYLevel package changes directory.
use super::*;

pub(super) struct RootMigration {
    pub from: String,
    pub to: String,
    pub handled_maps: BTreeSet<String>,
    pub map_paths: Vec<(String, String)>,
}

pub(crate) fn final_package(inventory: &Inventory) -> Result<Option<String>, String> {
    let members = inventory
        .nodes
        .iter()
        .filter_map(|node| node.current.as_ref())
        .flat_map(|candidate| &candidate.provides)
        .filter(|member| {
            member
                .guid
                .eq_ignore_ascii_case(mods::plugin_target::DIYLEVEL_GUID)
        })
        .collect::<Vec<_>>();
    let indices = mods::plugin_target::highest_indices(
        &members
            .iter()
            .map(|member| member.version.as_deref())
            .collect::<Vec<_>>(),
    );
    if indices.len() > 1 {
        return Err("framework.diyLevelAmbiguous".into());
    }
    indices
        .first()
        .map(|&index| {
            let path = members[index]
                .member_key
                .as_deref()
                .ok_or("modChanges.ambiguousOwnership")?;
            let canonical = mods::canonical_dll_name(path);
            Path::new(&canonical)
                .parent()
                .map(|path| path.to_string_lossy().replace('\\', "/"))
                .ok_or_else(|| "archive.pathEscape".into())
        })
        .transpose()
}

pub(super) fn prepare(
    builder: &mut Builder<'_>,
    solved: &Inventory,
) -> Result<Option<RootMigration>, String> {
    let Some(new) = final_package(solved)? else {
        return Ok(None);
    };
    let to = format!("{new}/levels");
    let from = builder
        .session
        .trusted
        .level_roots
        .package_root()
        .map(|old| {
            inventory::relative(&builder.roots.game_root, old).map(|old| format!("{old}/levels"))
        })
        .transpose()?
        .unwrap_or_else(|| to.clone());
    let bundled = builder
        .session
        .payloads
        .keys()
        .try_fold(false, |found, node| {
            super::super::level_trees::bundled_level_root(builder.session, node)
                .map(|root| found || root.is_some())
        })?;
    if inventory::key(&from) == inventory::key(&to) && !bundled {
        return Ok(None);
    }
    if builder.session.map_payload.is_some() {
        if inventory::key(&from) == inventory::key(&to) {
            // Same-root map imports keep their independent tree edits; their
            // composer adds only absent bundled directories without overlaps.
            return Ok(None);
        }
        // The import composer also applies every installed map's final switches.
        return Ok(Some(RootMigration {
            from,
            to,
            handled_maps: solved
                .nodes
                .iter()
                .filter(|node| node.kind == NodeKind::Map)
                .map(|node| node.id.clone())
                .collect(),
            map_paths: vec![],
        }));
    }
    let Some(composed) = super::super::level_trees::compose(builder.session, &builder.roots, &[])?
    else {
        return Ok(None);
    };
    builder.moves.extend(composed.mutations);
    Ok(Some(RootMigration {
        from: composed.from,
        to: composed.to,
        handled_maps: composed.handled_maps,
        map_paths: composed.map_paths,
    }))
}

pub(super) fn toggle_map(
    builder: &mut Builder<'_>,
    node: &ChangeNode,
    migration: Option<&RootMigration>,
) -> Result<(), String> {
    if migration.is_some_and(|migration| migration.handled_maps.contains(&node.id)) {
        return Ok(());
    }
    let Some(old) = builder
        .session
        .trusted
        .inventory
        .nodes
        .iter()
        .find(|old| old.id == node.id)
    else {
        return Ok(());
    };
    let files = builder
        .session
        .trusted
        .files
        .get(&node.id)
        .ok_or("modChanges.targetChanged")?;
    let directory = files
        .map_path
        .as_deref()
        .ok_or("modChanges.ambiguousOwnership")?;
    let mut internal = Vec::new();
    if let (Some(before), Some(after)) = (&old.current, &node.current) {
        for member in &after.provides {
            let key = member
                .member_key
                .as_deref()
                .ok_or("modChanges.ambiguousOwnership")?;
            let previous = before
                .provides
                .iter()
                .find(|member| member.member_key.as_deref() == Some(key))
                .ok_or("modChanges.targetChanged")?;
            if previous.enabled != member.enabled {
                internal.push((key.to_string(), member.enabled));
            }
        }
    }
    if old.enabled != node.enabled {
        let name = files
            .summary
            .map_dir_name
            .as_deref()
            .ok_or("modChanges.ambiguousOwnership")?;
        if name.is_empty() || name.contains(['/', '\\']) {
            return Err("archive.pathEscape".into());
        }
        let target = if node.enabled {
            let package = final_package(
                builder
                    .session
                    .solved
                    .as_ref()
                    .ok_or("modChanges.notPrepared")?,
            )?
            .ok_or("framework.diyLevelMissing")?;
            format!("{package}/levels/{name}")
        } else {
            inventory::relative(
                &builder.roots.game_root,
                &crate::level_sets::disabled_root(&builder.roots.game_root).join(name),
            )?
        };
        if !internal.is_empty() {
            let source = game(directory).resolve(&builder.roots)?;
            let staged = builder
                .session
                .staging
                .join(format!("map-switch-{}", uuid::Uuid::new_v4()));
            stage_tree(&source, &staged, &Fingerprint::capture(&source)?)?;
            super::super::level_trees::set_members(
                &staged,
                directory,
                node.current.as_ref().ok_or("modChanges.targetChanged")?,
            )?;
            let mut targets = BTreeSet::new();
            if Fingerprint::capture(&game(&target).resolve(&builder.roots)?)?
                != Fingerprint::Missing
            {
                return Err("modChanges.pathCollision".into());
            }
            super::super::level_trees::push_tree(
                &mut builder.moves,
                &mut targets,
                &builder.roots,
                directory,
                None,
            )?;
            super::super::level_trees::push_tree(
                &mut builder.moves,
                &mut targets,
                &builder.roots,
                &target,
                Some(staged),
            )?;
        } else {
            builder.add_move(&node.id, directory, &target)?;
        }
        builder.map_paths.push((directory.into(), target));
    } else {
        for (key, enabled) in internal {
            let from = builder.physical(&node.id, &key)?;
            builder.ensure_owner(&from, &node.id)?;
            let canonical = mods::canonical_dll_name(&from);
            let to = if enabled {
                canonical
            } else {
                mods::disabled_dll_file(&canonical)
            };
            builder.add_move(&node.id, &from, &to)?;
        }
    }
    Ok(())
}

pub(super) fn update_registry(
    builder: &mut Builder<'_>,
    migration: Option<&RootMigration>,
) -> Result<(), String> {
    if builder.session.map_payload.is_some() {
        return Ok(());
    }
    if migration.is_none() && builder.map_paths.is_empty() {
        return Ok(());
    }
    let relative = format!("manifests/{}", crate::level_sets::LEVEL_SET_PLUGIN_REGISTRY);
    let path = data(&relative).resolve(&builder.roots)?;
    if Fingerprint::capture(&path)? == Fingerprint::Missing {
        return Ok(());
    }
    let staged = generated(
        builder.session,
        "map-plugin-registry",
        &fs::read(&path).map_err(|error| error.to_string())?,
    )?;
    let mut map_paths = builder.map_paths.clone();
    if let Some(migration) = migration {
        map_paths.extend(migration.map_paths.iter().cloned());
    }
    if super::super::level_trees::rewrite_registry(
        &staged,
        migration
            .map(|migration| migration.from.as_str())
            .unwrap_or(""),
        migration
            .map(|migration| migration.to.as_str())
            .unwrap_or(""),
        &map_paths,
    )? {
        builder.write("map-plugin-registry", data(&relative), Some(staged))?;
    }
    Ok(())
}
