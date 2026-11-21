//! Commit one prepared whole-chain plan under the existing write lock.
use super::{
    application_progress::{
        ChangeApplicationOutcome, ChangeApplicationProgress, ItemKey, Recorder,
    },
    guard, inventory, live_game,
    model::{ActionKind, NodeKind, SolveStatus},
    mutations, preparation,
    session::*,
    transaction::*,
};
use crate::{app_state::AppState, game, models::OperationResult, mods};
use std::{fs, path::Path};
use tauri::{AppHandle, Emitter};

pub(crate) fn execute(
    app: &AppHandle,
    state: &AppState,
    id: &str,
    mut report: impl FnMut(ChangeApplicationProgress),
) -> ChangeApplicationOutcome {
    let mut progress = ChangeApplicationProgress::new(id);
    let result = execute_inner(app, state, id, |update| {
        progress.retain_terminal(&update);
        report(update);
    });
    ChangeApplicationOutcome {
        result: result.unwrap_or_else(|message| OperationResult { ok: false, message }),
        progress,
    }
}

fn execute_inner(
    app: &AppHandle,
    state: &AppState,
    id: &str,
    report: impl FnMut(ChangeApplicationProgress),
) -> Result<OperationResult, String> {
    let mut lease = SessionLease::take(state, id)?;
    let session = lease.session.as_mut().ok_or("modChanges.planExpired")?;
    commit(state, session, report)?;
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
    lease.session = None;
    if let Err(error) = cleanup(state, &staging) {
        log::warn!("整链已完成，暂存清理延后：{error}");
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

/// The write boundary is also used by isolated real-file orchestration tests.
/// Notification failures are deliberately outside the durable transaction.
pub(super) fn commit(
    state: &AppState,
    session: &mut PlanSession,
    report: impl FnMut(ChangeApplicationProgress),
) -> Result<(), String> {
    if !session.plan.prepared || session.plan.result.status != SolveStatus::Ready {
        return Err("modChanges.notPrepared".into());
    }
    let _operation = state.operation.lock().map_err(|_| "lock.poisoned")?;
    let config = state.config.load_or_default();
    let detected = game::detect_game(&config).path.ok_or("game.notFound")?;
    let root = game::resolve_existing_ancestor(Path::new(&detected))?;
    if crate::level_sets::recompress::has_pending_recovery(&root) {
        return Err("modChanges.recoveryRequired".into());
    }
    guard::ensure_recovered(state)?;
    let solved = session.solved.as_ref().ok_or("modChanges.notPrepared")?;
    if root != session.trusted.game_root {
        return Err("modChanges.targetChanged".into());
    }
    if !session.loose_file_claims.is_empty() {
        session.loose_file_claims.verify(state, session, solved)?;
    }
    if inventory::revision(state, &root, &config)? != session.trusted.revision {
        return Err("modChanges.targetChanged".into());
    }
    preparation::verify_fingerprints(&session.payload_fingerprints)?;
    inventory::validate_solution(
        &session.trusted,
        &session.request,
        solved,
        &session.unbound_toggle_roots(),
    )?;
    let roots = mutations::transaction_roots(state, session)?;
    let touches_mods = live_game::touches_mods(
        session,
        &roots,
        session
            .mutations
            .as_deref()
            .ok_or("modChanges.notPrepared")?,
    )?;
    let check_game = || {
        if touches_mods {
            guard::ensure_game_stopped(state, &root)?;
        }
        Ok(())
    };
    check_game()?;
    let changes = session.mutations.take().ok_or("modChanges.notPrepared")?;
    // Consuming frozen mutations consumes approval, including prepare failures.
    session.plan.prepared = false;
    if !changes.is_empty() {
        // Revoke old recompression leases before publishing Applying. No background
        // worker may write into a directory whose ownership is about to change.
        state.invalidate_local_mod_snapshot()?;
        // A failure or rollback cannot inherit evidence from an older commit.
        state.committed_watch_paths.publish(Default::default());
        let (changes, records) = changes
            .into_iter()
            .map(|owned| (owned.mutation, owned.items))
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
        let transaction = super::transaction::prepare(
            &roots.data_root.join("mod-change-transactions"),
            &roots,
            changes,
        )?;
        log::info!(
            "应用整链事务：事务={}，动作={}",
            transaction.id(),
            session.plan.result.actions.len()
        );
        // The game can start during private journal preparation. Check again
        // after verifying that journal, just before its first live write.
        match transaction.apply_checked(
            crate::archive::is_download_cancelled,
            check_game,
            |event| progress.observe(event),
        ) {
            Ok(watch_paths) => state.committed_watch_paths.publish(watch_paths),
            Err(error) => {
                progress.failed(error != "recycle.partialFailure");
                return Err(error);
            }
        }
    }
    // A durable Committed journal is final. A later cache/UI failure must not report
    // installation failure or cause users to replay a successfully committed batch.
    if let Err(error) = publish(state, &root) {
        log::error!("整链已提交，重建快照失败，等待页面重读：{error}");
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
    cleanup(state, &staging)?;
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
