//! 本地目录枚举、元数据批读与关卡集组装。

use super::paths::disabled_root;
use super::paths::levels_root;
use super::paths::migrate_legacy_disabled_root;
use super::plugin_registry::RegisteredLevelSetPlugin;
use super::plugin_registry::read_registered_plugins;
use super::requirements::read_required_runtime;
use super::zip_plugins::fallback_plugin_info;
use crate::asset_bundle::LevelSetMetadata;
use crate::framework::DIYLEVEL_DIR;
use crate::metadata_read::MetadataKind;
use crate::metadata_read::MetadataOutcome;
use crate::metadata_read::MetadataRequest;
use crate::models::DllPluginInfo;
use crate::models::LevelSetPlugin;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

/// 单个关卡集在磁盘上的状态。
#[derive(Debug, Clone)]
pub struct LevelSetOnDisk {
    /// 目录名（唯一键，也是开关时移动的目录名）。
    pub dir_name: String,
    /// `true` = 在 `levels/`，`false` = 在 `BepInEx/cache/modmanager_disabled_maps/`。
    pub enabled: bool,
    /// 目录的绝对路径（启用集 `levels/<目录名>`，停用集 `BepInEx/cache/modmanager_disabled_maps/<目录名>`）。
    ///
    /// 由扫描时已经拼好的那个目录直接带出，不要在别处再拼一次 —— 基目录
    /// 换个位置时只改一处。
    pub path: PathBuf,
    /// 解析出的元数据；解析失败时为 `None`（仍可通过 `error` 看到原因）。
    pub metadata: Option<LevelSetMetadata>,
    /// 解析失败的原因（tag），成功时为 `None`。
    ///
    /// **解析失败不影响开关**：卡片照常显示目录名，用户仍然能把它移出去。
    /// 一个坏掉的关卡集不该让整个列表不可用。
    pub error: Option<String>,
    /// `requires.txt` 声明的外部 runtime 最低版本；没有这个文件时为 `None`。
    ///
    /// 只对**本地安装**的包有意义：在线安装的地图以地图源的 `relations` 为准，
    /// 这个值只用来在没有线上声明时兜底（见 `level_set_to_info`）。
    pub required_runtime: Option<String>,
    /// 随这张地图集一起装在集目录里的插件（作者随包的修复 DLL）。
    ///
    /// 绑定关系就是物理位置：DLL 与 `info` 同处集目录，扫描时按文件名读回。
    /// 停用/启用整目录搬家时它们跟着一起走。
    pub plugins: Vec<LevelSetPlugin>,
}

/// 扫描进度回调：`(已完成数, 总数, 当前读到的目录名)`。
///
/// 要求 `Sync` 而不是 `FnMut`：多份 `info` 会被不同线程同时解析，进度自然来自
/// 多个线程。实现方自己负责线程安全（命令层用 `AppHandle::emit`，它本身是线程安全的）。
pub type ScanProgress<'a> = &'a (dyn Fn(usize, usize, &str) + Sync);

/// 扫描一个游戏目录下的全部关卡集（启用与停用）。
///
/// 单测与旧调用方的默认入口：并行度取 `map_info_concurrency` 的默认值（全部核心）。
pub fn scan_level_sets(game_root: &Path) -> Vec<LevelSetOnDisk> {
    scan_level_sets_inner(
        game_root,
        None,
        crate::metadata_read::resolve_concurrency(
            crate::metadata_read::DEFAULT_MAP_INFO_CONCURRENCY,
            crate::metadata_read::cores(),
        ),
        true,
        &|_, _, _| {},
    )
}

/// 同上，但逐条上报进度，并按调用方解析好的并行度读。
///
/// **为什么并行**：解析一份 `info` 是「整份读入 → 算 SHA-256 → LZMA 解压整段
/// 数据 → 解对象表」的独立 CPU 密集工作，彼此没有共享状态。真实目录里 13 个
/// 地图合计 337 MB（最大一份 101 MB），串行时始终只有一个核心在干活。
///
/// **为什么 info 与随包插件排同一批**：两者都由 [`crate::metadata_read`] 调度，
/// 分开排队会让「info 比 DLL 慢几十倍」的长尾空着线程；同一批里动态取号，
/// 谁先空闲谁接下一份。
///
/// **顺序仍然确定**：并行只改变谁先算完，结果按原下标归位；排序口径维持
/// 「先 `levels/` 再 `BepInEx/cache/modmanager_disabled_maps/`，各自按目录名排序」，界面不会因为谁先跑完而重排。
///
/// `manifests_dir` 用来读 BepInEx 表层包的随包插件登记表：那些 DLL 不在集
/// 目录里，归属只能按登记的身份匹配（见 [`read_registered_plugins`]）。传
/// `None` 时只看集目录（安装前身份扫描、单测都不需要登记表）。
pub fn scan_level_sets_with_progress(
    game_root: &Path,
    manifests_dir: Option<&Path>,
    workers: usize,
    progress: ScanProgress<'_>,
) -> Vec<LevelSetOnDisk> {
    scan_level_sets_inner(game_root, manifests_dir, workers, true, progress)
}

