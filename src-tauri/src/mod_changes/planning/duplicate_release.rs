//! Session-only consolidation of proven physical copies during a Release upgrade.
use crate::mod_changes::{
    candidates,
    evaluate::{DUPLICATE_ENABLED_PROVIDERS, matches, same_guid},
    inventory::{self, DuplicateReleaseUpgrade, NodeFiles, RetiredModCopy, TrustedInventory},
    model::{
        Candidate, ChangeNode, EvaluationState, MULTIPLE_INSTALLED_COPIES as AMBIGUOUS, NodeKind,
        SolveResult,
    },
    session::RootRequest,
    transaction::{Fingerprint, RootKind, TransactionPath, TransactionRoots, has_single_link},
};
use crate::{models::*, mods, releases};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

mod source_switch;

fn installed_groups(trusted: &TrustedInventory) -> impl Iterator<Item = (&str, &NodeFiles)> {
    trusted.files.values().filter_map(|files| {
        (files.summary.kind == NodeKind::Mod && !files.summary.dll_files.is_empty())
            .then_some(files.summary.repository_id.as_deref())
            .flatten()
            .map(|id| (id, files))
    })
}

pub(super) fn has_copies(trusted: &TrustedInventory, repo_id: &str) -> bool {
    installed_groups(trusted)
        .filter(|(id, _)| *id == repo_id)
        .nth(1)
        .is_some()
}

/// A single standalone provider with a changed GUID needs the same proof as
/// multiple copies. A normal same-GUID upgrade keeps the regular path.
fn historical_core(files: &NodeFiles, repo: &ModRepositoryConfig) -> bool {
    repo.download_mode == ModDownloadMode::Release
        && !repo.plugin_guid.trim().is_empty()
        && matches!(&files.summary.members[..], [member]
        if member.guid.as_deref().is_some_and(|guid| {
            !guid.trim().is_empty() && !same_guid(guid, &repo.plugin_guid)
        }))
}

pub(super) fn attach_root(
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
    if source_switch::attach_root(trusted, root, repo_id, data_root)? {
        return Ok(true);
    }
    let Some(repo) = trusted.config.repositories.iter().find(|r| r.id == repo_id) else {
        return Ok(false);
    };
    if !has_copies(trusted, repo_id)
        && !trusted
            .files
            .get(&root.intent.node_id)
            .is_some_and(|files| historical_core(files, repo))
    {
        return Ok(false);
    }
    let Some(candidate) =
        candidates::for_root_asset(repo, None, &trusted.config, root.asset_name.as_deref())
            .into_iter()
            .find(|c| c.id == candidate_id && c.installable)
    else {
        return Ok(false);
    };
    let default_assets = candidates::for_root_asset(repo, None, &trusted.config, None);
    let describe =
        || crate::mod_changes::evidence::duplicate_replacement(trusted, repo_id, AMBIGUOUS);
    let (tag, prerelease) =
        eligible_release(repo, &candidate, &default_assets).ok_or_else(describe)?;
    let proof = prove(trusted, repo, tag, prerelease, data_root).map_err(|error| {
        if error == AMBIGUOUS {
            describe()
        } else {
            error
        }
    })?;
    attach(trusted, repo_id, candidate, proof, None);
    Ok(true)
}

/// Consider only cached Releases. An installed repair's physical root must
/// keep its identity; only other groups can be consolidated as dependencies.
pub(super) fn attach_dependencies(
    trusted: &mut TrustedInventory,
    owner_id: &str,
    data_root: &Path,
) {
    let root_repo = trusted
        .files
        .get(owner_id)
        .and_then(|files| files.summary.repository_id.clone());
    let mut counts = BTreeMap::new();
    for (id, _) in installed_groups(trusted) {
        if root_repo.as_deref() != Some(id) {
            *counts.entry(id).or_insert(0usize) += 1;
        }
    }
    let ids = trusted
        .config
        .repositories
        .iter()
        .filter(|repo| {
            counts.get(repo.id.as_str()).is_some_and(|count| {
                *count > 1
                    || trusted
                        .files
                        .get(&format!("mod:{}", repo.id))
                        .is_some_and(|files| historical_core(files, repo))
            })
        })
        .map(|repo| repo.id.clone())
        .collect::<Vec<_>>();
    for repo_id in ids {
        let Some(repo) = trusted
            .config
            .repositories
            .iter()
            .find(|repo| repo.id == repo_id)
        else {
            continue;
        };
        let default_assets = candidates::for_root_asset(repo, None, &trusted.config, None);
        let mut blocked = Vec::new();
        let mut upgrade = None;
        for candidate in candidates::for_repository(repo, None, false, &trusted.config) {
            let Some((tag, prerelease)) = eligible_release(repo, &candidate, &default_assets)
            else {
                continue;
            };
            match prove(trusted, repo, tag, prerelease, data_root) {
                Ok(proof) => {
                    upgrade = Some((candidate, proof));
                    break;
                }
                Err(error) => {
                    log::debug!(
                        "整链无法确认重复副本升级：仓库={repo_id}，版本={tag}，原因={error}"
                    );
                    blocked.extend(candidate.provides);
                }
            }
        }
        if let Some((candidate, proof)) = upgrade {
            attach(trusted, &repo_id, candidate, proof, None);
        } else {
            if let Some(node) = trusted
                .inventory
                .nodes
                .iter_mut()
                .find(|node| node.id == format!("mod:{repo_id}"))
            {
                // Do not fall back to the regular installation path for a
                // unique historical GUID whose retirement was not proven.
                blocked.extend(
                    node.candidates
                        .iter()
                        .flat_map(|candidate| candidate.provides.iter().cloned()),
                );
                node.candidates.clear();
            }
            trusted.duplicate_releases.blocked.extend(blocked);
        }
    }
}

