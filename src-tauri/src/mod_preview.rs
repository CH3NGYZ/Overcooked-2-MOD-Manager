//! MOD 安装预览：本地文件 / 在线直链在真正安装之前的「解析 + 确认」阶段。
//!
//! 流程统一为两步，本地与在线共用同一套逻辑：
//!   1. `analyze`：把源文件放到暂存目录，解析里面每个 DLL 的元数据；
//!      在线源在这一步下载，因此「下载只发生一次」。
//!   2. `confirm`：用户在预览里确认后，从暂存目录取出文件走正式安装；
//!      `cancel` 或超时则删除暂存。
//!
//! 判定规则（单 mod 还是包）只看 **ZIP 内的 DLL 数量**：
//!   - 0 个：按包处理，没有可预览的 mod；
//!   - 1 个：单 mod，展示该 DLL 的详情；
//!   - >= 2 个：包，逐个解析，能读出 BepInPlugin 的列为 mod，
//!     读不出的（纯依赖，如 LevelEditorStub）列为一类「依赖」条目。
//! 也就是说「能否解析出 BepInPlugin」只决定它在预览里显示为 mod 还是依赖，
//! 不决定这个 ZIP 是不是包 —— DIYLevel 那种「一个真插件 + 一个纯依赖」
//! 必须被认成包，而不是单 mod。

use crate::commands::AppState;
use crate::game::read_file_metadata;
use crate::models::OperationResult;
use crate::mods::parse_bepin_plugin_metadata;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tauri::State;
use uuid::Uuid;
use zip::ZipArchive;

/// 预览暂存目录名（位于 `cache_dir()` 下）。
const PREVIEW_DIR: &str = "mod-preview";
/// 暂存有效期：超过这个时间的目录在启动时或新建预览时被清理。
const PREVIEW_TTL: Duration = Duration::from_secs(60 * 60);
/// 单个 DLL 允许读取的最大字节数（元数据只在头部，够用即可）。
const MAX_DLL_BYTES: u64 = 32 * 1024 * 1024;

/// 预览条目的角色：真正的 BepInEx 插件，还是纯依赖。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PreviewRole {
    Mod,
    Dependency,
}

/// 预览里的一行（一个 DLL）。
#[derive(Debug, Clone, Serialize)]
pub struct ModPreviewEntry {
    /// 相对安装包根目录的路径，用于展示。
    pub path: String,
    pub role: PreviewRole,
    /// 展示名称：BepInPlugin 名称 → PE 产品名 → 文件名。
    pub name: Option<String>,
    pub guid: Option<String>,
    pub version: Option<String>,
    pub title: Option<String>,
    pub product_name: Option<String>,
    pub company: Option<String>,
    pub copyright: Option<String>,
    pub original_filename: Option<String>,
}

/// 解析结果。
#[derive(Debug, Clone, Serialize)]
pub struct ModPreview {
    /// 后续 `confirm` / `cancel` 用这个 id 找到暂存目录。
    pub preview_id: String,
    /// `single` 表示单 mod，`package` 表示包含多个 DLL 的包。
    pub kind: &'static str,
    /// 安装后作为 MOD 包/单个 MOD 的名称。
    pub package_name: String,
    /// 源文件名（本地路径的文件名，或直链 URL 的最后一段）。
    pub file_name: String,
    /// 包内 DLL 总数。
    pub total_dlls: usize,
    /// 能读出 BepInPlugin 的数量。
    pub mod_count: usize,
    /// 纯依赖数量。
    pub dependency_count: usize,
    /// 逐 DLL 的解析结果（按包内顺序）。
    pub entries: Vec<ModPreviewEntry>,
    /// 来源：`repository` 表示从在线仓库资产下载，`manual` 表示本地文件或直链。
    /// 界面据此决定是否展示 Release 日志区块。
    pub origin: PreviewOrigin,
    /// 仓库安装时本次 Release 的版本标签（如 `v1.2.3`）；手动来源为空。
    pub release_tag: String,
    /// 仓库安装时本次 Release 的提交日志（Markdown）；手动来源为空。
    pub release_body: String,
}

