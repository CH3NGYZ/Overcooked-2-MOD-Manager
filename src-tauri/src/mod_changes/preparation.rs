//! Download the complete chosen chain, verify its real DLLs, then solve again.
use super::progress::{
    ChangePreparationProgress, PreparationError, PreparationOutcome, PreparationReporter,
    PreparationStage,
};
use super::{
    freshness, guard, inventory, live_game, model::*, mutations, planning, recovery_scope,
    session::*, transaction::*,
};
use crate::{
    app_state::AppState,
    archive,
    mods::repository_download::{RepositoryDownloadRequest, download_repository_asset},
};
use std::collections::HashMap;

pub(crate) async fn prepare(
    state: AppState,
    id: String,
    report: impl Fn(ChangePreparationProgress) + Send + Sync + 'static,
) -> PreparationOutcome {
    let progress = PreparationReporter::new(id.clone(), report);
    let outcome = prepare_inner(state, id, &progress).await;
    progress.finish(outcome)
}

async fn prepare_inner(
    state: AppState,
    id: String,
    progress: &PreparationReporter,
) -> Result<ChangePlan, PreparationError> {
    let mut lease = SessionLease::take(&state, &id)?;
    let op = lease
        .session
        .as_ref()
        .ok_or("modChanges.planExpired")?
        .operation_id()
        .to_owned();
    let trace = OperationTrace::new(&op, "prepare");
    log::info!("整链开始准备 op={op} plan={id}");
    let mut step = "validate_plan";
    let outcome = prepare_session(&state, &mut lease, progress, &trace, &mut step).await;
    if let Err(error) = &outcome {
        trace.failure(step, &error.message);
    }
    outcome
}

