//! Recovery gate for operations that can mutate files or invalidate recovery material.
use super::transaction::{self, RecoveryDecisionState, RecoveryEntry, TransactionRoots};
use crate::{app_state::AppState, game};
use std::path::Path;

pub(super) const REQUIRED: &str = "modChanges.recoveryRequired";

pub(crate) enum Decision {
    Pause,
    Retry,
    Abandon { high_risk: bool },
}

/// Legacy write IPCs remain callable for older clients, but authorization now
/// comes exclusively from a prepared, confirmed chain plan.
pub(crate) fn require_shared_plan<T>() -> Result<T, String> {
    Err("modChanges.installationBlocked".into())
}

pub(super) fn ensure_game_stopped(_state: &AppState, game_root: &Path) -> Result<(), String> {
    #[cfg(test)]
    if _state
        .game_running_for_test
        .load(std::sync::atomic::Ordering::Relaxed)
    {
        return Err("game.running".into());
    }
    game::ensure_game_stopped(game_root)
}

fn selected_roots(state: &AppState) -> Result<TransactionRoots, String> {
    let data_root = state
        .config
        .config_path()
        .parent()
        .ok_or(REQUIRED)?
        .to_path_buf();
    let config = state.config.load().map_err(|_| REQUIRED)?;
    let detected = game::detect_game_root(&config.game).ok_or(REQUIRED)?;
    let game_root = game::resolve_existing_ancestor(&detected)?;
    Ok(TransactionRoots {
        game_root,
        data_root,
    })
}

pub(crate) fn list(state: &AppState) -> Result<Vec<RecoveryEntry>, String> {
    let transaction_root = state.config.mod_change_transactions_dir();
    if !transaction::has_retained_material(&transaction_root)? {
        return Ok(Vec::new());
    }
    let roots = selected_roots(state).ok();
    transaction::list_recoveries(&transaction_root, roots.as_ref())
}

fn can_plan(entry: &RecoveryEntry) -> bool {
    entry.archived || entry.state == RecoveryDecisionState::Paused && entry.scope_known
}

fn blocks_unscoped_write(entry: &RecoveryEntry) -> bool {
    !entry.archived
        || entry
            .paths
            .iter()
            .any(|path| path.kind != transaction::RecoveryPathKind::Journal)
}

pub(super) fn records_for_plan(
    state: &AppState,
    own_transaction: Option<&str>,
) -> Result<Vec<RecoveryEntry>, String> {
    let mut entries = list(state).map_err(gate_error)?;
    entries.retain(|entry| Some(entry.id.as_str()) != own_transaction);
    if !entries.iter().all(can_plan) {
        return Err(REQUIRED.into());
    }
    Ok(entries)
}

/// Call while holding operation, including the eventual write.
pub(crate) fn check_unscoped_write(state: &AppState) -> Result<(), String> {
    if list(state)
        .map_err(gate_error)?
        .iter()
        .any(blocks_unscoped_write)
    {
        return Err(REQUIRED.into());
    }
    Ok(())
}

/// Call while holding operation. Known pauses/archives only allow planning;
/// committing requires an independent, fresh path check.
pub(super) fn ensure_plannable(state: &AppState) -> Result<bool, String> {
    let recovered = try_automatic(state).map_err(gate_error)?;
    records_for_plan(state, None)?;
    Ok(recovered)
}

pub(crate) fn ensure_recovered(state: &AppState) -> Result<bool, String> {
    let recovered = try_automatic(state).map_err(gate_error)?;
    check_unscoped_write(state)?;
    Ok(recovered)
}

fn gate_error(error: String) -> String {
    log::warn!(
        "未完成链式事务阻止当前操作，保留恢复材料：原因={}",
        crate::locale_tag::diagnostic_tag(&error),
    );
    REQUIRED.into()
}

pub(crate) fn notify(state: &AppState, inventory_changed: bool) {
    state
        .mod_change_recovery_events
        .send_modify(|(recovery, inventory)| {
            *recovery = recovery.wrapping_add(1);
            if inventory_changed {
                *inventory = inventory.wrapping_add(1);
            }
        });
}

fn recover_files<T>(
    state: &AppState,
    roots: &TransactionRoots,
    recover: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    ensure_game_stopped(state, &roots.game_root)?;
    if crate::level_sets::recompress::has_pending_recovery(&roots.game_root) {
        return Err("levelSet.recoveryRequired".into());
    }
    state.invalidate_local_mod_snapshot()?;
    state.committed_watch_paths.publish(Default::default());
    let result = recover();
    notify(state, true);
    result
}

pub(crate) fn try_automatic(state: &AppState) -> Result<bool, String> {
    if !list(state)?
        .iter()
        .any(|entry| entry.state == RecoveryDecisionState::Automatic)
    {
        return Ok(false);
    }
    let roots = selected_roots(state)?;
    let report = recover_files(state, &roots, || {
        transaction::recover_all(&state.config.mod_change_transactions_dir(), &roots)
    })?;
    if !report.blocked.is_empty() {
        return Err(REQUIRED.into());
    }
    Ok(!report.recovered.is_empty() || !report.cleaned.is_empty())
}

pub(crate) fn decide(
    state: &AppState,
    id: &str,
    revision: &str,
    decision: Decision,
) -> Result<Vec<RecoveryEntry>, String> {
    let _operation = state.operation.lock().map_err(|_| "lock.poisoned")?;
    let transaction_root = state.config.mod_change_transactions_dir();
    let roots = selected_roots(state);
    match decision {
        Decision::Pause => {
            transaction::pause_recovery(&transaction_root, id, revision, roots.ok().as_ref())?
        }
        Decision::Abandon { high_risk } => {
            transaction::abandon_recovery(
                &transaction_root,
                id,
                revision,
                roots.ok().as_ref(),
                high_risk,
            )?;
        }
        Decision::Retry => {
            let roots = roots?;
            recover_files(state, &roots, || {
                transaction::retry_recovery(&transaction_root, &roots, id, revision)
            })?;
            return list(state);
        }
    }
    notify(state, false);
    list(state)
}
