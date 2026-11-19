//! Download the complete chosen chain, verify its real DLLs, then solve again.
use super::progress::{
    ChangePreparationProgress, PreparationOutcome, PreparationReporter, PreparationStage,
};
use super::{inventory, model::*, planning, session::*, transaction::*};
use crate::{
    app_state::AppState,
    archive, game,
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
) -> Result<ChangePlan, String> {
    let mut lease = SessionLease::take(&state, &id)?;
    let session = lease.session.as_mut().ok_or("modChanges.planExpired")?;
    if session.plan.prepared {
        return Ok(session.plan.clone());
    }
    if session.plan.result.status != SolveStatus::Ready {
        return Err("modChanges.notPrepared".into());
    }
    progress.synchronize(session);
    let before = approval_signature(session)?;
    // Freeze and recheck existing payloads before extending the plan.
    let fresh_state = state.clone();
    let revision = session.trusted.revision.clone();
    let fingerprints = session.payload_fingerprints.clone();
    tauri::async_runtime::spawn_blocking(move || {
        ensure_fresh(&fresh_state, &revision)?;
        verify_fingerprints(&fingerprints)
    })
    .await
    .map_err(|e| e.to_string())??;
    archive::start_download();
    for (node, payload) in &session.payloads {
        session.verified_payloads.insert(
            (node.clone(), payload.candidate.id.clone()),
            payload.clone(),
        );
    }
    // Each iteration resolves the entire graph. A newly discovered package member
    // may change which asset another node needs; no live file is touched here.
    for _ in 0..512 {
        if archive::is_download_cancelled() {
            return Err("download.cancelled".into());
        }
        // Re-solving a map toggle can inspect live manifest ownership and DLL links.
        let resolve_state = state.clone();
        let mut owned = lease.session.take().ok_or("modChanges.planExpired")?;
        let (owned, outcome) = tauri::async_runtime::spawn_blocking(move || {
            let result = planning::resolve(&resolve_state, &mut owned);
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
            select_payloads(session)?;
            progress.stage(PreparationStage::Finalizing, None);
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
            if approval_signature(session)? != before {
                session.plan.plan_id = uuid::Uuid::new_v4().to_string();
            }
            session.plan.prepared = true;
            return Ok(session.plan.clone());
        };
        if let Some(payload) = session
            .verified_payloads
            .get(&(node_id.clone(), candidate.id.clone()))
        {
            let actual = payload.candidate.clone();
            replace_candidate(session, &node_id, &actual)?;
            progress.verified(&node_id, &actual.id);
            continue;
        }
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
        progress.stage(PreparationStage::Downloading, Some(&node_id));
        let reporter = progress.clone();
        let download_node = node_id.clone();
        let download_candidate = candidate.id.clone();
        let download = archive::observe_download_progress(
            move |download| reporter.download(&download_node, &download_candidate, download),
            download_repository_asset(
                None,
                &state,
                RepositoryDownloadRequest {
                    repository_id,
                    tag: candidate.tag.as_deref(),
                    asset_name: asset_name.as_deref(),
                    ..Default::default()
                },
            ),
        )
        .await?;
        progress.stage(PreparationStage::Verifying, Some(&node_id));
        let extract_state = state.clone();
        let trusted = session.trusted.clone();
        let target = session
            .staging
            .join(format!("asset-{}", uuid::Uuid::new_v4()));
        let extract_node = node_id.clone();
        let payload = tauri::async_runtime::spawn_blocking(move || {
            super::payload::from_downloaded(
                &extract_state,
                &trusted,
                download,
                &extract_node,
                &candidate,
                asset_name.as_deref(),
                &target,
            )
        })
        .await
        .map_err(|e| e.to_string())??;
        replace_candidate(session, &node_id, &payload.candidate)?;
        let verified_candidate = payload.candidate.id.clone();
        session
            .verified_payloads
            .insert((node_id.clone(), payload.candidate.id.clone()), payload);
        // A later network/validation error keeps both the verified candidate and
        // its payload available, with immutable bytes checked on the next call.
        planning::remember_payloads(session)?;
        progress.verified(&node_id, &verified_candidate);
    }
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
        });
    }
    session.plan.plan_id = uuid::Uuid::new_v4().to_string();
    planning::remember_payloads(session)?;
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
            session.payloads.insert(node.id.clone(), payload.clone());
        } else if session
            .map_payload
            .as_ref()
            .is_none_or(|batch| !batch.node_ids.contains(&node.id))
        {
            return Err("modChanges.notPrepared".into());
        }
    }
    Ok(())
}

pub(super) fn finish(state: &AppState, session: &mut PlanSession) -> Result<(), String> {
    ensure_fresh(state, &session.trusted.revision)?;
    verify_fingerprints(&session.payload_fingerprints)?;
    if let Some(solved) = &session.solved {
        inventory::validate_solution(
            &session.trusted,
            &session.request,
            solved,
            &session.unbound_toggle_roots(),
        )?;
    }
    let mut mutations = super::mutations::build(state, session)?;
    if session.map_payload.is_some() {
        mutations.extend(super::map_payload::mutations(state, session)?);
    }
    // FileChange already keeps the exact file source stamps. Freeze composed map
    // trees too, without replacing the proofs captured during Release verification.
    planning::remember_payloads(session)?;
    for owned in &mutations {
        if let Mutation::Tree(change) = &owned.mutation
            && let Some(source) = &change.source
        {
            session
                .payload_fingerprints
                .insert(source.clone(), Fingerprint::capture(source)?);
        }
    }
    session.mutations = Some(mutations);
    ensure_fresh(state, &session.trusted.revision)?;
    Ok(())
}

pub(crate) fn ensure_fresh(state: &AppState, expected: &str) -> Result<(), String> {
    let _operation = state.operation.lock().map_err(|_| "lock.poisoned")?;
    super::guard::ensure_recovered(state)?;
    let config = state.config.load_or_default();
    let root = game::detect_game(&config).path.ok_or("game.notFound")?;
    let root = game::resolve_existing_ancestor(std::path::Path::new(&root))?;
    game::ensure_game_stopped(&root)?;
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
        &result.impacts,
        members,
        &session.plan.retired_mod_copies,
        &session.plan.install_notes,
    ))
    .map_err(|e| e.to_string())
}