async fn prepare_session(
    state: &AppState,
    lease: &mut SessionLease,
    progress: &PreparationReporter,
    trace: &OperationTrace<'_>,
    step: &mut &'static str,
) -> Result<ChangePlan, PreparationError> {
    let session = lease.session.as_mut().ok_or("modChanges.planExpired")?;
    if session.plan.prepared {
        log::info!(
            "整链复用已准备方案 op={} plan={}",
            trace.op,
            session.plan.plan_id
        );
        return Ok(session.plan.clone());
    }
    if session.plan.result.status != SolveStatus::Ready {
        return Err("modChanges.notPrepared".into());
    }
    progress.synchronize(session);
    let before = approval_signature(session)?;
    let initial_plan = session.plan.plan_id.clone();
    let initial_actions = session.plan.result.actions.len();
    // Freeze and recheck existing payloads before extending the plan.
    let fresh_state = state.clone();
    *step = "verify_initial_revision_and_payloads";
    let session = lease.session.take().ok_or("modChanges.planExpired")?;
    let (session, outcome) = tauri::async_runtime::spawn_blocking(move || {
        let outcome = verify_preparation_freshness(&fresh_state, &session, None);
        (session, outcome)
    })
    .await
    .map_err(|e| e.to_string())?;
    lease.session = Some(session);
    outcome?;
    let session = lease.session.as_mut().ok_or("modChanges.planExpired")?;
    archive::start_download();
    for (node, payload) in &session.payloads {
        session.verified_payloads.insert(
            (node.clone(), payload.candidate.id.clone()),
            payload.clone(),
        );
    }
    // Each iteration resolves the entire graph. A newly discovered package member
    // may change which asset another node needs; no live file is touched here.
    for round in 1..=512 {
        *step = "before_resolve";
        if archive::is_download_cancelled() {
            return Err("download.cancelled".into());
        }
        // Re-solving a map toggle can inspect live manifest ownership and DLL links.
        *step = "resolve_verified_graph";
        let solve_stage = if round <= 4 {
            "verification"
        } else {
            VERIFICATION_SUMMARY_STAGE
        };
        if round == 5 {
            log::info!(
                "整链实包复核搜索明细已达 4 轮上限 op={}，后续仍记录方案结论",
                trace.op
            );
        }
        let resolve_state = state.clone();
        let mut owned = lease.session.take().ok_or("modChanges.planExpired")?;
        let (owned, outcome) = tauri::async_runtime::spawn_blocking(move || {
            let result = planning::resolve_stage(&resolve_state, &mut owned, solve_stage);
            (owned, result)
        })
        .await
        .map_err(|e| e.to_string())?;
        lease.session = Some(owned);
        outcome?;
        let session = lease.session.as_mut().ok_or("modChanges.planExpired")?;
        progress.synchronize(session);
        if session.plan.result.status != SolveStatus::Ready {
            break;
        }
        *step = "game_run_guard";
        live_game::check_plan(state, session)?;
        let pending = session
            .solved
            .as_ref()
            .ok_or("modChanges.notPrepared")?
            .nodes
            .iter()
            .filter_map(|node| {
                node.current
                    .as_ref()
                    .filter(|c| c.verification_pending)
                    .map(|c| (node.id.clone(), c.clone()))
            })
            .next();
        let Some((node_id, candidate)) = pending else {
            *step = "select_verified_payloads";
            select_payloads(session)?;
            progress.stage(PreparationStage::Finalizing, None);
            *step = "freeze_mutations";
            let prepare_state = state.clone();
            // Moving the session into the blocking task keeps all disk-heavy work off IPC.
            let mut owned = lease.session.take().ok_or("modChanges.planExpired")?;
            let outcome = tauri::async_runtime::spawn_blocking(move || {
                let result = finish(&prepare_state, &mut owned);
                (owned, result)
            })
            .await
            .map_err(|e| e.to_string())?;
            lease.session = Some(outcome.0);
            outcome.1?;
            let session = lease.session.as_mut().ok_or("modChanges.planExpired")?;
            *step = "compare_approval";
            let changed = approval_signature(session)? != before;
            if changed {
                session.plan.plan_id = uuid::Uuid::new_v4().to_string();
            }
            session.plan.prepared = true;
            log::info!(
                "整链复核结论 op={} initial_plan={initial_plan} plan={} approval_changed={changed} initial_actions={initial_actions} actions={} mutations={} rounds={round}",
                trace.op,
                session.plan.plan_id,
                session.plan.result.actions.len(),
                session.mutations.as_ref().map_or(0, Vec::len),
            );
            log_plan(
                trace.op,
                "awaiting_apply",
                &session.plan,
                trace.elapsed_ms(),
                true,
            );
            return Ok(session.plan.clone());
        };
        *step = "reuse_verified_candidate";
        if let Some(payload) = session
            .verified_payloads
            .get(&(node_id.clone(), candidate.id.clone()))
        {
            let actual = payload.candidate.clone();
            replace_candidate(session, &node_id, &actual)?;
            log::info!(
                "整链复用实包 op={} round={round} {}",
                trace.op,
                log_detail(format_args!("node={node_id:?} candidate={:?}", actual.id))
            );
            progress.verified(&node_id, &actual.id);
            continue;
        }
        *step = "select_download_asset";
        let repository_id = candidate
            .source_id
            .as_deref()
            .ok_or("modChanges.missingMetadata")?;
        let asset_name = session
            .wire_request
            .roots
            .iter()
            .find(|root| root.intent.node_id == node_id)
            .and_then(|r| r.asset_name.clone());
        log::info!(
            "整链下载候选 op={} round={round} {}",
            trace.op,
            log_detail(format_args!(
                "node={node_id:?} source={repository_id:?} tag={:?} requested_asset={asset_name:?}",
                candidate.tag,
            ))
        );
        *step = "download_asset";
        let download_started = std::time::Instant::now();
        progress.stage(PreparationStage::Downloading, Some(&node_id));
        let reporter = progress.clone();
        let download_node = node_id.clone();
        let download_candidate = candidate.id.clone();
        let download = archive::observe_download_progress(
            true,
            move |download| reporter.download(&download_node, &download_candidate, download),
            download_repository_asset(
                state,
                RepositoryDownloadRequest {
                    repository_id,
                    tag: candidate.tag.as_deref(),
                    asset_name: asset_name.as_deref(),
                    ..Default::default()
                },
            ),
        )
        .await?;
        let download_ms = download_started.elapsed().as_millis();
        *step = "verify_downloaded_asset";
        let verify_started = std::time::Instant::now();
        progress.stage(PreparationStage::Verifying, Some(&node_id));
        let extract_state = state.clone();
        let trusted = session.trusted.clone();
        let target = session
            .staging
            .join(format!("asset-{}", uuid::Uuid::new_v4()));
        let extract_node = node_id.clone();
        let payload = tauri::async_runtime::spawn_blocking(move || {
            let payload = super::payload::from_downloaded(
                &extract_state,
                &trusted,
                download,
                &extract_node,
                &candidate,
                asset_name.as_deref(),
                &target,
            )?;
            Ok::<_, String>((candidate, payload))
        })
        .await
        .map_err(|e| e.to_string())??;
        let (advertised, payload) = payload;
        *step = "record_verified_candidate";
        replace_candidate(session, &node_id, &payload.candidate)?;
        let verified_candidate = payload.candidate.id.clone();
        let actual = &payload.candidate;
        log::info!(
            "整链候选实包已验证 op={} round={round} download_ms={download_ms} verify_ms={} files={} {}",
            trace.op,
            verify_started.elapsed().as_millis(),
            payload.files.len(),
            log_detail(format_args!(
                "node={node_id:?} candidate={verified_candidate:?} tag={:?} previous_version={:?} version={:?} previous_members={} members={} provides_changed={} previous_relations={} relations={} relations_changed={} metadata_known={} metadata_known_changed={}",
                actual.tag,
                advertised.version,
                actual.version,
                advertised.provides.len(),
                actual.provides.len(),
                advertised.provides != actual.provides,
                advertised.relations.len(),
                actual.relations.len(),
                advertised.relations != actual.relations,
                actual.relations_known,
                advertised.relations_known != actual.relations_known,
            )),
        );
        if round <= DETAIL_LIMIT && log::log_enabled!(log::Level::Info) {
            if advertised.provides != actual.provides {
                log::info!(
                    "整链实包插件差异 op={} round={round} {}",
                    trace.op,
                    log_detail(format_args!(
                        "node={node_id:?} previous={:?} actual={:?}",
                        advertised.provides.iter().take(6).collect::<Vec<_>>(),
                        actual.provides.iter().take(6).collect::<Vec<_>>()
                    ))
                );
            }
            if advertised.relations != actual.relations {
                log::info!(
                    "整链实包关系差异 op={} round={round} {}",
                    trace.op,
                    log_detail(format_args!(
                        "node={node_id:?} previous={:?} actual={:?}",
                        advertised.relations.iter().take(6).collect::<Vec<_>>(),
                        actual.relations.iter().take(6).collect::<Vec<_>>()
                    ))
                );
            }
        } else if round == DETAIL_LIMIT + 1 {
            log::info!("整链实包差异明细已达 {DETAIL_LIMIT} 轮上限 op={}", trace.op);
        }
        session
            .verified_payloads
            .insert((node_id.clone(), payload.candidate.id.clone()), payload);
        // A later network/validation error keeps both the verified candidate and
        // its payload available, with immutable bytes checked on the next call.
        planning::remember_payloads(session)?;
        progress.verified(&node_id, &verified_candidate);
    }
    *step = "return_unready_plan";
    let session = lease.session.as_mut().ok_or("modChanges.planExpired")?;
    if session.plan.result.status == SolveStatus::Ready {
        session.plan.result.status = SolveStatus::Incomplete;
        session.plan.install_notes.clear();
        session.plan.result.blockers.push(Diagnostic {
            code: "search_limit".into(),
            owner_id: None,
            target_guid: None,
            message: "modChanges.reason.searchLimit".into(),
            incomplete: true,
            traces: Vec::new(),
            traces_truncated: false,
        });
    }
    session.plan.plan_id = uuid::Uuid::new_v4().to_string();
    planning::remember_payloads(session)?;
    log_plan(
        trace.op,
        "preparation_blocked",
        &session.plan,
        trace.elapsed_ms(),
        true,
    );
    Ok(session.plan.clone())
}

