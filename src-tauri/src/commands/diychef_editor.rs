use crate::{app_state::AppState, diychef::session, models::*};
use tauri::State;
#[tauri::command]
pub async fn open_diychef_editor(
    state: State<'_, AppState>,
    generation: u64,
    target_id: Option<String>,
) -> Result<ChefEditorOpen, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        session::open(&state, generation, target_id.as_deref())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn initialize_diychef_config(
    state: State<'_, AppState>,
    session_id: String,
    expected_revision: String,
) -> Result<ChefEditorSnapshot, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        session::initialize(&state, &session_id, &expected_revision)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn validate_diychef_draft(
    state: State<'_, AppState>,
    session_id: String,
    draft: ChefDraft,
) -> Result<ChefValidation, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || session::validate(&state, &session_id, &draft))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn save_diychef_config(
    state: State<'_, AppState>,
    session_id: String,
    draft: ChefDraft,
) -> Result<ChefEditorSnapshot, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || session::save(&state, &session_id, &draft))
        .await
        .map_err(|e| e.to_string())?
}
