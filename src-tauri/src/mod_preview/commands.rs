use super::analysis::{analyze_dll, analyze_zip, build_preview, source_extension};
use super::conflicts::attach_install_conflicts;
use super::model::{ModPreview, ModSource, ModSourceKind};
use super::staging::{
    ORIGIN_FILE, cleanup_stale_previews, preview_root, read_origin_record, stage_local,
};
use crate::app_state::AppState;
use crate::models::OperationResult;
use std::fs;
use std::path::PathBuf;
use tauri::State;
use uuid::Uuid;

#[tauri::command]
pub async fn analyze_mod_source(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    source: ModSource,
) -> Result<ModPreview, String> {
    cleanup_stale_previews(&state);
    let (local_path, file_name) = match source.kind {
        ModSourceKind::Local => {
            let path = PathBuf::from(source.value.trim());
            if !path.is_file() {
                return Err("mods.fileMissing".into());
            }
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or("mods.invalidFileName")?
                .to_string();
            (path, name)
        }
        ModSourceKind::Url => {
            let (path, name) =
                crate::direct_mod::download_for_preview(&app, &state, &source.value).await?;
            (path, name)
        }
    };
    let extension = source_extension(&file_name).ok_or("modPreview.unknownType")?;
    if extension != "dll" && extension != "zip" {
        return Err("modPreview.unsupportedType".into());
    }

    log::info!(
        "开始解析 MOD 安装源：类型={:?}，文件名={}",
        source.kind,
        file_name
    );
    let workers =
        crate::metadata_read::workers_from(state.config.load_or_default().mod_metadata_concurrency);
    let mut preview = if extension == "dll" {
        build_preview(vec![analyze_dll(&local_path, &file_name)], &file_name)
    } else {
        analyze_zip(&local_path, &file_name, workers)?
    };
    attach_install_conflicts(&state, &mut preview);

    // 暂存目录用 preview_id 命名，confirm / cancel 靠它找回文件。
    stage_local(&state, &preview, &local_path)?;
    if matches!(source.kind, ModSourceKind::Url) {
        // 在线源的原始下载文件已经复制进暂存，删掉避免占用 cache。
        let _ = fs::remove_file(&local_path);
    }
    log::info!(
        "MOD 安装源解析完成：预览={}，DLL 数={}，MOD 数={}，依赖数={}",
        preview.preview_id,
        preview.total_dlls,
        preview.mod_count,
        preview.dependency_count
    );
    Ok(preview)
}

#[tauri::command]
pub fn cancel_mod_preview(state: State<'_, AppState>, preview_id: String) -> Result<(), String> {
    let dir = preview_root(&state).join(safe_preview_id(&preview_id)?);
    if dir.is_dir() {
        fs::remove_dir_all(&dir).map_err(|error| error.to_string())?;
        log::debug!("已清理 MOD 安装预览暂存：预览={preview_id}");
    }
    Ok(())
}

#[tauri::command]
pub async fn confirm_mod_install(
    state: State<'_, AppState>,
    preview_id: String,
) -> Result<OperationResult, String> {
    /*
     * 确认安装要做两件重活：把暂存的安装包解压进游戏目录（几百 MB 按秒计），
     * 以及按仓库身份回收旧包。同步命令会在 IPC 线程上跑完这两件事，窗口整段
     * 不响应；搬到阻塞线程池后耗时不变，界面仍可交互。
     */
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || confirm_mod_install_blocking(&state, &preview_id))
        .await
        .map_err(|error| error.to_string())?
}

/// [`confirm_mod_install`] 的同步实现（在阻塞线程池里跑）。
fn confirm_mod_install_blocking(
    state: &AppState,
    preview_id: &str,
) -> Result<OperationResult, String> {
    let dir = preview_root(&state).join(safe_preview_id(&preview_id)?);
    let file = fs::read_dir(&dir)
        .map_err(|_| "modPreview.expired".to_string())?
        .flatten()
        .map(|entry| entry.path())
        // 跳过来源元数据：它和安装包放在同一个暂存目录里
        .find(|path| path.is_file() && path.file_name().is_some_and(|name| name != ORIGIN_FILE))
        .ok_or("modPreview.expired")?;
    log::info!("确认安装预览文件：预览={preview_id}");

    /*
     * 仓库来源的预览必须走仓库安装路径。
     *
     * 走 install_manual_mod 会给包分配新的 `manual-<uuid>` 清单，
     * 于是同仓库的旧包永远不会被回收 —— 用户装三次就得到三份副本。
     * 有来源记录时改走 install_staged_mod_repository，它会先按仓库身份
     * 回收所有旧包再解压新文件。
     */
    let result = match read_origin_record(&state, &preview_id) {
        Some(origin) => {
            log::info!(
                "确认安装仓库资产：预览={}，仓库={}，版本={}",
                preview_id,
                origin.repository_id,
                origin.release_tag
            );
            crate::mods::install::install_staged_mod_repository(
                &state,
                &file,
                &origin.repository_id,
                &origin.release_tag,
                origin.prerelease,
            )
        }
        None => crate::mods::install::install_manual_mod(&state, &file, None),
    };
    let _ = fs::remove_dir_all(&dir);
    result
}

/// 预览 id 必须是合法的 UUID，避免拼出跨目录路径。
pub(super) fn safe_preview_id(value: &str) -> Result<String, String> {
    let parsed = Uuid::parse_str(value.trim()).map_err(|_| "modPreview.invalidId".to_string())?;
    Ok(parsed.to_string())
}
