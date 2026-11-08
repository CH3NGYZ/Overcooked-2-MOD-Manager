//! 地图 ZIP 分类与识别编排。

use super::bepinex_layer::inspect_bepinex_layer;
use super::bepinex_layer::is_bepinex_layer_zip;
use super::identity::allocate_dir_name;
use super::identity::sanitize_dir_name;
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
use std::path::Path;

/// ZIP 里识别出的一个关卡集。
#[derive(Debug, Clone)]
pub struct ImportedLevelSet {
    /// 导入后使用的目录名。
    pub dir_name: String,
    /// 落盘形态：拍平（历史）还是 BepInEx 表层包原样解压。
    ///
    /// 安装流程按它分叉：表层包不改名、不搬插件，只把 ZIP 内路径原样落到
    /// 游戏目录并登记插件归属（见 [`install_bepinex_layer_sets`]）。
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
    /// 含 `info` 的目录数（>0 即视为地图包）。
    pub map_sets: usize,
    /// ZIP 里 `.dll` 条目数。
    pub plugins: usize,
}

/// 判断一个 ZIP 是地图包还是 MOD 包（**只读条目名**，不解析 info）。
///
/// 判据刻意只有一条：**含 `info` 就是地图包**。地图包常带修复插件，只按
/// 「有没有 DLL」判断会把它误判成 MOD；反过来纯 DLL 的包一定是 MOD。
pub fn classify_zip(zip_path: &Path) -> Result<ZipContents, String> {
    let file = fs::File::open(zip_path).map_err(|error| error.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| {
        crate::locale_tag::with_params("archive.zipReadFailed", &[("reason", &error.to_string())])
    })?;
    let mut info_dirs: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut plugins = 0usize;
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
        let name = entry.name().replace('\\', "/");
        if is_junk_entry(&name) {
            continue;
        }
        let (directory, file_name) = split_entry(&name);
        if is_info_file(&file_name) {
            info_dirs.insert(directory);
        }
        if file_name.to_lowercase().ends_with(".dll") {
            plugins += 1;
        }
    }
    Ok(ZipContents {
        map_sets: info_dirs.len(),
        plugins,
    })
}

#[cfg(test)]
mod test_support;
#[cfg(test)]
pub use test_support::inspect_zip;

/// 带进度的 ZIP 识别：`info` 按目录并行解析，其余流程与串行版一致。
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
        let name = entry.name().replace('\\', "/");
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

    /*
     * 分两阶段：先并行解析出每个目录的元数据与「基础目录名」，再统一分配目录名。
     *
     * 不能边解析边分配：同一批里共用同一个集名的几张图，谁先谁后决定谁拿到
     * 不带后缀的名字，而顺序又由 ZIP 条目与 BTreeMap 键序决定 —— 同样的包
     * 两次导入可能得到不同目录名。
     */
    let info_tasks: Vec<InfoDirectory> = info_candidates
        .into_iter()
        .map(|(directory, candidates)| InfoDirectory {
            directory,
            candidates,
        })
        .collect();
    let parsed_metadata =
        parse_zip_info_directories(zip_path, &info_tasks, info_workers, true, progress)?;

    let mut parsed_sets: Vec<(String, LevelSetMetadata, String)> =
        Vec::with_capacity(parsed_metadata.len());
    for (directory, metadata) in parsed_metadata {
        /*
         * 关卡集目录名：用 info 里的英文集名，而不是 ZIP 里的目录名。
         *
         * 后者常是作者的临时命名（`新建文件夹`、`levels`、甚至根目录为空串），
         * 而集名是作者在编辑器里真正填的名字，落到磁盘后还能和元数据对上。
         * 清理掉不能做目录名的字符。
         */
        let base_name = sanitize_dir_name(&metadata.name)
            .or_else(|| sanitize_dir_name(&directory.rsplit('/').next().unwrap_or("")))
            .ok_or_else(|| "levelSet.invalidName".to_string())?;
        parsed_sets.push((directory, metadata, base_name));
    }

    // 同一批里共用同一个基础名的有几张：大于 1 时它们**全部**带消歧后缀
    let mut base_counts: BTreeMap<String, usize> = BTreeMap::new();
    for (_, _, base_name) in &parsed_sets {
        *base_counts.entry(base_name.to_lowercase()).or_default() += 1;
    }

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
    let mut taken_dir_names = std::collections::HashSet::new();
    for (directory, metadata, base_name) in parsed_sets {
        let allow_base = base_counts
            .get(&base_name.to_lowercase())
            .copied()
            .unwrap_or(0)
            < 2;
        let dir_name = allocate_dir_name(&base_name, &metadata, &mut taken_dir_names, allow_base);

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

    /*
     * 目录名互不相同由 `allocate_dir_name` 保证：集名撞车时它会追加中文名、
     * 作者或 uid 后缀，而不是像早期那样整包拒绝。
     *
     * 实测「05_无前置地图合集」里 `gaoda`（胡闹糕中）与 `daxue`（胡闹达学）
     * 的英文集名与作者都写成 GAODA，只靠集名无法区分；中文名是唯一能把
     * 它们分开、又不随版本变化的字段。
     */
    log::info!(
        "ZIP 关卡集识别完成：文件={}，关卡集={}，随包插件={}",
        zip_path.display(),
        imported.len(),
        imported.iter().map(|set| set.plugins.len()).sum::<usize>()
    );
    Ok(imported)
}
