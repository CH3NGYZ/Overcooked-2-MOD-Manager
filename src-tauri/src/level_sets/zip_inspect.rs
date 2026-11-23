//! 地图 ZIP 分类与识别编排。

use super::bepinex_layer::inspect_bepinex_layer;
use super::bepinex_layer::is_bepinex_layer_zip;
use super::identity::allocate_dir_name;
use super::plugin_assignment::assign_plugins;
use super::zip_info::InfoDirectory;
use super::zip_info::is_info_file;
use super::zip_info::is_junk_entry;
use super::zip_info::parse_zip_info_directories;
use super::zip_info::split_entry;
use super::zip_plugins::PreparedZipPlugin;
use super::zip_plugins::fallback_plugin_info;
use super::zip_plugins::prepare_zip_plugin;
use crate::asset_bundle::LevelSetMetadata;
use crate::metadata_read::MetadataKind;
use crate::metadata_read::MetadataRequest;
use crate::models::DllPluginInfo;
use crate::models::LevelSetLayout;
use crate::models::ZipLevelSetPlugin;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::Path;
use zip::result::ZipError;

/// ZIP 里识别出的一个关卡集。
#[derive(Debug, Clone)]
pub struct ImportedLevelSet {
    /// 导入后使用的目录名。
    pub dir_name: String,
    /// 落盘形态：拍平（历史）还是 BepInEx 表层包原样解压。
    ///
    /// 安装流程按它分叉：表层包不改集目录名、不搬插件，地图子树定向到
    /// 当前 DIYLevel 根并登记插件归属（见 [`install_bepinex_layer_sets`]）。
    pub layout: LevelSetLayout,
    /// 该关卡集的 `info` 文件内容解析结果。
    pub metadata: LevelSetMetadata,
    /// ZIP 内该关卡集的文件：条目名 → 去掉关卡集前缀后的相对路径。
    ///
    /// 包含作者随包的修复插件（它们被归到集目录根，见 [`assign_plugins`]）。
    pub files: Vec<(String, String)>,
    /// 随这个关卡集一起安装的插件（只用于界面提示，落盘走 `files`）。
    ///
    /// 元数据与 MOD 预览同源（[`crate::mods::read_dll_details`]）：界面把每个
    /// 插件当成一个 MOD 展示名称/版本/作者，不再只给一串文件名。
    pub plugins: Vec<ZipLevelSetPlugin>,
}

/// ZIP 里有什么：关卡集数量与 DLL 数量（拖放分类用，不解析 info）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZipContents {
    /// 含 `info` 的目录数。
    pub map_sets: usize,
    /// ZIP 里 `.dll` 条目数。
    pub plugins: usize,
    pub mod_bundled_maps: bool,
}

/// 地图可带随图插件。只有实读出地图目录上层的 DIYLevel 才走 MOD 整包安装。
pub fn classify_zip(zip_path: &Path) -> Result<ZipContents, String> {
    let file = fs::File::open(zip_path).map_err(|error| {
        log::warn!(
            "ZIP 分类失败：阶段=open，路径={:?}，I/O={:?}，OS码={:?}",
            crate::file_check::log_path(zip_path),
            error.kind(),
            error.raw_os_error(),
        );
        error.to_string()
    })?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| {
        log_classification_error(zip_path, "open_archive", None, &error);
        crate::locale_tag::with_params("archive.zipReadFailed", &[("reason", &error.to_string())])
    })?;
    let mut info_dirs: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut plugin_entries = Vec::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|error| {
            log_classification_error(zip_path, "read_entry", Some(index), &error);
            crate::locale_tag::with_params(
                "archive.zipEntryReadFailed",
                &[("reason", &error.to_string())],
            )
        })?;
        if entry.is_dir() {
            continue;
        }
        let name = entry
            .name()
            .map_err(|error| {
                log::warn!(
                    "ZIP 分类失败：阶段=entry_name_encoding，路径={:?}，条目索引={index}",
                    crate::file_check::log_path(zip_path)
                );
                error.to_string()
            })?
            .replace('\\', "/");
        if is_junk_entry(&name) {
            continue;
        }
        let (directory, file_name) = split_entry(&name);
        if is_info_file(&file_name) {
            info_dirs.insert(directory);
        }
        if crate::mods::canonical_dll_name(&file_name)
            .to_ascii_lowercase()
            .ends_with(".dll")
        {
            plugin_entries.push((index, name));
        }
    }
    let mut mod_bundled_maps = false;
    for (index, name) in &plugin_entries {
        let (parent, _) = split_entry(name);
        let levels = if parent.is_empty() {
            "levels/".to_owned()
        } else {
            format!("{parent}/levels/").to_ascii_lowercase()
        };
        if !info_dirs.iter().any(|dir| {
            dir.to_ascii_lowercase()
                .strip_prefix(&levels)
                .is_some_and(|name| !name.is_empty() && !name.contains('/'))
        }) {
            continue;
        }
        let mut bytes = Vec::new();
        archive
            .by_index(*index)
            .and_then(|entry| {
                entry
                    .take(crate::mods::MAX_DLL_BYTES)
                    .read_to_end(&mut bytes)
                    .map_err(Into::into)
            })
            .map_err(|error| {
                log_classification_error(zip_path, "read_possible_framework", Some(*index), &error);
                crate::locale_tag::with_params(
                    "archive.zipEntryReadFailed",
                    &[("reason", &format!("{name}: {error}"))],
                )
            })?;
        if crate::mods::parse_bepin_plugin_metadata(&bytes)
            .and_then(|metadata| metadata.guid)
            .is_some_and(|guid| {
                guid.eq_ignore_ascii_case(crate::mods::plugin_target::DIYLEVEL_GUID)
            })
        {
            mod_bundled_maps = true;
            break;
        }
    }
    let contents = ZipContents {
        map_sets: info_dirs.len(),
        plugins: plugin_entries.len(),
        mod_bundled_maps,
    };
    log::info!(
        "ZIP 分类完成：路径={:?}，条目={}，含info目录={}，DLL={}，DIYLevel混合包={}",
        crate::file_check::log_path(zip_path),
        archive.len(),
        contents.map_sets,
        contents.plugins,
        contents.mod_bundled_maps,
    );
    Ok(contents)
}

