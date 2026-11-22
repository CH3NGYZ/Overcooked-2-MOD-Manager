//! Compose final map contents once when the unique DIYLevel package moves.
//! All edits happen below the private plan stage; live endpoints remain trees.
use super::{
    application_progress::{ItemKey, OwnedMutation},
    inventory::{self, key},
    model::*,
    mutations,
    session::PlanSession,
    transaction::*,
};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

mod bundled;
pub(super) use bundled::has_verified_bundled_map;
pub(crate) use bundled::{bundled_level_root, selected_bundled_level_root};
pub(super) use bundled::{framework_level_root, unique_framework_member};

pub(crate) struct Replacement {
    pub node_id: String,
    pub source: PathBuf,
    pub target_dir: String,
    pub old_path: Option<String>,
    pub member_prefix: String,
}

pub(super) struct Composition {
    pub mutations: Vec<OwnedMutation>,
    pub handled_maps: BTreeSet<String>,
    pub from: String,
    pub to: String,
    pub map_paths: Vec<(String, String)>,
}

pub(super) fn compose(
    session: &PlanSession,
    roots: &TransactionRoots,
    replacements: &[Replacement],
) -> Result<Option<Composition>, String> {
    let solved = session.solved.as_ref().ok_or("modChanges.notPrepared")?;
    let Some((framework, package)) = mutations::final_framework_package(solved)? else {
        return Ok(None);
    };
    let to = format!("{package}/levels");
    let from = session
        .trusted
        .level_roots
        .package_root()
        .map(|old| inventory::relative(&roots.game_root, old).map(|old| format!("{old}/levels")))
        .transpose()?
        .unwrap_or_else(|| to.clone());
    let same_root = key(&from) == key(&to);
    if same_root {
        return bundled::compose_missing_maps(session, roots, replacements, &to);
    }
    if mutations::paths_overlap(&from, &to) {
        return Err(
            mutations::InstallBlock::PathCollision.report(framework, &format!("{from}\n{to}"))
        );
    }
    let old = mutations::game(&from).resolve(roots)?;
    let before = map_tree_fingerprint(roots, &from, framework)?;
    if Fingerprint::capture(&mutations::game(&to).resolve(roots)?)? != Fingerprint::Missing {
        return Err(mutations::report_install_block(
            "archive.targetExists",
            framework,
            &[("path", &to)],
        ));
    }
    let stage = session
        .staging
        .join(format!("levels-{}", uuid::Uuid::new_v4()));
    if !stage.is_absolute()
        || !stage.starts_with(&session.staging)
        || stage.starts_with(&roots.game_root)
    {
        return Err("archive.pathEscape".into());
    }
    match before {
        Fingerprint::Directory { .. } => {
            stage_tree(&old, &stage, &before)?;
        }
        Fingerprint::Missing => fs::create_dir(&stage).map_err(|error| error.to_string())?,
        _ => unreachable!(),
    }
    let permissions = fs::metadata(&stage)
        .map_err(|error| error.to_string())?
        .permissions();
    writable(&stage)?;
    let mut result = Composition {
        mutations: vec![],
        handled_maps: BTreeSet::new(),
        from,
        to,
        map_paths: final_map_paths(session, roots)?,
    };
    let mut targets = BTreeSet::new();
    let mut old_tree_items = BTreeSet::from([ItemKey::new(framework, None)]);
    let mut new_tree_items = old_tree_items.clone();
    for node in solved
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Map)
    {
        result.handled_maps.insert(node.id.clone());
        let replacement = replacements
            .iter()
            .find(|replacement| replacement.node_id == node.id);
        let original = session
            .trusted
            .inventory
            .nodes
            .iter()
            .find(|old| old.id == node.id);
        let files = session
            .trusted
            .files
            .get(&node.id)
            .ok_or("modChanges.targetChanged")?;
        let changed = original.is_none_or(|original| {
            original.enabled != node.enabled || original.current != node.current
        });
        if replacement.is_none() && !changed {
            continue;
        }
        let current = node.current.as_ref().ok_or("modChanges.targetChanged")?;
        let mut items = BTreeSet::from([ItemKey::new(&node.id, None)]);
        if let Some(before) = original.and_then(|old| old.current.as_ref()) {
            for member in &current.provides {
                if let Some(key) = member.member_key.as_deref()
                    && before.provides.iter().any(|old| {
                        old.member_key.as_deref() == Some(key) && old.enabled != member.enabled
                    })
                {
                    items.insert(ItemKey::new(&node.id, Some(key)));
                }
            }
        }
        let name = replacement
            .map(|replacement| replacement.target_dir.as_str())
            .or(files.summary.map_dir_name.as_deref())
            .ok_or_else(|| {
                mutations::report_install_block(
                    "modChanges.installTargetOwnershipUnverified",
                    &node.id,
                    &[("name", &files.summary.label)],
                )
            })?;
        if name.is_empty() || name.contains(['/', '\\']) {
            return Err("archive.pathEscape".into());
        }
        let previous = replacement
            .and_then(|replacement| replacement.old_path.as_deref())
            .or(files.map_path.as_deref());
        let target = if node.enabled {
            format!("{}/{name}", result.to)
        } else {
            inventory::relative(
                &roots.game_root,
                &crate::level_sets::disabled_root(&roots.game_root).join(name),
            )?
        };
        let target_in_tree = mutations::within(&target, &result.to);
        let old_in_tree =
            previous.is_some_and(|previous| mutations::within(previous, &result.from));
        if old_in_tree {
            old_tree_items.extend(items.iter().cloned());
        }
        if target_in_tree {
            new_tree_items.extend(items.iter().cloned());
        }
        let old_staged = previous
            .filter(|_| old_in_tree)
            .map(|previous| child(&stage, previous, &result.from))
            .transpose()?;
        let source = replacement
            .map(|replacement| replacement.source.clone())
            .or_else(|| old_staged.clone())
            .or_else(|| previous.map(|previous| roots.game_root.join(previous)))
            .ok_or("modChanges.targetChanged")?;
        let destination = if target_in_tree {
            child(&stage, &target, &result.to)?
        } else {
            session
                .staging
                .join(format!("map-{}", uuid::Uuid::new_v4()))
        };
        let same_existing = old_staged
            .as_ref()
            .is_some_and(|old| key(&old.to_string_lossy()) == key(&destination.to_string_lossy()));
        if replacement.is_some() || !same_existing {
            if Fingerprint::capture(&destination)? != Fingerprint::Missing {
                if !same_existing {
                    return Err(mutations::report_install_block(
                        "archive.targetExists",
                        &node.id,
                        &[("path", &target)],
                    ));
                }
                remove_private(&session.staging, &destination)?;
            }
            let fingerprint = Fingerprint::capture(&source)?;
            stage_tree(&source, &destination, &fingerprint)?;
        }
        if let Some(previous) = previous {
            if replacement.is_some()
                && let Some(old) = old_staged.as_ref().filter(|old| **old != destination)
            {
                remove_private(&session.staging, old)?;
            }
            if replacement.is_none() && old_in_tree && !same_existing {
                remove_private(&session.staging, old_staged.as_ref().unwrap())?;
            }
            if !old_in_tree && key(previous) != key(&target) {
                push_tree(
                    &mut result.mutations,
                    &mut targets,
                    roots,
                    previous,
                    None,
                    items.iter().cloned(),
                )?;
            }
            if replacement.is_some() {
                preserve_replaced_tree_permissions(&roots.game_root.join(previous), &destination)?;
            }
        }
        let prefix = replacement
            .map(|replacement| replacement.member_prefix.as_str())
            .or(previous)
            .ok_or("modChanges.targetChanged")?;
        set_members(&destination, prefix, current)?;
        if !target_in_tree {
            let physical = mutations::game(&target).resolve(roots)?;
            if Fingerprint::capture(&physical)? != Fingerprint::Missing
                && previous.is_none_or(|previous| key(previous) != key(&target))
            {
                return Err(mutations::report_install_block(
                    "archive.targetExists",
                    &node.id,
                    &[("path", &target)],
                ));
            }
            push_tree(
                &mut result.mutations,
                &mut targets,
                roots,
                &target,
                Some(destination),
                items,
            )?;
        }
    }
    bundled::merge_missing_maps(session, roots, &stage, &result.to)?;
    fs::set_permissions(&stage, permissions).map_err(|error| error.to_string())?;
    if before != Fingerprint::Missing {
        // A new tree without a switched-off map must still follow removal of
        // its old tree, even when that removal includes the requested target.
        new_tree_items.extend(old_tree_items.iter().cloned());
        push_tree(
            &mut result.mutations,
            &mut targets,
            roots,
            &result.from,
            None,
            old_tree_items,
        )?;
    }
    push_tree(
        &mut result.mutations,
        &mut targets,
        roots,
        &result.to,
        Some(stage),
        new_tree_items,
    )?;
    Ok(Some(result))
}

