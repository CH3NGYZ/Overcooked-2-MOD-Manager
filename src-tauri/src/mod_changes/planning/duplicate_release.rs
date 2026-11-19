//! Session-only consolidation of proven physical copies during a Release upgrade.
use crate::mod_changes::{
    candidates,
    evaluate::same_guid,
    inventory::{self, NodeFiles, RetiredModCopy, TrustedInventory},
    model::{Candidate, ChangeNode, NodeKind},
    session::RootRequest,
    transaction::{Fingerprint, RootKind, TransactionPath, TransactionRoots, has_single_link},
};
use crate::{models::*, mods, releases};
use std::{
    cmp::Ordering,
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

const AMBIGUOUS: &str = "modChanges.multipleInstalledCopies";

pub(super) fn attach(
    trusted: &mut TrustedInventory,
    root: &RootRequest,
    data_root: &Path,
) -> Result<bool, String> {
    let (Some(repo_id), Some(candidate_id)) = (
        root.intent.node_id.strip_prefix("mod:"),
        root.intent.candidate_id.as_deref(),
    ) else {
        return Ok(false);
    };
    if !root.intent.enabled || root.intent.member_key.is_some() {
        return Ok(false);
    }
    let groups = trusted
        .files
        .values()
        .filter(|files| {
            files.summary.kind == NodeKind::Mod
                && files.summary.repository_id.as_deref() == Some(repo_id)
                && !files.summary.dll_files.is_empty()
        })
        .collect::<Vec<_>>();
    if groups.len() < 2 {
        return Ok(false);
    }
    let Some(repo) = trusted.config.repositories.iter().find(|r| r.id == repo_id) else {
        return Ok(false);
    };
    let Some(candidate) =
        candidates::for_root_asset(repo, None, &trusted.config, root.asset_name.as_deref())
            .into_iter()
            .find(|c| c.id == candidate_id && c.installable)
    else {
        return Ok(false);
    };
    let (tag, prerelease) = eligible_release(trusted, repo, &candidate).ok_or(AMBIGUOUS)?;
    let mut copies = Vec::with_capacity(groups.len());
    for files in groups {
        let id = &files.summary.node_id;
        if !trusted.duplicate_release_safe_nodes.contains(id) {
            return Err(AMBIGUOUS.into());
        }
        let node = trusted
            .inventory
            .nodes
            .iter()
            .find(|n| &n.id == id)
            .ok_or(AMBIGUOUS)?;
        let item = representative(trusted, repo, files).ok_or(AMBIGUOUS)?;
        if !is_affiliated(trusted, repo, item, files, tag, prerelease) {
            return Err(AMBIGUOUS.into());
        }
        copies.push(Copy { node, files, item });
    }
    let primary = select_primary(&copies, &repo.plugin_guid).ok_or(AMBIGUOUS)?;
    let roots = TransactionRoots {
        game_root: trusted.game_root.clone(),
        data_root: data_root.to_path_buf(),
    };
    let primary_identity = mods::PluginIdentity::from_installed(primary.item, &trusted.config);
    let primary_key = inventory::key(&primary.item.dll_file);
    let mut occupied = BTreeSet::new();
    for member in &primary.files.summary.members {
        if !occupied.insert(member.key.clone()) {
            return Err(AMBIGUOUS.into());
        }
        resolve_dll(&roots, &member.dll_file, member.enabled)?;
    }
    let mut combined = primary.node.clone();
    let mut files = primary.files.clone();
    let current = combined.current.as_mut().ok_or(AMBIGUOUS)?;
    // The same GUID may be present in several copies. Scope each old source
    // rule to the member that emitted it before the nodes were combined.
    for relation in &mut current.relations {
        if relation.owner_member_key.is_none()
            && relation
                .owner_guid
                .as_deref()
                .is_some_and(|guid| same_guid(guid, &repo.plugin_guid))
        {
            relation.owner_member_key = Some(primary_key.clone());
        }
    }
    let mut retired = Vec::new();
    let mut replaced_ids = BTreeSet::from([primary.node.id.clone()]);
    for copy in copies.iter().filter(|copy| copy.node.id != primary.node.id) {
        let identity = mods::PluginIdentity::from_installed(copy.item, &trusted.config);
        if !mods::same_release_plugin_identity(&primary_identity, &identity) {
            return Err(AMBIGUOUS.into());
        }
        let old = copy.node.current.as_ref().ok_or(AMBIGUOUS)?;
        let ([dll], [member], [provided]) = (
            copy.files.summary.dll_files.as_slice(),
            copy.files.summary.members.as_slice(),
            old.provides.as_slice(),
        ) else {
            return Err(AMBIGUOUS.into());
        };
        let member_key = inventory::key(&copy.item.dll_file);
        if !occupied.insert(member_key.clone())
            || dll != &member.dll_file
            || member.key != member_key
            || provided.member_key.as_deref() != Some(member_key.as_str())
        {
            return Err(AMBIGUOUS.into());
        }
        retired.push(retirement(&roots, copy, member_key)?);
        files.summary.dll_files.push(dll.clone());
        files.summary.members.push(member.clone());
        files.manifests.extend(copy.files.manifests.iter().cloned());
        current.provides.push(provided.clone());
        current
            .relations
            .extend(old.relations.iter().filter_map(|relation| {
                if relation.owner_member_key.is_none()
                    && relation
                        .owner_guid
                        .as_deref()
                        .is_some_and(|guid| !same_guid(guid, &provided.guid))
                {
                    return None;
                }
                let mut relation = relation.clone();
                relation.owner_member_key = provided.member_key.clone();
                relation.owner_guid = Some(provided.guid.clone());
                Some(relation)
            }));
        current.relations_known &= old.relations_known;
        combined.enabled |= copy.node.enabled;
        replaced_ids.insert(copy.node.id.clone());
    }
    // Retain the chosen core's version; its first scanned member was not
    // necessarily the core (a package may have optional siblings).
    current.version.clone_from(&primary.item.version);
    let id = root.intent.node_id.clone();
    combined.id.clone_from(&id);
    combined.mutable = true;
    files.summary.node_id.clone_from(&id);
    files.summary.enabled = combined.enabled;
    files.summary.candidates.push(inventory::CandidateSummary {
        id: candidate.id.clone(),
        tag: candidate.tag.clone(),
    });
    combined.candidates.push(candidate);
    trusted
        .inventory
        .nodes
        .retain(|n| !replaced_ids.contains(&n.id));
    trusted.files.retain(|id, _| !replaced_ids.contains(id));
    trusted.inventory.nodes.push(combined);
    trusted.files.insert(id.clone(), files);
    trusted.duplicate_release_retirements.insert(id, retired);
    Ok(true)
}

struct Copy<'a> {
    node: &'a ChangeNode,
    files: &'a NodeFiles,
    item: &'a InstalledMod,
}

