use crate::{app_state::AppState, models::ChefPreviewScene};
use tauri::State;

#[tauri::command]
pub async fn get_diychef_preview(
    state: State<'_, AppState>,
    session_id: String,
    chef_id: String,
    hat_id: String,
) -> Result<ChefPreviewScene, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::diychef::preview::get(&state, &session_id, &chef_id, &hat_id)
    })
    .await
    .map_err(|_| "diychef.previewUnavailable".to_string())?
}

#[tauri::command]
pub async fn read_diychef_preview_asset(
    state: State<'_, AppState>,
    session_id: String,
    asset_id: String,
) -> Result<tauri::ipc::Response, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::diychef::preview::read_asset(&state, &session_id, &asset_id)
            .map(tauri::ipc::Response::new)
    })
    .await
    .map_err(|_| "diychef.previewUnavailable".to_string())?
}
