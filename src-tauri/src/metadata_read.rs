//! 元数据并行读取的中间层。
//!
//! 两个数据线读的是**同一种东西**：一个磁盘文件里的版本号与身份信息。
//!
//! - MOD：`BepInEx/plugins/**/*.dll`（`mods::read_dll_details`：BepInPlugin →
//!   PE 版本资源 → 文件名兜底）；
//! - 地图：关卡集目录里的 `info`（`level_sets::read_info_metadata`：UnityFS
//!   解压 + 对象表解析，自带两级缓存）。
//!
//! 解压与解析是两个完全独立、彼此没有共享状态的工作，但历史上各写了一套
//! 编排：地图侧在 `scan_level_sets_with_progress` 里内联线程池，MOD 侧是串行
//! `map`，ZIP 预览又是一遍串行循环。于是「同时读几个」这件事散落在三处，
//! 谁也不能单独调、也没法配置。
//!
//! 这里把**编排**收拢成一层：调用方只给出「文件路径 + 要哪种元数据」，由本模块
//! 去重、查缓存、委托 concurrency 按上限并发、按原顺序返回。解析本身仍只各有一份实现，本模块
//! 不解析任何字节。
//!
//! ## 并发上限
//!
//! 解析 `info` 是 CPU 密集（几百 MB 的 UnityFS 要 LZMA 解压 + 走对象表），
//! 读 DLL 是 IO 密集但文件很小。两者默认都是 0（全部核心），可分别在设置里
//! 调整（`mod_metadata_concurrency` / `map_info_concurrency`），
//! 见 [`crate::concurrency::resolve_concurrency`]。
//!
//! ## 线程模型
//!
//! 本模块不再创建线程；所有批次复用 concurrency::run_bounded，由共享
//! run_queue 动态领取任务、隔离 panic 并按请求顺序收集结果。
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::UNIX_EPOCH;

use crate::asset_bundle::LevelSetMetadata;
use crate::mods::{DllDetails, read_dll_details};

/// 解析地图 `info` 的默认并发上限。
///
/// 与 DLL 侧同一个默认值（全部 CPU 核心）：一批里既有单份巨大的 `info`
/// （本机实测最大 101 MB），也有几十份小文件，固定 4 路会让小文件排在长尾
/// 后面；满核时大文件仍只占一个线程，其余核心继续消化小文件。用户可以在
/// 设置页改回固定值，真正执行前按本机核心数夹紧。
pub(crate) const DEFAULT_MAP_INFO_CONCURRENCY: u32 = crate::concurrency::AUTO_CONCURRENCY;

/// DLL 元数据缓存条数上限。超过就整体清空（与 `level_sets` 的 `info` 缓存
/// 同一套策略：这种缓存只服务于「反复扫同一批文件」，不需要精细淘汰）。
const DLL_DETAILS_CACHE_LIMIT: usize = 512;

/// 要读哪种元数据。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum MetadataKind {
    /// MOD 插件 DLL（也是地图随包插件）。
    ModDll,
    /// 关卡集的 `info`。
    LevelSetInfo,
}

/// 一次「读这个文件的元数据」的请求。
#[derive(Debug, Clone)]
pub(crate) struct MetadataRequest {
    pub(crate) path: PathBuf,
    /// 仅 `ModDll` 用：读不出 BepInPlugin 名称时用文件名兜底。
    pub(crate) display_name: String,
    pub(crate) kind: MetadataKind,
}

/// 读出来的元数据。
#[derive(Debug, Clone)]
pub(crate) enum MetadataValue {
    ModDll(DllDetails),
    LevelSetInfo(LevelSetMetadata),
}

impl MetadataValue {
    /// 「版本号」的唯一出口：调用方拿到解析结果后从这里取，不再各自认字段。
    pub(crate) fn version(&self) -> Option<&str> {
        let value = match self {
            MetadataValue::ModDll(details) => details.version.as_deref(),
            MetadataValue::LevelSetInfo(metadata) => Some(metadata.version.as_str()),
        };
        value.filter(|version| !version.trim().is_empty())
    }
}

/// 单个文件的结果：要么有值，要么有原因。
///
/// 单个文件失败**不会**让整批失败：一个坏 DLL 不该让整页 MOD 列表消失，
/// 一份坏 `info` 也只应该让那一张地图显示错误（与各调用方原有语义一致）。
#[derive(Debug, Clone)]
pub(crate) struct MetadataOutcome {
    pub(crate) value: Option<MetadataValue>,
    pub(crate) error: Option<String>,
}

impl MetadataOutcome {
    fn ok(value: MetadataValue) -> Self {
        Self {
            value: Some(value),
            error: None,
        }
    }

    fn failed(reason: impl Into<String>) -> Self {
        Self {
            value: None,
            error: Some(reason.into()),
        }
    }

    /// 取 MOD 元数据（类型不对时返回 `None`）。
    pub(crate) fn mod_dll(&self) -> Option<&DllDetails> {
        match &self.value {
            Some(MetadataValue::ModDll(details)) => Some(details),
            _ => None,
        }
    }

    /// 取地图元数据（类型不对时返回 `None`）。
    pub(crate) fn level_set_info(&self) -> Option<&LevelSetMetadata> {
        match &self.value {
            Some(MetadataValue::LevelSetInfo(metadata)) => Some(metadata),
            _ => None,
        }
    }
}

