//! DIYLevel ships sample maps inside its MOD ZIP. Map trees remain map-owned:
//! keep every existing directory intact and only seed entirely absent samples.
use super::*;

// The comparison root may be normalized; resolve using the original ZIP spelling.
fn map_directory(path: &str, levels: &str) -> Option<(String, String)> {
    let physical = path.replace('\\', "/");
    let parts = physical.split('/').collect::<Vec<_>>();
    let depth = levels.replace('\\', "/").split('/').count();
    if parts.len() <= depth + 1 || parts[depth].is_empty() {
        return None;
    }
    Some((parts[depth].into(), parts[..=depth].join("/")))
}

pub(in crate::mod_changes) fn has_verified_bundled_map(
    payload: &crate::mod_changes::session::Payload,
    levels: &str,
    name: &str,
) -> bool {
    payload
        .files
        .iter()
        .filter_map(|path| {
            if !mutations::within(path, levels) {
                return None;
            }
            let (directory, physical) = map_directory(path, levels)?;
            (key(&directory) == name).then_some(physical)
        })
        .next()
        .and_then(|path| crate::level_sets::locate_info_file(&payload.stage_root.join(path)))
        .is_some_and(|path| crate::level_sets::read_info_metadata(&path).is_ok())
}

pub(in crate::mod_changes) fn unique_framework_member<'a>(
    owner: &str,
    provides: &'a [ProvidedPlugin],
) -> Result<Option<&'a ProvidedPlugin>, String> {
    let mut frameworks = provides.iter().filter(|member| {
        member
            .guid
            .eq_ignore_ascii_case(crate::mods::plugin_target::DIYLEVEL_GUID)
    });
    let Some(framework) = frameworks.next() else {
        return Ok(None);
    };
    if let Some(other) = frameworks.next() {
        return Err(mutations::package_identity_conflict(
            owner,
            crate::mods::plugin_target::DIYLEVEL_GUID,
            [framework, other].into_iter().chain(frameworks),
        ));
    }
    Ok(Some(framework))
}

pub(in crate::mod_changes) fn framework_level_root(
    owner: &str,
    framework: &ProvidedPlugin,
) -> Result<String, String> {
    let path = framework.member_key.as_deref().ok_or_else(|| {
        mutations::report_install_block(
            "modChanges.installTargetOwnershipUnverified",
            owner,
            &[("name", "DIYLevel")],
        )
    })?;
    let package = Path::new(path)
        .parent()
        .ok_or("archive.pathEscape")?
        .to_string_lossy()
        .replace('\\', "/");
    Ok(format!("{package}/levels"))
}

pub(crate) fn bundled_level_root(
    session: &PlanSession,
    node: &str,
) -> Result<Option<String>, String> {
    let Some(payload) = session.payloads.get(node) else {
        return Ok(None);
    };
    let solved = session.solved.as_ref().ok_or("modChanges.notPrepared")?;
    let Some(current) = solved
        .nodes
        .iter()
        .find(|entry| entry.id == node)
        .and_then(|entry| entry.current.as_ref())
    else {
        return Ok(None);
    };
    if payload.candidate.id != current.id {
        return Ok(None);
    }
    selected_bundled_level_root(node, solved, current, payload)
}

pub(crate) fn selected_bundled_level_root(
    node: &str,
    solved: &Inventory,
    current: &Candidate,
    payload: &crate::mod_changes::session::Payload,
) -> Result<Option<String>, String> {
    let Some(framework) = unique_framework_member(node, &current.provides)? else {
        return Ok(None);
    };
    let levels = framework_level_root(node, framework)?;
    let Some(package) = mutations::final_package(solved)? else {
        return Ok(None);
    };
    // Only the unique selected framework's own package can seed its levels root.
    if key(&levels) != key(&format!("{package}/levels")) {
        return Ok(None);
    }
    Ok(payload
        .files
        .iter()
        .any(|path| mutations::within(path, &levels))
        .then_some(levels))
}

