use crate::app_state::AppState;
use crate::game;
use crate::models::DashboardData;
use crate::mods;
use crate::mods::status::collect_update_status;
use std::path::Path;
use tauri::State;

#[tauri::command]
pub async fn get_dashboard(state: State<'_, AppState>) -> Result<DashboardData, String> {
    /*
     * 第一次扫描要读全部 DLL 的元数据（之后走进程内缓存），debug 构建下能到
     * 1 s 以上；同步命令在 IPC 线程上内联执行，这一段就是窗口卡死。搬到
     * 阻塞线程池后耗时不变，但界面还能转圈、拖窗口。
     */
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
    let mods = game_info
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
    let updates = collect_update_status(&config, &mods);
    log::info!(
        "状态总览读取完成：已安装 MOD={}，更新状态={}，游戏已检测={}",
        mods.len(),
        updates.len(),
        game_info.path.is_some()
    );
    Ok(DashboardData {
        config: state.config.public_config(&config),
        game: game_info,
        mods,
        updates,
    })
}
