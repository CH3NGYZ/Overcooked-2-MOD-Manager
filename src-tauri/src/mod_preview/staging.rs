use super::analysis::{
    analyze_dll, analyze_zip, apply_repository_origin, build_preview, source_extension,
};
use super::conflicts::attach_install_conflicts;
use super::model::ModPreview;
use crate::app_state::AppState;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// 预览暂存目录名（位于 `cache_dir()` 下）。
const PREVIEW_DIR: &str = "mod-preview";
/// 暂存有效期：超过这个时间的目录在启动时或新建预览时被清理。
const PREVIEW_TTL: Duration = Duration::from_secs(60 * 60);

pub(super) fn preview_root(state: &AppState) -> PathBuf {
    state.config.cache_dir().join(PREVIEW_DIR)
}

/// 清理过期暂存目录（启动时和每次新建预览时调用）。
pub(crate) fn cleanup_stale_previews(state: &AppState) {
    let root = preview_root(state);
    let Ok(entries) = fs::read_dir(&root) else {
        return;
    };
    let now = SystemTime::now();
    let mut removed = 0usize;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let expired = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age > PREVIEW_TTL);
        if expired && fs::remove_dir_all(&path).is_ok() {
            removed += 1;
        }
    }
    if removed > 0 {
        log::info!("清理过期的 MOD 安装预览暂存：数量={removed}");
    }
}
/// 把源文件复制/下载到暂存目录，返回暂存里的文件路径。
pub(super) fn stage_local(
    state: &AppState,
    preview: &ModPreview,
    source: &Path,
) -> Result<PathBuf, String> {
    let dir = preview_root(state).join(&preview.preview_id);
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let target = dir.join(&preview.file_name);
    fs::copy(source, &target).map_err(|error| error.to_string())?;
    Ok(target)
}

/// 预览暂存目录里的来源元数据文件名。
pub(super) const ORIGIN_FILE: &str = "origin.json";

/// 仓库来源的预览在暂存目录里留一份来源记录。
///
/// `confirm_mod_install` 只拿到 `preview_id`，看不到当初是哪个仓库下的包。
/// 若不加这份记录，确认安装只能走 `install_manual_mod` —— 那条路每次生成
/// 新的 `manual-<uuid>` 清单，于是同仓库的旧包永远不会被回收，装完就是
/// 「同一个仓库三份副本」。有了它，确认时才能回到仓库安装路径去先回收。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PreviewOriginRecord {
    pub(super) repository_id: String,
    #[serde(default)]
    pub(super) release_tag: String,
    #[serde(default)]
    pub(super) prerelease: bool,
}

fn write_origin_record(
    state: &AppState,
    preview_id: &str,
    record: &PreviewOriginRecord,
) -> Result<(), String> {
    let dir = preview_root(state).join(preview_id);
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let text = serde_json::to_string_pretty(record).map_err(|error| error.to_string())?;
    fs::write(dir.join(ORIGIN_FILE), text).map_err(|error| error.to_string())
}

/// 读取预览的来源记录；手动来源（本地文件 / 直链）没有这份文件。
pub(super) fn read_origin_record(
    state: &AppState,
    preview_id: &str,
) -> Option<PreviewOriginRecord> {
    let path = preview_root(state).join(preview_id).join(ORIGIN_FILE);
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}
/// 把「已经下载好的仓库资产」直接落进预览暂存并解析。
///
/// 仓库安装流程的下载段包含代理选择与多源竞速，不适合在预览里重写一遍，
/// 所以由 `install_mod_repository` 在 `preview_only` 模式下先完成下载，
/// 再把临时文件交给这里解析。之后 `confirm_mod_install` 就能像处理
/// 手动安装一样安装它。
pub(crate) fn stage_downloaded_file(
    state: &AppState,
    source: &Path,
    file_name: &str,
    repository_id: &str,
    repository_name: &str,
    release_tag: &str,
    release_body: &str,
    prerelease: bool,
) -> Result<ModPreview, String> {
    cleanup_stale_previews(state);
    let extension = source_extension(file_name).ok_or("modPreview.unknownType")?;
    if extension != "dll" && extension != "zip" {
        return Err("modPreview.unsupportedType".into());
    }
    let workers =
        crate::metadata_read::workers_from(state.config.load_or_default().mod_metadata_concurrency);
    let mut preview = if extension == "dll" {
        build_preview(vec![analyze_dll(source, file_name)], file_name)
    } else {
        analyze_zip(source, file_name, workers)?
    };
    apply_repository_origin(&mut preview, repository_name, release_tag, release_body);
    attach_install_conflicts(state, &mut preview);
    stage_local(state, &preview, source)?;
    // 记录来源：确认安装时要回到仓库安装路径，才知道回收哪个仓库的旧包
    write_origin_record(
        state,
        &preview.preview_id,
        &PreviewOriginRecord {
            repository_id: repository_id.to_string(),
            release_tag: release_tag.to_string(),
            prerelease,
        },
    )?;
    log::info!(
        "仓库资产已送入预览暂存：预览={}，仓库={}，DLL 数={}，MOD 数={}，依赖数={}",
        preview.preview_id,
        repository_id,
        preview.total_dlls,
        preview.mod_count,
        preview.dependency_count
    );
    Ok(preview)
}
