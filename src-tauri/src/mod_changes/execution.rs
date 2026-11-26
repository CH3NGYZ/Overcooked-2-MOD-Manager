//! Commit one prepared whole-chain plan under the existing write lock.
use super::{
    application_progress::{
        ChangeApplicationOutcome, ChangeApplicationProgress, ItemKey, Recorder,
    },
    freshness, guard, inventory, live_game,
    model::{ActionKind, NodeKind, SolveStatus},
    mutations, preparation, recovery_scope,
    session::*,
    transaction::*,
};
use crate::{app_state::AppState, models::OperationResult, mods};
use std::{fs, path::Path};
use tauri::{AppHandle, Emitter};

pub(crate) fn execute(
    app: &AppHandle,
    state: &AppState,
    id: &str,
    recovery_approval: Option<&str>,
    mut report: impl FnMut(ChangeApplicationProgress),
) -> ChangeApplicationOutcome {
    let mut progress = ChangeApplicationProgress::new(id);
    let mut replan_available = false;
    let result = execute_inner(
        app,
        state,
        id,
        recovery_approval,
        |update| {
            progress.retain_terminal(&update);
            report(update);
        },
        &mut replan_available,
    );
    ChangeApplicationOutcome {
        result: result.unwrap_or_else(|message| OperationResult { ok: false, message }),
        replan_available,
        progress,
    }
}

fn execute_inner(
    app: &AppHandle,
    state: &AppState,
    id: &str,
    recovery_approval: Option<&str>,
    report: impl FnMut(ChangeApplicationProgress),
    replan_available: &mut bool,
) -> Result<OperationResult, String> {
    let mut lease = SessionLease::take(state, id)?;
    let session = lease.session.as_mut().ok_or("modChanges.planExpired")?;
    if let Err(error) = commit_with_recovery(state, session, recovery_approval, report) {
        *replan_available = can_replan_failure(session, &error);
        return Err(error);
    }
    // Only a completed solution can scope the notification. A map request may
    // also enable or replace MODs, so the original intent alone is insufficient.
    let actions = &session.plan.result.actions;
    let map_toggle_only = session.wire_request.map_import.is_none()
        && !actions.is_empty()
        && session.solved.as_ref().is_some_and(|inventory| {
            actions.iter().all(|action| {
                matches!(action.kind, ActionKind::Enable | ActionKind::Disable)
                    && inventory
                        .nodes
                        .iter()
                        .any(|node| node.id == action.node_id && node.kind == NodeKind::Map)
            })
        });
    let staging = session.staging.clone();
    let op = session.operation_id().to_owned();
    lease.session = None;
    if let Err(error) = cleanup(state, &staging) {
        log::warn!(
            "整链已完成，暂存清理延后 op={op} reason={}",
            crate::locale_tag::diagnostic_tag(&error)
        );
    }
    let _ = app.emit(
        "mod-changes-applied",
        serde_json::json!({ "map_toggle_only": map_toggle_only }),
    );
    Ok(OperationResult {
        ok: true,
        message: "modChanges.completed".into(),
    })
}

pub(super) fn can_replan_failure(session: &PlanSession, error: &str) -> bool {
    session.plan.prepared
        && session.mutations.is_some()
        && matches!(
            crate::locale_tag::diagnostic_tag(error),
            "modChanges.targetChanged" | "modChanges.inventoryChanged"
        )
}

/// The write boundary is also used by isolated real-file orchestration tests.
/// Notification failures are deliberately outside the durable transaction.
#[cfg(test)]
pub(super) fn commit(
    state: &AppState,
    session: &mut PlanSession,
    report: impl FnMut(ChangeApplicationProgress),
) -> Result<(), String> {
    commit_with_recovery(state, session, None, report)
}

pub(super) fn commit_with_recovery(
    state: &AppState,
    session: &mut PlanSession,
    recovery_approval: Option<&str>,
    report: impl FnMut(ChangeApplicationProgress),
) -> Result<(), String> {
    let op = session.operation_id().to_owned();
    let trace = OperationTrace::new(&op, "apply");
    let plan = session.plan.plan_id.clone();
    log::info!(
        "整链开始应用 op={op} plan={plan} prepared={} actions={} mutations={}",
        session.plan.prepared,
        session.plan.result.actions.len(),
        session.mutations.as_ref().map_or(0, Vec::len)
    );
    let mut step = "validate_plan";
    let outcome = commit_inner(state, session, recovery_approval, report, &trace, &mut step);
    match &outcome {
        Ok(()) => log::info!(
            "整链应用完成 op={op} plan={plan} elapsed_ms={}",
            trace.elapsed_ms()
        ),
        Err(error) => trace.failure(step, error),
    }
    outcome
}

