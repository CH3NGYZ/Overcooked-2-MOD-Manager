//! 嵌套地图探测、搬移计划与整组回滚。

use super::identity::allocate_dir_name;
use super::identity::sanitize_dir_name;
use super::info_cache::read_info_metadata;
use super::paths::LevelSetRoots;
use super::paths::collect_dir_names;
use super::paths::collect_files;
use super::paths::disabled_root;
use super::paths::levels_root;
use super::paths::remove_empty_directories;
use super::plugin_assignment::assign_plugins;
use super::scan::locate_info_file;
use super::zip_info::is_info_file;
use crate::asset_bundle::LevelSetMetadata;
use crate::models::LevelSetFlattenCandidate;
use crate::models::LevelSetFlattenReport;
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

/// 浅探测层数上限：`levels/<目录>` 自身算第 0 层，最多再往下 3 层找 `info`。
const FLATTEN_PROBE_DEPTH: usize = 3;

/// 有界浅探测：找出一个目录里（≤3 层）全部 `info` / `info_*` 文件。
fn find_nested_info_files(directory: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![(directory.to_path_buf(), 0usize)];
    while let Some((current, depth)) = stack.pop() {
        if depth > FLATTEN_PROBE_DEPTH {
            continue;
        }
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        let mut children: Vec<PathBuf> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                children.push(path);
                continue;
            }
            if !kind.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if is_info_file(&name) {
                found.push(path);
            }
        }
        children.sort();
        for child in children {
            stack.push((child, depth + 1));
        }
    }
    found.sort();
    found
}

/// 递归列出一个目录下的全部文件，路径相对该目录、正斜杠归一化。
fn collect_relative_files(directory: &Path) -> Vec<String> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(directory).follow_links(false) {
        let Ok(entry) = entry else {
            continue;
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let Ok(relative) = entry.path().strip_prefix(directory) else {
            continue;
        };
        files.push(relative.to_string_lossy().replace('\\', "/"));
    }
    files.sort();
    files
}

/// `path` 相对 `base` 的正斜杠路径（`path == base` 时是空串）。
fn relative_forward(base: &Path, path: &Path) -> String {
    path.strip_prefix(base)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default()
}

/// 拍平计划里的一个关卡集。
struct PlannedFlattenSet {
    /// `levels/` 下的直接子目录名（失败记录按它归属）。
    outer: String,
    info_path: PathBuf,
    metadata: LevelSetMetadata,
    /// 由集名派生的基础目录名（还没做撞车消歧）。
    base_name: String,
    /// 最终目录名（消歧后）。
    dir_name: String,
    /// 与 `info` 同目录、要一起搬走的文件（不含其它可解析的 `info*`）。
    resources: Vec<PathBuf>,
    /// 归属这个集的 DLL（作者放在 `plugins*` 子树里）。
    plugins: Vec<PathBuf>,
    /// 这些 DLL 同目录的资源文件。
    plugin_resources: Vec<PathBuf>,
}

/// 一键拍平：把 `levels/<目录>` 里嵌套的关卡集搬成 `levels/<规范集名>/`。
///
/// `dry_run` 只探测、不写盘。真正的搬移按外层目录分组：一个外层目录要么整体
/// 成功、要么整体回滚 —— 半个搬完的目录比不动更难收拾。
///
/// 空出来的嵌套壳与「没归到任何集」的剩余文件进回收站，不直接删除：它们可能
/// 是作者随包的资源或说明，静默删掉就找不回来了。
pub fn flatten_level_sets(
    roots: &LevelSetRoots,
    recycle_root: &Path,
    manifests_dir: &Path,
    dry_run: bool,
) -> Result<LevelSetFlattenReport, String> {
    flatten_level_sets_with(roots, recycle_root, manifests_dir, dry_run, &|path| {
        read_info_metadata(path)
    })
}