fn representative<'a>(
    trusted: &'a TrustedInventory,
    repo: &ModRepositoryConfig,
    files: &NodeFiles,
) -> Option<&'a InstalledMod> {
    let items = files
        .summary
        .members
        .iter()
        .map(|member| {
            trusted.installed_mods.iter().find(|item| {
                item.dll_file == member.dll_file
                    && item.enabled == member.enabled
                    && item.guid == member.guid
            })
        })
        .collect::<Option<Vec<_>>>()?;
    let mut core = items.iter().copied().filter(|item| {
        item.guid
            .as_deref()
            .is_some_and(|guid| same_guid(guid, &repo.plugin_guid))
    });
    match core.next() {
        Some(item) if core.next().is_none() => Some(item),
        None if items.len() == 1 => items.first().copied(),
        _ => None,
    }
}

fn eligible_release<'a>(
    trusted: &TrustedInventory,
    repo: &ModRepositoryConfig,
    candidate: &'a Candidate,
) -> Option<(&'a str, bool)> {
    let tag = candidate.tag.as_deref()?;
    let prerelease = candidate.provides.first()?.prerelease?;
    if repo.download_mode != ModDownloadMode::Release
        || repo.plugin_guid.trim().is_empty()
        || repo.last_error.is_some()
        || releases::repository_coordinates(&repo.username, &repo.repository).is_err()
        || !line_allows(repo.channel, prerelease)
        || mods::compare_mod_versions(tag, tag).is_none()
    {
        return None;
    }
    // A UI pin can choose a download, but cannot authorize retiring other
    // copies when a later installable Release exists on this line.
    let has_newer = candidates::for_root_asset(repo, None, &trusted.config, None)
        .into_iter()
        .filter(|other| {
            other.installable
                && other.provides.first().and_then(|p| p.prerelease) == Some(prerelease)
        })
        .filter_map(|other| other.tag)
        .any(|other| {
            other != tag
                && releases::compare_release_versions(tag, &other) != Some(Ordering::Greater)
        });
    (!has_newer).then_some((tag, prerelease))
}

