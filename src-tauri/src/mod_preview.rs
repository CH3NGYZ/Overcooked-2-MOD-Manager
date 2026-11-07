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

use crate::app_state::AppState;
use crate::models::{DllRole, InstallConflict, InstalledMod, OperationResult};
use crate::mods::{
    DllDetails, MAX_DLL_BYTES, compare_mod_versions, file_stem_name, placeholder_dll_details,
    read_dll_details,
};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashSet;
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

/// 预览里的一行（一个 DLL）。
#[derive(Debug, Clone, Serialize)]
pub struct ModPreviewEntry {
    /// 相对安装包根目录的路径，用于展示。
    pub path: String,
    pub role: DllRole,
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
    /// 本地已有的同身份 MOD（安装前要卸载的那些）。
    ///
    /// 只在 `analyze_mod_source` / `stage_downloaded_file` 里填；安装预览模型
    /// 直接复用，前端据此在确认界面先提示「将卸载本地旧副本」。
    #[serde(default)]
    pub conflicts: Vec<InstallConflict>,
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

/// 把一份已解析的 DLL 元数据摊平成预览行。
///
/// 解析规则只有一份，在 [`crate::mods::read_dll_details`]（BepInPlugin → PE
/// 版本资源 → 文件名兜底，角色由 guid/version 是否为空决定），这里只负责
/// 版式。`display_path` 是包内相对路径（展示用）。
fn preview_entry(display_path: &str, details: &DllDetails) -> ModPreviewEntry {
    if details.role == DllRole::Dependency {
        log::debug!(
            "MOD 预览：判定为依赖（未读出 BepInPlugin 元数据）：路径={}",
            display_path
        );
    }
    ModPreviewEntry {
        path: display_path.to_string(),
        role: details.role,
        name: details.name.clone(),
        guid: details.guid.clone(),
        version: details.version.clone(),
        title: details.title.clone(),
        product_name: details.product_name.clone(),
        company: details.company.clone(),
        copyright: details.copyright.clone(),
        original_filename: details.original_filename.clone(),
    }
}

/// 单个 DLL 的预览行（不走批次：只有一个文件，批次只会多一次线程开销）。
fn analyze_dll(path: &Path, display_path: &str) -> ModPreviewEntry {
    preview_entry(display_path, &read_dll_details(path, display_path))
}

/// 分析 ZIP：遍历条目，仅解析 DLL（其余文件忽略）。
///
/// ZIP 里的 DLL 必须先落到临时文件才能读 PE 版本资源，所以先整批解压、再交给
/// `metadata_read` 统一并发解析 —— 与本地 MOD 扫描、地图随包插件走同一条读取
/// 路径，`workers` 就是 MOD 元数据并行度。
fn analyze_zip(path: &Path, file_name: &str, workers: usize) -> Result<ModPreview, String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|error| {
        crate::locale_tag::with_params("archive.zipReadFailed2", &[("reason", &error.to_string())])
    })?;
    // ZIP 里的 DLL 需要落到临时文件才能读 PE 版本资源。
    // 用一个自建临时目录而不是 tempfile crate（项目没有这个依赖），
    // 分析结束立即删除。
    let scratch = std::env::temp_dir().join(format!("oc2-preview-{}", Uuid::new_v4()));
    fs::create_dir_all(&scratch).map_err(|error| error.to_string())?;
    let result = (|| -> Result<Vec<ModPreviewEntry>, String> {
        let mut staged: Vec<(PathBuf, String)> = Vec::new();
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).map_err(|error| {
                crate::locale_tag::with_params(
                    "archive.zipEntryReadFailed2",
                    &[("reason", &error.to_string())],
                )
            })?;
            if entry.is_dir() {
                continue;
            }
            let Some(enclosed) = entry.enclosed_name() else {
                return Err(crate::locale_tag::with_params(
                    "archive.unsafePath2",
                    &[("entry", &entry.name().to_string())],
                ));
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
            staged.push((temp, display));
        }
        /*
         * 解压完再统一读：解析彼此独立，交给中间层去重 + 限并发，结果按
         * `staged` 原顺序返回，预览行的顺序与 ZIP 里的条目顺序一致。
         */
        let requests = staged
            .iter()
            .map(|(temp, display)| crate::metadata_read::MetadataRequest {
                path: temp.clone(),
                display_name: display.clone(),
                kind: crate::metadata_read::MetadataKind::ModDll,
            })
            .collect::<Vec<_>>();
        let outcomes = crate::metadata_read::read_metadata_batch(&requests, workers, &|_, _| {});
        Ok(staged
            .into_iter()
            .zip(outcomes)
            .map(|((_, display), outcome)| {
                let details = outcome
                    .mod_dll()
                    .cloned()
                    .unwrap_or_else(|| placeholder_dll_details(&display));
                preview_entry(&display, &details)
            })
            .collect())
    })();
    let _ = fs::remove_dir_all(&scratch);
    Ok(build_preview(result?, file_name))
}

