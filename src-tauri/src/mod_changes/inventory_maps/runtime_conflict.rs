//! A map can carry a newer copy of a separately installed BepInEx plugin.
use super::*;
use crate::mod_changes::{
    evaluate::{same_guid, targets},
    mutations,
    session::PlanSession,
    transaction::*,
};
use std::{cmp::Ordering, fs};

/// Only enabling one installed map can offer the switch. Keep this inferred
/// conflict inside the solve, out of the cached inventory and map metadata.
pub(in crate::mod_changes) fn solve_toggle(
    state: &AppState,
    session: &PlanSession,
    original: &Inventory,
    loose_releases: &mut mutations::LooseReleaseSearch<'_>,
) -> Result<Option<(SolveResult, Option<Inventory>)>, String> {
    let [root] = session.request.roots.as_slice() else {
        return Ok(None);
    };
    if !root.enabled
        || root.candidate_id.is_some()
        || root.member_key.is_some()
        || session.wire_request.map_import.is_some()
    {
        return Ok(None);
    }
    let trusted = &session.trusted;
    let Some((map_index, map)) = original.nodes.iter().enumerate().find(|(_, node)| {
        node.id == root.node_id && node.kind == NodeKind::Map && !node.enabled && node.mutable
    }) else {
        return Ok(None);
    };
    let Some(candidate) = &map.current else {
        return Ok(None);
    };
    let manifests_dir = state.config.manifests_dir();
    let roots = TransactionRoots {
        data_root: state
            .config
            .config_path()
            .parent()
            .ok_or("archive.pathEscape")?
            .into(),
        game_root: trusted.game_root.clone(),
    };
    let manifests = std::cell::OnceCell::new();
    let mut conflicts = Vec::new();
    let mut duplicates = BTreeMap::new();
    for member in &candidate.provides {
        let guid = member.guid.trim();
        if guid.is_empty() || same_guid(guid, crate::mods::plugin_target::DIYLEVEL_GUID) {
            continue;
        }
        let copies = targets(original, guid, false);
        // Map enable activates every internal member, even one currently disabled on disk.
        let would_load = copies.iter().filter_map(|(index, plugin)| {
            (*index == map_index || (original.nodes[*index].enabled && plugin.enabled))
                .then_some(*plugin)
        });
        let Some(error) = duplicate_runtime(guid, would_load) else {
            continue;
        };
        duplicates.insert(guid.to_ascii_lowercase(), error);
        let only_external = match &copies[..] {
            [_, _] => copies.iter().find(|(index, _)| *index != map_index),
            _ => None,
        };
        let eligible = only_external.is_some_and(|(index, external)| {
            let versions = member.version.as_deref().zip(external.version.as_deref());
            versions.is_some_and(|(inside, outside)| {
                matches!(
                    crate::releases::compare_release_versions(inside, outside),
                    Some(Ordering::Equal | Ordering::Greater)
                )
            }) && standalone_switchable(trusted, &roots, *index, external, || {
                manifests.get_or_init(|| archive::read_manifests(&manifests_dir))
            })
        });
        let Some(member_key) = member.member_key.as_ref().filter(|_| eligible) else {
            continue;
        };
        let conflict = OwnedRelation {
            owner_member_key: Some(member_key.clone()),
            owner_guid: Some(guid.into()),
            relation: RepositoryRelation {
                kind: RelationKind::Conflicts,
                guid: guid.into(),
                version: String::new(),
                prerelease: None,
                optional: false,
            },
        };
        if !candidate.relations.contains(&conflict) {
            conflicts.push(conflict);
        }
    }
    if duplicates.is_empty() {
        return Ok(None);
    }
    let mut solve = |inventory: &Inventory| {
        super::super::solve::solve_with_policy(
            inventory,
            &session.request,
            Some(trusted),
            Some(&mut *loose_releases),
            session.dependency_policy,
            Some(session.operation_id()),
        )
    };
    let (mut result, solved) = if conflicts.is_empty() {
        solve(original)?
    } else {
        let mut input = original.clone();
        if let Some(current) = &mut input.nodes[map_index].current {
            current.relations.extend(conflicts);
        }
        let (mut result, mut solved) = solve(&input)?;
        // The map root fixes its candidate, so only its switches may differ.
        if let Some(current) = solved
            .as_mut()
            .and_then(|s| s.nodes[map_index].current.as_mut())
        {
            current.relations.clone_from(&candidate.relations);
        }
        result.evaluations = crate::mod_changes::evaluate(solved.as_ref().unwrap_or(original));
        (result, solved)
    };
    // A Requires edge can encounter the duplicate before the ownership guard.
    // Keep the original blocker, and do not mislabel unrelated or stale input.
    if solved.is_none() {
        let duplicate_blockers: Vec<_> = result
            .blockers
            .iter()
            .filter(|blocker| blocker.code == "relation_has_no_solution")
            .filter_map(|blocker| {
                let guid = blocker.target_guid.as_deref()?.trim().to_ascii_lowercase();
                Some(Diagnostic {
                    code: "duplicate_active_identity".into(),
                    owner_id: Some(map.id.clone()),
                    target_guid: None, // The parameterized message includes the GUID.
                    message: duplicates.get(&guid)?.clone(),
                    incomplete: blocker.incomplete,
                    traces: Vec::new(),
                    traces_truncated: false,
                })
            })
            .collect();
        result.blockers.extend(duplicate_blockers);
    }
    Ok(Some((result, solved)))
}

