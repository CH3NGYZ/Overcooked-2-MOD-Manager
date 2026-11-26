//! Resolve UI intents against trusted disk/source snapshots; never accept UI inventory.
use super::{
    candidates,
    inventory::{self, *},
    model::*,
    session::*,
    solve::DependencyPolicy,
};
use crate::{app_state::AppState, archive, releases};
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    collections::{BTreeSet, HashMap},
    fs,
    path::Path,
};

mod duplicate_release;

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
    let (id, label, name_i18n, source, candidate, map_id, kind) =
        if let Some(map_id) = subject.map_id {
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
                map.name_i18n.clone(),
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
                repo.name_i18n.clone(),
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
                name_i18n,
                map_name_zh: None,
                kind,
                presentation_kind: None,
                enabled: false,
                repository_id: source,
                package_id: None,
                dll_files: vec![],
                members: vec![],
                bundle_parent_node_id: None,
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
    let op = uuid::Uuid::new_v4().to_string();
    let trace = OperationTrace::new(&op, "plan");
    let map = wire_request.map_import.as_ref();
    log::info!(
        "整链入口 op={op} roots={} preview_roots={} exact_roots={} map_import={} map_selected={} map_download_requested={} map_source_declared={} bulk_toggle={} scoped_toggle={} repair={}",
        wire_request.roots.len(),
        wire_request
            .roots
            .iter()
            .filter(|root| root.intent.node_id.starts_with("preview:"))
            .count(),
        wire_request
            .roots
            .iter()
            .filter(|root| root.intent.candidate_id.is_some())
            .count(),
        map.is_some(),
        map.map_or(0, |map| map.selected.len()),
        map.is_some_and(|map| map.download_id.is_some()),
        map.is_some_and(|map| map.origin.is_some()),
        wire_request.bulk_toggle,
        wire_request.toggle_scope.is_some(),
        wire_request.repair_dependencies_of.is_some(),
    );
    let mut step = "validate_request";
    let outcome = create_inner(state, wire_request, &trace, &mut step);
    match &outcome {
        Ok(plan) => log_plan(&op, "plan_returned", plan, trace.elapsed_ms(), false),
        Err(error) => trace.failure(step, error),
    }
    outcome
}