fn missing_maps(
    session: &PlanSession,
    roots: &TransactionRoots,
    stage: &Path,
    destination: &str,
    replacements: &[Replacement],
) -> Result<Vec<(String, PathBuf, ItemKey)>, String> {
    let mut result = Vec::new();
    for (owner, payload) in &session.payloads {
        let Some(prefix) = bundled_level_root(session, owner)? else {
            continue;
        };
        let source_roots = TransactionRoots {
            game_root: payload.stage_root.clone(),
            data_root: roots.data_root.clone(),
        };
        let mut names = std::collections::BTreeMap::new();
        for path in &payload.files {
            if !mutations::within(path, &prefix) {
                continue;
            }
            let Some((name, physical)) = map_directory(path, &prefix) else {
                return Err(mutations::report_install_block(
                    "levelSet.bundledMapDirectoryRequired",
                    owner,
                    &[("path", path)],
                ));
            };
            names.entry(name).or_insert(physical);
        }
        for (name, physical) in names {
            let target = stage.join(&name);
            let disabled = crate::level_sets::disabled_root(&roots.game_root).join(&name);
            // Never revive a disabled map or partially merge an older shipped map
            // into a user's installed version, even if the package manifest owns it.
            if Fingerprint::capture(&target)? != Fingerprint::Missing
                || Fingerprint::capture(&disabled)? != Fingerprint::Missing
                || replacements
                    .iter()
                    .any(|replacement| key(&replacement.target_dir) == key(&name))
                || session.solved.as_ref().is_some_and(|solved| {
                    solved.nodes.iter().any(|node| {
                        node.kind == NodeKind::Map
                            && session.trusted.files.get(&node.id).is_some_and(|files| {
                                files
                                    .summary
                                    .map_dir_name
                                    .as_deref()
                                    .is_some_and(|existing| key(existing) == key(&name))
                            })
                    })
                })
            {
                log::debug!(
                    "保留现有地图，不写入MOD自带副本：节点={owner}，路径={destination}/{name}"
                );
                continue;
            }
            // New map-owned DLLs require their own graph node and verified
            // relations. A framework replacement cannot silently install them.
            if let Some(path) = payload.files.iter().find(|path| {
                let lower = path.to_ascii_lowercase();
                mutations::within(path, &physical)
                    && (lower.ends_with(".dll") || lower.ends_with(".dll.disabled"))
            }) {
                return Err(mutations::report_install_block(
                    "modChanges.installBundledMapPluginUnverified",
                    owner,
                    &[("path", path)],
                ));
            }
            let source = mutations::game(&physical).resolve(&source_roots)?;
            result.push((name, source, ItemKey::new(owner, None)));
        }
    }
    Ok(result)
}

pub(super) fn merge_missing_maps(
    session: &PlanSession,
    roots: &TransactionRoots,
    stage: &Path,
    destination: &str,
) -> Result<(), String> {
    for (name, source, _) in missing_maps(session, roots, stage, destination, &[])? {
        stage_tree(&source, &stage.join(name), &Fingerprint::capture(&source)?)?;
    }
    Ok(())
}

pub(super) fn compose_missing_maps(
    session: &PlanSession,
    roots: &TransactionRoots,
    replacements: &[Replacement],
    destination: &str,
) -> Result<Option<Composition>, String> {
    let target = mutations::game(destination).resolve(roots)?;
    let missing = missing_maps(session, roots, &target, destination, replacements)?;
    if missing.is_empty() {
        return Ok(None);
    }
    let mut result = Composition {
        mutations: vec![],
        handled_maps: BTreeSet::new(),
        from: destination.into(),
        to: destination.into(),
        map_paths: vec![],
    };
    let mut targets = BTreeSet::new();
    for (name, source, item) in missing {
        // A same-root replacement must not copy every installed user map merely
        // to seed a few shipped examples. Each absent map is its own tree write.
        let staged = session
            .staging
            .join(format!("bundled-map-{}", uuid::Uuid::new_v4()));
        stage_tree(&source, &staged, &Fingerprint::capture(&source)?)?;
        push_tree(
            &mut result.mutations,
            &mut targets,
            roots,
            &format!("{destination}/{name}"),
            Some(staged),
            [item],
        )?;
    }
    Ok(Some(result))
}

#[cfg(test)]
mod tests {
    use super::map_directory;

    #[test]
    fn bundled_map_source_retains_zip_case_instead_of_normalized_member_case() {
        assert_eq!(
            map_directory(
                "BepInEx/plugins/OC2DIYLevel/levels/OC1_Story/nested/info",
                "bepinex/plugins/oc2diylevel/levels",
            ),
            Some((
                "OC1_Story".into(),
                "BepInEx/plugins/OC2DIYLevel/levels/OC1_Story".into(),
            ))
        );
        assert!(map_directory("Package/levels/info", "package/levels").is_none());
        assert!(map_directory("Package/levels", "package/levels").is_none());
    }
}
