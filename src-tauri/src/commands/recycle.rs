use crate::app_state::AppState;
use crate::display_path::display_path;
use crate::game;
use crate::models::{ModRecycleEntry, OperationResult};
use crate::recycle_bin;
use std::path::PathBuf;
use tauri::State;

#[tauri::command]
pub fn get_mod_recycle_bin(state: State<'_, AppState>) -> Result<Vec<ModRecycleEntry>, String> {
    log::debug!("读取 MOD 回收站列表请求");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    recycle_bin::list(&state.config.mod_recycle_bin_dir())
        .map(|entries| {
            // 记录里存的是删除时的 canonical 路径，只在送给前端时换成展示形态。
            entries
                .into_iter()
                .map(|mut entry| {
                    entry.game_path = display_path(&entry.game_path);
                    entry
                })
                .collect()
        })
        .map_err(|error| {
            log::warn!("读取 MOD 回收站失败：{}", error);
            error
        })
}

#[tauri::command]
pub async fn restore_recycled_mod(
    state: State<'_, AppState>,
    id: String,
) -> Result<OperationResult, String> {
    // 回收站在数据根、游戏目录在别的盘时，恢复就是复制 + 删除：按 MB 计的重活
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || restore_recycled_mod_blocking(&state, &id))
        .await
        .map_err(|error| error.to_string())?
}

/// [`restore_recycled_mod`] 的同步实现（在阻塞线程池里跑）。
fn restore_recycled_mod_blocking(state: &AppState, id: &str) -> Result<OperationResult, String> {
    log::info!("请求恢复回收站 MOD：ID={}", id);
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    state.invalidate_local_mod_snapshot()?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    let original = recycle_bin::original_game_path(&state.config.mod_recycle_bin_dir(), &id)?;
    game::ensure_game_stopped(&game_root)?;
    game::ensure_game_stopped(&original)?;
    recycle_bin::restore(
        &state.config.mod_recycle_bin_dir(),
        &state.config.manifests_dir(),
        &game_root,
        &id,
    )
    .map_err(|error| {
        log::warn!("恢复回收站 MOD 失败：ID={}，原因={}", id, error);
        error
    })?;
    Ok(OperationResult {
        ok: true,
        message: "recycle.modRestored".into(),
    })
}

#[tauri::command]
pub async fn delete_recycled_mod(
    state: State<'_, AppState>,
    id: String,
) -> Result<OperationResult, String> {
    // 彻底删除是磁盘删除，量大的包要几秒：不能占着 IPC 线程
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || delete_recycled_mod_blocking(&state, &id))
        .await
        .map_err(|error| error.to_string())?
}

/// [`delete_recycled_mod`] 的同步实现（在阻塞线程池里跑）。
fn delete_recycled_mod_blocking(state: &AppState, id: &str) -> Result<OperationResult, String> {
    log::info!("请求彻底删除回收站 MOD：ID={}", id);
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    recycle_bin::delete(&state.config.mod_recycle_bin_dir(), &id).map_err(|error| {
        log::warn!("彻底删除回收站 MOD 失败：ID={}，原因={}", id, error);
        error
    })?;
    Ok(OperationResult {
        ok: true,
        message: "recycle.modPermanentlyDeleted".into(),
    })
}

#[tauri::command]
pub async fn delete_all_recycled_mods(
    state: State<'_, AppState>,
) -> Result<OperationResult, String> {
    // 清空回收站是整目录删除：重活，搬到阻塞线程池
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || delete_all_recycled_mods_blocking(&state))
        .await
        .map_err(|error| error.to_string())?
}

/// [`delete_all_recycled_mods`] 的同步实现（在阻塞线程池里跑）。
fn delete_all_recycled_mods_blocking(state: &AppState) -> Result<OperationResult, String> {
    log::info!("请求清空回收站");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let removed =
        recycle_bin::delete_all(&state.config.mod_recycle_bin_dir()).map_err(|error| {
            log::warn!("清空回收站失败：原因={}", error);
            error
        })?;
    Ok(OperationResult {
        ok: true,
        message: crate::locale_tag::with_params(
            "recycle.emptied",
            &[("count", &removed.to_string())],
        ),
    })
}
