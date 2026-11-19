//! Resolve UI intents against trusted disk/source snapshots; never accept UI inventory.
use super::{
    candidates,
    inventory::{self, *},
    model::*,
    session::*,
};
use crate::{app_state::AppState, game, releases};
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, collections::HashMap, fs};

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct EvaluationSubject {
    pub repository_id: Option<String>,
    pub tag: Option<String>,
    pub map_id: Option<String>,
    pub map_dir_name: Option<String>,
}
#[derive(Serialize)]
pub(crate) struct EvaluationSnapshot {
    pub revision: String,
    pub nodes: Vec<NodeSummary>,
    pub evaluations: Vec<NodeEvaluation>,
}

pub(crate) fn evaluate(
    state: &AppState,
    subjects: Vec<EvaluationSubject>,
) -> Result<EvaluationSnapshot, String> {
    let mut trusted = (*inventory::cached(state)?).clone();
    for subject in subjects.into_iter().take(512) {
        add_subject(&mut trusted, subject)?;
    }
    Ok(EvaluationSnapshot {
        revision: trusted.revision,
        nodes: trusted.files.values().map(|f| f.summary.clone()).collect(),
        evaluations: super::evaluate(&trusted.inventory),
    })
}

fn add_subject(trusted: &mut TrustedInventory, subject: EvaluationSubject) -> Result<(), String> {
    let (id, label, source, candidate, map_id, kind) = if let Some(map_id) = subject.map_id {
        let Some(map) = trusted
            .config
            .map_repositories
            .iter()
            .find(|map| map.id == map_id)
        else {
            return Ok(());
        };
        let tag = subject.tag.clone();
        let mut relations = crate::mods::effective_map_relations(map, tag.as_deref());
        super::inventory_maps::require_framework(&mut relations);
        (
            format!(
                "subject:map:{map_id}:{}",
                tag.as_deref().unwrap_or_default()
            ),
            map.name.clone(),
            None,
            Candidate {
                id: "subject".into(),
                source_id: Some(map_id.clone()),
                tag: tag.clone(),
                version: tag,
                provides: vec![],
                relations: owned(relations, None),
                relations_known: true,
                installable: false,
                raw: false,
                verification_pending: false,
            },
            Some(map_id),
            NodeKind::Map,
        )
    } else if let Some(repo_id) = subject.repository_id {
        let Some(repo) = trusted
            .config
            .repositories
            .iter()
            .find(|repo| repo.id == repo_id)
        else {
            return Ok(());
        };
        let candidate = candidates::for_repository(repo, None, true, &trusted.config)
            .into_iter()
            .find(|c| c.tag == subject.tag || subject.tag.is_none());
        let Some(mut candidate) = candidate else {
            return Ok(());
        };
        candidate.installable = false;
        (
            format!(
                "subject:{repo_id}:{}",
                candidate.tag.as_deref().unwrap_or_default()
            ),
            repo.name.clone(),
            Some(repo_id),
            candidate,
            None,
            NodeKind::Mod,
        )
    } else {
        return Ok(());
    };
    if trusted.files.contains_key(&id) {
        return Ok(());
    }
    let subject_tag = candidate.tag.clone();
    let choices = vec![CandidateSummary {
        id: candidate.id.clone(),
        tag: candidate.tag.clone(),
    }];
    trusted.inventory.nodes.push(ChangeNode {
        id: id.clone(),
        kind: kind.clone(),
        current: Some(candidate),
        enabled: false,
        mutable: false,
        candidates: vec![],
        candidates_complete: true,
    });
    trusted.files.insert(
        id.clone(),
        NodeFiles {
            summary: NodeSummary {
                node_id: id,
                label,
                kind,
                enabled: false,
                repository_id: source,
                package_id: None,
                dll_files: vec![],
                members: vec![],
                map_dir_name: subject.map_dir_name,
                map_id,
                subject_tag,
                candidates: choices,
            },
            manifests: vec![],
            map_path: None,
            map_metadata: None,
        },
    );
    Ok(())
}