fn standalone_switchable<'a>(
    trusted: &TrustedInventory,
    roots: &TransactionRoots,
    index: usize,
    external: &ProvidedPlugin,
    manifests: impl FnOnce() -> &'a Vec<archive::InstallManifest>,
) -> bool {
    let node = &trusted.inventory.nodes[index];
    let Some(files) = trusted.files.get(&node.id) else {
        return false;
    };
    // A registered map payload is not independently owned by the MOD page.
    if !node.enabled
        || !node.mutable
        || node.kind != NodeKind::Mod
        || node.id.starts_with("map-dll:")
    {
        return false;
    }
    let Some(member_key) = external.member_key.as_deref() else {
        return false;
    };
    let mut paths = files
        .summary
        .members
        .iter()
        .filter(|member| member.key == member_key && member.enabled);
    let Some(path) = paths.next().map(|member| member.dll_file.as_str()) else {
        return false;
    };
    if paths.next().is_some() || !mutations::within(path, "BepInEx/plugins") {
        return false;
    }
    let owned_by = |manifest: &archive::InstallManifest| {
        manifest
            .files
            .iter()
            .any(|file| key(&crate::mods::canonical_dll_name(file)) == member_key)
    };
    if manifests()
        .iter()
        .filter(|manifest| owned_by(manifest))
        .any(|claim| {
            !files
                .manifests
                .iter()
                .any(|manifest| manifest.repository_id == claim.repository_id)
        })
    {
        return false;
    }
    let (Ok(active), Ok(inactive)) = (
        mutations::game(path).resolve(roots),
        mutations::game(&crate::mods::disabled_dll_file(path)).resolve(roots),
    ) else {
        return false;
    };
    // Resolve like the eventual transaction; never trial a rename in the live tree.
    matches!(fs::symlink_metadata(inactive), Err(error) if error.kind() == std::io::ErrorKind::NotFound)
        && fs::File::open(active).is_ok_and(|file| has_single_link(&file))
}

pub(in crate::mod_changes) fn duplicate_runtime<'a>(
    guid: &str,
    members: impl IntoIterator<Item = &'a ProvidedPlugin>,
) -> Option<String> {
    let mut members = members
        .into_iter()
        .map(|member| member.member_key.as_deref().unwrap_or(guid));
    let mut paths = vec![members.next()?, members.next()?];
    paths.extend(members);
    paths.sort_unstable();
    Some(crate::locale_tag::with_params(
        "modChanges.duplicatePluginRuntime",
        &[("guid", guid), ("paths", &paths.join(" · "))],
    ))
}