fn child(root: &Path, path: &str, prefix: &str) -> Result<PathBuf, String> {
    if !mutations::within(path, prefix) || path.len() <= prefix.len() {
        return Err("archive.pathEscape".into());
    }
    Ok(root.join(&path[prefix.len() + 1..]))
}

fn writable(path: &Path) -> Result<(), String> {
    let mut permissions = fs::metadata(path)
        .map_err(|error| error.to_string())?
        .permissions();
    permissions.set_readonly(false);
    fs::set_permissions(path, permissions).map_err(|error| error.to_string())
}

pub(crate) fn set_members(root: &Path, prefix: &str, candidate: &Candidate) -> Result<(), String> {
    let permissions = fs::metadata(root)
        .map_err(|error| error.to_string())?
        .permissions();
    writable(root)?;
    for member in &candidate.provides {
        let key = member.member_key.as_deref().ok_or_else(|| {
            mutations::report_install_block(
                "modChanges.installTargetOwnershipUnverified",
                &candidate.id,
                &[("name", prefix)],
            )
        })?;
        let canonical = child(root, &crate::mods::canonical_dll_name(key), prefix)?;
        let disabled = PathBuf::from(crate::mods::disabled_dll_file(&canonical.to_string_lossy()));
        let (from, to) = match (canonical.is_file(), disabled.is_file()) {
            (true, false) if !member.enabled => (&canonical, &disabled),
            (false, true) if member.enabled => (&disabled, &canonical),
            (true, false) | (false, true) => continue,
            (true, true) => {
                return Err(mutations::InstallBlock::BothStates.report(&candidate.id, key));
            }
            (false, false) => {
                return Err(mutations::InstallBlock::Missing.report(&candidate.id, key));
            }
        };
        Fingerprint::capture(from)?;
        Fingerprint::capture(to)?;
        let parent = from.parent().ok_or("archive.pathEscape")?;
        let prior = fs::metadata(parent)
            .map_err(|error| error.to_string())?
            .permissions();
        writable(parent)?;
        let renamed = fs::rename(from, to).map_err(|error| error.to_string());
        fs::set_permissions(parent, prior).map_err(|error| error.to_string())?;
        renamed?;
    }
    fs::set_permissions(root, permissions).map_err(|error| error.to_string())
}

