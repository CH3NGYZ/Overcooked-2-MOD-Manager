use super::model::{ModPreview, ModPreviewEntry, PreviewOrigin};
use crate::models::DllRole;
use crate::mods::{
    DllDetails, MAX_DLL_BYTES, file_stem_name, placeholder_dll_details, read_dll_details,
};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use zip::ZipArchive;

/// 判断一个文件是 DLL 还是 ZIP（按扩展名）。
pub(super) fn source_extension(name: &str) -> Option<String> {
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
pub(super) fn analyze_dll(path: &Path, display_path: &str) -> ModPreviewEntry {
    preview_entry(display_path, &read_dll_details(path, display_path))
}

/// 分析 ZIP：遍历条目，仅解析 DLL（其余文件忽略）。
///
/// ZIP 里的 DLL 必须先落到临时文件才能读 PE 版本资源，所以先整批解压、再交给
/// `metadata_read` 统一并发解析 —— 与本地 MOD 扫描、地图随包插件走同一条读取
/// 路径，`workers` 就是 MOD 元数据并行度。
pub(super) fn analyze_zip(
    path: &Path,
    file_name: &str,
    workers: usize,
) -> Result<ModPreview, String> {
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

pub(super) fn build_preview(entries: Vec<ModPreviewEntry>, file_name: &str) -> ModPreview {
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
pub(super) fn apply_repository_origin(
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
