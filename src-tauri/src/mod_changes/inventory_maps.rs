//! Map ownership and physical DLL activation are distinct facts.
use super::{inventory::*, model::*};
use crate::{app_state::AppState, archive, level_sets, models::*};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub(super) mod runtime_conflict;

pub(crate) fn append(
    state: &AppState,
    config: &AppConfig,
    root: &Path,
    roots: &level_sets::LevelSetRoots,
    inventory: &mut Inventory,
    files: &mut BTreeMap<String, NodeFiles>,
) -> Result<BTreeSet<String>, String> {
    let manifests = state.config.manifests_dir();
    append_at(&manifests, config, root, roots, inventory, files)
}

pub(crate) fn append_at(
    manifests: &Path,
    config: &AppConfig,
    root: &Path,
    roots: &level_sets::LevelSetRoots,
    inventory: &mut Inventory,
    files: &mut BTreeMap<String, NodeFiles>,
) -> Result<BTreeSet<String>, String> {
    let snapshots = archive::read_map_relation_snapshots(manifests);
    let registered = level_sets::read_registered_plugins(manifests);
    let sets = level_sets::scan_level_sets_with_progress(
        roots,
        Some(manifests),
        crate::concurrency::workers_from(config.map_info_concurrency),
        &|_, _, _| {},
    );
    let mut names = BTreeMap::<String, usize>::new();
    for set in &sets {
        *names.entry(key(&set.dir_name)).or_default() += 1;
    }
    let mut toggle_unbound_nodes = BTreeSet::new();
    for set in &sets {
        let path = relative(root, &set.path)?;
        let unique = names.get(&key(&set.dir_name)) == Some(&1);
        let id = if unique {
            format!("map:{}", set.dir_name)
        } else {
            format!("map:{}:{path}", set.dir_name)
        };
        let mut info = level_sets::view::level_set_to_info(set.clone());
        level_sets::view::bind_map_relation_snapshot(&mut info, config, &snapshots);
        let historical = if info.map_id.is_none() {
            snapshots.entries.values().find(|entry| {
                level_sets::level_set_identity_matches(
                    &info.name,
                    info.name_zh.as_deref(),
                    &info.author,
                    &entry.level_set_name,
                    Some(&entry.level_set_name_zh),
                    &entry.author,
                )
            })
        } else {
            None
        };
        // The view returns early when a source disappears. Its installed
        // snapshot still owns the installed map's declarations.
        if let Some(snapshot) = historical {
            let relations = crate::mods::select_release_relations(
                &snapshot.release_relations,
                Some(&info.version),
            )
            .map(|relations| relations.to_vec())
            .unwrap_or_else(|| snapshot.relations.clone());
            if !relations.is_empty() {
                info.relations = relations;
            }
        }
        require_framework(&mut info.relations);
        let (internal, member_roles) =
            internal_members(root, &set.path, config.mod_metadata_concurrency)?;
        let mut relationships = owned(info.relations.clone(), None);
        let mut known = set.error.is_none();
        for member in &internal {
            let (relations, member_known) = member_relations(config, member);
            relationships.extend(relations);
            // The map declaration governs its bundled asset. A contradictory
            // source declaration cannot silently be selected by list order.
            known &= member.guid.is_empty() || member_known != Some(false);
        }
        let mut member_keys = BTreeSet::new();
        let members_unique = internal
            .iter()
            .filter_map(|member| member.member_key.as_ref())
            .all(|member| member_keys.insert(member));
        // Bundled-member declarations do not bind the local map itself, and
        // an unreadable info file cannot establish that a map is unbound.
        if unique
            && members_unique
            && set.error.is_none()
            && !info.name.trim().is_empty()
            && info.map_id.is_none()
            && historical.is_none()
        {
            toggle_unbound_nodes.insert(id.clone());
        }
        inventory.nodes.push(ChangeNode {
            id: id.clone(),
            kind: NodeKind::Map,
            enabled: set.enabled,
            mutable: unique && members_unique,
            candidates: vec![],
            candidates_complete: true,
            current: Some(Candidate {
                id: format!("installed:{id}"),
                source_id: info.map_id.clone(),
                tag: None,
                version: Some(info.version.clone()),
                provides: internal.clone(),
                relations: relationships,
                relations_known: known,
                installable: true,
                raw: false,
                verification_pending: false,
            }),
        });
        files.insert(
            id.clone(),
            NodeFiles {
                summary: NodeSummary {
                    node_id: id,
                    label: info.name,
                    name_i18n: BTreeMap::new(),
                    map_name_zh: info.name_zh,
                    kind: NodeKind::Map,
                    presentation_kind: None,
                    enabled: set.enabled,
                    repository_id: None,
                    package_id: None,
                    dll_files: internal
                        .iter()
                        .filter_map(|plugin| plugin.member_key.clone())
                        .collect(),
                    members: internal
                        .into_iter()
                        .filter_map(|plugin| {
                            Some(MemberSummary {
                                role: member_roles.get(plugin.member_key.as_ref()?).copied(),
                                key: plugin.member_key.clone()?,
                                dll_file: plugin.member_key?,
                                enabled: plugin.enabled,
                                guid: (!plugin.guid.is_empty()).then_some(plugin.guid),
                            })
                        })
                        .collect(),
                    map_dir_name: Some(set.dir_name.clone()),
                    map_id: info.map_id,
                    subject_tag: None,
                    candidates: vec![],
                },
                manifests: vec![],
                map_path: Some(path),
                map_metadata: set.metadata.clone(),
            },
        );
    }
    // Registered DLLs may stay in plugins after their map is disabled or removed.
    // Enumerate the registry itself so every physical loader input survives.
    for entry in &registered {
        let owners = sets
            .iter()
            .filter(|set| {
                set.metadata
                    .as_ref()
                    .is_some_and(|metadata| entry.owner.matches(metadata))
            })
            .collect::<Vec<_>>();
        let map_id = (owners.len() == 1)
            .then(|| {
                let mut info = level_sets::view::level_set_to_info(owners[0].clone());
                level_sets::view::bind_map_relation_snapshot(&mut info, config, &snapshots);
                info.map_id
            })
            .flatten();
        append_external(
            config,
            root,
            &entry.dll,
            owners.len() == 1,
            map_id,
            inventory,
            files,
        )?;
    }
    Ok(toggle_unbound_nodes)
}