fn is_affiliated(
    trusted: &TrustedInventory,
    repo: &ModRepositoryConfig,
    item: &InstalledMod,
    files: &NodeFiles,
    tag: &str,
    prerelease: bool,
) -> bool {
    let Some(version) = item.version.as_deref() else {
        return false;
    };
    if item.is_dependency
        || item.bundled_with_dll.is_some()
        || item
            .guid
            .as_deref()
            .is_none_or(|guid| guid.trim().is_empty())
        || item.repository_id.as_deref() != Some(repo.id.as_str())
        || item.prerelease != prerelease
        || mods::compare_mod_versions(tag, version) != Some(Ordering::Greater)
        || releases::compare_release_versions(tag, version) != Some(Ordering::Greater)
        || files.manifests.len() != usize::from(item.package_id.is_some())
    {
        return false;
    }
    let binding_matches = |binding: &ModRepositoryBinding| {
        binding.is_valid()
            && binding.matches_identity(repo)
            && line_allows(binding.channel, prerelease)
            && releases::compare_release_versions(tag, &binding.tag) == Some(Ordering::Greater)
    };
    for manifest in &files.manifests {
        if item.package_id.as_deref() != Some(manifest.repository_id.as_str())
            || manifest
                .repository_binding
                .as_ref()
                .is_some_and(|b| !binding_matches(b))
            || (!manifest.release_tag.trim().is_empty()
                && releases::compare_release_versions(tag, &manifest.release_tag)
                    != Some(Ordering::Greater))
        {
            return false;
        }
    }
    if let Some(binding) = &item.repository_binding {
        return binding_matches(binding);
    }
    // A preference among different upstreams is not evidence of origin.
    item.repository_identity_match.is_some()
        && item.repository_ids.iter().any(|id| id == &repo.id)
        && item.repository_ids.iter().all(|id| {
            trusted
                .config
                .repositories
                .iter()
                .find(|source| source.id == *id)
                .is_some_and(|source| {
                    source.provider == repo.provider
                        && source.username.eq_ignore_ascii_case(&repo.username)
                        && source.repository.eq_ignore_ascii_case(&repo.repository)
                })
        })
}

fn line_allows(channel: Channel, prerelease: bool) -> bool {
    matches!(channel, Channel::All)
        || matches!(
            (channel, prerelease),
            (Channel::Stable, false) | (Channel::Beta, true)
        )
}

fn select_primary<'a>(copies: &'a [Copy<'a>], guid: &str) -> Option<&'a Copy<'a>> {
    let contenders = copies
        .iter()
        .filter(|copy| {
            copy.item
                .guid
                .as_deref()
                .is_some_and(|value| same_guid(guid, value))
        })
        .collect::<Vec<_>>();
    // Tied winners do not authorize choosing an arbitrary physical copy.
    contenders.iter().copied().find(|copy| {
        contenders.iter().copied().all(|other| {
            std::ptr::eq(*copy, other)
                || copy
                    .item
                    .version
                    .as_deref()
                    .zip(other.item.version.as_deref())
                    .and_then(|(left, right)| mods::compare_mod_versions(left, right))
                    == Some(Ordering::Greater)
        })
    })
}