fn remove_private(root: &Path, path: &Path) -> Result<(), String> {
    if path == root || !path.starts_with(root) {
        return Err("archive.pathEscape".into());
    }
    if Fingerprint::capture(path)? == Fingerprint::Missing {
        return Err("modChanges.targetChanged".into());
    }
    // Directory readonly flags can deny child removal on supported filesystems.
    // This is a private copy; the live tree remains untouched and retains its modes.
    for entry in walkdir::WalkDir::new(path).follow_links(false) {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry.file_type().is_symlink() {
            return Err("archive.pathEscape".into());
        }
        if entry.file_type().is_dir() {
            crate::archive::clear_readonly(entry.path())?;
        }
    }
    for entry in walkdir::WalkDir::new(path)
        .follow_links(false)
        .contents_first(true)
    {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry.file_type().is_symlink() {
            return Err("archive.pathEscape".into());
        }
        crate::archive::clear_readonly(entry.path())?;
        if entry.file_type().is_dir() {
            fs::remove_dir(entry.path())
        } else {
            fs::remove_file(entry.path())
        }
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(super) fn map_destination(
    game_root: &Path,
    name: &str,
    enabled: bool,
    package: Option<&str>,
) -> Result<String, String> {
    if name.is_empty() || name.contains(['/', '\\']) {
        return Err("archive.pathEscape".into());
    }
    if enabled {
        let package = package.ok_or("framework.diyLevelMissing")?;
        Ok(format!("{package}/levels/{name}"))
    } else {
        inventory::relative(
            game_root,
            &crate::level_sets::disabled_root(game_root).join(name),
        )
    }
}

/// Map ownership paths are rewritten by the same final physical projection for
/// imports and ordinary toggles. A disabled owner can retain internal DLL rows.
pub(crate) fn final_map_paths(
    session: &PlanSession,
    roots: &TransactionRoots,
) -> Result<Vec<(String, String)>, String> {
    let solved = session.solved.as_ref().ok_or("modChanges.notPrepared")?;
    let package = mutations::final_package(solved)?;
    let mut result = Vec::new();
    for node in solved
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Map)
    {
        let files = session
            .trusted
            .files
            .get(&node.id)
            .ok_or("modChanges.targetChanged")?;
        let name = files.summary.map_dir_name.as_deref().ok_or_else(|| {
            mutations::report_install_block(
                "modChanges.installTargetOwnershipUnverified",
                &node.id,
                &[("name", &files.summary.label)],
            )
        })?;
        let target = map_destination(&roots.game_root, name, node.enabled, package.as_deref())?;
        if let Some(old) = files.map_path.as_ref() {
            result.push((old.clone(), target.clone()));
        }
        // Imported internal registry paths still use the private installer's
        // package/name, even when identity keeps an older live directory name.
        if let Some(batch) = &session.map_payload {
            result.extend(batch.registry_paths(&node.id, &target));
        }
    }
    Ok(result)
}

pub(super) fn map_tree_fingerprint(
    roots: &TransactionRoots,
    relative: &str,
    owner: &str,
) -> Result<Fingerprint, String> {
    let expected = Fingerprint::capture(&mutations::game(relative).resolve(roots)?)?;
    if matches!(expected, Fingerprint::File { .. }) {
        return Err(mutations::report_install_block(
            "levelSet.mapDirectoryRequired",
            owner,
            &[("path", relative)],
        ));
    }
    Ok(expected)
}

pub(super) fn push_tree(
    result: &mut Vec<OwnedMutation>,
    targets: &mut BTreeSet<String>,
    roots: &TransactionRoots,
    path: &str,
    source: Option<PathBuf>,
    items: impl IntoIterator<Item = ItemKey>,
) -> Result<(), String> {
    if targets
        .iter()
        .any(|old| mutations::paths_overlap(old, path))
    {
        return Err(mutations::InstallBlock::PathCollision.report("map-tree", path));
    }
    targets.insert(key(path));
    let target = mutations::game(path);
    let expected = map_tree_fingerprint(roots, path, "map-tree")?;
    if expected == Fingerprint::Missing && source.is_none() {
        return Err(mutations::InstallBlock::Missing.report("map-tree", path));
    }
    result.push(OwnedMutation::new(
        Mutation::Tree(TreeChange {
            target,
            source,
            expected,
        }),
        items,
    ));
    Ok(())
}

pub(crate) fn rewrite_registry(
    path: &Path,
    from: &str,
    to: &str,
    map_paths: &[(String, String)],
) -> Result<bool, String> {
    let registry_name = format!(
        "manifests/{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("level-set-plugins.json")
    );
    let unreadable =
        || mutations::InstallBlock::MapRecordsUnreadable.report("map-registry", &registry_name);
    if Fingerprint::capture(path).map_err(|_| unreadable())? == Fingerprint::Missing {
        return Ok(false);
    }
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(path).map_err(|_| unreadable())?)
            .map_err(|_| unreadable())?;
    if value
        .get("version")
        .and_then(|value| value.as_u64())
        .is_none_or(|version| version > 1)
    {
        return Err(unreadable());
    }
    let entries = value
        .get_mut("entries")
        .and_then(|value| value.as_array_mut())
        .ok_or_else(unreadable)?;
    let mut targets = BTreeSet::new();
    let mut changed = false;
    for entry in entries {
        let old = entry
            .get("dll")
            .and_then(|value| value.as_str())
            .ok_or_else(unreadable)?
            .replace('\\', "/");
        let mapping = map_paths
            .iter()
            .filter(|(previous, _)| mutations::within(&old, previous))
            .max_by_key(|(previous, _)| previous.len());
        let next = if let Some((previous, target)) = mapping {
            changed |= key(previous) != key(target);
            format!("{target}{}", &old[previous.len()..])
        } else if !from.is_empty() && mutations::within(&old, from) {
            changed = true;
            format!("{to}{}", &old[from.len()..])
        } else {
            old
        };
        if !targets.insert(key(&crate::mods::canonical_dll_name(&next))) {
            return Err(mutations::InstallBlock::PathCollision.report("map-registry", &next));
        }
        entry["dll"] = next.into();
    }
    if changed {
        fs::write(
            path,
            serde_json::to_vec_pretty(&value).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(changed)
}
