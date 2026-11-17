use crate::app_state::AppState;
use crate::archive;
use crate::game;
use crate::models::{OperationResult, ProxyMode};
use crate::mods::install::mod_repository_install_identity;
use crate::recycle_bin;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri::State;

use crate::mods::repository_download::{ModRepositoryInstallOutcome, run_repository_install};

#[tauri::command]
pub async fn install_mod_repository(
    app: AppHandle,
    state: State<'_, AppState>,
    repository_id: String,
    tag: Option<String>,
    asset_name: Option<String>,
    asset_proxy_mode: Option<ProxyMode>,
    asset_proxy_id: Option<String>,
    preview_only: Option<bool>,
    // 界面语言（`zh-CN` / `en` / …）。用来在下载进度标签与安装预览标题里
    // 显示仓库的本地化名称；缺省时回退到仓库中文名。
    locale: Option<String>,
) -> Result<ModRepositoryInstallOutcome, String> {
    run_repository_install(
        app,
        state.inner().clone(),
        repository_id,
        tag,
        asset_name,
        asset_proxy_mode,
        asset_proxy_id,
        preview_only,
        locale,
        true,
    )
    .await
}

#[tauri::command]
pub async fn uninstall_repository(
    state: State<'_, AppState>,
    repository_id: String,
) -> Result<OperationResult, String> {
    // 回收整包要解析安装清单并移动文件：重活，搬到阻塞线程池
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        uninstall_repository_blocking(&state, &repository_id)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// [`uninstall_repository`] 的同步实现（在阻塞线程池里跑）。
fn uninstall_repository_blocking(
    state: &AppState,
    repository_id: &str,
) -> Result<OperationResult, String> {
    log::info!("开始卸载在线仓库 MOD：仓库={}", repository_id);
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    crate::mod_changes::guard::ensure_recovered(&state)?;
    state.invalidate_local_mod_snapshot()?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;
    let package_id = if let Some(repo) = config
        .repositories
        .iter()
        .find(|repo| repo.id == repository_id)
    {
        mod_repository_install_identity(repo, &config, &state.config.manifests_dir())?.id
    } else {
        repository_id.to_string()
    };
    let manifest = archive::read_manifest(&state.config.manifests_dir(), &package_id)
        .ok_or("mods.unknownFiles")?;
    if config.recycle_deleted_mods {
        let entry = recycle_bin::recycle_package(
            &state.config.mod_recycle_bin_dir(),
            &state.config.manifests_dir(),
            &game_root,
            &manifest.repository_id,
            None,
        )?;
        log::info!(
            "在线仓库 MOD 卸载完成（已回收）：仓库={}，文件数={}",
            repository_id,
            entry.file_count
        );
    } else {
        recycle_bin::purge_package(
            &state.config.mod_recycle_bin_dir(),
            &state.config.manifests_dir(),
            &game_root,
            &manifest.repository_id,
            None,
        )?;
        log::info!(
            "在线仓库 MOD 卸载完成（已直接删除）：仓库={}",
            repository_id
        );
    }
    Ok(OperationResult {
        ok: true,
        message: if config.recycle_deleted_mods {
            "message.mods.packageMovedToRecycleBin".into()
        } else {
            "message.mods.packageDeleted".into()
        },
    })
}