fn replace_candidate(
    session: &mut PlanSession,
    id: &str,
    actual: &Candidate,
) -> Result<(), String> {
    let node = session
        .trusted
        .inventory
        .nodes
        .iter_mut()
        .find(|node| node.id == id)
        .ok_or("modChanges.targetChanged")?;
    let candidate = node
        .candidates
        .iter_mut()
        .find(|candidate| candidate.id == actual.id)
        .ok_or("modChanges.targetChanged")?;
    *candidate = actual.clone();
    Ok(())
}

pub(super) fn select_payloads(session: &mut PlanSession) -> Result<(), String> {
    let solved = session.solved.as_ref().ok_or("modChanges.notPrepared")?;
    let mut selected = HashMap::new();
    for node in &solved.nodes {
        let Some(current) = &node.current else {
            continue;
        };
        let old = session
            .trusted
            .inventory
            .nodes
            .iter()
            .find(|old| old.id == node.id)
            .and_then(|old| old.current.as_ref());
        if old.is_some_and(|old| old.id == current.id) {
            continue;
        }
        if let Some(payload) = session
            .verified_payloads
            .get(&(node.id.clone(), current.id.clone()))
        {
            selected.insert(node.id.clone(), payload.clone());
        } else if session
            .map_payload
            .as_ref()
            .is_none_or(|batch| !batch.node_ids.contains(&node.id))
        {
            return Err("modChanges.notPrepared".into());
        }
    }
    session.payloads = selected;
    Ok(())
}

