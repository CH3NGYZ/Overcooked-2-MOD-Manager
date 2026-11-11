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
    let epoch = crate::diychef::preview::cache_epoch();
    crate::diychef::preview::require_epoch(epoch)?;
    crate::diychef::preview::configure_concurrency(
        state.config.load_or_default().diychef_preview_concurrency,
    );
    let permit = crate::diychef::preview::scene_permit().await?;
    crate::diychef::preview::require_epoch(epoch)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        crate::diychef::preview::get(&state, &session_id, &chef_id, &hat_id, epoch)
    })
    .await
    .map_err(|_| "diychef.previewUnavailable".to_string())?
}

/// Only generated previews are removed; callers cannot choose a directory.
#[tauri::command]
pub async fn clear_diychef_cache() -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(crate::diychef::preview::clear_cache)
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
