use super::*;
use crate::mod_changes::{model::ChangeRequest, mutations::related_release_source};

pub(super) fn attach_root(
    trusted: &mut TrustedInventory,
    root: &RootRequest,
    repository_id: &str,
    data_root: &Path,
) -> Result<bool, String> {
    let Some(repo) = repository(trusted, repository_id) else {
        return Ok(false);
    };
    let asset_name = root.asset_name.as_deref();
    let Some(candidate) = candidates::for_root_asset(repo, None, &trusted.config, asset_name)
        .into_iter()
        .find(|candidate| {
            root.intent.candidate_id.as_ref() == Some(&candidate.id) && candidate.installable
        })
    else {
        return Ok(false);
    };
    let automatic_dll = zip_candidate(repo, &candidate, asset_name)
        .then(|| {
            replacement_dlls(trusted)
                .find(|(id, _)| *id == repository_id)
                .map(|(_, dll)| dll.to_owned())
        })
        .flatten();
    project(
        trusted,
        repository_id,
        vec![candidate],
        automatic_dll.as_deref(),
        None,
        data_root,
    )
}

pub(in crate::mod_changes::planning) fn attach_dependencies(
    trusted: &mut TrustedInventory,
    request: &ChangeRequest,
    data_root: &Path,
) {
    let replacements = replacement_dlls(trusted)
        .map(|(id, dll)| (id.to_owned(), dll.to_owned()))
        .collect::<Vec<_>>();
    for (id, dll) in replacements {
        let Some(repo) = repository(trusted, &id) else {
            continue;
        };
        let candidates = candidates::for_repository(repo, None, false, &trusted.config)
            .into_iter()
            .filter(|candidate| zip_candidate(repo, candidate, None))
            .collect();
        if let Err(error) = project(
            trusted,
            &id,
            candidates,
            Some(&dll),
            Some(request),
            data_root,
        ) {
            log::debug!("整链无法确认换源候选：仓库={id}，原因={error}");
        }
    }
}

fn replacement_dlls(trusted: &TrustedInventory) -> impl Iterator<Item = (&str, &str)> {
    trusted.installed_mods.iter().filter_map(|item| {
        Some((
            item.replacement_repository_id.as_deref()?,
            item.dll_file.as_str(),
        ))
    })
}

fn repository<'a>(
    trusted: &'a TrustedInventory,
    repository_id: &str,
) -> Option<&'a ModRepositoryConfig> {
    trusted
        .config
        .repositories
        .iter()
        .find(|repo| repo.id == repository_id && !repo.plugin_guid.trim().is_empty())
}

fn zip_candidate(
    repo: &ModRepositoryConfig,
    candidate: &Candidate,
    asset_name: Option<&str>,
) -> bool {
    candidate.installable
        && !candidate.raw
        && repo
            .cached_releases
            .iter()
            .find(|release| candidate.tag.as_ref() == Some(&release.tag_name))
            .and_then(|release| {
                releases::select_install_asset(
                    &releases::RepositoryRequest::from(repo),
                    &repo.asset_rules,
                    release,
                    asset_name,
                )
            })
            .is_some_and(|asset| {
                Path::new(&asset.name)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
            })
}

fn project(
    trusted: &mut TrustedInventory,
    repository_id: &str,
    mut candidates: Vec<Candidate>,
    automatic_dll: Option<&str>,
    dependency_request: Option<&ChangeRequest>,
    data_root: &Path,
) -> Result<bool, String> {
    if candidates.is_empty() {
        return Ok(false);
    }
    let Some(repo) = repository(trusted, repository_id) else {
        return Ok(false);
    };
    let target_id = format!("mod:{repository_id}");
    let Some(target_node) = trusted
        .inventory
        .nodes
        .iter()
        .find(|node| node.id == target_id && node.current.is_none())
    else {
        return Ok(false);
    };
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
        || dependency_request.is_some_and(|request| {
            request
                .roots
                .iter()
                .any(|root| root.node_id == node.id || root.node_id == target_id)
                || request
                    .repair_dependencies_of
                    .as_deref()
                    .is_some_and(|owner| owner == node.id || owner == target_id)
        })
    {
        return Ok(false);
    }
    let Some(files) = trusted.files.get(&node.id) else {
        return Ok(false);
    };
    let [manifest] = files.manifests.as_slice() else {
        return Ok(false);
    };
    let Some(key) = plugin.member_key.as_deref() else {
        return Ok(false);
    };
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
    let automatic = automatic_dll == Some(item.dll_file.as_str());
    candidates.retain(|candidate| {
        let (Some(tag), Some(prerelease)) = (
            candidate.tag.as_deref(),
            candidate
                .provides
                .first()
                .and_then(|plugin| plugin.prerelease),
        ) else {
            return false;
        };
        (automatic && repo.channel.allows_prerelease(prerelease))
            || (dependency_request.is_none()
                && related_release_source(&trusted.config, repo, manifest, tag, prerelease))
    });
    if candidates.is_empty() {
        return Ok(false);
    }
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
    let target = trusted.files.get(&target_id).ok_or(AMBIGUOUS)?;
    summary.summary.label.clone_from(&target.summary.label);
    summary
        .summary
        .name_i18n
        .clone_from(&target.summary.name_i18n);
    let mut projected = node.clone();
    projected.candidates_complete = target_node.candidates_complete;
    let proof = Consolidation {
        node: projected,
        files: summary,
        retired: vec![],
        replaced_ids: BTreeSet::from([node.id.clone(), target_id]),
    };
    attach(trusted, repository_id, candidates, proof, Some(retired));
    Ok(true)
}