fn create_inner(
    state: &AppState,
    wire_request: PlanRequest,
    trace: &OperationTrace<'_>,
    step: &mut &'static str,
) -> Result<ChangePlan, String> {
    let repair = wire_request.repair_dependencies_of.as_deref();
    if wire_request.roots.len() > 1024
        || (repair.is_none() && wire_request.roots.is_empty() && wire_request.map_import.is_none())
        || (repair.is_some()
            && (!wire_request.roots.is_empty()
                || wire_request.map_import.is_some()
                || wire_request.toggle_scope.is_some()
                || wire_request.bulk_toggle))
    {
        return Err("modChanges.installationBlocked".into());
    }
    *step = "lease_and_recovery";
    let mut lease = SessionLease::creating(state)?;
    let recovery_revisions = {
        let _operation = state.operation.lock().map_err(|_| "lock.poisoned")?;
        super::guard::ensure_plannable(state)?;
        super::recovery_scope::revisions(&super::guard::records_for_plan(state, None)?)
    };
    *step = "installed_inventory";
    let trusted = (*inventory::cached(state)?).clone();
    *step = "validate_target_scope";
    ensure_toggle_scope(state, &trusted, &wire_request)?;
    if let Some(id) = repair {
        let installed = trusted.files.get(id).is_some_and(|files| {
            !files.summary.dll_files.is_empty() || files.summary.map_dir_name.is_some()
        });
        if !installed
            || !trusted
                .inventory
                .nodes
                .iter()
                .any(|node| node.id == id && node.can_repair_dependencies())
        {
            return Err("modChanges.targetChanged".into());
        }
    }
    let data = state
        .config
        .config_path()
        .parent()
        .ok_or("archive.pathEscape")?
        .to_path_buf();
    #[cfg(windows)]
    {
        *step = "verify_write_volumes";
        super::transaction::check_write_roots(
            &trusted.game_root,
            &data,
            &state.config.mod_change_transactions_dir(),
        )?;
    }
    *step = "create_staging";
    let staging = data.join("mod-change-staging").join(trace.op);
    super::transaction::Fingerprint::capture(&staging)?;
    fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let request = ChangeRequest {
        roots: wire_request
            .roots
            .iter()
            .map(|root| root.intent.clone())
            .collect(),
        repair_dependencies_of: wire_request.repair_dependencies_of.clone(),
    };
    let empty = SolveResult {
        status: SolveStatus::Incomplete,
        actions: vec![],
        evaluations: vec![],
        blockers: vec![],
        notices: vec![],
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
        dependency_policy: DependencyPolicy::default(),
        plan,
        recovery_revisions,
        solved: None,
        payloads: HashMap::new(),
        verified_payloads: HashMap::new(),
        staging,
        payload_fingerprints: HashMap::new(),
        map_payload: None,
        mutations: None,
        release_target_claims: Default::default(),
        created: std::time::Instant::now(),
    };
    let result = (|| -> Result<ChangePlan, String> {
        *step = "attach_preview";
        for root in session.wire_request.roots.clone() {
            if let Some(preview) = root.intent.node_id.strip_prefix("preview:") {
                attach_preview(state, &mut session, preview)?;
            }
        }
        // Manual previews already changed session.trusted. Merge exact remote roots there too.
        let exact_upgrade = match session.wire_request.roots.as_slice() {
            [root]
                if root.intent.enabled
                    && root.intent.member_key.is_none()
                    && root
                        .intent
                        .candidate_id
                        .as_deref()
                        .is_some_and(|id| id.starts_with("release:"))
                    && session.wire_request.map_import.is_none()
                    && session.wire_request.toggle_scope.is_none()
                    && !session.wire_request.bulk_toggle =>
            {
                Some(root.intent.node_id.as_str())
            }
            _ => None,
        };
        *step = "attach_exact_root";
        for root in &session.wire_request.roots {
            if !root.intent.node_id.starts_with("preview:") {
                attach_exact_root(
                    &mut session.trusted,
                    root,
                    exact_upgrade.map(|_| data.as_path()),
                )?;
            }
        }
        if let Some(owner_id) = exact_upgrade.or(session.request.repair_dependencies_of.as_deref())
        {
            duplicate_release::attach_dependencies(&mut session.trusted, owner_id, &data);
        }
        if session.wire_request.map_import.is_some() {
            *step = "attach_map_import";
            super::map_payload::attach(state, &mut session)?;
        }
        session.dependency_policy = if session.wire_request.roots.is_empty()
            && session
                .map_payload
                .as_ref()
                .is_some_and(|batch| batch.keeps_all_maps_disabled())
        {
            DependencyPolicy::MapFilesOnly
        } else {
            DependencyPolicy::for_plan(
                &session.trusted.inventory,
                &session.request,
                session.map_payload.is_some(),
            )
        };
        *step = "initial_solve";
        resolve_stage(state, &mut session, "initial")?;
        *step = "game_run_guard";
        super::live_game::check_plan(state, &session)?;
        *step = "freeze_payloads";
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

fn attach_exact_root(
    trusted: &mut TrustedInventory,
    root: &RootRequest,
    duplicate_upgrade_data_root: Option<&Path>,
) -> Result<(), String> {
    if let Some(data_root) = duplicate_upgrade_data_root
        && duplicate_release::attach_root(trusted, root, data_root)?
    {
        return Ok(());
    }
    let Some(index) = trusted
        .inventory
        .nodes
        .iter()
        .position(|node| node.id == root.intent.node_id)
    else {
        return Err(missing_root_error(trusted, root));
    };
    let node = &mut trusted.inventory.nodes[index];
    if node.is_registered_map_plugin()
        || (node.kind == NodeKind::Map
            && (root.intent.candidate_id.is_some() || root.intent.member_key.is_some()))
    {
        return Err("modChanges.installationBlocked".into());
    }
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

fn missing_root_error(trusted: &TrustedInventory, root: &RootRequest) -> String {
    // Multiple installed groups intentionally have physical IDs, with no repository
    // alias. Diagnose only an otherwise valid repository request; never pick a copy.
    if let (Some(repo_id), Some(id)) = (
        root.intent.node_id.strip_prefix("mod:"),
        root.intent.candidate_id.as_deref(),
    ) && duplicate_release::has_copies(trusted, repo_id)
        && trusted.config.repositories.iter().any(|repo| {
            repo.id == repo_id
                && candidates::for_root_asset(
                    repo,
                    None,
                    &trusted.config,
                    root.asset_name.as_deref(),
                )
                .iter()
                .any(|candidate| candidate.id == id && candidate.installable)
        })
    {
        return super::evidence::duplicate_replacement(trusted, repo_id, MULTIPLE_INSTALLED_COPIES);
    }
    "modChanges.targetChanged".into()
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
    let mut conflicting_paths = std::collections::BTreeSet::new();
    let matches: Vec<_> = session
        .trusted
        .inventory
        .nodes
        .iter()
        .filter(|node| {
            node.kind == NodeKind::Mod
                && node.current.as_ref().is_some_and(|current| {
                    let mut matching_members = current.provides.iter().filter(|old| {
                        payload.candidate.provides.iter().any(|new| {
                            (!old.guid.is_empty() && new.guid.eq_ignore_ascii_case(&old.guid))
                                || (old.member_key.is_some() && old.member_key == new.member_key)
                        })
                    });
                    let Some(first) = matching_members.next() else {
                        return false;
                    };
                    conflicting_paths.extend(
                        std::iter::once(first)
                            .chain(matching_members)
                            .map(|member| member.member_key.as_deref().unwrap_or(&node.id)),
                    );
                    true
                })
        })
        .map(|node| node.id.clone())
        .collect();
    if matches.len() > 1 {
        let paths = conflicting_paths.into_iter().collect::<Vec<_>>().join("\n");
        return Err(super::mutations::report_install_block(
            "modChanges.localModMultipleTargets",
            &id,
            &[("paths", &paths)],
        ));
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
        let name_i18n = payload
            .manifest
            .as_ref()
            .map(|manifest| manifest.package_name_i18n.clone())
            .unwrap_or_default();
        let label = archive::primary_translation(&name_i18n);
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
                    label,
                    name_i18n,
                    map_name_zh: None,
                    kind: NodeKind::Mod,
                    presentation_kind: None,
                    enabled: false,
                    repository_id: None,
                    package_id: None,
                    dll_files: vec![],
                    members: vec![],
                    bundle_parent_node_id: None,
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

#[cfg(test)]
pub(crate) fn resolve(state: &AppState, session: &mut PlanSession) -> Result<(), String> {
    resolve_stage(state, session, "fixture")
}

pub(super) fn resolve_stage(
    state: &AppState,
    session: &mut PlanSession,
    stage: &'static str,
) -> Result<(), String> {
    let op = session.operation_id().to_owned();
    let trace = OperationTrace::new(&op, stage);
    let mut step = "policy";
    let outcome = resolve_inner(state, session, &trace, &mut step);
    match &outcome {
        Ok(()) => log_plan(
            &op,
            stage,
            &session.plan,
            trace.elapsed_ms(),
            stage == "initial" || session.plan.result.status != SolveStatus::Ready,
        ),
        Err(error) => trace.failure(step, error),
    }
    outcome
}

fn resolve_inner(
    state: &AppState,
    session: &mut PlanSession,
    trace: &OperationTrace<'_>,
    step: &mut &'static str,
) -> Result<(), String> {
    let original = &session.trusted.inventory;
    let op = trace.op;
    let stage = trace.stage;
    log::info!(
        "整链开始求解 op={op} stage={stage} policy={:?} preference={} verified={} roots={} repair={}",
        session.dependency_policy,
        session.dependency_policy.diagnostic_preference(),
        session.verified_payloads.len(),
        session.request.roots.len(),
        session.request.repair_dependencies_of.is_some(),
    );
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
    *step = "load_claim_index";
    let mut releases = super::mutations::LooseReleaseSearch::new(state, session)?;
    *step = "solve_graph";
    let (mut result, mut solved) = match super::inventory_maps::runtime_conflict::solve_toggle(
        state,
        session,
        &input,
        &mut releases,
        trace,
    )? {
        Some(plan) => plan,
        None => super::solve::solve_with_policy(
            &input,
            &session.request,
            Some(&session.trusted),
            Some(&mut releases),
            session.dependency_policy,
            session
                .map_payload
                .as_ref()
                .map_or_else(BTreeSet::new, |batch| batch.detached_active_members()),
            Some(trace),
        )?,
    };
    *step = "resolve_claims";
    let claims = releases.finish(state, solved.as_ref())?;
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
    *step = "validate_solution";
    let retirements = if let Some(solved) = &solved {
        inventory::validate_solution(session, solved, &unbound)?;
        session
            .trusted
            .duplicate_releases
            .selected(original, solved)?
    } else {
        duplicate_release::explain_blockers(&session.trusted, &mut result);
        Default::default()
    };
    if let Some(batch) = &session.map_payload {
        result.notices.extend(batch.notices.iter().cloned());
    }
    result.notices.extend(
        session
            .request
            .roots
            .iter()
            .filter_map(|root| session.payloads.get(&root.node_id))
            .flat_map(|payload| payload.notices.iter().cloned()),
    );
    session.release_target_claims = claims;
    let provider_choices = super::evidence::choices(&session.trusted, &result, solved.as_ref());
    session.plan = presentation(
        &session.trusted,
        &session.request,
        result,
        session.plan.plan_id.clone(),
        false,
    );
    session.plan.retired_mod_copies = retirements;
    session.plan.provider_choices = provider_choices;
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
                    let payload = Payload::verified(
                        &session.verified_payloads,
                        &session.payloads,
                        &node.id,
                        &candidate.id,
                    );
                    let staged_roles = session
                        .map_payload
                        .as_ref()
                        .and_then(|batch| batch.member_roles.get(&candidate.id));
                    let staged_files = session
                        .map_payload
                        .as_ref()
                        .and_then(|batch| batch.member_files.get(&candidate.id));
                    let roles = payload
                        .map(|payload| &payload.member_roles)
                        .or(staged_roles);
                    if payload.is_some() && node.kind == NodeKind::Mod {
                        summary.presentation_kind = (candidate
                            .provides
                            .iter()
                            .filter(|member| member.member_key.is_some())
                            .count()
                            > 1)
                        .then_some(PresentationKind::ModPackage);
                    } else if let (Some(roles), [member]) =
                        (staged_roles, candidate.provides.as_slice())
                    {
                        if node.kind == NodeKind::Mod {
                            summary.presentation_kind = member
                                .member_key
                                .as_ref()
                                .and_then(|key| roles.get(key))
                                .copied()
                                .map(Into::into);
                        }
                    }
                    summary.members = candidate
                        .provides
                        .iter()
                        .map(|p| MemberSummary {
                            key: p.member_key.clone().unwrap_or_default(),
                            dll_file: p
                                .member_key
                                .as_ref()
                                .and_then(|key| {
                                    staged_files
                                        .and_then(|files| files.get(key))
                                        .cloned()
                                        .or_else(|| {
                                            payload
                                                .and_then(|payload| payload.member_file(key))
                                                .map(crate::mods::canonical_dll_name)
                                        })
                                        .or_else(|| {
                                            summary
                                                .members
                                                .iter()
                                                .find(|member| member.key == *key)
                                                .map(|member| member.dll_file.clone())
                                        })
                                })
                                .or_else(|| p.member_key.clone())
                                .unwrap_or_default(),
                            enabled: node.enabled && p.enabled,
                            guid: (!p.guid.is_empty()).then(|| p.guid.clone()),
                            role: p.member_key.as_ref().and_then(|key| {
                                if let Some(roles) = roles {
                                    roles.get(key).copied()
                                } else {
                                    summary
                                        .members
                                        .iter()
                                        .find(|member| member.key == *key)
                                        .and_then(|member| member.role)
                                }
                            }),
                        })
                        .collect();
                }
            }
        }
    }
    session.plan.bundled_dlls = super::bundled_display::collect(
        &session.trusted,
        &session.plan.nodes,
        &session.plan.result.actions,
        solved.as_ref(),
    );
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
