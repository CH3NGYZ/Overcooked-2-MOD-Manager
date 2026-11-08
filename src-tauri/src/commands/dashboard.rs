use crate::app_state::AppState;
use crate::game;
use crate::models::{DashboardData, InstalledMod, UpdateStatus};
use crate::mods;
use crate::mods::status::collect_update_status;
use std::path::Path;
use tauri::State;

#[tauri::command]
pub async fn get_dashboard(state: State<'_, AppState>) -> Result<DashboardData, String> {
    // Game detection may touch disk; keep the IPC thread available.
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || get_dashboard_blocking(&state))
        .await
        .map_err(|error| error.to_string())?
}

/// [`get_dashboard`] 的同步实现（在阻塞线程池里跑）。
fn get_dashboard_blocking(state: &AppState) -> Result<DashboardData, String> {
    log::debug!("读取状态总览");
    let config = state.config.load_or_default();
    let game_info = game::detect_game(&config);
    *state.game_state.lock().map_err(|error| error.to_string())? = Some(game_info.clone());
    Ok(DashboardData {
        config: state.config.public_config(&config),
        game: game_info,
    })
}

/// Local DLL scanning is independent of online configuration and game summary.
#[tauri::command]
pub async fn get_local_mods(state: State<'_, AppState>) -> Result<Vec<InstalledMod>, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let config = state.config.load_or_default();
        let game_info = game::detect_game(&config);
        Ok(game_info
            .path
            .as_ref()
            .map(|path| {
                mods::scan_mods_with_manifests(
                    Path::new(path),
                    &config,
                    Some(&state.config.manifests_dir()),
                )
            })
            .unwrap_or_default())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Compare the already scanned local snapshot after online refresh settles.
/// No DLL scan and no network request; this command only returns display data.
#[tauri::command]
pub fn get_mod_updates(
    state: State<'_, AppState>,
    installed: Vec<InstalledMod>,
) -> Vec<UpdateStatus> {
    collect_update_status(&state.config.load_or_default(), &installed)
}