/// Report the rejected candidate on its actual failed dependency edge, not
/// on an ancestor or on an otherwise successful plan.
pub(super) fn explain_blockers(trusted: &TrustedInventory, result: &mut SolveResult) {
    let blocked = &trusted.duplicate_releases.blocked;
    if blocked.is_empty() {
        return;
    }
    for trace in result
        .blockers
        .iter_mut()
        .flat_map(|blocker| &mut blocker.traces)
    {
        if let Some(step) = trace.last_mut()
            && (matches!(
                step.state,
                EvaluationState::Missing
                    | EvaluationState::Disabled
                    | EvaluationState::VersionMismatch
            ) || step.reason == DUPLICATE_ENABLED_PROVIDERS)
            && let Some(relation) = &step.relation
            && relation.kind == RelationKind::Requires
            && !relation.optional
            && blocked.iter().any(|plugin| {
                same_guid(&plugin.guid, &relation.guid) && matches(relation, plugin) == Some(true)
            })
        {
            step.reason = AMBIGUOUS.into();
        }
    }
}

struct Consolidation {
    node: ChangeNode,
    files: NodeFiles,
    retired: Vec<RetiredModCopy>,
    replaced_ids: BTreeSet<String>,
}

fn prove(
    trusted: &TrustedInventory,
    repo: &ModRepositoryConfig,
    tag: &str,
    prerelease: bool,
    data_root: &Path,
) -> Result<Consolidation, String> {
    let groups = installed_groups(trusted)
        .filter_map(|(id, files)| (id == repo.id).then_some(files))
        .collect::<Vec<_>>();
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
        let identity = mods::PluginIdentity::from_installed(item, &trusted.config);
        copies.push(Copy {
            node,
            files,
            item,
            identity,
        });
    }
    let primary = select_primary(&copies, repo).ok_or(AMBIGUOUS)?;
    let roots = TransactionRoots {
        game_root: trusted.game_root.clone(),
        data_root: data_root.to_path_buf(),
    };
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
                .zip(primary.item.guid.as_deref())
                .is_some_and(|(owner, guid)| same_guid(owner, guid))
        {
            relation.owner_member_key = Some(primary_key.clone());
        }
    }
    let mut retired = Vec::new();
    if !primary.has_guid(&repo.plugin_guid) {
        // All physical copies are predecessors of the new GUID. Even the
        // representative is an explicit retirement and cannot survive the
        // replacement payload as a second provider.
        retired.push(retirement(&roots, primary, primary_key)?);
    }
    let mut replaced_ids = BTreeSet::from([primary.node.id.clone()]);
    for copy in copies.iter().filter(|copy| copy.node.id != primary.node.id) {
        if !mods::same_release_plugin_identity(&primary.identity, &copy.identity) {
            return Err(AMBIGUOUS.into());
        }
        let old = copy.node.current.as_ref().ok_or(AMBIGUOUS)?;
        let member_key = inventory::key(&copy.item.dll_file);
        if !occupied.insert(member_key.clone()) {
            return Err(AMBIGUOUS.into());
        }
        retired.push(retirement(&roots, copy, member_key)?);
        let provided = &old.provides[0]; // `retirement` verifies the standalone shape.
        files.summary.dll_files.push(copy.item.dll_file.clone());
        files
            .summary
            .members
            .push(copy.files.summary.members[0].clone());
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
    Ok(Consolidation {
        node: combined,
        files,
        retired,
        replaced_ids,
    })
}