/// 批次进度回调：`(请求下标, 该请求的结果)`。
///
/// 在**工作线程**里逐条调用，要求 `Sync`；实现方自己保证线程安全
/// （地图扫描用原子计数 + `AppHandle::emit`，两者都满足）。
pub(crate) type MetadataProgress<'a> = &'a (dyn Fn(usize, &MetadataOutcome) + Sync);

/// 读一批文件的元数据。
///
/// 返回值长度与顺序**严格对应** `requests`：调用方可以按下标直接落回自己的
/// 数据结构，不需要按路径反查。重复的 `(kind, canonical path)` 只读一次。
pub(crate) fn read_metadata_batch(
    requests: &[MetadataRequest],
    workers: usize,
    progress: MetadataProgress<'_>,
) -> Vec<MetadataOutcome> {
    if requests.is_empty() {
        return Vec::new();
    }

    /*
     * 去重：同一个文件被两条线引用（ZIP 里重复条目、同一 DLL 既是包内条目
     * 又是随包插件）时只排一次任务，避免同一份几十 MB 的 info 被并行解析两遍。
     */
    let mut job_index: HashMap<(MetadataKind, PathBuf), usize> = HashMap::new();
    let mut jobs: Vec<MetadataRequest> = Vec::new();
    let mut request_to_job: Vec<usize> = Vec::with_capacity(requests.len());
    for request in requests {
        let key = (request.kind, canonical_key(&request.path));
        let index = *job_index.entry(key).or_insert_with(|| {
            jobs.push(request.clone());
            jobs.len() - 1
        });
        request_to_job.push(index);
    }

    let workers = workers.clamp(1, jobs.len().max(1));
    let results = crate::concurrency::run_bounded(&jobs, workers, |index, job| {
        let outcome = read_one(job);
        progress(index, &outcome);
        outcome
    });

    let mut by_job: Vec<Option<MetadataOutcome>> = vec![None; jobs.len()];
    for (index, outcome) in results {
        by_job[index] = outcome;
    }

    let failed = by_job
        .iter()
        .filter(|outcome| outcome.as_ref().is_some_and(|item| item.error.is_some()))
        .count();
    log::debug!(
        "元数据批次完成：请求数={}，去重后任务数={}，并发={}，失败={}",
        requests.len(),
        jobs.len(),
        workers,
        failed
    );

    request_to_job
        .into_iter()
        .map(|index| {
            by_job[index]
                .clone()
                .unwrap_or_else(|| MetadataOutcome::failed("metadataRead.taskFailed"))
        })
        .collect()
}

/// 读一个文件的元数据（批次内部使用）。
fn read_one(job: &MetadataRequest) -> MetadataOutcome {
    match job.kind {
        MetadataKind::ModDll => {
            let details = read_dll_details_cached(&job.path, &job.display_name);
            MetadataOutcome::ok(MetadataValue::ModDll(details))
        }
        MetadataKind::LevelSetInfo => match crate::level_sets::read_info_metadata(&job.path) {
            Ok(metadata) => MetadataOutcome::ok(MetadataValue::LevelSetInfo(metadata)),
            Err(error) => MetadataOutcome::failed(error),
        },
    }
}

/// 缓存键：能 `canonicalize` 就用规范路径（大小写、短名、verbatim 前缀都对
/// 得上），失败时退回原路径 —— 读不到的文件本来也只会得到一次失败结果。
fn canonical_key(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// DLL 元数据的进程内缓存条目。
struct DllCacheEntry {
    len: u64,
    mtime_nanos: u128,
    details: DllDetails,
}

fn dll_details_cache() -> &'static Mutex<HashMap<(PathBuf, String), DllCacheEntry>> {
    static CACHE: OnceLock<Mutex<HashMap<(PathBuf, String), DllCacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 带进程内缓存的 [`read_dll_details`]。
///
/// 缓存指纹与 `level_sets` 的 `info` 缓存同一套：`(长度, mtime)` —— 文件被
/// 替换（安装、覆盖、手改）时 mtime 会变，指纹不同就不会命中。已知的边界与
/// 那边一样：有工具在保留 mtime 的前提下替换内容时旧值会留着，重启管理器即
/// 恢复（进程内缓存）。
fn read_dll_details_cached(path: &Path, display_name: &str) -> DllDetails {
    let key = canonical_key(path);
    let fingerprint = fs::metadata(&key).ok().map(|metadata| {
        let mtime_nanos = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|delta| delta.as_nanos())
            .unwrap_or(0);
        (metadata.len(), mtime_nanos)
    });
    let cache_key = (key.clone(), display_name.to_string());
    if let Some((len, mtime_nanos)) = fingerprint {
        if let Ok(cache) = dll_details_cache().lock() {
            if let Some(entry) = cache.get(&cache_key) {
                if entry.len == len && entry.mtime_nanos == mtime_nanos {
                    return entry.details.clone();
                }
            }
        }
    }

    let details = read_dll_details(&key, display_name);
    if let Some((len, mtime_nanos)) = fingerprint {
        if let Ok(mut cache) = dll_details_cache().lock() {
            if cache.len() >= DLL_DETAILS_CACHE_LIMIT {
                cache.clear();
            }
            cache.insert(
                cache_key,
                DllCacheEntry {
                    len,
                    mtime_nanos,
                    details: details.clone(),
                },
            );
        }
    }
    details
}

#[cfg(test)]
#[path = "metadata_read/tests.rs"]
mod tests;