/// 预览内容的来源，决定界面展示哪些来源专属信息。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PreviewOrigin {
    Repository,
    Manual,
}

/// 源类型：本地路径或在线直链。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModSourceKind {
    Local,
    Url,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModSource {
    pub kind: ModSourceKind,
    pub value: String,
}

fn preview_root(state: &AppState) -> PathBuf {
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

/// 判断一个文件是 DLL 还是 ZIP（按扩展名）。
fn source_extension(name: &str) -> Option<String> {
    Path::new(name)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
}

fn display_name_for(name: &str) -> String {
    Path::new(name)
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(name)
        .to_string()
}

/// 解析单个 DLL 的元数据。
///
/// 顺序（与用户确认的规则一致）：
///   1. BepInEx 插件元数据（BepInPlugin）：能读到就是 mod；
///   2. 回退到 PE 版本资源里的产品名/标题；
///   3. 仍然读不到有效 tag（名称+版本）时，认定为「依赖」。
fn analyze_dll(path: &Path, display_path: &str) -> ModPreviewEntry {
    let bytes = File::open(path)
        .ok()
        .and_then(|file| {
            let mut buffer = Vec::new();
            file.take(MAX_DLL_BYTES).read_to_end(&mut buffer).ok()?;
            Some(buffer)
        })
        .unwrap_or_default();
    let plugin = parse_bepin_plugin_metadata(&bytes);
    let metadata = read_file_metadata(path);

    let name = plugin
        .as_ref()
        .and_then(|item| item.name.clone())
        .or_else(|| metadata.product_name.clone())
        .or_else(|| metadata.title.clone())
        .or_else(|| Some(display_name_for(display_path)));
    let version = plugin.as_ref().and_then(|item| item.version.clone());
    let guid = plugin.as_ref().and_then(|item| item.guid.clone());

    // 判定依赖：没有 BepInEx 插件标识（无 GUID），且也没有可比较的版本号。
    let role = if guid.is_none() && version.is_none() {
        PreviewRole::Dependency
    } else {
        PreviewRole::Mod
    };
    if role == PreviewRole::Dependency {
        log::debug!(
            "MOD 预览：判定为依赖（未读出 BepInPlugin 元数据）：路径={}",
            display_path
        );
    }
    ModPreviewEntry {
        path: display_path.to_string(),
        role,
        name,
        guid,
        version,
        title: metadata.title,
        product_name: metadata.product_name,
        company: metadata.company,
        copyright: metadata.copyright,
        original_filename: metadata.original_filename,
    }
}

/// 分析 ZIP：遍历条目，仅解析 DLL（其余文件忽略）。
fn analyze_zip(path: &Path, file_name: &str) -> Result<ModPreview, String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|error| format!("ZIP 读取失败: {error}"))?;
    let mut entries = Vec::new();
    // ZIP 里的 DLL 需要落到临时文件才能读 PE 版本资源。
    // 用一个自建临时目录而不是 tempfile crate（项目没有这个依赖），
    // 分析结束立即删除。
    let scratch = std::env::temp_dir().join(format!("oc2-preview-{}", Uuid::new_v4()));
    fs::create_dir_all(&scratch).map_err(|error| error.to_string())?;
    let result = (|| -> Result<Vec<ModPreviewEntry>, String> {
        for index in 0..archive.len() {
            let mut entry = archive
                .by_index(index)
                .map_err(|error| format!("ZIP 条目读取失败: {error}"))?;
            if entry.is_dir() {
                continue;
            }
            let Some(enclosed) = entry.enclosed_name() else {
                return Err(format!("拒绝不安全 ZIP 路径: {}", entry.name()));
            };
            if !enclosed
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("dll"))
            {
                continue;
            }
            let display = enclosed.to_string_lossy().replace('\\', "/");
            let mut buffer = Vec::new();
            entry
                .by_ref()
                .take(MAX_DLL_BYTES)
                .read_to_end(&mut buffer)
                .map_err(|error| error.to_string())?;
            let temp = scratch.join(format!("{index}.dll"));
            fs::write(&temp, &buffer).map_err(|error| error.to_string())?;
            entries.push(analyze_dll(&temp, &display));
        }
        Ok(entries)
    })();
    let _ = fs::remove_dir_all(&scratch);
    Ok(build_preview(result?, file_name))
}