fn attach(
    trusted: &mut TrustedInventory,
    repo_id: &str,
    candidate: Candidate,
    proof: Consolidation,
    source_switch: Option<RetiredModCopy>,
) {
    let Consolidation {
        node: mut combined,
        mut files,
        retired,
        replaced_ids,
    } = proof;
    let id = format!("mod:{repo_id}");
    combined.id.clone_from(&id);
    combined.mutable = true;
    let candidate_id = candidate.id.clone();
    files.summary.node_id.clone_from(&id);
    files.summary.enabled = combined.enabled;
    if source_switch.is_some() {
        files.summary.repository_id = Some(repo_id.into());
    }
    files.summary.candidates.clear();
    files.summary.candidates.push(inventory::CandidateSummary {
        id: candidate.id.clone(),
        tag: candidate.tag.clone(),
    });
    combined.candidates = vec![candidate];
    trusted
        .inventory
        .nodes
        .retain(|n| !replaced_ids.contains(&n.id));
    trusted.files.retain(|id, _| !replaced_ids.contains(id));
    trusted.inventory.nodes.push(combined);
    trusted.files.insert(id.clone(), files);
    trusted.duplicate_releases.upgrades.insert(
        id,
        DuplicateReleaseUpgrade {
            candidate_id,
            copies: retired,
            source_switch,
        },
    );
}

struct Copy<'a> {
    node: &'a ChangeNode,
    files: &'a NodeFiles,
    item: &'a InstalledMod,
    identity: mods::PluginIdentity<'a>,
}

impl Copy<'_> {
    fn has_guid(&self, guid: &str) -> bool {
        self.item
            .guid
            .as_deref()
            .is_some_and(|value| same_guid(guid, value))
    }
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
    repo: &ModRepositoryConfig,
    candidate: &'a Candidate,
    default_assets: &[Candidate],
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
    let has_newer = default_assets
        .iter()
        .filter(|other| {
            other.installable
                && other.provides.first().and_then(|p| p.prerelease) == Some(prerelease)
        })
        .filter_map(|other| other.tag.as_deref())
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

fn select_primary<'a>(copies: &'a [Copy<'a>], repo: &ModRepositoryConfig) -> Option<&'a Copy<'a>> {
    let mut contenders = copies
        .iter()
        .filter(|copy| copy.has_guid(&repo.plugin_guid))
        .collect::<Vec<_>>();
    if contenders.is_empty() {
        if copies
            .iter()
            .any(|copy| !copy.identity.matches_release_core(repo))
        {
            return None;
        }
        contenders.extend(copies);
    }
    // An absent field cannot mask disagreement between two other copies.
    let fields: [fn(&Copy<'a>) -> Option<&'a str>; 2] = [
        |copy: &Copy<'a>| copy.item.product_name.as_deref(),
        |copy: &Copy<'a>| copy.item.original_filename.as_deref(),
    ];
    for field in fields {
        let mut names = copies
            .iter()
            .filter_map(field)
            .filter(|value| !value.trim().is_empty());
        if let Some(first) = names.next()
            && names.any(|other| !first.trim().eq_ignore_ascii_case(other.trim()))
        {
            return None;
        }
    }
    // The selected Release is strictly newer than every copy. A tie among
    // locals cannot affect the final provider. Preserve the unique package's
    // context if it is tied with standalone DLLs, otherwise use a stable order.
    let order = |copy: &Copy<'_>| (copy.files.summary.members.len() > 1, copy.item.enabled);
    contenders.into_iter().max_by(|left, right| {
        mods::compare_mod_versions(
            left.item.version.as_deref().unwrap_or_default(),
            right.item.version.as_deref().unwrap_or_default(),
        )
        .unwrap_or(Ordering::Equal)
        .then_with(|| order(left).cmp(&order(right)))
        .then_with(|| right.node.id.cmp(&left.node.id))
    })
}

fn retirement(
    roots: &TransactionRoots,
    copy: &Copy<'_>,
    member_key: String,
) -> Result<RetiredModCopy, String> {
    let canonical = &copy.item.dll_file;
    let disabled = mods::disabled_dll_file(canonical);
    let ([dll], [member], Some(current)) = (
        copy.files.summary.dll_files.as_slice(),
        copy.files.summary.members.as_slice(),
        copy.node.current.as_ref(),
    ) else {
        return Err(AMBIGUOUS.into());
    };
    if current.provides.len() != 1
        || current.provides[0].member_key.as_deref() != Some(member_key.as_str())
        || dll != canonical
        || &member.dll_file != canonical
        || member.key != member_key
        || !canonical.starts_with(&format!("{}/", mods::file_state::PLUGINS_DIR))
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