pub(super) fn finish(state: &AppState, session: &mut PlanSession) -> Result<(), PreparationError> {
    verify_preparation_freshness(state, session, None)?;
    if let Some(solved) = &session.solved {
        session
            .release_target_claims
            .verify(state, session, solved)?;
        inventory::validate_solution(session, solved, &session.unbound_toggle_roots())?;
    }
    let roots = mutations::transaction_roots(state, session)?;
    if let Some(batch) = &session.map_payload {
        batch.verify_claims(&roots, session)?;
    }
    let changes = mutations::assemble(session, roots.clone())?;
    // FileChange already keeps the exact file source stamps. Freeze composed map
    // trees too, without replacing the proofs captured during Release verification.
    planning::remember_payloads(session)?;
    for owned in &changes {
        if let Mutation::Tree(change) = &owned.mutation
            && let Some(source) = &change.source
        {
            let fingerprint = session
                .payload_fingerprints
                .entry(source.path.clone())
                .or_insert_with(|| source.fingerprint.clone());
            if *fingerprint != source.fingerprint {
                return Err("modChanges.payloadChanged".into());
            }
        }
    }
    verify_preparation_freshness(state, session, Some(&changes))?;
    if live_game::touches_mods(session, &roots, &changes)? {
        guard::ensure_game_stopped(state, &roots.game_root)?;
    }
    session.plan.untracked_overwrites = session
        .release_target_claims
        .overwrites(session.operation_id());
    if let Some(batch) = &session.map_payload {
        session.plan.untracked_overwrites.extend(batch.overwrites());
    }
    let _operation = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let review = recovery_scope::review(state, session, &roots, &changes, None)?;
    session
        .release_target_claims
        .check_manifest_replacements(session, &roots, &changes)?;
    session.plan.recovery_conflicts = review.conflicts;
    session.plan.recovery_approval = review.approval;
    session.mutations = Some(changes);
    Ok(())
}

fn verify_preparation_freshness(
    state: &AppState,
    session: &PlanSession,
    frozen: Option<&[super::application_progress::OwnedMutation]>,
) -> Result<(), String> {
    let _operation = state.operation.lock().map_err(|_| "lock.poisoned")?;
    let recovered = guard::ensure_plannable(state)?;
    recovery_scope::check_plan_revisions(state, session, None)?;
    let (root, config) = freshness::current_target(state)?;
    let scope = freshness::check_scope(state, session, &root, &config)?;
    if crate::level_sets::recompress::has_pending_recovery(&root) {
        return Err("levelSet.recoveryRequired".into());
    }
    verify_fingerprints(&session.payload_fingerprints)?;
    freshness::check_revision(state, session, &root, &scope, recovered, frozen)
}

pub(crate) fn ensure_fresh(state: &AppState, expected: &str) -> Result<(), String> {
    let _operation = state.operation.lock().map_err(|_| "lock.poisoned")?;
    guard::ensure_plannable(state)?;
    let (root, config) = freshness::current_target(state)?;
    if inventory::revision(state, &root, &config)? != expected {
        return Err("modChanges.targetChanged".into());
    }
    Ok(())
}

pub(crate) fn verify_fingerprints(
    expected: &HashMap<std::path::PathBuf, Fingerprint>,
) -> Result<(), String> {
    for (path, before) in expected {
        if Fingerprint::capture(path)? != *before {
            return Err("modChanges.payloadChanged".into());
        }
    }
    Ok(())
}

/// Approval is bound to effective actions, roots, relations and actual provisions.
/// Explanations may become more precise after verification without changing the plan.
pub(super) fn approval_signature(session: &PlanSession) -> Result<Vec<u8>, String> {
    let result = &session.plan.result;
    let actions = result
        .actions
        .iter()
        .map(|action| {
            (
                &action.node_id,
                &action.member_key,
                &action.kind,
                &action.from_candidate_id,
                &action.to_candidate_id,
                &action.from_tag,
                &action.to_tag,
            )
        })
        .collect::<Vec<_>>();
    let members = session.solved.as_ref().map(|inventory| {
        inventory
            .nodes
            .iter()
            .filter(|node| {
                result
                    .actions
                    .iter()
                    .any(|action| action.node_id == node.id)
            })
            .map(|node| {
                (
                    node.id.clone(),
                    node.current.as_ref().map(|candidate| {
                        (
                            candidate
                                .provides
                                .iter()
                                .map(|p| (&p.guid, &p.version, p.enabled))
                                .collect::<Vec<_>>(),
                            &candidate.relations,
                        )
                    }),
                )
            })
            .collect::<Vec<_>>()
    });
    serde_json::to_vec(&(
        &session.request,
        &result.status,
        actions,
        &result.evaluations,
        &result.blockers,
        &result.notices,
        &result.impacts,
        members,
        &session.plan.retired_mod_copies,
        &session.plan.untracked_overwrites,
        &session.plan.install_notes,
    ))
    .map_err(|e| e.to_string())
}
