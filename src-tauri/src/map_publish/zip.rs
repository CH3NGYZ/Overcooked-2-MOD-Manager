#[path = "../map_publish_zip_cache.rs"]
pub(super) mod zip_cache;

use super::http::action_error;
use super::http::request_error;
use super::validation::invalid_asset;
use super::validation::validate_asset_name;
use crate::app_state::AppState;
use crate::display_path::display_path;
use crate::models::PublishZipInspection;
use std::collections::BTreeMap;
use std::io::Read;
use tauri::State;

/// 本地识别和上传共用的附件上限；流式上传不意味着可以绕过识别边界。
pub(super) const MAX_ASSET_BYTES: u64 = 256 * 1024 * 1024;

/// 保留识别 DTO 的文件名版本提示，不能把它当作包内版本。
/// 发布上传命名使用 info_version；这里优先最后一段多段数字，否则取最后一段整数。
fn version_from_file_name(stem: &str) -> String {
    let mut runs: Vec<String> = Vec::new();
    let mut current = String::new();
    for ch in stem.chars() {
        if ch.is_ascii_digit() || ch == '.' {
            current.push(ch);
        } else if !current.is_empty() {
            runs.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        runs.push(current);
    }
    let valid = |run: &String| {
        let trimmed = run.trim_matches('.');
        !trimmed.is_empty()
            && trimmed
                .split('.')
                .all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit()))
    };
    runs.iter()
        .rev()
        .find(|run| valid(run) && run.trim_matches('.').contains('.'))
        .or_else(|| runs.iter().rev().find(|run| valid(run)))
        .map(|run| run.trim_matches('.').to_string())
        .unwrap_or_default()
}

/// 从 ZIP 里读关卡集根目录的 `requires.txt`：第一行有效内容是最低 runtime 版本。
///
/// 与已安装目录同一条判定（`level_sets::read_required_runtime` 只看集目录根）：
/// 只认相对路径恰好等于 `requires.txt` 的条目，子目录里的同名文件不算。
fn zip_required_runtime(path: &std::path::Path, files: &[(String, String)]) -> Option<String> {
    let entry_name = files
        .iter()
        .find(|(_, relative)| relative.eq_ignore_ascii_case("requires.txt"))?
        .0
        .clone();
    let file = std::fs::File::open(path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    let mut entry = archive.by_name(&entry_name).ok()?;
    let mut text = String::new();
    entry.read_to_string(&mut text).ok()?;
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
}

fn inspect_publish_zip_blocking(path: &std::path::Path) -> Result<PublishZipInspection, String> {
    if !path.is_file() {
        return Err("mapPublish.zipUnreadable".to_string());
    }
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string();
    let stem = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    // 发布页只需要一张图的身份与版本；合集 ZIP 也先全部识别出来，让界面能
    // 明确告诉作者「这个包里有多张图」，而不是悄悄挑第一张。
    let sets = crate::level_sets::inspect_zip_with_progress(
        path,
        crate::concurrency::workers_from(crate::concurrency::AUTO_CONCURRENCY),
        crate::concurrency::workers_from(crate::concurrency::AUTO_CONCURRENCY),
        &|_, _, _| {},
    )?;
    let first = sets
        .first()
        .ok_or_else(|| "mapPublish.zipNoLevelSet".to_string())?;
    let requires = zip_required_runtime(path, &first.files)
        .and_then(|min_version| crate::level_sets::runtime_relation(&min_version))
        .map(|relation| {
            let mut table = BTreeMap::new();
            table.insert(relation.guid, relation.version);
            table
        })
        .unwrap_or_default();
    Ok(PublishZipInspection {
        file_size: std::fs::metadata(path)
            .map_err(|_| "mapPublish.zipUnreadable".to_string())?
            .len(),
        file_name,
        mapset_version: version_from_file_name(stem),
        info_version: first.metadata.version.clone(),
        level_set_name: first.metadata.name.clone(),
        level_set_name_zh: first.metadata.name_zh.clone().unwrap_or_default(),
        author: first.metadata.author.clone(),
        level_count: first.metadata.levels.len(),
        file_count: first.files.len(),
        set_count: sets.len(),
        requires,
    })
}

/// 选择或拖入地图 ZIP 时本地识别，不要求已有 Release。
///
/// 会解析 `info`（UnityFS + LZMA），属于重活，必须走 `spawn_blocking`：
/// 同步命令会冻住主线程（见 `AGENTS.md` 第 5 节）。
#[tauri::command]
pub async fn inspect_publish_zip(path: String) -> Result<PublishZipInspection, String> {
    let path = std::path::PathBuf::from(path.trim());
    tauri::async_runtime::spawn_blocking(move || zip_cache::inspect(&path))
        .await
        .map_err(|error| request_error(&error.to_string()))?
}

/// Release 附件暂存目录（数据根下 `cache/publish-staging`）。
const PUBLISH_STAGING_DIR: &str = "publish-staging";

/// 把作者选中的本地 ZIP 复制成 Release 附件该用的名字。
///
/// 独立复制入口保留原文件；发布页面上传直接读源文件并指定远端附件名，
/// 不依赖这份暂存副本。
#[tauri::command]
pub async fn stage_publish_zip(
    state: State<'_, AppState>,
    path: String,
    file_name: String,
) -> Result<String, String> {
    // `State` 不能跨 await，先把要用的路径取出来再进阻塞线程池（几十 MB 的
    // 复制是 IO 活，同步命令会冻住主线程，见 `AGENTS.md` 第 5 节）
    let staging = state.config.cache_dir().join(PUBLISH_STAGING_DIR);
    tauri::async_runtime::spawn_blocking(move || {
        stage_publish_zip_into(&staging, &path, &file_name)
    })
    .await
    .map_err(|error| request_error(&error.to_string()))?
}

/// [`stage_publish_zip`] 的同步实现（拆出来才能在临时目录上测）。
fn stage_publish_zip_into(
    staging: &std::path::Path,
    path: &str,
    file_name: &str,
) -> Result<String, String> {
    let file_name = validate_asset_name(file_name)?;
    // 复制入口只收 ZIP 扩展名；内容识别仍由独立的解析入口负责。
    if !file_name.to_ascii_lowercase().ends_with(".zip") {
        return Err(invalid_asset("extension"));
    }
    let source = std::path::PathBuf::from(path.trim());
    let is_zip = source
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"));
    if !is_zip || !source.is_file() {
        log::warn!(
            "暂存发布附件失败：源文件不是可读的 ZIP，路径={}",
            display_path(&source)
        );
        return Err("mapPublish.zipUnreadable".to_string());
    }
    std::fs::create_dir_all(staging).map_err(|error| {
        log::error!(
            "创建发布附件暂存目录失败：目录={}，原因={error}",
            display_path(staging)
        );
        action_error("stage", &error.to_string())
    })?;
    // 每次使用独立目录，重试或源文件本身在暂存目录时也不会覆盖原文件。
    let staging = staging.join(uuid::Uuid::new_v4().simple().to_string());
    std::fs::create_dir(&staging).map_err(|error| action_error("stage", &error.to_string()))?;
    let target = staging.join(&file_name);
    std::fs::copy(&source, &target).map_err(|error| {
        log::error!(
            "复制发布附件失败：源={}，目标={}，原因={error}",
            display_path(&source),
            display_path(&target)
        );
        action_error("stage", &error.to_string())
    })?;
    log::info!("发布附件已暂存：目标={}", display_path(&target));
    Ok(target.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests;