fn log_classification_error(path: &Path, stage: &str, index: Option<usize>, error: &ZipError) {
    let (reason, io, method) = match error {
        ZipError::Io(error) => ("io", Some((error.kind(), error.raw_os_error())), None),
        ZipError::InvalidArchive(_) => ("invalid_archive", None, None),
        ZipError::UnsupportedArchive(reason) if *reason == ZipError::PASSWORD_REQUIRED => {
            ("password_required", None, None)
        }
        ZipError::UnsupportedArchive(_) => ("unsupported_archive", None, None),
        ZipError::InvalidPassword => ("invalid_password", None, None),
        ZipError::FileNotFound => ("entry_missing", None, None),
        ZipError::CompressionMethodNotSupported(method) => {
            ("unsupported_compression", None, Some(method))
        }
        _ => ("other_zip_error", None, None),
    };
    log::warn!(
        "ZIP 分类失败：阶段={stage}，路径={:?}，条目索引={index:?}，类别={reason}，I/O={io:?}，压缩方式={method:?}",
        crate::file_check::log_path(path),
    );
}

pub(crate) fn require_map_only_zip(path: &Path) -> Result<(), String> {
    if classify_zip(path)?.mod_bundled_maps {
        return Err("levelSet.zipContainsModAndMaps".into());
    }
    Ok(())
}

#[cfg(test)]
mod test_support;
#[cfg(test)]
pub use test_support::inspect_zip;