fn commit_inner(
    state: &AppState,
    session: &mut PlanSession,
    recovery_approval: Option<&str>,
    report: impl FnMut(ChangeApplicationProgress),
    trace: &OperationTrace<'_>,
    step: &mut &'static str,
) -> Result<(), String> {
    if !session.plan.prepared || session.plan.result.status != SolveStatus::Ready {
        return Err("modChanges.notPrepared".into());
    }
    *step = "lock_and_recovery";
    let _operation = state.operation.lock().map_err(|_| "lock.poisoned")?;
    recovery_scope::check_plan_revisions(state, session, None)?;
    let (root, config) = freshness::current_target(state)?;
    if crate::level_sets::recompress::has_pending_recovery(&root) {
        return Err("levelSet.recoveryRequired".into());
    }
    *step = "verify_game_root";
    let solved = session.solved.as_ref().ok_or("modChanges.notPrepared")?;
    let scope = freshness::check_scope(state, session, &root, &config)?;
    *step = "verify_release_claims";
    if !session.release_target_claims.is_empty() {
        session
            .release_target_claims
            .verify(state, session, solved)?;
    }
    *step = "verify_frozen_payloads";
    preparation::verify_fingerprints(&session.payload_fingerprints)?;
    *step = "validate_solution";
    inventory::validate_solution(session, solved, &session.unbound_toggle_roots())?;
    *step = "verify_map_claims";
    let roots = mutations::transaction_roots(state, session)?;
    if let Some(batch) = &session.map_payload {
        batch.verify_claims(&roots, session)?;
    }
    session.release_target_claims.verify_orphan_game_writes(
        &roots,
        session
            .mutations
            .as_deref()
            .ok_or("modChanges.notPrepared")?,
    )?;
    *step = "game_run_guard";
    let touches_mods = live_game::touches_mods(
        session,
        &roots,
        session
            .mutations
            .as_deref()
            .ok_or("modChanges.notPrepared")?,
    )?;
    let check_game = || -> Result<(), String> {
        if touches_mods {
            guard::ensure_game_stopped(state, &root)?;
        }
        Ok(())
    };
    check_game()?;
    *step = "verify_installed_revision";
    freshness::check_revision(
        state,
        session,
        &root,
        &scope,
        false,
        session.mutations.as_deref(),
    )?;
    *step = "verify_recovery_scope";
    let verify_recovery = |session: &PlanSession,
                           changes: &[super::application_progress::OwnedMutation],
                           own_transaction| {
        recovery_scope::review(state, session, &roots, changes, own_transaction)?
            .verify(session.plan.recovery_approval.as_deref(), recovery_approval)
    };
    verify_recovery(
        session,
        session
            .mutations
            .as_deref()
            .ok_or("modChanges.notPrepared")?,
        None,
    )?;
    *step = "consume_approval";
    let changes = session.mutations.take().ok_or("modChanges.notPrepared")?;
    // Consuming frozen mutations consumes approval, including prepare failures.
    session.plan.prepared = false;
    if !changes.is_empty() {
        let written = session.release_target_claims.reinstall_writes(&changes);
        // Revoke old recompression leases before publishing Applying. No background
        // worker may write into a directory whose ownership is about to change.
        *step = "revoke_snapshot";
        state.invalidate_local_mod_snapshot()?;
        // A failure or rollback cannot inherit evidence from an older commit.
        state.committed_watch_paths.publish(Default::default());
        let (mutations, records) = changes
            .iter()
            .map(|owned| (owned.mutation.clone(), owned.items.clone()))
            .unzip();
        let action_items = session
            .plan
            .result
            .actions
            .iter()
            .map(|action| ItemKey::new(&action.node_id, action.member_key.as_deref()));
        let retired_items = session
            .plan
            .retired_mod_copies
            .iter()
            .flat_map(|(id, copies)| {
                copies
                    .iter()
                    .map(|copy| ItemKey::new(id, Some(&copy.member_key)))
            });
        let mut progress = Recorder::new(
            &session.plan.plan_id,
            action_items.chain(retired_items),
            records,
            report,
        );
        *step = "prepare_transaction";
        let transaction_started = std::time::Instant::now();
        let op = trace.op;
        let transaction = prepare_for_operation(
            &state.config.mod_change_transactions_dir(),
            &roots,
            mutations,
            Some(op),
            session.release_target_claims.guarded_original_files(),
        )
        .inspect_err(|_| guard::notify(state, false))?;
        let transaction_id = transaction.id().to_owned();
        log::info!(
            "应用整链事务 op={op} txn={transaction_id} actions={} prepare_ms={}",
            session.plan.result.actions.len(),
            transaction_started.elapsed().as_millis()
        );
        // The game can start during private journal preparation. Check again
        // after verifying that journal, just before its first live write.
        let transaction_stage = std::cell::Cell::new("verify_journal_before_write");
        let check_targets = || -> Result<(), String> {
            transaction_stage.set("verify_live_claims_before_write");
            check_game()?;
            verify_recovery(session, &changes, Some(&transaction_id))?;
            if (session
                .map_payload
                .as_ref()
                .is_some_and(|batch| batch.has_claims())
                || !session.release_target_claims.is_empty())
                && inventory::revision(
                    state,
                    &root,
                    &state
                        .config
                        .load()
                        .map_err(|_| "modChanges.targetChanged")?,
                )? != session.trusted.revision
            {
                return Err("modChanges.targetChanged".into());
            }
            if let Some(batch) = &session.map_payload {
                batch.verify_claims(&roots, session)?;
            }
            if !session.release_target_claims.is_empty() {
                session
                    .release_target_claims
                    .verify(state, session, solved)?;
            }
            transaction_stage.set("write_and_verify_mutations");
            Ok(())
        };
        *step = "apply_transaction";
        let applied = transaction.apply_checked(
            crate::archive::is_download_cancelled,
            check_targets,
            || {
                transaction_stage.set("verify_domain_before_commit");
                if let Some(batch) = &session.map_payload {
                    super::map_payload::verify_external_counterparts(batch, &roots, solved)?;
                }
                session
                    .release_target_claims
                    .verify_reinstalled_paths(state, &roots, &written)?;
                transaction_stage.set("persist_commit");
                Ok(())
            },
            |event| {
                if let RecordProgress::Failed(index) = &event {
                    log::info!(
                        "整链事务记录失败 op={op} txn={transaction_id} record_no={} step={}",
                        index + 1,
                        transaction_stage.get()
                    );
                }
                progress.observe(event);
            },
        );
        guard::notify(state, false);
        match applied {
            Ok(watch_paths) => {
                log::info!(
                    "整链事务已提交 op={op} txn={transaction_id} elapsed_ms={}",
                    transaction_started.elapsed().as_millis()
                );
                state.committed_watch_paths.publish(watch_paths);
            }
            Err(error) => {
                let recovery_required = needs_recovery(&error);
                *step = transaction_stage.get();
                log::info!(
                    "整链事务未返回提交成功 op={op} txn={transaction_id} last_step={} recovery_required={recovery_required} elapsed_ms={}",
                    *step,
                    transaction_started.elapsed().as_millis()
                );
                progress.failed(!recovery_required);
                return Err(error);
            }
        }
    }
    // A durable Committed journal is final. A later cache/UI failure must not report
    // installation failure or cause users to replay a successfully committed batch.
    if let Err(error) = publish(state, &root) {
        log::error!(
            "整链应用后重建快照失败，等待页面重读 op={} reason={}",
            trace.op,
            crate::locale_tag::diagnostic_tag(&error)
        );
    }
    Ok(())
}