fn build_preview(entries: Vec<ModPreviewEntry>, file_name: &str) -> ModPreview {
    let total_dlls = entries.len();
    let mod_count = entries
        .iter()
        .filter(|entry| entry.role == PreviewRole::Mod)
        .count();
    let dependency_count = total_dlls - mod_count;
    ModPreview {
        preview_id: Uuid::new_v4().to_string(),
        // 0 或 >= 2 个 DLL 都按包处理：只有恰好 1 个 DLL 才是单 mod。
        kind: if total_dlls == 1 { "single" } else { "package" },
        package_name: display_name_for(file_name),
        file_name: file_name.to_string(),
        total_dlls,
        mod_count,
        dependency_count,
        entries,
        // 默认按手动来源构造；仓库安装会把这两个字段改写为实际 Release 信息。
        origin: PreviewOrigin::Manual,
        release_tag: String::new(),
        release_body: String::new(),
    }
}

/// 把一份预览标记为「仓库来源」，并写入该来源特有的信息。
///
/// 标题用仓库名：资产文件名往往是 `IMGUI.HiDPI.Patcher.Mono-v0.4.4` 这类
/// 技术命名，用户认的是仓库名，所以覆盖掉 `build_preview` 按文件名推出来的
/// `package_name`。仓库名为空时保留原名，避免把标题清空。
///
/// RAW 仓库没有 Release，`release_tag` / `release_body` 为空串，
/// 界面据此隐藏版本与提交日志区块。
fn apply_repository_origin(
    preview: &mut ModPreview,
    repository_name: &str,
    release_tag: &str,
    release_body: &str,
) {
    preview.origin = PreviewOrigin::Repository;
    if !repository_name.trim().is_empty() {
        preview.package_name = repository_name.to_string();
    }
    preview.release_tag = release_tag.to_string();
    preview.release_body = release_body.to_string();
}

/// 把源文件复制/下载到暂存目录，返回暂存里的文件路径。
fn stage_local(state: &AppState, preview: &ModPreview, source: &Path) -> Result<PathBuf, String> {
    let dir = preview_root(state).join(&preview.preview_id);
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let target = dir.join(&preview.file_name);
    fs::copy(source, &target).map_err(|error| error.to_string())?;
    Ok(target)
}

/// 预览暂存目录里的来源元数据文件名。
const ORIGIN_FILE: &str = "origin.json";