/// 扫描本体：`migrate_legacy` 区分「地图页扫描」与「安装前身份扫描」。
///
/// 三段的理由：枚举只读目录与 `requires.txt`（很快，先拿到总数才能给出
/// 「3/13」这种真实进度）；解析全部交给中间层按一批跑（info 与 DLL 混排）；
/// 组装只做纯内存拼装，按原下标归位。
pub(super) fn scan_level_sets_inner(
    game_root: &Path,
    manifests_dir: Option<&Path>,
    workers: usize,
    migrate_legacy: bool,
    progress: ScanProgress<'_>,
) -> Vec<LevelSetOnDisk> {
    let base = game_root.join(DIYLEVEL_DIR);
    // 旧版本的停用根在 `plugins/` 里，先搬走再枚举（见 `migrate_legacy_disabled_root`）
    if migrate_legacy {
        migrate_legacy_disabled_root(game_root);
    }
    let targets = collect_level_set_targets(game_root);
    let total = targets.len();
    progress(0, total, "");
    if total == 0 {
        log::debug!("关卡集扫描完成：数量=0，根={}", base.display());
        return Vec::new();
    }

    /*
     * 一次批次装下全部 `info` 与随包插件：中间层负责按内容去重（同一份文件
     * 被两个关卡集引用时只读一次）与动态取号调度。
     */
    let mut requests: Vec<MetadataRequest> = Vec::new();
    let mut request_set: Vec<Option<usize>> = Vec::new();
    let mut info_request: Vec<Option<usize>> = vec![None; total];
    let mut plugin_requests: Vec<Vec<usize>> = vec![Vec::new(); total];
    let pending: Vec<AtomicUsize> = (0..total).map(|_| AtomicUsize::new(0)).collect();
    for (index, target) in targets.iter().enumerate() {
        if let Some(info_path) = &target.info_path {
            info_request[index] = Some(requests.len());
            requests.push(MetadataRequest {
                path: info_path.clone(),
                display_name: String::new(),
                kind: MetadataKind::LevelSetInfo,
            });
            request_set.push(Some(index));
            pending[index].fetch_add(1, Ordering::Relaxed);
        }
        for (path, file_name, _) in &target.plugins {
            plugin_requests[index].push(requests.len());
            requests.push(MetadataRequest {
                path: path.clone(),
                display_name: file_name.clone(),
                kind: MetadataKind::ModDll,
            });
            request_set.push(Some(index));
            pending[index].fetch_add(1, Ordering::Relaxed);
        }
    }

    /*
     * BepInEx 表层包的随包插件不在集目录里，归属只能按登记表的身份匹配
     * （见 `read_registered_plugins`）。它们与 `info` 塞进同一批：DLL 元数据
     * 读取彼此独立，合批才能让线程池始终有活干。
     *
     * 过期记录（文件已被手动删掉）与「已被手工搬回集目录」的条目不再请求：
     * 前者读不到，后者会与集目录顶层 DLL 重复成两张卡。
     */
    let registered = manifests_dir
        .map(read_registered_plugins)
        .unwrap_or_default();
    let active_registered: Vec<&RegisteredLevelSetPlugin> = registered
        .iter()
        .filter(|entry| {
            let path = game_root.join(&entry.dll);
            path.is_file()
                && !targets
                    .iter()
                    .any(|target| target.plugins.iter().any(|(local, _, _)| local == &path))
        })
        .collect();
    let mut registered_requests: Vec<usize> = Vec::with_capacity(active_registered.len());
    for entry in &active_registered {
        let file_name = entry
            .dll
            .rsplit('/')
            .next()
            .unwrap_or(&entry.dll)
            .to_string();
        registered_requests.push(requests.len());
        requests.push(MetadataRequest {
            path: game_root.join(&entry.dll),
            display_name: file_name,
            kind: MetadataKind::ModDll,
        });
        // 登记项不属于任何一张卡，进度只按「出卡数」算
        request_set.push(None);
    }

    let finished = AtomicUsize::new(0);
    let outcomes = crate::metadata_read::read_metadata_batch(&requests, workers, &|index, _| {
        /*
         * 一张图的 `info` 与随包插件都读完才报它：进度数字等于「已出卡数量」，
         * 而不是「已读文件数」—— 后者会在一张图内部跳好几次，进度条看着乱。
         */
        let Some(set) = request_set[index] else {
            return;
        };
        if pending[set].fetch_sub(1, Ordering::Relaxed) == 1 {
            let done = finished.fetch_add(1, Ordering::Relaxed) + 1;
            progress(done, total, &targets[set].dir_name);
        }
    });

    /*
     * 没有任何可读文件的目标（空目录、只有 `requires.txt`）进不了批次，
     * 批次回调也就不会报它；补一次进度，保证最终「已读取 x/y」对齐。
     *
     * 只补**真的没有请求**的目标。按 `pending == 0` 判断是错的：有请求的
     * 目标读完后计数同样是 0，会被当成「没进批次」再报一遍，进度条最后冲到
     * 2×总数（实测 54 张图收尾显示 108/54）。请求数在建批次时已经知道，
     * 直接看它，不再用运行期计数反推。
     */
    let mut done = finished.load(Ordering::Relaxed);
    for (index, target) in targets.iter().enumerate() {
        if info_request[index].is_none() && plugin_requests[index].is_empty() {
            done += 1;
            progress(done, total, &target.dir_name);
        }
    }

    let mut outcomes: Vec<Option<MetadataOutcome>> = outcomes.into_iter().map(Some).collect();
    let registered_outcomes: Vec<Option<MetadataOutcome>> = registered_requests
        .iter()
        .map(|request| outcomes[*request].take())
        .collect();
    let mut sets = Vec::with_capacity(total);
    for (index, target) in targets.into_iter().enumerate() {
        let info = info_request[index].and_then(|request| outcomes[request].take());
        let plugins = plugin_requests[index]
            .iter()
            .map(|request| outcomes[*request].take())
            .collect::<Vec<_>>();
        /*
         * 登记 DLL 按身份归属到集：一个表层包可能带多张图，登记表按
         * `levelSetName + name_zh + author` 匹配，与冲突判定同一套规则。
         * `info` 读不出来的集拿不到身份，也就不认领任何登记项。
         */
        let registered_plugins = info
            .as_ref()
            .and_then(|outcome| outcome.level_set_info())
            .map(|metadata| {
                active_registered
                    .iter()
                    .enumerate()
                    .filter_map(|(reg_index, entry)| {
                        if !entry.owner.matches(metadata) {
                            return None;
                        }
                        let file_name = entry
                            .dll
                            .rsplit('/')
                            .next()
                            .unwrap_or(&entry.dll)
                            .to_string();
                        // 登记项的元数据按 reg_index 对齐（registered_outcomes 就是按
                        // registered_requests 顺序建的），不能拿 requests 里的全局下标去索引。
                        let info = registered_outcomes
                            .get(reg_index)
                            .and_then(|outcome| outcome.as_ref())
                            .and_then(|outcome| outcome.mod_dll())
                            .cloned()
                            .map(DllPluginInfo::from)
                            .unwrap_or_else(|| fallback_plugin_info(&file_name));
                        Some(LevelSetPlugin {
                            file_name,
                            enabled: true,
                            info,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        sets.push(assemble_level_set(
            target,
            info,
            &plugins,
            registered_plugins,
        ));
    }
    log::debug!(
        "关卡集扫描完成：数量={}，根={}，并行度={}",
        sets.len(),
        base.display(),
        workers
    );
    sets
}

/// 枚举一个游戏目录下全部关卡集目录（启用根 + 停用根，各自按目录名排序）。
fn collect_level_set_targets(game_root: &Path) -> Vec<LevelSetTarget> {
    let mut targets = Vec::new();
    for (root, enabled) in [
        (levels_root(game_root), true),
        (disabled_root(game_root), false),
    ] {
        let Ok(entries) = fs::read_dir(&root) else {
            continue;
        };
        let mut names: Vec<String> = entries
            .flatten()
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        for name in names {
            targets.push(LevelSetTarget::collect(root.join(&name), name, enabled));
        }
    }
    targets
}

/// 一个待读的关卡集目录：只记录「去哪里读什么」，不碰 `info` 内容。
pub(super) struct LevelSetTarget {
    pub(super) directory: PathBuf,
    pub(super) dir_name: String,
    pub(super) enabled: bool,
    pub(super) required_runtime: Option<String>,
    pub(super) info_path: Option<PathBuf>,
    /// 集目录顶层的随包插件：磁盘路径 + 落盘文件名 + 是否启用。
    pub(super) plugins: Vec<(PathBuf, String, bool)>,
}

impl LevelSetTarget {
    pub(super) fn collect(directory: PathBuf, dir_name: String, enabled: bool) -> Self {
        // 与 info 无关，独立读取：info 读不出来时这两个值照样能用
        let required_runtime = read_required_runtime(&directory);
        let info_path = locate_info_file(&directory);
        let plugins = collect_level_set_plugin_files(&directory);
        Self {
            directory,
            dir_name,
            enabled,
            required_runtime,
            info_path,
            plugins,
        }
    }
}

/// 读集目录里的随包插件文件清单：DLL 与 `info` 同处一层，看到就绑，不猜名字。
///
/// 绑定关系不存额外的表 —— 插件导入后本来就被放进集目录根（见
/// [`assign_plugins`]），扫描按文件名读回即可；停用/启用整目录搬家时它们
/// 跟着走，导出地图包时整目录打包也不会漏。`.dll.disabled` 是手动禁用的
/// 副本，照样带出，由 `enabled` 标出来。
///
/// BepInEx 表层包是例外：插件按作者规划留在 `BepInEx/plugins/**`，不在集目录
/// 里，归属由 `manifests/level-set-plugins.json` 登记（[`read_registered_plugins`]），
/// 扫描时按身份并入（见 `scan_level_sets_inner`）。
///
/// 这里只列文件、不解析：元数据由 [`crate::metadata_read`] 统一按批次读，
/// 与 MOD 预览同源（[`crate::mods::read_dll_details`]）。
fn collect_level_set_plugin_files(directory: &Path) -> Vec<(PathBuf, String, bool)> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut plugins: Vec<(PathBuf, String, bool)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        let (base, enabled) = match name.strip_suffix(".disabled") {
            Some(base) => (base, false),
            None => (name.as_str(), true),
        };
        if !base.to_lowercase().ends_with(".dll") {
            continue;
        }
        plugins.push((path, name, enabled));
    }
    plugins.sort_by(|left, right| left.1.cmp(&right.1));
    plugins
}

/// 把「目录清单 + 批次读到的元数据」组装成一张关卡集。
///
/// `info` 缺失或解析失败都只让这张卡带上 `error`，插件与 `requires.txt`
/// 照常保留 —— 解析失败不影响开关（见 [`LevelSetOnDisk::error`]）。
///
/// `registered_plugins` 是 BepInEx 表层包登记在 `manifests/level-set-plugins.json`
/// 里、归属这张图的 DLL：它们不在集目录里，位置与元数据都由调用方按身份匹配好。
pub(super) fn assemble_level_set(
    target: LevelSetTarget,
    info: Option<MetadataOutcome>,
    plugins: &[Option<MetadataOutcome>],
    registered_plugins: Vec<LevelSetPlugin>,
) -> LevelSetOnDisk {
    let (metadata, error) = match (&target.info_path, info) {
        (None, _) => (None, Some("levelSet.infoMissing".to_string())),
        (Some(info_path), Some(outcome)) => match outcome.level_set_info() {
            Some(metadata) => (Some(metadata.clone()), None),
            None => {
                let reason = outcome.error.clone().unwrap_or_default();
                log::warn!(
                    "关卡集元数据解析失败：目录={}，文件={}，原因={}",
                    target.dir_name,
                    info_path.display(),
                    reason
                );
                (None, Some(reason))
            }
        },
        // 批次返回的条数与请求一一对应，走不到这里；真到了也只当解析失败
        (Some(_), None) => (None, Some(String::new())),
    };
    let mut plugins: Vec<LevelSetPlugin> = target
        .plugins
        .iter()
        .enumerate()
        .map(|(index, (_, file_name, enabled))| LevelSetPlugin {
            file_name: file_name.clone(),
            enabled: *enabled,
            info: plugins
                .get(index)
                .and_then(|outcome| outcome.as_ref())
                .and_then(|outcome| outcome.mod_dll())
                .cloned()
                .map(DllPluginInfo::from)
                .unwrap_or_else(|| fallback_plugin_info(file_name)),
        })
        .collect();
    plugins.extend(registered_plugins);
    plugins.sort_by(|left, right| left.file_name.cmp(&right.file_name));
    LevelSetOnDisk {
        dir_name: target.dir_name,
        enabled: target.enabled,
        path: target.directory,
        metadata,
        error,
        required_runtime: target.required_runtime,
        plugins,
    }
}

/// 找到关卡集的 `info` 文件。
///
/// 优先固定名 `info`；否则取唯一一个 `info_*`。有多个 `info_*` 时不猜，
/// 返回 `None` 让上层报「缺少 info」—— 与其加载错的那一个，不如明确失败。
pub(super) fn locate_info_file(directory: &Path) -> Option<PathBuf> {
    let fixed = directory.join("info");
    if fixed.is_file() {
        return Some(fixed);
    }
    let entries = fs::read_dir(directory).ok()?;
    let mut candidates: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| {
            entry.file_type().is_ok_and(|kind| kind.is_file())
                && entry.file_name().to_string_lossy().starts_with("info_")
        })
        .map(|entry| entry.path())
        .collect();
    if candidates.len() == 1 {
        candidates.pop()
    } else {
        None
    }
}