/// 拍平本体：`parse_info` 可注入，单测用它喂假元数据（不依赖真实 `info` 字节）。
pub(super) fn flatten_level_sets_with<F>(
    roots: &LevelSetRoots,
    recycle_root: &Path,
    manifests_dir: &Path,
    dry_run: bool,
    parse_info: &F,
) -> Result<LevelSetFlattenReport, String>
where
    F: Fn(&Path) -> Result<LevelSetMetadata, String>,
{
    let game_root = roots.game_root();
    if roots.package_root().is_none() && dry_run {
        return Ok(LevelSetFlattenReport::default());
    }
    let levels_dir = levels_root(roots)?;
    if !levels_dir.is_dir() {
        return Ok(LevelSetFlattenReport::default());
    }
    let mut names: Vec<String> = fs::read_dir(&levels_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();

    let mut candidates = Vec::new();
    for name in &names {
        let directory = levels_dir.join(name);
        // 顶层就有 `info` 的目录本来就是一张卡，扫描认得出，不需要整理
        if locate_info_file(&directory).is_some() {
            continue;
        }
        let nested = find_nested_info_files(&directory);
        if nested.is_empty() {
            continue;
        }
        candidates.push(LevelSetFlattenCandidate {
            dir_name: name.clone(),
            nested_infos: nested.len(),
        });
    }
    if dry_run {
        return Ok(LevelSetFlattenReport {
            candidates,
            flattened: 0,
            failed: Vec::new(),
        });
    }

    let mut plans: Vec<PlannedFlattenSet> = Vec::new();
    let mut base_counts: BTreeMap<String, usize> = BTreeMap::new();
    for candidate in &candidates {
        let outer_dir = levels_dir.join(&candidate.dir_name);
        let entries = collect_relative_files(&outer_dir);
        // 解析每份嵌套 info；解析不出来的当随包资源（实测 `宜/` 里的 `info_yil`）
        let mut parsed: Vec<(PathBuf, LevelSetMetadata)> = Vec::new();
        for info_path in find_nested_info_files(&outer_dir) {
            match parse_info(&info_path) {
                Ok(metadata) => parsed.push((info_path, metadata)),
                Err(reason) => log::debug!(
                    "拍平：嵌套 info 不是关卡集，按资源保留：文件={}，原因={}",
                    info_path.display(),
                    reason
                ),
            }
        }
        if parsed.is_empty() {
            log::info!(
                "拍平：目录里没有可识别的关卡集，跳过：目录={}",
                candidate.dir_name
            );
            continue;
        }
        /*
         * 插件归属复用 ZIP 导入的两级规则：`assign_plugins` 只认「与 info 同
         * 目录」和「`plugins*` 子树」两种位置，正好覆盖作者的真实写法。
         */
        let set_dirs: Vec<(String, LevelSetMetadata, String)> = parsed
            .iter()
            .map(|(info_path, metadata)| {
                let source_dir = info_path.parent().unwrap_or(&outer_dir);
                let relative = relative_forward(&outer_dir, source_dir);
                let base = sanitize_dir_name(&metadata.name)
                    .or_else(|| {
                        source_dir
                            .file_name()
                            .and_then(|name| sanitize_dir_name(&name.to_string_lossy()))
                    })
                    .unwrap_or_else(|| "LevelSet".to_string());
                (relative, metadata.clone(), base)
            })
            .collect();
        let plugin_owner = assign_plugins(&entries, &set_dirs);
        // 同一批里多张集共用同一个基础名时全部带后缀（与 ZIP 导入同规则）
        for (_, _, base) in &set_dirs {
            *base_counts.entry(base.to_lowercase()).or_default() += 1;
        }
        // 同目录资源只归第一张集：两个集共处一目录时资源归属无从判断
        let mut claimed: HashSet<PathBuf> = HashSet::new();
        /*
         * 只有**可解析**的 info 属于各自的集；解析不出来的 info*（实测
         * `宜/` 里的 `info_yil`）是随包资源，跟着第一张集一起搬走。
         */
        let parsed_infos: HashSet<PathBuf> = parsed.iter().map(|(path, _)| path.clone()).collect();
        for (index, (info_path, metadata)) in parsed.into_iter().enumerate() {
            let source_dir = info_path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| outer_dir.clone());
            let mut resources = Vec::new();
            if let Ok(siblings) = fs::read_dir(&source_dir) {
                let mut siblings: Vec<_> = siblings.flatten().collect();
                siblings.sort_by_key(|entry| entry.file_name());
                for sibling in siblings {
                    if !sibling.file_type().is_ok_and(|kind| kind.is_file()) {
                        continue;
                    }
                    let path = sibling.path();
                    let name = sibling.file_name().to_string_lossy().into_owned();
                    if parsed_infos.contains(&path) || name.to_lowercase().ends_with(".dll") {
                        continue;
                    }
                    if claimed.insert(path.clone()) {
                        resources.push(path);
                    }
                }
            }
            let mut plugins = Vec::new();
            let mut plugin_resources = Vec::new();
            for entry in plugin_owner
                .get(&set_dirs[index].0)
                .cloned()
                .unwrap_or_default()
            {
                let path = outer_dir.join(&entry);
                if !path.is_file() {
                    continue;
                }
                if !claimed.insert(path.clone()) {
                    continue;
                }
                plugins.push(path.clone());
                // 插件同目录的资源（配置、依赖文件）一起搬，否则插件找不到它们
                if let Some(parent) = path.parent() {
                    if let Ok(siblings) = fs::read_dir(parent) {
                        let mut siblings: Vec<_> = siblings.flatten().collect();
                        siblings.sort_by_key(|entry| entry.file_name());
                        for sibling in siblings {
                            if !sibling.file_type().is_ok_and(|kind| kind.is_file()) {
                                continue;
                            }
                            let sibling_path = sibling.path();
                            let name = sibling.file_name().to_string_lossy().into_owned();
                            if name.to_lowercase().ends_with(".dll") {
                                continue;
                            }
                            if claimed.insert(sibling_path.clone()) {
                                plugin_resources.push(sibling_path);
                            }
                        }
                    }
                }
            }
            plans.push(PlannedFlattenSet {
                outer: candidate.dir_name.clone(),
                info_path,
                metadata,
                base_name: set_dirs[index].2.clone(),
                dir_name: String::new(),
                resources,
                plugins,
                plugin_resources,
            });
        }
    }

    /*
     * 目录名与 ZIP 导入同一套消歧：先避开 levels 与停用根里已有的目录名，
     * 同一批重名的全部带后缀 —— 结果不取决于目录遍历顺序。
     */
    let mut taken = collect_dir_names(&levels_dir);
    taken.extend(collect_dir_names(&disabled_root(game_root)));
    for plan in &mut plans {
        let allow_base = base_counts
            .get(&plan.base_name.to_lowercase())
            .copied()
            .unwrap_or(0)
            < 2;
        plan.dir_name = allocate_dir_name(&plan.base_name, &plan.metadata, &mut taken, allow_base);
    }

    let mut flattened = 0usize;
    let mut failed: Vec<String> = Vec::new();
    for candidate in &candidates {
        let indices: Vec<usize> = plans
            .iter()
            .enumerate()
            .filter(|(_, plan)| plan.outer == candidate.dir_name)
            .map(|(index, _)| index)
            .collect();
        if indices.is_empty() {
            continue;
        }
        let outer_dir = levels_dir.join(&candidate.dir_name);
        let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
        let mut created: Vec<PathBuf> = Vec::new();
        match execute_flatten_group(&plans, &indices, &levels_dir, &mut moved, &mut created) {
            Ok(count) => {
                flattened += count;
                let leftovers = collect_files(game_root, &outer_dir).unwrap_or_default();
                if !leftovers.is_empty() {
                    if let Err(error) = crate::recycle_bin::stage(
                        recycle_root,
                        manifests_dir,
                        game_root,
                        candidate.dir_name.clone(),
                        None,
                        None,
                        Vec::new(),
                        leftovers,
                        None,
                        &mut || Ok(()),
                    ) {
                        log::warn!(
                            "拍平：目录剩余文件进回收站失败，保留现场：目录={}，原因={}",
                            candidate.dir_name,
                            error
                        );
                    }
                }
                remove_empty_directories(&outer_dir);
                log::info!("拍平完成：目录={}，关卡集={}", candidate.dir_name, count);
            }
            Err(reason) => {
                rollback_flatten_group(&moved, &created);
                log::warn!("拍平失败：目录={}，原因={}", candidate.dir_name, reason);
                failed.push(format!("{}: {}", candidate.dir_name, reason));
            }
        }
    }
    log::info!(
        "一键拍平完成：候选={}，成功={}，失败={}",
        candidates.len(),
        flattened,
        failed.len()
    );
    Ok(LevelSetFlattenReport {
        candidates,
        flattened,
        failed,
    })
}