/// 仓库来源的预览在暂存目录里留一份来源记录。
///
/// `confirm_mod_install` 只拿到 `preview_id`，看不到当初是哪个仓库下的包。
/// 若不加这份记录，确认安装只能走 `install_manual_mod` —— 那条路每次生成
/// 新的 `manual-<uuid>` 清单，于是同仓库的旧包永远不会被回收，装完就是
/// 「同一个仓库三份副本」。有了它，确认时才能回到仓库安装路径去先回收。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PreviewOriginRecord {
    repository_id: String,
    #[serde(default)]
    release_tag: String,
    #[serde(default)]
    prerelease: bool,
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
fn read_origin_record(state: &AppState, preview_id: &str) -> Option<PreviewOriginRecord> {
    let path = preview_root(state).join(preview_id).join(ORIGIN_FILE);
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// 把「已经下载好的仓库资产」直接落进预览暂存并解析。
///
/// 仓库安装流程的下载段包含代理选择与多源竞速，不适合在预览里重写一遍，
/// 所以由 `install_repository` 在 `preview_only` 模式下先完成下载，
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
    let extension = source_extension(file_name).ok_or("无法识别文件类型")?;
    if extension != "dll" && extension != "zip" {
        return Err("只支持 .dll 或 .zip 文件".into());
    }
    let mut preview = if extension == "dll" {
        build_preview(vec![analyze_dll(source, file_name)], file_name)
    } else {
        analyze_zip(source, file_name)?
    };
    apply_repository_origin(&mut preview, repository_name, release_tag, release_body);
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

#[tauri::command]
pub async fn analyze_mod_source(
    state: State<'_, AppState>,
    source: ModSource,
) -> Result<ModPreview, String> {
    cleanup_stale_previews(&state);
    let (local_path, file_name) = match source.kind {
        ModSourceKind::Local => {
            let path = PathBuf::from(source.value.trim());
            if !path.is_file() {
                return Err("所选文件不存在".into());
            }
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or("文件名无效")?
                .to_string();
            (path, name)
        }
        ModSourceKind::Url => {
            let (path, name) =
                crate::direct_mod::download_for_preview(&state, &source.value).await?;
            (path, name)
        }
    };
    let extension = source_extension(&file_name).ok_or("无法识别文件类型")?;
    if extension != "dll" && extension != "zip" {
        return Err("只支持 .dll 或 .zip 文件".into());
    }

    log::info!(
        "开始解析 MOD 安装源：类型={:?}，文件名={}",
        source.kind,
        file_name
    );
    let preview = if extension == "dll" {
        build_preview(vec![analyze_dll(&local_path, &file_name)], &file_name)
    } else {
        analyze_zip(&local_path, &file_name)?
    };

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
pub fn confirm_mod_install(
    state: State<'_, AppState>,
    preview_id: String,
) -> Result<OperationResult, String> {
    let dir = preview_root(&state).join(safe_preview_id(&preview_id)?);
    let file = fs::read_dir(&dir)
        .map_err(|_| "预览已过期，请重新选择文件".to_string())?
        .flatten()
        .map(|entry| entry.path())
        // 跳过来源元数据：它和安装包放在同一个暂存目录里
        .find(|path| path.is_file() && path.file_name().is_some_and(|name| name != ORIGIN_FILE))
        .ok_or("预览已过期，请重新选择文件")?;
    log::info!("确认安装预览文件：预览={preview_id}");

    /*
     * 仓库来源的预览必须走仓库安装路径。
     *
     * 走 install_manual_mod 会给包分配新的 `manual-<uuid>` 清单，
     * 于是同仓库的旧包永远不会被回收 —— 用户装三次就得到三份副本。
     * 有来源记录时改走 install_staged_repository，它会先按仓库身份
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
            crate::commands::install_staged_repository(
                &state,
                &file,
                &origin.repository_id,
                &origin.release_tag,
                origin.prerelease,
            )
        }
        None => crate::commands::install_manual_mod(&state, &file, None),
    };
    let _ = fs::remove_dir_all(&dir);
    result
}

/// 预览 id 必须是合法的 UUID，避免拼出跨目录路径。
fn safe_preview_id(value: &str) -> Result<String, String> {
    let parsed = Uuid::parse_str(value.trim()).map_err(|_| "预览标识无效".to_string())?;
    Ok(parsed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(role: PreviewRole) -> ModPreviewEntry {
        ModPreviewEntry {
            path: "x.dll".into(),
            role,
            name: Some("x".into()),
            guid: None,
            version: None,
            title: None,
            product_name: None,
            company: None,
            copyright: None,
            original_filename: None,
        }
    }

    #[test]
    fn single_dll_is_a_single_mod() {
        let preview = build_preview(vec![entry(PreviewRole::Mod)], "a.zip");
        assert_eq!(preview.kind, "single");
        assert_eq!(preview.total_dlls, 1);
        assert_eq!(preview.mod_count, 1);
        assert_eq!(preview.dependency_count, 0);
        assert_eq!(preview.package_name, "a");
    }

    #[test]
    fn repository_origin_replaces_file_name_with_repository_name() {
        // 资产文件名是技术命名，用户认的是仓库名
        let mut preview = build_preview(
            vec![entry(PreviewRole::Mod)],
            "IMGUI.HiDPI.Patcher.Mono-v0.4.4.zip",
        );
        assert_eq!(preview.package_name, "IMGUI.HiDPI.Patcher.Mono-v0.4.4");

        apply_repository_origin(
            &mut preview,
            "BepInEx 配置管理器 - HiDPI 缩放修复",
            "v0.4.4",
            "## Changes",
        );
        assert_eq!(preview.origin, PreviewOrigin::Repository);
        assert_eq!(preview.package_name, "BepInEx 配置管理器 - HiDPI 缩放修复");
        assert_eq!(preview.release_tag, "v0.4.4");
        assert_eq!(preview.release_body, "## Changes");
    }

    #[test]
    fn empty_repository_name_keeps_filename_as_title() {
        // 仓库名为空时不能把标题清空，否则界面只剩一个空白标题
        let mut preview = build_preview(vec![entry(PreviewRole::Mod)], "fallback.zip");
        apply_repository_origin(&mut preview, "   ", "", "");
        assert_eq!(preview.package_name, "fallback");
    }

    #[test]
    fn manual_origin_keeps_zip_name_as_title() {
        // 手动安装（本地文件 / 直链）不经过 apply_repository_origin，标题就是文件名
        let preview = build_preview(vec![entry(PreviewRole::Mod)], "MyLocalModPack-v1.2.zip");
        assert_eq!(preview.origin, PreviewOrigin::Manual);
        assert_eq!(preview.package_name, "MyLocalModPack-v1.2");
        assert!(preview.release_tag.is_empty());
    }

    #[test]
    fn one_plugin_plus_one_dependency_is_still_a_package() {
        // DIYLevel 那种情况：真正的插件 + 纯依赖，必须按包处理，
        // 「能否解析出 BepInPlugin」不决定它是不是包。
        let preview = build_preview(
            vec![entry(PreviewRole::Mod), entry(PreviewRole::Dependency)],
            "DIYLevel.zip",
        );
        assert_eq!(preview.kind, "package");
        assert_eq!(preview.total_dlls, 2);
        assert_eq!(preview.mod_count, 1);
        assert_eq!(preview.dependency_count, 1);
    }

    #[test]
    fn multiple_plugins_are_a_package() {
        let preview = build_preview(
            vec![entry(PreviewRole::Mod), entry(PreviewRole::Mod)],
            "pack.zip",
        );
        assert_eq!(preview.kind, "package");
        assert_eq!(preview.mod_count, 2);
        assert_eq!(preview.dependency_count, 0);
    }

    #[test]
    fn zero_dlls_is_a_package_without_mods() {
        let preview = build_preview(Vec::new(), "assets.zip");
        assert_eq!(preview.kind, "package");
        assert_eq!(preview.total_dlls, 0);
        assert_eq!(preview.mod_count, 0);
        assert_eq!(preview.package_name, "assets");
    }

    #[test]
    fn preview_id_must_be_a_uuid() {
        assert!(safe_preview_id("../escape").is_err());
        assert!(safe_preview_id("not-a-uuid").is_err());
        let id = Uuid::new_v4().to_string();
        assert_eq!(safe_preview_id(&id).unwrap(), id);
    }

    #[test]
    fn origin_record_round_trips_repository_identity() {
        // 确认安装时要靠这份记录回到仓库安装路径：
        // 丢了它就只能走 install_manual_mod，同仓库旧包永远不被回收。
        let record = PreviewOriginRecord {
            repository_id: "repo-1".into(),
            release_tag: "v1.8.7.23".into(),
            prerelease: true,
        };
        let text = serde_json::to_string(&record).unwrap();
        let parsed: PreviewOriginRecord = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed.repository_id, "repo-1");
        assert_eq!(parsed.release_tag, "v1.8.7.23");
        assert!(parsed.prerelease);
    }

    #[test]
    fn origin_record_tolerates_missing_optional_fields() {
        // 只带 repository_id 的最简记录也要能读：老预览或手工构造的情况
        let parsed: PreviewOriginRecord =
            serde_json::from_str(r#"{"repository_id":"repo-2"}"#).unwrap();
        assert_eq!(parsed.repository_id, "repo-2");
        assert!(parsed.release_tag.is_empty());
        assert!(!parsed.prerelease);
    }
}
