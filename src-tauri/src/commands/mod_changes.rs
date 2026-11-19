//! IPC accepts intent and opaque backend plan IDs, never caller-owned inventories.
use crate::{
    app_state::AppState,
    mod_changes::{
        planning::{EvaluationSnapshot, EvaluationSubject},
        session::{ChangePlan, PlanRequest},
    },
    models::OperationResult,
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
    app: AppHandle,
    state: State<'_, AppState>,
    plan_id: String,
    on_update: tauri::ipc::Channel<crate::mod_changes::progress::ChangePreparationProgress>,
) -> Result<crate::mod_changes::progress::PreparationOutcome, String> {
    Ok(crate::mod_changes::preparation::prepare(
        app,
        state.inner().clone(),
        plan_id,
        move |progress| {
            // A detached window must not turn successful staging into an install error.
            let _ = on_update.send(progress);
        },
    )
    .await)
}

#[tauri::command]
pub(crate) async fn execute_mod_changes(
    app: AppHandle,
    state: State<'_, AppState>,
    plan_id: String,
    on_update: tauri::ipc::Channel<
        crate::mod_changes::application_progress::ChangeApplicationProgress,
    >,
) -> Result<OperationResult, String> {
    let state = state.inner().clone();
    // Preserve repository_cache → operation ordering for the final source check.
    let cache = state.repository_cache.clone().lock_owned().await;
    tauri::async_runtime::spawn_blocking(move || {
        let _cache = cache;
        crate::mod_changes::execution::execute(&app, &state, &plan_id, move |progress| {
            // Closing the UI cannot abort or invalidate the filesystem transaction.
            let _ = on_update.send(progress);
        })
    })
    .await
    .map_err(|e| e.to_string())?
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
