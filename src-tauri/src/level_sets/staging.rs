use crate::game;
use crate::models::{AppConfig, LevelSetZipProgress, MapRepositoryConfig, ZipLevelSet};
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri::Emitter;

/// 把 ZIP 里识别出的关卡集转成 IPC 模型（冲突列表依赖当前游戏目录）。
///
/// `stage_map_source`（本地 / 直链地图来源）与 `stage_map_release`（在线地图
/// Release）必须给出完全相同的行，否则同一个 ZIP 在不同入口上的「会卸载本地
/// 旧副本」提示会不一致。
///
/// 识别期间逐份发 `level-set-zip-progress`：一个上百 MB 的合集在 debug 构建
/// 里要几十秒，界面只有转圈的话用户判断不了是在跑还是卡死。
///
/// 冲突是**批次级**的：它回答「这次导入会与本地哪些副本撞身份」，与批次里
/// 具体是哪一张无关，所以同一份列表挂到每个 `ZipLevelSet` 上，界面按
/// `handle` 去重展示（单集 ZIP 与逐集拆分完全等价）。
pub(crate) fn zip_level_sets(
    app: &AppHandle,
    config: &AppConfig,
    path: &Path,
) -> Result<Vec<ZipLevelSet>, String> {
    // `info` 是地图线、随包插件是 DLL（MOD 线），两条线各有自己的并行度设置。
    let progress_app = app.clone();
    let sets = crate::level_sets::inspect_zip_with_progress(
        path,
        crate::metadata_read::workers_from(config.map_info_concurrency),
        crate::metadata_read::workers_from(config.mod_metadata_concurrency),
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
    let game_root = game::detect_game(config).path;
    let conflicts = game_root
        .as_deref()
        .map(|root| {
            crate::level_sets::find_install_conflicts_with_workers(
                Path::new(root),
                &sets,
                crate::metadata_read::workers_from(config.map_info_concurrency),
            )
        })
        .unwrap_or_default();
    Ok(sets
        .into_iter()
        .map(|set| ZipLevelSet {
            conflicts: conflicts.clone(),
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
/// 只接受 `cache_dir` 之内的普通文件；`None` 表示文件已经不在（重复清理、
/// 或安装流程自己搬走了），调用方按成功处理。路径来自前端，必须先
/// canonicalize 再比较 —— 否则 `..`、符号链接、目录联接都能绕过前缀判断。
pub(crate) fn cache_file_to_discard(
    cache_dir: &Path,
    path: &str,
) -> Result<Option<PathBuf>, String> {
    let Ok(target) = fs::canonicalize(path.trim()) else {
        return Ok(None);
    };
    let cache_dir = fs::canonicalize(cache_dir).map_err(|_| "levelSet.outsideCache".to_string())?;
    if !target.is_file() || !target.starts_with(&cache_dir) {
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
