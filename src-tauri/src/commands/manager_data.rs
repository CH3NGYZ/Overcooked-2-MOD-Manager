use crate::app_state::AppState;
use tauri::State;

#[tauri::command]
pub fn get_manager_data_directory() -> Result<String, String> {
    crate::user_paths::root().map(|root| crate::display_path::display_path(&root))
}

#[tauri::command]
pub async fn clear_all_config(
    state: State<'_, AppState>,
    locale: Option<String>,
) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::manager_data::clear_and_exit(&state, locale)
    })
    .await
    .map_err(|error| {
        crate::locale_tag::with_params(
            "tools.clearAllConfigFailed",
            &[("reason", &error.to_string())],
        )
    })?
}

#[tauri::command]
pub async fn migrate_manager_data(
    state: State<'_, AppState>,
    parent_directory: String,
    locale: Option<String>,
) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::manager_data::migrate_and_exit(
            &state,
            std::path::Path::new(&parent_directory),
            locale,
        )
    })
    .await
    .map_err(|error| {
        crate::locale_tag::with_params("tools.migrateDataFailed", &[("reason", &error.to_string())])
    })?
}