/// 执行一个外层目录下的全部搬移；任一步失败由调用方回滚。
fn execute_flatten_group(
    plans: &[PlannedFlattenSet],
    indices: &[usize],
    levels_dir: &Path,
    moved: &mut Vec<(PathBuf, PathBuf)>,
    created: &mut Vec<PathBuf>,
) -> Result<usize, String> {
    for &index in indices {
        let plan = &plans[index];
        let target = levels_dir.join(&plan.dir_name);
        if target.exists() {
            return Err(crate::locale_tag::with_params(
                "levelSet.targetExists",
                &[("name", &plan.dir_name)],
            ));
        }
        fs::create_dir_all(&target).map_err(|error| error.to_string())?;
        created.push(target.clone());
        // `info_*` 落成固定名 `info`：DLL 只认这个（与 ZIP 导入同一规则）
        move_flatten_file(&plan.info_path, &target.join("info"), moved)?;
        for resource in &plan.resources {
            let Some(name) = resource.file_name() else {
                continue;
            };
            move_flatten_file(resource, &target.join(name), moved)?;
        }
        for plugin in &plan.plugins {
            let Some(name) = plugin.file_name() else {
                continue;
            };
            move_flatten_file(plugin, &target.join(name), moved)?;
        }
        for resource in &plan.plugin_resources {
            let Some(name) = resource.file_name() else {
                continue;
            };
            move_flatten_file(resource, &target.join(name), moved)?;
        }
    }
    Ok(indices.len())
}

/// 搬一个文件并记下「从哪到哪」，供失败时逆序还原。
fn move_flatten_file(
    from: &Path,
    to: &Path,
    moved: &mut Vec<(PathBuf, PathBuf)>,
) -> Result<(), String> {
    if to.exists() {
        return Err(crate::locale_tag::with_params(
            "levelSet.targetExists",
            &[("name", &to.to_string_lossy().to_string())],
        ));
    }
    fs::rename(from, to).map_err(|error| {
        crate::locale_tag::with_params("levelSet.writeFailed", &[("reason", &error.to_string())])
    })?;
    moved.push((from.to_path_buf(), to.to_path_buf()));
    Ok(())
}

/// 回滚一个外层目录的搬移：文件逆序搬回原处，再删掉新建的集目录。
fn rollback_flatten_group(moved: &[(PathBuf, PathBuf)], created: &[PathBuf]) {
    for (from, to) in moved.iter().rev() {
        if let Some(parent) = from.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if fs::rename(to, from).is_err() {
            log::warn!(
                "拍平回滚失败，文件留在新位置：从={}，到={}",
                to.display(),
                from.display()
            );
        }
    }
    for directory in created.iter().rev() {
        remove_empty_directories(directory);
    }
}
