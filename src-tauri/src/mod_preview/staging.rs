use super::analysis::{
    analyze_dll, analyze_zip, apply_repository_origin, build_preview, source_extension,
};
use super::conflicts::attach_install_conflicts;
use super::model::ModPreview;
use crate::app_state::AppState;
use crate::archive::release_download::DownloadedRelease;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// 预览暂存目录名（位于 `cache_dir()` 下）。
const PREVIEW_DIR: &str = "mod-preview";
/// 暂存有效期：超过这个时间的目录在启动时或新建预览时被清理。
const PREVIEW_TTL: Duration = Duration::from_secs(60 * 60);

pub(crate) fn preview_root(state: &AppState) -> PathBuf {
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
/// 把用户选择的源文件复制到暂存目录，保持原件不变。
pub(super) fn stage_local(
    state: &AppState,
    preview: &ModPreview,
    source: &Path,
) -> Result<(), String> {
    stage_file(state, preview, |_, target| {
        fs::copy(source, target)
            .map(|_| ())
            .map_err(|error| error.to_string())
    })
}

fn stage_file(
    state: &AppState,
    preview: &ModPreview,
    place: impl FnOnce(&Path, &Path) -> Result<(), String>,
) -> Result<(), String> {
    let name = &preview.file_name;
    if name.trim().is_empty()
        || name == "."
        || name == ".."
        || name.contains(['/', '\\', ':', '\0'])
    {
        return Err("source.fileNameUnsafe".into());
    }
    let root = preview_root(state);
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let dir = tempfile::Builder::new()
        .prefix(&preview.preview_id)
        .rand_bytes(0)
        .tempdir_in(root)
        .map_err(|error| error.to_string())?;
    // Keep only complete previews. On failure this guard removes this new directory.
    place(dir.path(), &dir.path().join(name))?;
    let _ = dir.keep();
    Ok(())
}

/// 预览暂存目录里的来源元数据文件名。
pub(crate) const ORIGIN_FILE: &str = "origin.json";

/// 仓库来源的预览在暂存目录里留一份来源记录。
///
/// 链式计划只接受预览 id；来源记录把下载时的仓库、tag 与预发布身份
/// 绑定到暂存文件，供实际包校验再次核对，避免被当作无来源的手动包。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PreviewOriginRecord {
    pub(crate) repository_id: String,
    #[serde(default)]
    pub(crate) release_tag: String,
    #[serde(default)]
    pub(crate) prerelease: bool,
}

fn write_origin_record(dir: &Path, record: &PreviewOriginRecord) -> Result<(), String> {
    let text = serde_json::to_string_pretty(record).map_err(|error| error.to_string())?;
    fs::write(dir.join(ORIGIN_FILE), text).map_err(|error| error.to_string())
}

/// 把「已经下载好的仓库资产」直接落进预览暂存并解析。
///
/// 仓库安装流程的下载段包含代理选择与多源竞速，不适合在预览里重写一遍，
/// 所以由 `install_mod_repository` 在 `preview_only` 模式下先完成下载，
/// 再把临时文件交给这里解析。链式计划按同一预览 id 校验、准备与提交。
pub(crate) fn stage_downloaded_file(
    state: &AppState,
    source: DownloadedRelease,
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
        crate::concurrency::workers_from(state.config.load_or_default().mod_metadata_concurrency);
    let mut preview = if extension == "dll" {
        build_preview(vec![analyze_dll(source.path(), file_name)], file_name)
    } else {
        analyze_zip(source.path(), file_name, workers)?
    };
    apply_repository_origin(&mut preview, repository_name, release_tag, release_body);
    attach_install_conflicts(state, &mut preview, Some((repository_id, release_tag)));
    stage_file(state, &preview, |dir, target| {
        write_origin_record(
            dir,
            &PreviewOriginRecord {
                repository_id: repository_id.to_string(),
                release_tag: release_tag.to_string(),
                prerelease,
            },
        )?;
        // Both locations are in the data cache. Transfer ownership without
        // another full ZIP copy on top of the persistent cache.
        fs::rename(source.path(), target).map_err(|error| error.to_string())
    })?;
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