fn publish(state: &AppState, root: &Path) -> Result<(), String> {
    let config = state.config.load_or_default();
    let installed =
        mods::scan_mods_with_manifests(root, &config, Some(&state.config.manifests_dir()));
    let mut snapshots = state
        .local_mod_snapshot
        .lock()
        .map_err(|_| "lock.poisoned")?;
    let generation = snapshots.begin(Some(root));
    snapshots.publish(generation, Some(root), installed)?;
    drop(snapshots);
    let inventory = inventory::build(state)?;
    state
        .mod_changes
        .lock()
        .map_err(|_| "lock.poisoned")?
        .snapshot = Some(std::sync::Arc::new(inventory));
    Ok(())
}

pub(crate) fn cancel(state: &AppState, id: &str) -> Result<(), String> {
    let mut lease = SessionLease::take(state, id)?;
    let session = lease.session.as_ref().ok_or("modChanges.planExpired")?;
    let staging = session.staging.clone();
    // A plan cannot own an Applying journal: execution holds the session lease.
    let trace = OperationTrace::new(session.operation_id(), "cancel_plan");
    if let Err(error) = cleanup(state, &staging) {
        trace.failure("cleanup_staging", &error);
        return Err(error);
    }
    log::info!(
        "整链取消 op={} stage=cancel_plan plan={} prepared={} elapsed_ms={}",
        trace.op,
        session.plan.plan_id,
        session.plan.prepared,
        trace.elapsed_ms()
    );
    lease.session = None;
    Ok(())
}

pub(super) fn cleanup(state: &AppState, staging: &Path) -> Result<(), String> {
    let root = state
        .config
        .config_path()
        .parent()
        .ok_or("archive.pathEscape")?
        .join("mod-change-staging");
    if staging.parent() != Some(root.as_path())
        || staging
            .file_name()
            .and_then(|name| name.to_str())
            .is_none_or(|name| uuid::Uuid::parse_str(name).is_err())
    {
        return Err("archive.pathEscape".into());
    }
    // Verify the exact task-owned path before recursive removal; never user ZIPs.
    if Fingerprint::capture(staging)? == Fingerprint::Missing {
        return Ok(());
    }
    fs::remove_dir_all(staging).map_err(|error| error.to_string())
}