fn build_preview(entries: Vec<ModPreviewEntry>, file_name: &str) -> ModPreview {
    let total_dlls = entries.len();
    let mod_count = entries
        .iter()
        .filter(|entry| entry.role == DllRole::Mod)
        .count();
    let dependency_count = total_dlls - mod_count;
    ModPreview {
        preview_id: Uuid::new_v4().to_string(),
        // 0 或 >= 2 个 DLL 都按包处理：只有恰好 1 个 DLL 才是单 mod。
        kind: if total_dlls == 1 { "single" } else { "package" },
        package_name: file_stem_name(file_name),
        file_name: file_name.to_string(),
        total_dlls,
        mod_count,
        dependency_count,
        entries,
        conflicts: Vec::new(),
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

/// 找出「这次要装的 MOD」与本地已装副本的身份冲突。
///
/// 只报告、不卸载：卸载由前端确认后逐个走 `uninstall_local_mod` 的整包语义
/// （它按安装清单回收整包），安装事务里不掺卸载。同一个 MOD 不管版本相同、
/// 升级还是降级都算同一个东西 —— 并存只会得到两份互相顶掉的 DLL。
///
/// 匹配规则（唯一实现，前端不要另判）：
///   · 只看 `role == DllRole::Mod` 的条目：纯依赖不算冲突源，依赖是随包带着
///     的 DLL，装成依赖不会顶掉本地同一个插件；
///   · 两边都有 GUID 时只比 GUID：它是 BepInPlugin 的稳定身份，不同作者可能
///     起同名 DLL，但不会共用 GUID；
///   · 任一方缺 GUID（读不出元数据）才退回比名字，忽略大小写与首尾空白；
///     两边名字都空时不算命中，否则「读不出元数据」会把所有同类包都判成冲突。
///
/// 命中项按安装包去重：同一个包里的多个 DLL 属于同一次卸载（`uninstall_local_mod`
/// 是整包语义），每组只报一条，`handle` 取该组第一份命中的 `dll_file`。
/// 顺序沿用 `installed` 的顺序，不排序 —— 界面按列表顺序展示才与 MOD 管理页一致。
pub fn find_install_conflicts(
    installed: &[InstalledMod],
    entries: &[ModPreviewEntry],
) -> Vec<InstallConflict> {
    let candidates: Vec<&ModPreviewEntry> = entries
        .iter()
        .filter(|entry| entry.role == DllRole::Mod)
        .collect();
    if candidates.is_empty() {
        return Vec::new();
    }
    let mut groups: HashSet<String> = HashSet::new();
    let mut conflicts = Vec::new();
    for existing in installed {
        let Some(entry) = candidates
            .iter()
            .find(|entry| same_plugin(entry, existing))
            .copied()
        else {
            continue;
        };
        // 包 ID 是「同一次卸载」的粒度；没有包 ID（单 MOD）才按 DLL 分组。
        let group = trimmed(existing.package_id.as_deref())
            .map(str::to_string)
            .unwrap_or_else(|| existing.dll_file.clone());
        if !groups.insert(group) {
            continue;
        }
        conflicts.push(InstallConflict {
            name: conflict_name(existing),
            version: existing.version.clone().unwrap_or_default(),
            handle: existing.dll_file.clone(),
            enabled: existing.enabled,
            same_version: same_version(entry.version.as_deref(), existing.version.as_deref()),
        });
    }
    conflicts
}

/// 给预览补上冲突列表（安装入口在 `build_preview` 之后、`stage_local` 之前调）。
///
/// 探测不到游戏目录时留空：没装游戏就没有已装副本。扫描走
/// `mods::scan_mods_with_manifests`（与 MOD 管理页同一份），所以「包内副本」
/// 这类整包视角与界面显示的完全一致。
fn attach_install_conflicts(state: &AppState, preview: &mut ModPreview) {
    let config = state.config.load_or_default();
    let Some(game_root) = crate::game::detect_game(&config).path else {
        return;
    };
    let installed = crate::mods::scan_mods_with_manifests(
        Path::new(&game_root),
        &config,
        Some(&state.config.manifests_dir()),
    );
    preview.conflicts = find_install_conflicts(&installed, &preview.entries);
}

/// 预览条目与本地已装 MOD 是否指同一个插件。
fn same_plugin(entry: &ModPreviewEntry, installed: &InstalledMod) -> bool {
    if let (Some(entry_guid), Some(installed_guid)) = (
        trimmed(entry.guid.as_deref()),
        trimmed(installed.guid.as_deref()),
    ) {
        return entry_guid.eq_ignore_ascii_case(installed_guid);
    }
    let Some(entry_name) = trimmed(entry.name.as_deref()) else {
        return false;
    };
    let installed_name = installed.name.trim();
    !installed_name.is_empty() && entry_name.eq_ignore_ascii_case(installed_name)
}

/// 冲突行的展示名：包名优先（一个包里多个 DLL 时用户认包），缺省回落插件名。
fn conflict_name(installed: &InstalledMod) -> String {
    trimmed(installed.package_name.as_deref())
        .unwrap_or(installed.name.trim())
        .to_string()
}

/// 本次要装的版本与本地已装是否相同。
///
/// 两边都有版本号时先按 BepInEx 的数字段比较（`mods::compare_mod_versions`，
/// 与「本地谁在生效」同一把尺子）；读不出数字段说明作者写的不是版本号，
/// 退回原文比较。缺失方按空串参与比较 —— 两边都读不出时算相同。
fn same_version(target: Option<&str>, local: Option<&str>) -> bool {
    let target = target.unwrap_or_default().trim();
    let local = local.unwrap_or_default().trim();
    if target.is_empty() || local.is_empty() {
        return target == local;
    }
    match compare_mod_versions(target, local) {
        Some(ordering) => ordering == Ordering::Equal,
        None => target == local,
    }
}

/// `Option<&str>` 去掉首尾空白后的非空值；空白字符串等同于「没有」。
fn trimmed(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
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
            crate::mods::install::install_staged_repository(
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
fn safe_preview_id(value: &str) -> Result<String, String> {
    let parsed = Uuid::parse_str(value.trim()).map_err(|_| "modPreview.invalidId".to_string())?;
    Ok(parsed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(role: DllRole) -> ModPreviewEntry {
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
        let preview = build_preview(vec![entry(DllRole::Mod)], "a.zip");
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
            vec![entry(DllRole::Mod)],
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
        let mut preview = build_preview(vec![entry(DllRole::Mod)], "fallback.zip");
        apply_repository_origin(&mut preview, "   ", "", "");
        assert_eq!(preview.package_name, "fallback");
    }

    #[test]
    fn manual_origin_keeps_zip_name_as_title() {
        // 手动安装（本地文件 / 直链）不经过 apply_repository_origin，标题就是文件名
        let preview = build_preview(vec![entry(DllRole::Mod)], "MyLocalModPack-v1.2.zip");
        assert_eq!(preview.origin, PreviewOrigin::Manual);
        assert_eq!(preview.package_name, "MyLocalModPack-v1.2");
        assert!(preview.release_tag.is_empty());
    }

    #[test]
    fn one_plugin_plus_one_dependency_is_still_a_package() {
        // DIYLevel 那种情况：真正的插件 + 纯依赖，必须按包处理，
        // 「能否解析出 BepInPlugin」不决定它是不是包。
        let preview = build_preview(
            vec![entry(DllRole::Mod), entry(DllRole::Dependency)],
            "DIYLevel.zip",
        );
        assert_eq!(preview.kind, "package");
        assert_eq!(preview.total_dlls, 2);
        assert_eq!(preview.mod_count, 1);
        assert_eq!(preview.dependency_count, 1);
    }

    #[test]
    fn multiple_plugins_are_a_package() {
        let preview = build_preview(vec![entry(DllRole::Mod), entry(DllRole::Mod)], "pack.zip");
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

    /// 本地已装 MOD 的最小构造：默认字段与真实扫描一致，测试只写关心的字段。
    fn installed_mod(dll_file: &str, extra: serde_json::Value) -> InstalledMod {
        let mut base = serde_json::json!({
            "name": dll_file, "dll_file": dll_file, "enabled": true,
            "version_source": "BepInPlugin", "update_available": false,
            "duplicate_guid": false, "status": "latest",
        });
        if let (Some(base_map), Some(extra_map)) = (base.as_object_mut(), extra.as_object()) {
            for (key, value) in extra_map {
                base_map.insert(key.clone(), value.clone());
            }
        }
        serde_json::from_value(base).unwrap()
    }

    fn preview_entry(
        path: &str,
        role: DllRole,
        name: Option<&str>,
        guid: Option<&str>,
        version: Option<&str>,
    ) -> ModPreviewEntry {
        ModPreviewEntry {
            path: path.into(),
            role,
            name: name.map(str::to_string),
            guid: guid.map(str::to_string),
            version: version.map(str::to_string),
            title: None,
            product_name: None,
            company: None,
            copyright: None,
            original_filename: None,
        }
    }

    #[test]
    fn conflicts_match_by_guid_ignoring_case_and_whitespace() {
        // 同一个 GUID 就是同一个插件：名字被改过也算冲突，卸载句柄用本地那份。
        let installed = vec![installed_mod(
            "BepInEx/plugins/FastThrow.dll",
            serde_json::json!({
                "guid": "com.example.fastthrow", "version": "1.2.0",
                "package_id": "repo-fast-throw", "package_name": "快速投掷修复包",
            }),
        )];
        let entries = vec![preview_entry(
            "FastThrow.dll",
            DllRole::Mod,
            Some("另一个显示名"),
            Some("  COM.Example.FastThrow "),
            Some("1.2.0"),
        )];

        let conflicts = find_install_conflicts(&installed, &entries);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].name, "快速投掷修复包");
        assert_eq!(conflicts[0].version, "1.2.0");
        assert_eq!(conflicts[0].handle, "BepInEx/plugins/FastThrow.dll");
        assert!(conflicts[0].enabled);
        assert!(conflicts[0].same_version);
    }

    #[test]
    fn conflicts_fall_back_to_name_when_either_side_lacks_a_guid() {
        // 读不出 BepInPlugin 的 DLL 只有名字可比；名字两边都空时不能算同一份。
        let installed = vec![installed_mod(
            "BepInEx/plugins/Plain.dll",
            serde_json::json!({ "name": "PlainMod" }),
        )];
        assert_eq!(
            find_install_conflicts(
                &installed,
                &[preview_entry(
                    "Plain.dll",
                    DllRole::Mod,
                    Some(" plainmod "),
                    Some("com.example.plain"),
                    None,
                )],
            )
            .len(),
            1
        );
        assert!(
            find_install_conflicts(
                &installed,
                &[preview_entry("Plain.dll", DllRole::Mod, None, None, None)],
            )
            .is_empty()
        );
    }

    #[test]
    fn differing_guids_are_not_conflicts_even_with_the_same_name() {
        // 不同作者可能起同名 DLL，但不会共用 GUID：GUID 都在时以 GUID 为准。
        let installed = vec![installed_mod(
            "BepInEx/plugins/Shared.dll",
            serde_json::json!({ "guid": "com.author-a.shared", "name": "Shared" }),
        )];
        let entries = vec![preview_entry(
            "Shared.dll",
            DllRole::Mod,
            Some("Shared"),
            Some("com.author-b.shared"),
            None,
        )];
        assert!(find_install_conflicts(&installed, &entries).is_empty());
    }

    #[test]
    fn dependency_entries_never_count_as_conflicts() {
        // 依赖是随包带的 DLL，装成依赖不会顶掉本地同一个插件，不该提示卸载。
        let installed = vec![installed_mod(
            "BepInEx/plugins/Runtime.dll",
            serde_json::json!({ "guid": "com.example.runtime", "version": "2.0.0" }),
        )];
        let entries = vec![preview_entry(
            "Runtime.dll",
            DllRole::Dependency,
            Some("Runtime"),
            Some("com.example.runtime"),
            Some("2.0.0"),
        )];
        assert!(find_install_conflicts(&installed, &entries).is_empty());
    }

    #[test]
    fn conflicts_are_deduplicated_per_installed_package() {
        // 同一个包里的两个 DLL 属于同一次卸载：只报一条，句柄取第一份命中的。
        let installed = vec![
            installed_mod(
                "BepInEx/plugins/Pack/Main.dll",
                serde_json::json!({
                    "name": "Main", "guid": "com.example.pack.main", "version": "1.0.0",
                    "package_id": "repo-pack", "package_name": "测试包",
                }),
            ),
            installed_mod(
                "BepInEx/plugins/Pack/Extra.dll",
                serde_json::json!({
                    "name": "Extra", "guid": "com.example.pack.extra", "version": "1.0.0",
                    "package_id": "repo-pack", "package_name": "测试包",
                }),
            ),
        ];
        let entries = vec![
            preview_entry(
                "Main.dll",
                DllRole::Mod,
                Some("Main"),
                Some("com.example.pack.main"),
                Some("1.0.0"),
            ),
            preview_entry(
                "Extra.dll",
                DllRole::Mod,
                Some("Extra"),
                Some("com.example.pack.extra"),
                Some("1.0.0"),
            ),
        ];

        let conflicts = find_install_conflicts(&installed, &entries);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].handle, "BepInEx/plugins/Pack/Main.dll");
    }

    #[test]
    fn conflicts_keep_installed_order_and_report_disabled_copies() {
        // 已有重复副本要全部列出（关着的也要卸），顺序沿用本地扫描顺序。
        let installed = vec![
            installed_mod(
                "BepInEx/plugins/A.dll",
                serde_json::json!({ "name": "A", "guid": "com.example.a", "version": "1.0" }),
            ),
            installed_mod(
                "BepInEx/plugins/A-disabled.dll.disabled",
                serde_json::json!({
                    "name": "A", "guid": "com.example.a", "version": "1.0", "enabled": false,
                }),
            ),
        ];
        let entries = vec![preview_entry(
            "A.dll",
            DllRole::Mod,
            Some("A"),
            Some("com.example.a"),
            Some("1.0"),
        )];

        let conflicts = find_install_conflicts(&installed, &entries);
        assert_eq!(conflicts.len(), 2);
        assert_eq!(conflicts[0].handle, "BepInEx/plugins/A.dll");
        assert!(conflicts[0].enabled);
        assert_eq!(
            conflicts[1].handle,
            "BepInEx/plugins/A-disabled.dll.disabled"
        );
        assert!(!conflicts[1].enabled);
    }

    #[test]
    fn same_version_follows_bepinex_numeric_comparison() {
        // 「重装」还是「版本变化」必须与本地谁的版本更新判定同一把尺子：
        // 1.0 与 1.0.0 是同一个版本，1.9.8 与 1.9.8.1 不是。
        assert!(same_version(Some("1.0"), Some("1.0.0")));
        assert!(!same_version(Some("1.9.8"), Some("1.9.8.1")));
        assert!(same_version(Some(" v1.2.3 "), Some("1.2.3")));
        // 不是纯数字版本号：退回原文比较
        assert!(same_version(Some("final"), Some("final")));
        assert!(!same_version(Some("final"), Some("latest")));
        // 两边都读不出时算相同，只有一边读不出时不能声称同版本
        assert!(same_version(None, None));
        assert!(!same_version(Some("1.0"), None));
    }
}
