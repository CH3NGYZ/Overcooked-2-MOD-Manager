use crate::{app_state::AppState, diychef::session, models::*};
use tauri::{AppHandle, Emitter, State};
#[tauri::command]
pub async fn migrate_diychef_resources(
    state: State<'_, AppState>,
    generation: u64,
    target_id: String,
) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::diychef::migration::migrate(&state, generation, &target_id)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn open_diychef_editor(
    app: AppHandle,
    state: State<'_, AppState>,
    generation: u64,
    target_id: Option<String>,
) -> Result<ChefEditorOpen, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        session::open(&state, generation, target_id.as_deref(), |target| {
            if let Some((root, target)) = target {
                state.diychef_watch.watch(root, target, move |payload| {
                    if let Err(error) =
                        app.emit(crate::diychef::watch::FILES_CHANGED_EVENT, payload)
                    {
                        log::debug!("发送暖暖文件变化事件失败：{error}");
                    }
                })
            } else {
                state.diychef_watch.stop();
                0
            }
        })
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

#[tauri::command]
pub async fn delete_diychef_resource(
    state: State<'_, AppState>,
    session_id: String,
    chef_id: String,
) -> Result<ChefEditorSnapshot, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        session::delete_resource(&state, &session_id, &chef_id)
    })
    .await
    .map_err(|e| e.to_string())?
}