pub(crate) fn require_framework(relations: &mut Vec<RepositoryRelation>) {
    if !relations.iter().any(|relation| {
        relation.kind == RelationKind::Requires
            && !relation.optional
            && relation
                .guid
                .eq_ignore_ascii_case(crate::mods::plugin_target::DIYLEVEL_GUID)
    }) {
        relations.push(RepositoryRelation {
            kind: RelationKind::Requires,
            guid: crate::mods::plugin_target::DIYLEVEL_GUID.into(),
            version: String::new(),
            prerelease: None,
            optional: false,
        });
    }
}

// None is no independent source declaration; false is contradictory metadata.
pub(crate) fn member_relations(
    config: &AppConfig,
    member: &ProvidedPlugin,
) -> (Vec<OwnedRelation>, Option<bool>) {
    let declarations = config
        .repositories
        .iter()
        .filter(|repo| {
            !member.guid.is_empty() && repo.plugin_guid.eq_ignore_ascii_case(&member.guid)
        })
        .map(|repo| crate::mods::effective_relations(repo, member.version.as_deref()))
        .collect::<Vec<_>>();
    let Some(first) = declarations.first() else {
        return (vec![], None);
    };
    if declarations.iter().any(|relations| relations != first) {
        return (vec![], Some(false));
    }
    (
        first
            .iter()
            .cloned()
            .map(|relation| OwnedRelation {
                owner_member_key: member.member_key.clone(),
                owner_guid: Some(member.guid.clone()),
                relation,
            })
            .collect(),
        Some(true),
    )
}