pub(crate) fn create(state: &AppState, wire_request: PlanRequest) -> Result<ChangePlan, String> {
    if wire_request.roots.len() > 1024
        || (wire_request.roots.is_empty() && wire_request.map_import.is_none())
    {
        return Err("modChanges.installationBlocked".into());
    }
    let mut lease = SessionLease::creating(state)?;
    {
        let _operation = state.operation.lock().map_err(|_| "lock.poisoned")?;
        super::guard::ensure_recovered(state)?;
    }
    let trusted = (*inventory::cached(state)?).clone();
    ensure_toggle_scope(state, &trusted, &wire_request)?;
    game::ensure_game_stopped(&trusted.game_root)?;
    let data = state
        .config
        .config_path()
        .parent()
        .ok_or("archive.pathEscape")?
        .to_path_buf();
    let staging = data
        .join("mod-change-staging")
        .join(uuid::Uuid::new_v4().to_string());
    super::transaction::Fingerprint::capture(&staging)?;
    fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let request = ChangeRequest {
        roots: wire_request
            .roots
            .iter()
            .map(|root| root.intent.clone())
            .collect(),
    };
    let empty = SolveResult {
        status: SolveStatus::Incomplete,
        actions: vec![],
        evaluations: vec![],
        blockers: vec![],
        impacts: vec![],
    };
    let plan = presentation(
        &trusted,
        &request,
        empty,
        uuid::Uuid::new_v4().to_string(),
        false,
    );
    let mut session = PlanSession {
        trusted: trusted.clone(),
        request,
        wire_request,
        plan,
        solved: None,
        payloads: HashMap::new(),
        verified_payloads: HashMap::new(),
        staging,
        payload_fingerprints: HashMap::new(),
        map_payload: None,
        mutations: None,
        created: std::time::Instant::now(),
    };
    let result = (|| {
        for root in session.wire_request.roots.clone() {
            if let Some(preview) = root.intent.node_id.strip_prefix("preview:") {
                attach_preview(state, &mut session, preview)?;
            }
        }
        // Manual previews already changed session.trusted. Merge exact remote roots there too.
        for root in &session.wire_request.roots {
            if !root.intent.node_id.starts_with("preview:") {
                attach_exact_root(&mut session.trusted, root)?;
            }
        }
        if session.wire_request.map_import.is_some() {
            super::map_payload::attach(state, &mut session)?;
        }
        resolve(state, &mut session)?;
        remember_payloads(&mut session)?;
        Ok(session.plan.clone())
    })();
    match result {
        Ok(plan) => {
            lease.session = Some(session);
            Ok(plan)
        }
        Err(error) => {
            let _ = super::execution::cleanup(state, &session.staging);
            Err(error)
        }
    }
}

fn ensure_toggle_scope(
    state: &AppState,
    trusted: &TrustedInventory,
    request: &PlanRequest,
) -> Result<(), String> {
    let Some(scope) = &request.toggle_scope else {
        return Ok(());
    };
    let [root] = request.roots.as_slice() else {
        return Err("modChanges.targetChanged".into());
    };
    let summary = &trusted
        .files
        .get(&root.intent.node_id)
        .ok_or("modChanges.targetChanged")?
        .summary;
    if scope.revision != trusted.revision {
        return Err("modChanges.targetChanged".into());
    }
    // Node IDs for installed items are game-relative. The scan that supplied the
    // clicked card must still belong to the root recognized by fresh inventory.
    let snapshot = state
        .local_mod_snapshot
        .lock()
        .map_err(|_| "lock.poisoned")?
        .get(scope.generation, &trusted.game_root)
        .map_err(|_| "modChanges.targetChanged")?;
    if summary.kind == NodeKind::Map {
        let package = snapshot
            .diy_level_target
            .as_deref()
            .and_then(|target| snapshot.mods.iter().find(|item| item.dll_file == target))
            .map(|item| {
                crate::mods::plugin_target::directory(
                    &trusted.game_root,
                    &item.dll_file,
                    item.enabled,
                )
                .map_err(|_| "modChanges.targetChanged")
            })
            .transpose()?;
        if package.as_deref() != trusted.level_roots.package_root() {
            return Err("modChanges.targetChanged".into());
        }
    }
    Ok(())
}

