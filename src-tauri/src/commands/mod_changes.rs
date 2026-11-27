//! IPC accepts intent and opaque backend plan IDs, never caller-owned inventories.
use crate::{
    app_state::AppState,
    mod_changes::{
        application_progress::{ChangeApplicationOutcome, ChangeApplicationProgress},
        guard::{self, Decision},
        install_notes::{self, InstallNote},
        planning::{EvaluationSnapshot, EvaluationSubject},
        session::{ChangePlan, PlanRequest},
        transaction::RecoveryEntry,
    },
};
use tauri::{AppHandle, State};

#[tauri::command]
pub(crate) async fn evaluate_mod_changes(
    state: State<'_, AppState>,
    subjects: Option<Vec<EvaluationSubject>>,
) -> Result<EvaluationSnapshot, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::mod_changes::planning::evaluate(&state, subjects.unwrap_or_default())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) async fn preview_mod_install_note(
    state: State<'_, AppState>,
    repository_id: String,
) -> Result<Option<InstallNote>, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || install_notes::preview(&state, &repository_id))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) async fn plan_mod_changes(
    state: State<'_, AppState>,
    request: PlanRequest,
) -> Result<ChangePlan, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::mod_changes::planning::create(&state, request)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) async fn prepare_mod_changes(
    state: State<'_, AppState>,
    plan_id: String,
    on_update: tauri::ipc::Channel<crate::mod_changes::progress::ChangePreparationProgress>,
) -> Result<crate::mod_changes::progress::PreparationOutcome, String> {
    Ok(
        crate::mod_changes::preparation::prepare(state.inner().clone(), plan_id, move |progress| {
            // A detached window must not turn successful staging into an install error.
            let _ = on_update.send(progress);
        })
        .await,
    )
}

#[tauri::command]
pub(crate) async fn execute_mod_changes(
    app: AppHandle,
    state: State<'_, AppState>,
    plan_id: String,
    recovery_approval: Option<String>,
    on_update: tauri::ipc::Channel<ChangeApplicationProgress>,
) -> Result<ChangeApplicationOutcome, String> {
    let state = state.inner().clone();
    // Preserve repository_cache → operation ordering for the final source check.
    let cache = state.repository_cache.clone().lock_owned().await;
    tauri::async_runtime::spawn_blocking(move || {
        let _cache = cache;
        crate::mod_changes::execution::execute(
            &app,
            &state,
            &plan_id,
            recovery_approval.as_deref(),
            move |progress| {
                // Closing the UI cannot abort or invalidate the filesystem transaction.
                let _ = on_update.send(progress);
            },
        )
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn cancel_mod_changes(
    state: State<'_, AppState>,
    plan_id: String,
) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::mod_changes::execution::cancel(&state, &plan_id)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) async fn get_mod_change_recoveries(
    state: State<'_, AppState>,
) -> Result<Vec<RecoveryEntry>, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _operation = state.operation.lock().map_err(|_| "lock.poisoned")?;
        guard::list(&state)
    })
    .await
    .map_err(|error| error.to_string())?
}

async fn decide_recovery(
    state: AppState,
    transaction_id: String,
    revision: String,
    decision: Decision,
) -> Result<Vec<RecoveryEntry>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        guard::decide(&state, &transaction_id, &revision, decision)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) async fn pause_mod_change_recovery(
    state: State<'_, AppState>,
    transaction_id: String,
    revision: String,
) -> Result<Vec<RecoveryEntry>, String> {
    decide_recovery(
        state.inner().clone(),
        transaction_id,
        revision,
        Decision::Pause,
    )
    .await
}

#[tauri::command]
pub(crate) async fn retry_mod_change_recovery(
    state: State<'_, AppState>,
    transaction_id: String,
    revision: String,
) -> Result<Vec<RecoveryEntry>, String> {
    decide_recovery(
        state.inner().clone(),
        transaction_id,
        revision,
        Decision::Retry,
    )
    .await
}

#[tauri::command]
pub(crate) async fn abandon_mod_change_recovery(
    state: State<'_, AppState>,
    transaction_id: String,
    revision: String,
    high_risk: bool,
) -> Result<Vec<RecoveryEntry>, String> {
    decide_recovery(
        state.inner().clone(),
        transaction_id,
        revision,
        Decision::Abandon { high_risk },
    )
    .await
}