fn append_external(
    config: &AppConfig,
    root: &Path,
    dll: &str,
    ownership_known: bool,
    map_id: Option<String>,
    inventory: &mut Inventory,
    files: &mut BTreeMap<String, NodeFiles>,
) -> Result<(), String> {
    let active = crate::game::safe_game_path(root, dll)?;
    let disabled = crate::game::safe_game_path(root, &crate::mods::disabled_dll_file(dll))?;
    if !active.is_file() && !disabled.is_file() {
        return Ok(());
    }
    let physical = if active.is_file() { &active } else { &disabled };
    let details = crate::mods::read_dll_details(physical, dll);
    let id = format!("map-dll:{}", key(dll));
    let enabled = active.is_file();
    let member = ProvidedPlugin {
        member_key: Some(key(dll)),
        guid: details.guid.clone().unwrap_or_default(),
        version: details.version.clone(),
        prerelease: None,
        enabled,
    };
    let (mut relations, known) = member_relations(config, &member);
    let mut framework = Vec::new();
    require_framework(&mut framework);
    relations.extend(framework.into_iter().map(|relation| OwnedRelation {
        owner_member_key: member.member_key.clone(),
        owner_guid: None,
        relation,
    }));
    inventory.nodes.push(ChangeNode {
        id: id.clone(),
        kind: NodeKind::Mod,
        enabled,
        mutable: ownership_known && !(active.is_file() && disabled.is_file()),
        candidates: vec![],
        candidates_complete: true,
        current: Some(Candidate {
            id: format!("installed:{id}"),
            source_id: None,
            tag: None,
            version: details.version,
            provides: vec![member],
            relations,
            relations_known: known != Some(false) && ownership_known,
            installable: true,
            raw: false,
            verification_pending: false,
        }),
    });
    files.insert(
        id.clone(),
        NodeFiles {
            summary: NodeSummary {
                node_id: id,
                label: details.name.unwrap_or_else(|| dll.into()),
                name_i18n: BTreeMap::new(),
                map_name_zh: None,
                kind: NodeKind::Mod,
                presentation_kind: Some(details.role.into()),
                enabled,
                repository_id: None,
                package_id: None,
                dll_files: vec![dll.into()],
                members: vec![MemberSummary {
                    key: key(dll),
                    dll_file: dll.into(),
                    enabled,
                    guid: details.guid,
                    role: Some(details.role),
                }],
                map_dir_name: None,
                map_id,
                subject_tag: None,
                candidates: vec![],
            },
            manifests: vec![],
            map_path: None,
            map_metadata: None,
        },
    );
    Ok(())
}

type InternalMembers = (Vec<ProvidedPlugin>, BTreeMap<String, DllRole>);

fn internal_members(
    root: &Path,
    directory: &Path,
    concurrency: u32,
) -> Result<InternalMembers, String> {
    let entries = walkdir::WalkDir::new(directory)
        .follow_links(false)
        .into_iter()
        .map(|entry| entry.map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    if entries.iter().any(|entry| entry.file_type().is_symlink()) {
        return Err("archive.pathEscape".into());
    }
    let paths = entries
        .into_iter()
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            name.ends_with(".dll") || name.ends_with(".dll.disabled")
        })
        .collect::<Vec<_>>();
    let requests = paths
        .iter()
        .map(|entry| crate::metadata_read::MetadataRequest {
            path: entry.path().to_path_buf(),
            display_name: entry.file_name().to_string_lossy().into_owned(),
            kind: crate::metadata_read::MetadataKind::ModDll,
        })
        .collect::<Vec<_>>();
    let outcomes = crate::metadata_read::read_metadata_batch(
        &requests,
        crate::concurrency::workers_from(concurrency),
        &|_, _| {},
    );
    let mut members = Vec::with_capacity(paths.len());
    let mut roles = BTreeMap::new();
    for (entry, outcome) in paths.iter().zip(outcomes) {
        let path = relative(root, entry.path())?;
        let key = key(&crate::mods::canonical_dll_name(&path));
        let details = outcome.mod_dll();
        if let Some(details) = details {
            roles.insert(key.clone(), details.role);
        }
        members.push(ProvidedPlugin {
            member_key: Some(key),
            guid: details
                .and_then(|metadata| metadata.guid.clone())
                .unwrap_or_default(),
            version: outcome
                .value
                .as_ref()
                .and_then(crate::metadata_read::MetadataValue::version)
                .map(str::to_string),
            prerelease: None,
            enabled: !path.to_ascii_lowercase().ends_with(".disabled"),
        });
    }
    Ok((members, roles))
}