fn attach_exact_root(trusted: &mut TrustedInventory, root: &RootRequest) -> Result<(), String> {
    let node = trusted
        .inventory
        .nodes
        .iter_mut()
        .find(|node| node.id == root.intent.node_id)
        .ok_or("modChanges.targetChanged")?;
    let Some(candidate_id) = &root.intent.candidate_id else {
        return Ok(());
    };
    let Some(repo_id) = trusted
        .files
        .get(&node.id)
        .and_then(|f| f.summary.repository_id.as_deref())
    else {
        if node.current.as_ref().is_some_and(|c| &c.id == candidate_id) {
            return Ok(());
        }
        return Err("modChanges.targetChanged".into());
    };
    let repo = trusted
        .config
        .repositories
        .iter()
        .find(|r| r.id == repo_id)
        .ok_or("source.repositoryMissing")?;
    if node
        .current
        .as_ref()
        .is_some_and(|candidate| &candidate.id == candidate_id)
    {
        return Ok(());
    }
    let candidate = candidates::for_root_asset(
        repo,
        node.current.as_ref(),
        &trusted.config,
        root.asset_name.as_deref(),
    )
    .into_iter()
    .find(|c| &c.id == candidate_id)
    .ok_or("modChanges.targetChanged")?;
    if let Some(name) = &root.asset_name {
        let tag = candidate.tag.as_deref().ok_or("modChanges.targetChanged")?;
        let release = repo
            .cached_releases
            .iter()
            .find(|r| r.tag_name == tag)
            .ok_or("modChanges.targetChanged")?;
        releases::select_install_asset(
            &releases::RepositoryRequest::from(repo),
            &repo.asset_rules,
            release,
            Some(name),
        )
        .ok_or("modChanges.targetChanged")?;
    }
    node.candidates.retain(|c| &c.id != candidate_id);
    node.candidates.push(candidate);
    Ok(())
}

