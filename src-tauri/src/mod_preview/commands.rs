use super::analysis::{analyze_dll, analyze_zip, build_preview, source_extension};
use super::conflicts::attach_install_conflicts;
use super::model::{ModPreview, ModSource, ModSourceKind};
use super::staging::{cleanup_stale_previews, preview_root, stage_local};
use crate::app_state::AppState;
use crate::mod_changes::{downloaded_source::Source, transaction::Fingerprint};
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
    let downloaded = matches!(source.kind, ModSourceKind::Url)
        .then(|| Fingerprint::capture(&local_path))
        .transpose()?;
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
        crate::concurrency::workers_from(state.config.load_or_default().mod_metadata_concurrency);
    let mut preview = if extension == "dll" {
        build_preview(vec![analyze_dll(&local_path, &file_name)], &file_name)
    } else {
        analyze_zip(&local_path, &file_name, workers)?
    };
    attach_install_conflicts(&state, &mut preview, None);

    // 暂存目录用 preview_id 命名，confirm / cancel 靠它找回文件。
    stage_local(&state, &preview, &local_path)?;
    if let Some(downloaded) = downloaded {
        let path = preview_root(&state)
            .join(&preview.preview_id)
            .join(&preview.file_name);
        let fingerprint = Fingerprint::capture(&path)?;
        let _ = fs::remove_file(&local_path);
        if !fingerprint.same_content(&downloaded) {
            return Err("modChanges.payloadChanged".into());
        }
        state.downloaded_sources.record(
            Uuid::parse_str(&preview.preview_id).map_err(|_| "modPreview.invalidId")?,
            &path,
            fingerprint,
            Source::ModUrl,
        )?;
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
    state.downloaded_sources.forget_path(&dir);
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
    let _ = (state, preview_id);
    crate::mod_changes::guard::require_shared_plan()
}

/// 预览 id 必须是合法的 UUID，避免拼出跨目录路径。
pub(super) fn safe_preview_id(value: &str) -> Result<String, String> {
    let parsed = Uuid::parse_str(value.trim()).map_err(|_| "modPreview.invalidId".to_string())?;
    Ok(parsed.to_string())
}