fn retirement(
    roots: &TransactionRoots,
    copy: &Copy<'_>,
    member_key: String,
) -> Result<RetiredModCopy, String> {
    let canonical = &copy.item.dll_file;
    let disabled = mods::disabled_dll_file(canonical);
    if !canonical.starts_with(&format!("{}/", mods::file_state::PLUGINS_DIR))
        || !canonical.to_ascii_lowercase().ends_with(".dll")
        || copy.files.manifests.iter().any(|manifest| {
            let [file] = manifest.files.as_slice() else {
                return true;
            };
            file.replace('\\', "/") != *canonical && file.replace('\\', "/") != disabled
        })
    {
        return Err(AMBIGUOUS.into());
    }
    let (present, path) = resolve_dll(roots, canonical, copy.item.enabled)?;
    let fingerprint = Fingerprint::capture(&path)?;
    if !matches!(fingerprint, Fingerprint::File { .. }) {
        return Err(AMBIGUOUS.into());
    }
    let details = mods::read_dll_details(&path, &present);
    if details.guid != copy.item.guid
        || details.version.as_deref().map(str::trim) != copy.item.version.as_deref()
        || details.plugin_name != copy.item.plugin_name
        || details.product_name != copy.item.product_name
        || details.original_filename != copy.item.original_filename
        || Fingerprint::capture(&path)? != fingerprint
    {
        return Err("modChanges.targetChanged".into());
    }
    let manifest_fingerprint = copy
        .files
        .manifests
        .first()
        .map(|manifest| -> Result<Fingerprint, String> {
            let path = TransactionPath {
                root: RootKind::Data,
                relative: format!("manifests/{}.toml", manifest.repository_id),
            }
            .resolve(roots)?;
            if !fs::File::open(&path).is_ok_and(|file| has_single_link(&file)) {
                return Err(AMBIGUOUS.into());
            }
            let fingerprint = Fingerprint::capture(&path)?;
            let text = fs::read_to_string(&path).map_err(|_| AMBIGUOUS)?;
            let current: crate::archive::InstallManifest =
                toml::from_str(&text).map_err(|_| AMBIGUOUS)?;
            if !matches!(fingerprint, Fingerprint::File { .. })
                || serde_json::to_vec(&current).map_err(|_| AMBIGUOUS)?
                    != serde_json::to_vec(manifest).map_err(|_| AMBIGUOUS)?
                || Fingerprint::capture(&path)? != fingerprint
            {
                return Err("modChanges.targetChanged".into());
            }
            Ok(fingerprint)
        })
        .transpose()?;
    Ok(RetiredModCopy {
        member_key,
        label: copy.item.name.clone(),
        version: copy.item.version.clone().ok_or(AMBIGUOUS)?,
        dll_file: present,
        manifest_id: copy.item.package_id.clone(),
        fingerprint,
        manifest_fingerprint,
    })
}

fn resolve_dll(
    roots: &TransactionRoots,
    canonical: &str,
    enabled: bool,
) -> Result<(String, PathBuf), String> {
    let disabled = mods::disabled_dll_file(canonical);
    let (present, absent) = if enabled {
        (canonical, disabled.as_str())
    } else {
        (disabled.as_str(), canonical)
    };
    let resolve = |relative: &str| {
        TransactionPath {
            root: RootKind::Game,
            relative: relative.into(),
        }
        .resolve(roots)
    };
    let path = resolve(present)?;
    if resolve(absent)?.try_exists().map_err(|_| AMBIGUOUS)?
        || !path.is_file()
        || !fs::File::open(&path).is_ok_and(|file| has_single_link(&file))
    {
        return Err(AMBIGUOUS.into());
    }
    Ok((present.to_string(), path))
}
