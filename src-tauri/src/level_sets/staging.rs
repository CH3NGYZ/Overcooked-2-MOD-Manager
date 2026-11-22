use crate::models::{AppConfig, LevelSetZipProgress, MapRepositoryConfig, ZipLevelSet};
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri::Emitter;

/// ZIP 暂存只识别包内地图；本地归属由整链规划检查。
pub(crate) fn zip_level_sets(
    app: &AppHandle,
    config: &AppConfig,
    path: &Path,
) -> Result<Vec<ZipLevelSet>, String> {
    crate::level_sets::require_map_only_zip(path)?;
    // `info` 是地图线、随包插件是 DLL（MOD 线），两条线各有自己的并行度设置。
    let progress_app = app.clone();
    let sets = crate::level_sets::inspect_zip_with_progress(
        path,
        crate::concurrency::workers_from(config.map_info_concurrency),
        crate::concurrency::workers_from(config.mod_metadata_concurrency),
        &move |done, total, current| {
            let _ = progress_app.emit(
                "level-set-zip-progress",
                LevelSetZipProgress {
                    phase: "info".into(),
                    done,
                    total,
                    current: current.to_string(),
                },
            );
        },
    )?;
    Ok(sets
        .into_iter()
        .map(|set| ZipLevelSet {
            dir_name: set.dir_name,
            layout: set.layout,
            name: set.metadata.name,
            name_zh: set.metadata.name_zh,
            author: set.metadata.author,
            version: set.metadata.version,
            level_count: set.metadata.levels.len(),
            file_count: set.files.len(),
            plugins: set.plugins,
        })
        .collect())
}

/// 校验「这个路径可以当作暂存文件丢弃」，返回解析后的真实路径。
///
/// 只接受下载器直接放在缓存根的 UUID.download / 分卷合并 UUID.zip，
/// 不能删除压缩恢复材料或其它缓存。`None` 表示文件已经不在（重复清理）。
pub(crate) fn cache_file_to_discard(
    cache_dir: &Path,
    path: &str,
) -> Result<Option<PathBuf>, String> {
    let Ok(target) = fs::canonicalize(path.trim()) else {
        return Ok(None);
    };
    let cache_dir = fs::canonicalize(cache_dir).map_err(|_| "levelSet.outsideCache".to_string())?;
    let generated_download = target
        .file_stem()
        .and_then(|name| name.to_str())
        .and_then(|name| {
            uuid::Uuid::parse_str(name)
                .ok()
                .map(|id| id.to_string() == name)
        })
        .unwrap_or(false)
        && target
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| matches!(extension, "download" | "zip"));
    if !target.is_file() || target.parent() != Some(cache_dir.as_path()) || !generated_download {
        return Err("levelSet.outsideCache".into());
    }
    Ok(Some(target))
}

/// 校验在线地图 Release ZIP 是否包含与仓库声明身份匹配的主地图。
///
/// ZIP 可以额外携带前置地图或其它关卡集：它们保留在同一导入批次中，后续按各自
/// 身份处理冲突；只有主地图必须与仓库声明相符，以免更新绑定错位。
fn map_release_set_matches(set: &ZipLevelSet, map: &MapRepositoryConfig) -> bool {
    crate::level_sets::level_set_identity_matches(
        &set.name,
        set.name_zh.as_deref(),
        &set.author,
        &map.level_set_name,
        Some(map.level_set_name_zh.as_str()).filter(|value| !value.is_empty()),
        &map.author,
    )
}

pub(crate) fn validate_map_release_sets<'a>(
    sets: &'a [ZipLevelSet],
    map: &MapRepositoryConfig,
) -> Result<&'a ZipLevelSet, String> {
    if sets.is_empty() {
        return Err("levelSet.zipWithoutInfo".into());
    }
    if let Some(primary) = sets.iter().find(|set| map_release_set_matches(set, map)) {
        return Ok(primary);
    }
    let set = &sets[0];
    log::warn!(
        "关卡集与仓库声明不一致：地图={}，期望={} / {}，实际={} / {}",
        map.name,
        map.level_set_name,
        map.author,
        set.name,
        set.author
    );
    Err(crate::locale_tag::with_params(
        "levelSet.zipMismatch",
        &[
            (
                "expected",
                &format!("{} / {}", map.level_set_name, map.author),
            ),
            ("actual", &format!("{} / {}", set.name, set.author)),
        ],
    ))
}

#[cfg(test)]
mod map_source_staging_tests;