fn attach_preview(
    state: &AppState,
    session: &mut PlanSession,
    preview: &str,
) -> Result<(), String> {
    let id = format!("preview:{preview}");
    let candidate = Candidate {
        id: id.clone(),
        source_id: None,
        tag: None,
        version: None,
        provides: vec![],
        relations: vec![],
        relations_known: false,
        installable: true,
        raw: false,
        verification_pending: true,
    };
    let payload = super::payload::from_preview(
        state,
        &session.trusted,
        preview,
        &id,
        &candidate,
        None,
        None,
        &session.staging.join(format!("manual-{preview}")),
    )?;
    // A manual file replaces a unique matching installed unit, preserving package atomicity.
    let matches: Vec<_> = session
        .trusted
        .inventory
        .nodes
        .iter()
        .filter(|node| {
            node.kind == NodeKind::Mod
                && node.current.as_ref().is_some_and(|current| {
                    current.provides.iter().any(|old| {
                        payload.candidate.provides.iter().any(|new| {
                            (!old.guid.is_empty() && new.guid.eq_ignore_ascii_case(&old.guid))
                                || (old.member_key.is_some() && old.member_key == new.member_key)
                        })
                    })
                })
        })
        .map(|node| node.id.clone())
        .collect();
    if matches.len() > 1 {
        return Err("modChanges.ambiguousOwnership".into());
    }
    if let Some(existing) = matches.first() {
        let node = session
            .trusted
            .inventory
            .nodes
            .iter_mut()
            .find(|node| &node.id == existing)
            .ok_or("modChanges.targetChanged")?;
        let old_id = node.id.clone();
        node.id = id.clone();
        node.candidates.push(payload.candidate.clone());
        if let Some(mut files) = session.trusted.files.remove(&old_id) {
            files.summary.node_id = id.clone();
            session.trusted.files.insert(id.clone(), files);
        }
    } else {
        session.trusted.inventory.nodes.push(ChangeNode {
            id: id.clone(),
            kind: NodeKind::Mod,
            current: None,
            enabled: false,
            mutable: true,
            candidates: vec![payload.candidate.clone()],
            candidates_complete: true,
        });
        session.trusted.files.insert(
            id.clone(),
            NodeFiles {
                summary: NodeSummary {
                    node_id: id.clone(),
                    label: preview.into(),
                    kind: NodeKind::Mod,
                    enabled: false,
                    repository_id: None,
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
    for root in &mut session.request.roots {
        if root.node_id == id {
            root.candidate_id = Some(payload.candidate.id.clone());
        }
    }
    // Re-extract after identifying the old atomic package, so exact members keep
    // their original enable state instead of inheriting the archive defaults.
    let payload = if !matches.is_empty() {
        super::payload::from_preview(
            state,
            &session.trusted,
            preview,
            &id,
            &candidate,
            None,
            None,
            &session.staging.join(format!("manual-verified-{preview}")),
        )?
    } else {
        payload
    };
    if let Some(node) = session
        .trusted
        .inventory
        .nodes
        .iter_mut()
        .find(|node| node.id == id)
    {
        node.candidates
            .retain(|candidate| candidate.id != payload.candidate.id);
        node.candidates.push(payload.candidate.clone());
    }
    session.payloads.insert(id, payload);
    Ok(())
}

pub(crate) fn resolve(state: &AppState, session: &mut PlanSession) -> Result<(), String> {
    let original = &session.trusted.inventory;
    let unbound = session.unbound_toggle_roots();
    let mut input = Cow::Borrowed(original);
    // Project only the toggled nodes' outgoing declarations. Their actual
    // GUIDs still participate in reverse dependencies and inferred conflicts.
    if !unbound.is_empty() {
        for node in &mut input.to_mut().nodes {
            if unbound.contains(node.id.as_str())
                && let Some(current) = &mut node.current
            {
                current.relations.clear();
                current.relations_known = true;
            }
        }
    }
    let (mut result, mut solved) =
        super::inventory_maps::runtime_conflict::solve_toggle(state, session, &input)?
            .unwrap_or_else(|| super::solve_with_inventory(&input, &session.request));
    if !unbound.is_empty() {
        if let Some(solved) = &mut solved {
            for (node, installed) in solved.nodes.iter_mut().zip(&original.nodes) {
                if unbound.contains(node.id.as_str())
                    && let (Some(current), Some(previous)) = (&mut node.current, &installed.current)
                    && current.id == previous.id
                {
                    current.relations.clone_from(&previous.relations);
                    current.relations_known = previous.relations_known;
                }
            }
        }
        result.evaluations = super::evaluate(solved.as_ref().unwrap_or(original));
    }
    if let Some(solved) = &solved {
        inventory::validate_solution(&session.trusted, &session.request, solved, &unbound)?;
    }
    session.plan = presentation(
        &session.trusted,
        &session.request,
        result,
        session.plan.plan_id.clone(),
        false,
    );
    if let Some(inventory) = &solved {
        session.plan.install_notes = super::install_notes::for_plan(
            &session.trusted.config,
            &session.trusted.managed_mods,
            &session.plan.result,
            inventory,
            &session.payloads,
        );
        for summary in &mut session.plan.nodes {
            if let Some(node) = inventory
                .nodes
                .iter()
                .find(|node| node.id == summary.node_id)
            {
                summary.enabled = node.enabled;
                if let Some(candidate) = &node.current {
                    summary.members = candidate
                        .provides
                        .iter()
                        .map(|p| MemberSummary {
                            key: p.member_key.clone().unwrap_or_default(),
                            dll_file: p.member_key.clone().unwrap_or_default(),
                            enabled: node.enabled && p.enabled,
                            guid: Some(p.guid.clone()),
                        })
                        .collect();
                }
            }
        }
    }
    session.solved = solved;
    Ok(())
}

pub(crate) fn remember_payloads(session: &mut PlanSession) -> Result<(), String> {
    let mut fingerprints = session
        .payloads
        .values()
        .chain(session.verified_payloads.values())
        .map(|payload| {
            (
                payload.stage_root.clone(),
                payload.stage_fingerprint.clone(),
            )
        })
        .collect::<HashMap<_, _>>();
    if let Some(payload) = &session.map_payload {
        for path in [&payload.stage_root, &payload.manifests_root] {
            let expected = match session.payload_fingerprints.get(path) {
                Some(expected) => expected.clone(),
                None => super::transaction::Fingerprint::capture(path)?,
            };
            fingerprints.insert(path.clone(), expected);
        }
        fingerprints.insert(
            payload.source_path.clone(),
            payload.source_fingerprint.clone(),
        );
    }
    session.payload_fingerprints = fingerprints;
    Ok(())
}
