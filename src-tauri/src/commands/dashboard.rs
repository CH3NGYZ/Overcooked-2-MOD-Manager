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
pub async fn get_local_mods(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<crate::models::LocalModSnapshot, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let config = state.config.load_or_default();
        let game_info = game::detect_game(&config);
        let scan_root = game_info
            .path
            .as_deref()
            .map(Path::new)
            .map(game::resolve_existing_ancestor)
            .transpose()?;
        let generation = {
            let _operation = state
                .operation
                .lock()
                .map_err(|_| "lock.poisoned".to_string())?;
            if state.config.load_or_default().game != config.game {
                return Err("diychef.staleScan".into());
            }
            state
                .local_mod_snapshot
                .lock()
                .map_err(|e| e.to_string())?
                .begin(scan_root.as_deref())
        };
        let installed = game_info
            .path
            .as_ref()
            .map(|path| {
                mods::scan_mods_with_manifests(
                    Path::new(path),
                    &config,
                    Some(&state.config.manifests_dir()),
                )
            })
            .unwrap_or_default();
        let _operation = state
            .operation
            .lock()
            .map_err(|_| "lock.poisoned".to_string())?;
        let latest_config = state.config.load_or_default();
        let latest = game::detect_game(&latest_config);
        if latest_config.game != config.game || latest.path.as_deref() != game_info.path.as_deref()
        {
            return Err("diychef.staleScan".into());
        }
        let installed = reconcile_local_mods(&state, installed);
        let snapshot = state
            .local_mod_snapshot
            .lock()
            .map_err(|e| e.to_string())?
            .publish(generation, scan_root.as_deref(), installed)?;
        drop(_operation);
        crate::level_sets::recompress::watch_selected(&state, &app);
        crate::level_sets::recompress::enqueue_installed(&state, &app, None);
        Ok(snapshot)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Re-associate parsed DLL facts after a source arrives, without rescanning disk.
#[tauri::command]
pub fn rebind_local_mods(
    state: State<'_, AppState>,
    installed: Vec<InstalledMod>,
) -> Vec<InstalledMod> {
    reconcile_local_mods(&state, installed)
}

fn reconcile_local_mods(state: &AppState, mut installed: Vec<InstalledMod>) -> Vec<InstalledMod> {
    // Scans may have started before a new TOML was applied. Always bind against
    // the latest configuration, and update shared API scheduling priorities too.
    let config = state.config.load_or_default();
    mods::rebind_installed_mods(&config, &mut installed);
    crate::api_scheduler::set_installed(
        "mod",
        config
            .repositories
            .iter()
            .filter(|repo| {
                installed.iter().any(|item| {
                    item.repository_id.as_deref() == Some(repo.id.as_str())
                        || item.repository_ids.contains(&repo.id)
                })
            })
            .map(|repo| {
                crate::api_scheduler::repository_key(
                    &repo.provider,
                    &repo.username,
                    &repo.repository,
                )
            }),
    );
    installed
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
