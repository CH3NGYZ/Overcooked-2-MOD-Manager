//! Trusted disk inventory. UI arrays never authorize a file or select its owner.
use super::{install_notes::ManagedMods, model::*};
use crate::{app_state::AppState, archive, game, level_sets, models::*, mods};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

mod declared_bundles;
mod duplicate_releases;
pub(super) use duplicate_releases::{DuplicateReleaseUpgrade, DuplicateReleases};

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MemberSummary {
    pub key: String,
    pub dll_file: String,
    pub enabled: bool,
    pub guid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<DllRole>,
}

/// UI category only. The solver's `kind` continues to describe mutation ownership.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PresentationKind {
    ModPackage,
    Plugin,
    Dependency,
}

impl From<DllRole> for PresentationKind {
    fn from(role: DllRole) -> Self {
        match role {
            DllRole::Mod => Self::Plugin,
            DllRole::Dependency => Self::Dependency,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RetiredModCopy {
    pub member_key: String,
    pub label: String,
    pub version: String,
    pub dll_file: String,
    #[serde(skip)]
    pub(super) manifest_id: Option<String>,
    #[serde(skip)]
    pub(super) fingerprint: super::transaction::Fingerprint,
    #[serde(skip)]
    pub(super) manifest_fingerprint: Option<super::transaction::Fingerprint>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct NodeSummary {
    pub node_id: String,
    pub label: String,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub name_i18n: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_name_zh: Option<String>,
    pub kind: NodeKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presentation_kind: Option<PresentationKind>,
    pub enabled: bool,
    pub repository_id: Option<String>,
    pub package_id: Option<String>,
    pub dll_files: Vec<String>,
    pub members: Vec<MemberSummary>,
    pub map_dir_name: Option<String>,
    pub map_id: Option<String>,
    pub subject_tag: Option<String>,
    pub candidates: Vec<CandidateSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CandidateSummary {
    pub id: String,
    pub tag: Option<String>,
}

#[derive(Clone)]
pub(crate) struct NodeFiles {
    pub summary: NodeSummary,
    pub manifests: Vec<archive::InstallManifest>,
    /// All paths are backend-created game-relative paths, not display paths.
    pub map_path: Option<String>,
    pub map_metadata: Option<crate::asset_bundle::LevelSetMetadata>,
}

#[derive(Clone)]
pub(crate) struct TrustedInventory {
    pub inventory: Inventory,
    pub files: BTreeMap<String, NodeFiles>,
    pub revision: String,
    pub(super) replan_scope: Option<String>,
    pub game_root: PathBuf,
    pub level_roots: level_sets::LevelSetRoots,
    pub config: AppConfig,
    pub managed_mods: ManagedMods,
    /// Installed nodes with no current, filtered or historical binding evidence.
    pub toggle_unbound_nodes: BTreeSet<String>,
    pub(super) manual_declaration_notices: BTreeMap<String, Vec<String>>,
    pub(super) installed_mods: Arc<[InstalledMod]>,
    /// Physical ownership checks separate from the missing repository alias.
    pub(super) duplicate_release_safe_nodes: BTreeSet<String>,
    /// Scan-proved DLL companions with no install-manifest removal right.
    pub(super) declared_bundle_switch_members: BTreeMap<String, BTreeSet<String>>,
    /// Filled only on the plan's private inventory, never in the scan cache.
    pub(super) duplicate_releases: DuplicateReleases,
}

pub(crate) fn relative(root: &Path, path: &Path) -> Result<String, String> {
    path.strip_prefix(root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .map_err(|_| "archive.pathEscape".into())
}

pub(crate) fn key(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    if cfg!(windows) {
        normalized.to_lowercase()
    } else {
        normalized
    }
}

pub(super) fn replan_scope(
    state: &AppState,
    root: &Path,
    config: &AppConfig,
) -> Result<String, String> {
    static KEY: OnceLock<uuid::Uuid> = OnceLock::new();
    let selection = state.config.data_selection();
    if selection.0 != config.data_provider {
        return Err("modChanges.targetChanged".into());
    }
    let mut hash = Sha256::new();
    hash.update(KEY.get_or_init(uuid::Uuid::new_v4).as_bytes());
    hash.update(selection.1.to_le_bytes());
    for path in [root, state.config.config_path().as_path()] {
        let bytes = path.as_os_str().as_encoded_bytes();
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    hash.update(
        serde_json::to_vec(&(config, config.data_provider))
            .map_err(|_| "modChanges.targetChanged")?,
    );
    Ok(format!("{:x}", hash.finalize()))
}

pub(crate) fn revision(
    state: &AppState,
    root: &Path,
    config: &AppConfig,
) -> Result<String, String> {
    revision_for_scope(state, root, &replan_scope(state, root, config)?)
}

pub(super) fn revision_for_scope(
    state: &AppState,
    root: &Path,
    scope: &str,
) -> Result<String, String> {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    scope.hash(&mut hash);
    for directory in [
        root.join("BepInEx/plugins"),
        level_sets::disabled_root(root),
        state.config.manifests_dir(),
    ] {
        if !directory.exists() {
            continue;
        }
        let mut entries = Vec::new();
        for entry in walkdir::WalkDir::new(&directory).follow_links(false) {
            let entry = entry.map_err(|e| e.to_string())?;
            // Windows directory enumeration may retain the parent's timestamp
            // from before its newest child was closed. Read current metadata by
            // path rather than hashing WalkDir's cached enumeration metadata.
            let meta = std::fs::symlink_metadata(entry.path()).map_err(|e| e.to_string())?;
            if meta.file_type().is_symlink() {
                return Err("archive.pathEscape".into());
            }
            entries.push((entry.path().to_path_buf(), meta.len(), meta.modified().ok()));
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries.hash(&mut hash);
    }
    Ok(format!("{:016x}", hash.finish()))
}

pub(crate) fn owned(
    relations: Vec<RepositoryRelation>,
    guid: Option<String>,
) -> Vec<OwnedRelation> {
    relations
        .into_iter()
        .map(|relation| OwnedRelation {
            owner_member_key: None,
            owner_guid: guid.clone(),
            relation,
        })
        .collect()
}

fn current_candidate(
    items: &[InstalledMod],
    repo: Option<&ModRepositoryConfig>,
    manifests: &[archive::InstallManifest],
    id: &str,
    config: &AppConfig,
) -> (Candidate, Vec<String>) {
    let installed_tag = manifests
        .first()
        .map(|m| m.release_tag.clone())
        .filter(|t| !t.is_empty())
        .or_else(|| {
            items
                .first()?
                .repository_binding
                .as_ref()
                .map(|b| b.tag.clone())
        });
    let primary = items
        .iter()
        .filter(|item| {
            !item.is_dependency && !item.release_companion && item.bundled_with_dll.is_none()
        })
        .max_by_key(|item| match item.repository_identity_match {
            Some(RepositoryIdentityMatch::Guid) => 2,
            Some(RepositoryIdentityMatch::Metadata) => 1,
            None => 0,
        });
    let version = primary.and_then(|item| item.version.clone());
    let effective_version = primary.and_then(mods::local_update_version);
    let changed_release = primary.is_some_and(|item| {
        item.repository_binding
            .as_ref()
            .is_some_and(|binding| effective_version != Some(binding.tag.as_str()))
    });
    // An installed candidate's display/ordering tag is only valid while the
    // DLL still represents that Release. The original tag stays in the manifest.
    let tag = if changed_release { None } else { installed_tag };
    let mut relations = repo
        .map(|r| {
            owned(
                mods::effective_relations(r, tag.as_deref().or(version.as_deref())),
                (!r.plugin_guid.is_empty()).then(|| r.plugin_guid.clone()),
            )
        })
        .unwrap_or_else(|| {
            if changed_release {
                // The only snapshot here describes the pre-replacement Release.
                return vec![];
            }
            items
                .iter()
                .flat_map(|i| {
                    i.relations.iter().cloned().map(|relation| OwnedRelation {
                        owner_member_key: Some(key(&i.dll_file)),
                        owner_guid: i.guid.clone(),
                        relation,
                    })
                })
                .collect()
        });
    let manual = matches!(manifests, [manifest] if manifest.is_manual());
    let provides = items
        .iter()
        .map(|i| ProvidedPlugin {
            member_key: Some(key(&i.dll_file)),
            guid: i.guid.clone().unwrap_or_default(),
            version: i.version.clone(),
            prerelease: (!manual).then_some(i.prerelease),
            enabled: i.enabled,
        })
        .collect::<Vec<_>>();
    let version = if manual {
        match super::level_trees::unique_framework_member(id, &provides) {
            Ok(Some(framework)) => framework.version.clone(),
            Ok(None) => version,
            Err(_) => None,
        }
    } else {
        version
    };
    let mut relations_known = repo.is_some()
        || (!changed_release
            && (!manifests.is_empty() || items.iter().any(|item| !item.relations.is_empty())));
    let mut notices = Vec::new();
    if manual {
        let declarations = super::payload::manual_relations::for_members(config, &provides);
        relations = declarations.relations;
        notices = declarations.notices;
        relations_known = notices.is_empty();
    } else {
        for member in &provides {
            if member.guid.is_empty()
                || repo.is_some_and(|repo| repo.plugin_guid.eq_ignore_ascii_case(&member.guid))
            {
                continue;
            }
            let (member_relations, known) = super::inventory_maps::member_relations(config, member);
            relations.extend(member_relations);
            relations_known &= known != Some(false);
        }
    }
    (
        Candidate {
            id: format!("installed:{id}"),
            source_id: repo.map(|r| r.id.clone()),
            tag,
            version,
            provides,
            relations,
            relations_known,
            installable: true,
            raw: false,
            verification_pending: false,
        },
        notices,
    )
}

pub(crate) fn build(state: &AppState) -> Result<TrustedInventory, String> {
    let config = state.config.load_or_default();
    let detected = game::detect_game(&config).path.ok_or("game.notFound")?;
    let game_root = game::resolve_existing_ancestor(Path::new(&detected))?;
    let scope = replan_scope(state, &game_root, &config)?;
    let stamp = revision_for_scope(state, &game_root, &scope)?;
    let installed =
        mods::scan_mods_with_manifests(&game_root, &config, Some(&state.config.manifests_dir()));
    let managed_mods = ManagedMods::from_scan(&installed);
    let manifests = archive::read_manifests(&state.config.manifests_dir());
    let frameworks = mods::plugin_target::eligible(&installed, mods::plugin_target::DIYLEVEL_GUID);
    if frameworks.len() > 1 {
        return Err("framework.diyLevelAmbiguous".into());
    }
    let package = frameworks
        .first()
        .map(|i| mods::plugin_target::directory(&game_root, &i.dll_file, i.enabled))
        .transpose()?;
    let level_roots = level_sets::LevelSetRoots::new(game_root.clone(), package);
    let installed_mods: Arc<[_]> = installed.clone().into();
    let mut manifest_claims = HashMap::<String, usize>::new();
    for path in manifests.iter().flat_map(|manifest| &manifest.files) {
        *manifest_claims
            .entry(mods::manifest_dll_key(path))
            .or_default() += 1;
    }
    let grouped = declared_bundles::group_mods(installed);
    let mut repo_count: HashMap<String, usize> = HashMap::new();
    for items in grouped.values() {
        let ids = items
            .iter()
            .filter_map(|i| i.repository_id.as_ref())
            .collect::<std::collections::BTreeSet<_>>();
        if ids.len() == 1 {
            *repo_count
                .entry((*ids.first().unwrap()).clone())
                .or_default() += 1;
        }
    }
    let mut inventory = Inventory::default();
    let mut files = BTreeMap::new();
    let mut toggle_unbound_nodes = BTreeSet::new();
    let mut manual_declaration_notices = BTreeMap::new();
    let mut duplicate_release_safe_nodes = BTreeSet::new();
    let mut declared_bundle_switch_members = BTreeMap::new();
    for (fallback, items) in grouped {
        let ids = items
            .iter()
            .filter_map(|i| i.repository_id.as_ref())
            .collect::<std::collections::BTreeSet<_>>();
        let repo = (ids.len() == 1)
            .then(|| {
                config
                    .repositories
                    .iter()
                    .find(|r| Some(&r.id) == ids.first().copied())
            })
            .flatten();
        let unique = repo.is_some_and(|r| repo_count.get(&r.id) == Some(&1));
        let id = if unique {
            format!("mod:{}", repo.unwrap().id)
        } else {
            fallback
        };
        let owned_manifests: Vec<_> = manifests
            .iter()
            .filter(|m| {
                items
                    .iter()
                    .any(|i| i.package_id.as_deref() == Some(&m.repository_id))
            })
            .cloned()
            .collect();
        let (current, notices) = current_candidate(&items, repo, &owned_manifests, &id, &config);
        if !notices.is_empty() {
            manual_declaration_notices.insert(current.id.clone(), notices);
        }
        if items.iter().all(|item| {
            item.repository_id.is_none()
                && item.repository_ids.is_empty()
                && item.repository_binding.is_none()
                && (item.package_id.is_none() || item.manual_install)
                && !config.repositories.iter().any(|repo| {
                    mods::repository_matches_identity(
                        repo,
                        item.guid.as_deref(),
                        item.product_name.as_deref(),
                        item.original_filename.as_deref(),
                    )
                })
        }) {
            toggle_unbound_nodes.insert(id.clone());
        }
        let enabled = items.iter().any(|i| i.enabled);
        let representative = items.iter().find(|item| item.bundled_with_dll.is_none());
        let (label, name_i18n) = mod_label(repo, representative);
        let summary = NodeSummary {
            node_id: id.clone(),
            label,
            name_i18n,
            map_name_zh: None,
            kind: NodeKind::Mod,
            presentation_kind: (items.len() > 1).then_some(PresentationKind::ModPackage),
            enabled,
            repository_id: repo.map(|r| r.id.clone()),
            package_id: representative.and_then(|i| i.package_id.clone()),
            dll_files: items.iter().map(|i| i.dll_file.clone()).collect(),
            members: items
                .iter()
                .map(|i| MemberSummary {
                    key: key(&i.dll_file),
                    dll_file: i.dll_file.clone(),
                    enabled: i.enabled,
                    guid: i.guid.clone(),
                    role: if i.is_dependency {
                        Some(DllRole::Dependency)
                    } else if i.guid.is_some() || i.plugin_name.is_some() || i.version.is_some() {
                        Some(DllRole::Mod)
                    } else {
                        None
                    },
                })
                .collect(),
            map_dir_name: None,
            map_id: None,
            subject_tag: None,
            candidates: Vec::new(),
        };
        let candidates = if unique {
            super::candidates::for_repository(repo.unwrap(), Some(&current), false, &config)
        } else {
            Vec::new()
        };
        let mut safe = true;
        for item in &items {
            let owners = manifests
                .iter()
                .filter(|manifest| {
                    manifest
                        .files
                        .iter()
                        .any(|file| key(&mods::canonical_dll_name(file)) == key(&item.dll_file))
                })
                .count();
            let active = game::safe_game_path(&game_root, &item.dll_file)?;
            let disabled =
                game::safe_game_path(&game_root, &mods::disabled_dll_file(&item.dll_file))?;
            if owners > 1 || (active.is_file() && disabled.is_file()) {
                safe = false;
            }
        }
        let declared_members = items
            .iter()
            .filter(|item| item.bundled_with_dll.is_some())
            .map(|item| key(&item.dll_file))
            .collect::<BTreeSet<_>>();
        if safe
            && repo.is_some()
            && declared_members.is_empty()
            && owned_manifests.iter().all(|manifest| {
                manifest
                    .files
                    .iter()
                    .all(|file| manifest_claims.get(&mods::manifest_dll_key(file)) == Some(&1))
            })
            && items.iter().all(|item| {
                let expected = usize::from(item.package_id.is_some());
                manifest_claims
                    .get(&mods::manifest_dll_key(&item.dll_file))
                    .copied()
                    .unwrap_or_default()
                    == expected
            })
        {
            duplicate_release_safe_nodes.insert(id.clone());
        }
        if !declared_members.is_empty() {
            declared_bundle_switch_members.insert(id.clone(), declared_members);
        }
        inventory.nodes.push(ChangeNode {
            id: id.clone(),
            kind: NodeKind::Mod,
            current: Some(current),
            enabled,
            mutable: safe && (repo.is_none() || unique),
            candidates,
            candidates_complete: repo.is_none_or(|r| {
                r.cached_releases_complete
                    && r.last_error.is_none()
                    && r.releases_cached_at.is_some()
            }),
        });
        files.insert(
            id,
            NodeFiles {
                summary,
                manifests: owned_manifests,
                map_path: None,
                map_metadata: None,
            },
        );
    }
    for repo in &config.repositories {
        if !crate::config::mod_repository_is_available(&config, repo, &config.region)
            || repo_count.contains_key(&repo.id)
        {
            continue;
        }
        let id = format!("mod:{}", repo.id);
        let candidates = super::candidates::for_repository(repo, None, false, &config);
        inventory.nodes.push(ChangeNode {
            id: id.clone(),
            kind: NodeKind::Mod,
            current: None,
            enabled: false,
            mutable: true,
            candidates,
            candidates_complete: repo.cached_releases_complete
                && repo.last_error.is_none()
                && repo.releases_cached_at.is_some(),
        });
        files.insert(
            id.clone(),
            NodeFiles {
                summary: NodeSummary {
                    node_id: id,
                    label: repo.name.clone(),
                    name_i18n: repo.name_i18n.clone(),
                    map_name_zh: None,
                    kind: NodeKind::Mod,
                    presentation_kind: None,
                    enabled: false,
                    repository_id: Some(repo.id.clone()),
                    package_id: None,
                    dll_files: vec![],
                    members: vec![],
                    map_dir_name: None,
                    map_id: None,
                    subject_tag: None,
                    candidates: vec![],
                },
                manifests: vec![],
                map_path: None,
                map_metadata: None,
            },
        );
    }
    toggle_unbound_nodes.extend(super::inventory_maps::append(
        state,
        &config,
        &game_root,
        &level_roots,
        &mut inventory,
        &mut files,
    )?);
    for node in &inventory.nodes {
        if let Some(entry) = files.get_mut(&node.id) {
            entry.summary.candidates = node
                .current
                .iter()
                .chain(&node.candidates)
                .map(|c| CandidateSummary {
                    id: c.id.clone(),
                    tag: c.tag.clone(),
                })
                .collect();
        }
    }
    if revision(state, &game_root, &state.config.load_or_default())? != stamp {
        return Err("modChanges.targetChanged".into());
    }
    Ok(TrustedInventory {
        inventory,
        files,
        revision: stamp,
        replan_scope: Some(scope),
        game_root,
        level_roots,
        config,
        managed_mods,
        toggle_unbound_nodes,
        manual_declaration_notices,
        installed_mods,
        duplicate_release_safe_nodes,
        declared_bundle_switch_members,
        duplicate_releases: DuplicateReleases::default(),
    })
}

fn mod_label(
    repo: Option<&ModRepositoryConfig>,
    installed: Option<&InstalledMod>,
) -> (String, BTreeMap<String, String>) {
    if let Some(repo) = repo.filter(|repo| {
        !repo.name.trim().is_empty() || repo.name_i18n.values().any(|name| !name.trim().is_empty())
    }) {
        return (repo.name.clone(), repo.name_i18n.clone());
    }
    let Some(installed) = installed else {
        return Default::default();
    };
    let name = [
        installed.package_name.as_deref(),
        installed.plugin_name.as_deref(),
        installed.title.as_deref(),
        installed.product_name.as_deref(),
        Some(installed.name.as_str()),
    ]
    .into_iter()
    .flatten()
    .find(|name| !name.trim().is_empty())
    .unwrap_or_default();
    (name.into(), installed.package_name_i18n.clone())
}

/// Reuse one backend-owned scan until either source selection or physical facts change.
pub(crate) fn cached(state: &AppState) -> Result<std::sync::Arc<TrustedInventory>, String> {
    // Evaluation must never publish half-applied facts. Writers already use this
    // same lock; recovery invalidates the cache before a new snapshot is read.
    let _operation = state.operation.lock().map_err(|_| "lock.poisoned")?;
    super::guard::ensure_recovered(state)?;
    let config = state.config.load_or_default();
    let detected = game::detect_game(&config).path.ok_or("game.notFound")?;
    let root = game::resolve_existing_ancestor(Path::new(&detected))?;
    let stamp = revision(state, &root, &config)?;
    if let Some(snapshot) = state
        .mod_changes
        .lock()
        .map_err(|_| "lock.poisoned")?
        .snapshot
        .clone()
        && snapshot.revision == stamp
        && snapshot.game_root == root
    {
        return Ok(snapshot);
    }
    let snapshot = std::sync::Arc::new(build(state)?);
    state
        .mod_changes
        .lock()
        .map_err(|_| "lock.poisoned")?
        .snapshot = Some(snapshot.clone());
    Ok(snapshot)
}

/// Ownership and duplicate barriers apply only to the chosen roots, changed nodes
/// and their required closure. Unrelated baseline problems never authorize writes.
pub(crate) fn validate_solution(
    trusted: &TrustedInventory,
    request: &ChangeRequest,
    solved: &Inventory,
    suppressed: &BTreeSet<&str>,
) -> Result<(), String> {
    use super::evaluate::{matches, relation_active, targets};
    use std::collections::BTreeSet;
    if let Some(id) = request.repair_dependencies_of.as_deref() {
        let before = trusted.inventory.nodes.iter().find(|node| node.id == id);
        let after = solved.nodes.iter().find(|node| node.id == id);
        if before.is_none() || before != after {
            return Err("modChanges.targetChanged".into());
        }
    }
    let mut affected = request
        .roots
        .iter()
        .map(|root| root.node_id.clone())
        .collect::<BTreeSet<_>>();
    for node in &solved.nodes {
        if trusted
            .inventory
            .nodes
            .iter()
            .find(|original| original.id == node.id)
            .is_none_or(|original| {
                original.current != node.current || original.enabled != node.enabled
            })
        {
            affected.insert(node.id.clone());
        }
    }
    loop {
        let before = affected.len();
        for node in &solved.nodes {
            if !node.enabled
                || !affected.contains(&node.id)
                || suppressed.contains(node.id.as_str())
            {
                continue;
            }
            let Some(candidate) = &node.current else {
                continue;
            };
            for owned in &candidate.relations {
                if owned.relation.kind != RelationKind::Requires
                    || owned.relation.optional
                    || !relation_active(candidate, owned, false)
                {
                    continue;
                }
                for (index, plugin) in targets(solved, &owned.relation.guid, true) {
                    if matches(&owned.relation, plugin) != Some(false) {
                        affected.insert(solved.nodes[index].id.clone());
                    }
                }
            }
        }
        if before == affected.len() {
            break;
        }
    }
    // Plugin GUIDs cannot represent the native loader. Reuse its real disk
    // detection at every approval/write gate, including local ZIP installs.
    // Disabling content remains possible even if the loader has been removed.
    if solved.nodes.iter().any(|node| {
        node.enabled
            && affected.contains(&node.id)
            && (trusted
                .inventory
                .nodes
                .iter()
                .find(|old| old.id == node.id)
                .is_none_or(|old| {
                    !old.enabled
                        || old.current.as_ref().map(|c| &c.id)
                            != node.current.as_ref().map(|c| &c.id)
                        || node.current.iter().flat_map(|c| &c.provides).any(|member| {
                            member.enabled
                                && !old
                                    .current
                                    .iter()
                                    .flat_map(|c| &c.provides)
                                    .any(|previous| {
                                        previous.member_key == member.member_key && previous.enabled
                                    })
                        })
                })
                || request
                    .roots
                    .iter()
                    .any(|root| root.node_id == node.id && root.enabled))
    }) && !crate::bepinex::installed(&trusted.game_root)
    {
        return Err("framework.bepInExRequiredBeforeMods".into());
    }
    use super::mutations::{InstallBlock, node_label, report_install_block};
    let mut ids = BTreeSet::new();
    for node in &solved.nodes {
        if !ids.insert(&node.id) && affected.contains(&node.id) {
            return Err(report_install_block(
                "modChanges.installTargetIdentityConflict",
                &node.id,
                &[("name", node_label(trusted, &node.id))],
            ));
        }
        if !affected.contains(&node.id) {
            continue;
        }
        if !node.mutable {
            return Err(report_install_block(
                "modChanges.installTargetOwnershipUnverified",
                &node.id,
                &[("name", node_label(trusted, &node.id))],
            ));
        }
        let Some(candidate) = &node.current else {
            continue;
        };
        let mut members = BTreeSet::new();
        for plugin in &candidate.provides {
            if let Some(member) = &plugin.member_key {
                let normalized = key(member);
                if !members.insert(normalized.clone()) {
                    return Err(InstallBlock::PathCollision.report(&node.id, member));
                }
                if solved.nodes.iter().any(|other| {
                    other.id != node.id
                        && other.current.as_ref().is_some_and(|candidate| {
                            candidate.provides.iter().any(|plugin| {
                                plugin
                                    .member_key
                                    .as_ref()
                                    .is_some_and(|path| key(path) == normalized)
                            })
                        })
                }) {
                    return Err(InstallBlock::PathOwned.report(&node.id, member));
                }
            }
            if node.enabled
                && plugin.enabled
                && !plugin.guid.trim().is_empty()
                && let Some(error) = super::inventory_maps::runtime_conflict::duplicate_runtime(
                    &plugin.guid,
                    targets(solved, &plugin.guid, true)
                        .into_iter()
                        .map(|(_, member)| member),
                )
            {
                return Err(error);
            }
        }
    }
    Ok(())
}
