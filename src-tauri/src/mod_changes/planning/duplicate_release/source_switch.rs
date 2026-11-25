use super::*;
use crate::mod_changes::mutations::related_release_source;

pub(super) fn attach_root(
    trusted: &mut TrustedInventory,
    root: &RootRequest,
    repository_id: &str,
    data_root: &Path,
) -> Result<bool, String> {
    let Some(repo) = trusted
        .config
        .repositories
        .iter()
        .find(|repo| repo.id == repository_id)
    else {
        return Ok(false);
    };
    if repo.source_id.is_none()
        || repo.plugin_guid.trim().is_empty()
        || !trusted
            .inventory
            .nodes
            .iter()
            .any(|node| node.id == root.intent.node_id && node.current.is_none())
    {
        return Ok(false);
    }
    let mut providers = trusted
        .inventory
        .nodes
        .iter()
        .flat_map(|node| {
            node.current
                .iter()
                .flat_map(move |current| current.provides.iter().map(move |plugin| (node, plugin)))
        })
        .filter(|(_, plugin)| same_guid(&repo.plugin_guid, &plugin.guid));
    let Some((node, plugin)) = providers.next() else {
        return Ok(false);
    };
    if providers.next().is_some()
        || node.kind != NodeKind::Mod
        || !node.mutable
        || node.id.starts_with("map-dll:")
        || trusted
            .declared_bundle_switch_members
            .contains_key(&node.id)
    {
        return Ok(false);
    }
    let Some(files) = trusted.files.get(&node.id) else {
        return Ok(false);
    };
    let [manifest] = files.manifests.as_slice() else {
        return Ok(false);
    };
    let Some(candidate) =
        candidates::for_root_asset(repo, None, &trusted.config, root.asset_name.as_deref())
            .into_iter()
            .find(|candidate| {
                root.intent.candidate_id.as_ref() == Some(&candidate.id) && candidate.installable
            })
    else {
        return Ok(false);
    };
    let (Some(tag), Some(prerelease), Some(key)) = (
        candidate.tag.as_deref(),
        candidate
            .provides
            .first()
            .and_then(|plugin| plugin.prerelease),
        plugin.member_key.as_deref(),
    ) else {
        return Ok(false);
    };
    if !related_release_source(&trusted.config, repo, manifest, tag, prerelease) {
        return Ok(false);
    }
    let Some(item) = representative(trusted, repo, files).filter(|item| {
        !item.is_dependency
            && item.bundled_with_dll.is_none()
            && item.package_id.as_deref() == Some(manifest.repository_id.as_str())
            && item
                .guid
                .as_deref()
                .is_some_and(|guid| same_guid(guid, &repo.plugin_guid))
    }) else {
        return Ok(false);
    };
    let roots = TransactionRoots {
        game_root: trusted.game_root.clone(),
        data_root: data_root.into(),
    };
    let copy = Copy {
        node,
        files,
        item,
        identity: mods::PluginIdentity::from_installed(item, &trusted.config),
    };
    let retired = retirement(&roots, &copy, key.into())?;
    let mut summary = files.clone();
    let target = trusted.files.get(&root.intent.node_id).ok_or(AMBIGUOUS)?;
    summary.summary.label.clone_from(&target.summary.label);
    summary
        .summary
        .name_i18n
        .clone_from(&target.summary.name_i18n);
    let proof = Consolidation {
        node: node.clone(),
        files: summary,
        retired: vec![],
        replaced_ids: BTreeSet::from([node.id.clone(), root.intent.node_id.clone()]),
    };
    attach(trusted, repository_id, candidate, proof, Some(retired));
    Ok(true)
}