/// 带进度的 ZIP 识别：`info` 按目录并行解析，其余流程与串行版一致。
///
/// 下载包的物理路径是缓存文件，`downloaded_file_name` 才是普通地图包的命名来源。
/// 本地包直接用 `zip_path` 的文件名，BepInEx 表层包保留包内路径。
/// 合集按 ZIP 名加后缀分配独立目录，游戏只扫描 `levels` 的直接子目录。
///
/// `progress(done, total, current)` 在**每份候选 `info` 开始解析前**回调：
/// `done` 是已解析完的目录数（同一目录有多个候选 `info` 时它会停着不动），
/// `total` 是待解析的目录数，`current` 是这一份的 ZIP 内条目名。
///
/// 为什么 `info` 也要并行：它是 UnityFS 容器，解析要解 LZMA 块，是纯 CPU 活；
/// 一个几十集的合集串行解析实测要几十秒。ZIP 里的 `info` 没有磁盘路径
/// （直接读条目字节），走不了 [`crate::metadata_read`] 的按路径中间层，
/// 所以这里按「目录」分工，每个线程各自打开一份 `ZipArchive`。
pub fn inspect_zip_with_progress(
    zip_path: &Path,
    downloaded_file_name: Option<&str>,
    info_workers: usize,
    plugin_workers: usize,
    progress: &(dyn Fn(usize, usize, &str) + Sync),
) -> Result<Vec<ImportedLevelSet>, String> {
    let file = fs::File::open(zip_path).map_err(|error| error.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| {
        crate::locale_tag::with_params("archive.zipReadFailed", &[("reason", &error.to_string())])
    })?;

    /*
     * 先收集全部条目名，再按「info 的父目录」分组。
     *
     * 注意 `__MACOSX/` 与 `.DS_Store`：macOS 打包会带上它们，里面甚至有
     * `__MACOSX/<集名>/._info` 这种影子文件。不排除会被当成第二个关卡集，
     * 或者让「唯一 info」的判定失效。
     */
    let mut by_directory: BTreeMap<String, Vec<String>> = BTreeMap::new();
    /*
     * 一个目录可能同时有多个 info，但只有其中一个是真正的关卡集。
     *
     * 实测「04_前置地图合集」的 `宜/` 目录里有 `info_yi`（真关卡集）与
     * `info_yil`（连 `LevelSetInfoSO` 对象都没有，只是随包带进来的资源）。
     * 早期实现按「目录 → 单个 info」记录，同目录后出现的覆盖前面那个，
     * 留下哪个完全取决于 ZIP 条目顺序 —— 该包里 `info_yil` 排在后面，
     * 于是整个 ZIP 报 `missingLevelSetInfo`，另外 19 个正常关卡集一起进不来。
     *
     * 所以这里保留全部候选，稍后逐个试解析，取第一个能读出关卡集的。
     */
    // 候选条目：ZIP 下标 + 条目名（并行解析时每个线程要按条目名上报进度）
    let mut info_candidates: BTreeMap<String, Vec<(usize, String)>> = BTreeMap::new();
    // ZIP 内全部文件条目名，供「包单位」把 plugins 子树里的 DLL 归到地图上
    let mut all_entries: Vec<String> = Vec::new();
    // 条目名 → ZIP 索引：读随包插件内容时用它定位（`by_name` 每次线性扫中央目录）
    let mut index_by_name: HashMap<String, usize> = HashMap::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|error| {
            crate::locale_tag::with_params(
                "archive.zipEntryReadFailed",
                &[("reason", &error.to_string())],
            )
        })?;
        if entry.is_dir() {
            continue;
        }
        let name = entry
            .name()
            .map_err(|error| error.to_string())?
            .replace('\\', "/");
        if is_junk_entry(&name) {
            continue;
        }
        let (directory, file_name) = split_entry(&name);
        all_entries.push(name.clone());
        index_by_name.insert(name.clone(), index);
        by_directory
            .entry(directory.clone())
            .or_default()
            .push(name.clone());
        if is_info_file(&file_name) {
            info_candidates
                .entry(directory)
                .or_default()
                .push((index, name));
        }
    }

    /*
     * BepInEx 表层包先判：它的 `info` 也在 ZIP 里，但落盘形态完全不同
     * （原样解压 + 登记表），必须走另一条识别路径。
     */
    if is_bepinex_layer_zip(&all_entries) {
        return inspect_bepinex_layer(
            zip_path,
            &all_entries,
            &by_directory,
            &index_by_name,
            info_candidates,
            info_workers,
            plugin_workers,
            progress,
        );
    }

    if info_candidates.is_empty() {
        return Err("levelSet.zipWithoutInfo".into());
    }

    let archive_name = match downloaded_file_name {
        Some(file_name) => {
            crate::archive::validate_asset_file_name(file_name)?;
            Path::new(file_name)
        }
        None => zip_path,
    };
    let base_name = archive_name
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let info_tasks: Vec<InfoDirectory> = info_candidates
        .into_iter()
        .map(|(directory, candidates)| InfoDirectory {
            directory,
            candidates,
        })
        .collect();
    let parsed_metadata =
        parse_zip_info_directories(zip_path, &info_tasks, info_workers, true, progress)?;

    let allow_base = parsed_metadata.len() == 1;
    let mut taken_dir_names = std::collections::HashSet::new();
    let parsed_sets = parsed_metadata
        .into_iter()
        .map(|(directory, metadata)| {
            let dir_name =
                allocate_dir_name(base_name, &metadata, &mut taken_dir_names, allow_base);
            (directory, metadata, dir_name)
        })
        .collect::<Vec<_>>();

    /*
     * 插件归属先算一次再落盘：同一批里的集名还要做消歧，但归属看的是 ZIP 里
     * 的原始目录，与最终目录名无关。
     */
    let plugin_owner = assign_plugins(&all_entries, &parsed_sets);

    /*
     * 随包插件的 PE 版本资源只能从真实文件读：整份 ZIP 共用一份 scratch 目录，
     * 识别结束即删。目录建不出来时降级（插件仍按文件名展示），不影响识别。
     */
    let mut scratch = (!plugin_owner.is_empty())
        .then(|| std::env::temp_dir().join(format!("oc2-level-plugins-{}", uuid::Uuid::new_v4())));
    if let Some(dir) = &scratch {
        if let Err(error) = fs::create_dir_all(dir) {
            log::warn!(
                "随包插件元数据：临时目录创建失败，降级为文件名兜底：目录={}，原因={}",
                dir.display(),
                error
            );
            scratch = None;
        }
    }

    /*
     * 插件条目先解到 scratch，再一次性按批次解析：解压是 ZIP 的顺序 IO，
     * 解析是彼此独立的 PE 读取，分两段才能让多个 DLL 真正重叠。
     */
    let mut prepared_plugins: BTreeMap<String, Vec<PreparedZipPlugin>> = BTreeMap::new();
    for (directory, entries) in &plugin_owner {
        let prepared = entries
            .iter()
            .map(|entry| {
                prepare_zip_plugin(
                    &mut archive,
                    index_by_name.get(entry).copied(),
                    entry,
                    scratch.as_deref(),
                )
            })
            .collect::<Vec<_>>();
        prepared_plugins.insert(directory.clone(), prepared);
    }
    let mut requests: Vec<MetadataRequest> = Vec::new();
    for prepared in prepared_plugins.values_mut() {
        for plugin in prepared.iter_mut() {
            let Some(temp) = plugin.temp.as_ref() else {
                continue;
            };
            plugin.request = Some(requests.len());
            requests.push(MetadataRequest {
                path: temp.clone(),
                display_name: plugin.file_name.clone(),
                kind: MetadataKind::ModDll,
            });
        }
    }
    let outcomes = crate::metadata_read::read_metadata_batch(&requests, plugin_workers, &|_, _| {});

    let mut imported = Vec::with_capacity(parsed_sets.len());
    for (directory, metadata, dir_name) in parsed_sets {
        let prefix = if directory.is_empty() {
            String::new()
        } else {
            format!("{directory}/")
        };
        let mut files: Vec<(String, String)> = by_directory
            .remove(&directory)
            .unwrap_or_default()
            .into_iter()
            .map(|name| {
                let relative = name
                    .strip_prefix(&prefix)
                    .map(str::to_string)
                    .unwrap_or_else(|| name.clone());
                (name, relative)
            })
            .collect();
        /*
         * 作者的修复插件跟着地图走：条目原来在 ZIP 的 plugins 子树里，落盘时
         * 相对路径只取文件名，也就是放进集目录根。这样停用/启用（整目录
         * rename）天然把插件和地图一起开关 —— 否则关掉地图、插件还在 BepInEx
         * 里生效。
         */
        let prepared = prepared_plugins.remove(&directory).unwrap_or_default();
        let mut plugins = Vec::with_capacity(prepared.len());
        for plugin in prepared {
            // 落盘相对路径只取文件名 → 插件进集目录根，随目录一起开关。
            // 插件本来就躺在集目录里（第一级绑定）时 `files` 已经收过这一条，
            // 再 push 会让安装事务重复写同一个文件、`file_count` 也跟着虚高。
            if !files.iter().any(|(source, _)| source == &plugin.path) {
                files.push((plugin.path.clone(), plugin.file_name.clone()));
            }
            let info = plugin
                .request
                .and_then(|index| outcomes.get(index))
                .and_then(|outcome| outcome.mod_dll())
                .cloned()
                .map(DllPluginInfo::from)
                .unwrap_or_else(|| fallback_plugin_info(&plugin.file_name));
            plugins.push(ZipLevelSetPlugin {
                path: plugin.path,
                file_name: plugin.file_name,
                install_path: None,
                info,
            });
        }
        imported.push(ImportedLevelSet {
            dir_name,
            layout: LevelSetLayout::Flattened,
            metadata,
            files,
            plugins,
        });
    }

    if let Some(dir) = &scratch {
        let _ = fs::remove_dir_all(dir);
    }

    log::info!(
        "ZIP 关卡集识别完成：文件={}，关卡集={}，随包插件={}",
        zip_path.display(),
        imported.len(),
        imported.iter().map(|set| set.plugins.len()).sum::<usize>()
    );
    Ok(imported)
}
