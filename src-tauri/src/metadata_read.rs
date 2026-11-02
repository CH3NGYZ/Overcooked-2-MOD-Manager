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
//! 去重、查缓存、按上限并发、按原顺序返回。解析本身仍只各有一份实现，本模块
//! 不解析任何字节。
//!
//! ## 并发上限
//!
//! 解析 `info` 是 CPU 密集（几百 MB 的 UnityFS 要 LZMA 解压 + 走对象表），
//! 读 DLL 是 IO 密集但文件很小（本机实测 20 个插件共 0.9 MB）。两者默认值
//! 不同，且都能在设置里调（`mod_metadata_concurrency` / `map_info_concurrency`），
//! 见 [`resolve_concurrency`]。
//!
//! ## 线程模型
//!
//! 每个批次临时起 `workers` 个线程做完就退出，不做常驻线程池：一个批次的
//! 线程创建开销是几十微秒，而常驻池会带来生命周期、退出与测试隔离问题，
//! 对这个使用强度不值。动态取号（`AtomicUsize`）而不是静态切片，因为同一个
//! 批次里文件大小能差几百倍（最大 101 MB、最小 0.3 MB），静态切分会让拿到
//! 大文件的线程成为长尾。
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::UNIX_EPOCH;

use crate::asset_bundle::LevelSetMetadata;
use crate::mods::{DllDetails, read_dll_details};

/// 配置值取它时表示「用全部 CPU 核心」。
pub(crate) const AUTO_CONCURRENCY: u32 = 0;

/// 解析地图 `info` 的默认并发上限。
///
/// 地图的临界路径通常是**单份巨大的 `info`**（本机实测 13 份合计 336.9 MB，
/// 最大一份 101 MB）：它无法再拆，4 路与满核在这条路径上耗时相同，其余文件
/// 由剩下的 3 个线程并行就够盖住。默认 4 既跑满 IO/CPU 流水，又不会在机械盘
/// 上把随机读打散。
pub(crate) const DEFAULT_MAP_INFO_CONCURRENCY: u32 = 4;

/// DLL 元数据缓存条数上限。超过就整体清空（与 `level_sets` 的 `info` 缓存
/// 同一套策略：这种缓存只服务于「反复扫同一批文件」，不需要精细淘汰）。
const DLL_DETAILS_CACHE_LIMIT: usize = 512;

/// 当前机器的可用核心数，至少 1。
pub(crate) fn cores() -> usize {
    std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(1)
        .max(1)
}

/// 把配置里的并发值解析成实际线程数。
///
/// `0` = 全部核心；其余夹到 `1..=cores`。上限永远按**当前机器**算，配置里
/// 存的是用户意图（例如 8），换到 4 核机器上按 4 跑，不会起 8 个线程。
pub(crate) fn resolve_concurrency(stored: u32, cores: usize) -> usize {
    let cores = cores.max(1);
    if stored == AUTO_CONCURRENCY {
        cores
    } else {
        (stored as usize).clamp(1, cores)
    }
}

/// 配置值 → 本机实际线程数（`0` = 全部核心）。
///
/// 调用方只关心「这次批次开几个线程」，不该各自重复 `resolve_concurrency(x,
/// cores())`：写两遍就有一处会忘了夹紧。
pub(crate) fn workers_from(stored: u32) -> usize {
    resolve_concurrency(stored, cores())
}

/// 保存设置时把并发值夹到本机允许的范围（`0` = 全部核心，原样保留）。
///
/// 与 [`resolve_concurrency`] 的区别：那个返回「这次开几个线程」，这个返回
/// 「写回配置文件的值」——`0` 不能在这里被展开成核心数，否则换到核心更多的
/// 机器上就永远是旧机器的上限了。
pub(crate) fn clamp_stored_concurrency(stored: u32) -> u32 {
    if stored == AUTO_CONCURRENCY {
        AUTO_CONCURRENCY
    } else {
        stored.clamp(1, cores() as u32)
    }
}

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
    let results = run_bounded(&jobs, workers, |index, job| {
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

/// 通用有界并发执行器。
///
/// 结果按原下标升序返回；某一条 panic 只让那一条变成 `None`（同批其它项照常
/// 返回），调用方决定怎么降级。
pub(crate) fn run_bounded<T, R>(
    items: &[T],
    workers: usize,
    run: impl Fn(usize, &T) -> R + Sync,
) -> Vec<(usize, Option<R>)>
where
    T: Sync,
    R: Send,
{
    if items.is_empty() {
        return Vec::new();
    }
    let workers = workers.clamp(1, items.len());
    let next = AtomicUsize::new(0);
    let mut collected: Vec<(usize, Option<R>)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                let next = &next;
                let run = &run;
                scope.spawn(move || {
                    let mut local: Vec<(usize, Option<R>)> = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        if index >= items.len() {
                            break;
                        }
                        let value = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            run(index, &items[index])
                        }))
                        .ok();
                        if value.is_none() {
                            log::warn!("元数据读取任务 panic，已隔离：下标={index}");
                        }
                        local.push((index, value));
                    }
                    local
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().unwrap_or_default())
            .collect()
    });
    collected.sort_by_key(|(index, _)| *index);
    collected
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

/// 清空 DLL 元数据缓存（测试用；也用于验证「文件变了必须重读」）。
#[cfg(test)]
pub(crate) fn clear_dll_details_cache() {
    if let Ok(mut cache) = dll_details_cache().lock() {
        cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("oc2-metadata-read-{tag}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).expect("应能创建临时目录");
        dir
    }

    #[test]
    fn resolve_concurrency_treats_zero_as_all_cores_and_clamps_above() {
        assert_eq!(resolve_concurrency(0, 8), 8);
        assert_eq!(resolve_concurrency(4, 8), 4);
        assert_eq!(resolve_concurrency(1, 8), 1);
        assert_eq!(resolve_concurrency(99, 8), 8, "超过核心数按核心数算");
        assert_eq!(resolve_concurrency(0, 0), 1, "核心数兜底至少 1");
    }

    #[test]
    fn run_bounded_never_exceeds_the_worker_limit() {
        let items: Vec<usize> = (0..16).collect();
        let in_flight = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let results = run_bounded(&items, 4, |_, _| {
            let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(20));
            in_flight.fetch_sub(1, Ordering::SeqCst);
            true
        });
        assert!(peak.load(Ordering::SeqCst) <= 4, "并发峰值不得超过 4");
        assert_eq!(results.len(), 16);
        assert!(
            results
                .iter()
                .all(|(index, value)| value.is_some() && *index < 16)
        );
    }

    #[test]
    fn run_bounded_keeps_request_order_and_isolates_panics() {
        let items: Vec<usize> = (0..6).collect();
        let results = run_bounded(&items, 3, |index, _| {
            if index == 2 {
                panic!("故意 panic");
            }
            std::thread::sleep(std::time::Duration::from_millis(10u64 * (6 - index) as u64));
            index
        });
        let indexes: Vec<usize> = results.iter().map(|(index, _)| *index).collect();
        assert_eq!(indexes, vec![0, 1, 2, 3, 4, 5], "必须按原下标升序返回");
        assert!(results[2].1.is_none(), "panic 的那一条只丢自己");
        assert!(results[0].1.is_some() && results[5].1.is_some());
    }

    /// 测试用的最小可解析 DLL：真实文件 + BepInPlugin 元数据。
    fn write_fake_dll(path: &Path, guid: &str, version: &str) {
        let mut bytes = b"MZ".to_vec();
        bytes.extend_from_slice(&crate::mods::plugin_blob(guid, "Fake Mod", version));
        fs::write(path, bytes).expect("应能写入假 DLL");
    }

    #[test]
    fn batch_returns_one_outcome_per_request_in_order_and_isolates_errors() {
        let dir = temp_dir("order");
        let good = dir.join("good.dll");
        write_fake_dll(&good, "com.example.good", "1.2.3");
        let missing = dir.join("missing.dll");
        let requests = vec![
            // `read_dll_details` 对缺失文件不报错（按空字节降级），
            // 用返回 `Result` 的 info 读取来验「失败只错自己」。
            MetadataRequest {
                path: missing.clone(),
                display_name: String::new(),
                kind: MetadataKind::LevelSetInfo,
            },
            MetadataRequest {
                path: good.clone(),
                display_name: "good.dll".into(),
                kind: MetadataKind::ModDll,
            },
        ];
        let outcomes = read_metadata_batch(&requests, 2, &|_, _| {});
        assert_eq!(outcomes.len(), 2, "长度必须与请求一一对应");
        assert!(outcomes[0].error.is_some(), "缺失文件只错自己");
        assert!(
            outcomes[1].mod_dll().is_some(),
            "另一个文件不受影响：{:?}",
            outcomes[1].error
        );
        assert_eq!(
            outcomes[1].value.as_ref().and_then(MetadataValue::version),
            Some("1.2.3"),
            "版本号走统一出口"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn duplicate_requests_read_the_file_once() {
        let dir = temp_dir("dedupe");
        let dll = dir.join("dup.dll");
        write_fake_dll(&dll, "com.example.dup", "2.0.0");
        let request = |display: &str| MetadataRequest {
            path: dll.clone(),
            display_name: display.into(),
            kind: MetadataKind::ModDll,
        };
        let seen = AtomicUsize::new(0);
        let outcomes =
            read_metadata_batch(&[request("dup.dll"), request("dup.dll")], 2, &|_, _| {
                seen.fetch_add(1, Ordering::SeqCst);
            });
        assert_eq!(seen.load(Ordering::SeqCst), 1, "同一文件只读一次");
        assert_eq!(outcomes.len(), 2);
        assert!(outcomes.iter().all(|outcome| outcome.mod_dll().is_some()));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn dll_cache_hits_until_the_file_changes() {
        let dir = temp_dir("cache");
        let dll = dir.join("cached.dll");
        write_fake_dll(&dll, "com.example.cached", "1.0.0");
        clear_dll_details_cache();
        let requests = [MetadataRequest {
            path: dll.clone(),
            display_name: "cached.dll".into(),
            kind: MetadataKind::ModDll,
        }];
        let first = read_metadata_batch(&requests, 1, &|_, _| {});
        assert_eq!(
            first[0].value.as_ref().and_then(MetadataValue::version),
            Some("1.0.0")
        );
        // 覆盖成新版本：mtime 变化后必须重读，不能命中旧缓存。
        std::thread::sleep(std::time::Duration::from_millis(20));
        write_fake_dll(&dll, "com.example.cached", "9.9.9");
        let second = read_metadata_batch(&requests, 1, &|_, _| {});
        assert_eq!(
            second[0].value.as_ref().and_then(MetadataValue::version),
            Some("9.9.9"),
            "文件变了必须重新解析"
        );
        clear_dll_details_cache();
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_batch_returns_no_outcomes() {
        let outcomes = read_metadata_batch(&[], 4, &|_, _| {});
        assert!(outcomes.is_empty());
    }

    /// 真实地图目录的端到端基准（`#[ignore]`，本地手动跑）。
    ///
    /// 进程内缓存是冷的，所以要用两个进程对比：
    ///
    /// ```text
    /// OC2_BENCH_WORKERS=1 cargo test --manifest-path src-tauri/Cargo.toml --lib -- --ignored --nocapture bench_real_metadata_batch
    /// OC2_BENCH_WORKERS=4 cargo test --manifest-path src-tauri/Cargo.toml --lib -- --ignored --nocapture bench_real_metadata_batch
    /// ```
    ///
    /// 目录用 `OC2_BENCH_LEVELS` 覆盖，默认取开发者本机的游戏安装。磁盘缓存
    /// （`cache/level-info`）两边都是热的，所以量的是「之后每次启动」的成本；
    /// 首次导入的解析成本见 `asset_bundle::bench_real_level_sets`。
    #[test]
    #[ignore]
    fn bench_real_metadata_batch() {
        use std::time::Instant;

        let root = PathBuf::from(std::env::var("OC2_BENCH_LEVELS").unwrap_or_else(|_| {
            "E:/SteamLibrary/steamapps/common/Overcooked! 2/BepInEx/plugins/OC2DIYLevel".into()
        }));
        if !root.is_dir() {
            eprintln!("跳过：没有关卡集目录（设置 OC2_BENCH_LEVELS 指向 OC2DIYLevel）");
            return;
        }
        let workers = std::env::var("OC2_BENCH_WORKERS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or_else(cores);
        let mut requests = Vec::new();
        for side in ["levels", "disabled"] {
            let Ok(entries) = fs::read_dir(root.join(side)) else {
                continue;
            };
            for entry in entries.flatten() {
                let dir = entry.path();
                if !dir.is_dir() {
                    continue;
                }
                let Ok(files) = fs::read_dir(&dir) else {
                    continue;
                };
                for file in files.flatten() {
                    let name = file.file_name().to_string_lossy().into_owned();
                    if !name.starts_with("info") {
                        continue;
                    }
                    requests.push(MetadataRequest {
                        path: file.path(),
                        display_name: String::new(),
                        kind: MetadataKind::LevelSetInfo,
                    });
                    break;
                }
            }
        }
        if requests.is_empty() {
            eprintln!("跳过：{} 下一个 info 都没找到", root.display());
            return;
        }
        let total_bytes: u64 = requests
            .iter()
            .filter_map(|request| fs::metadata(&request.path).ok())
            .map(|meta| meta.len())
            .sum();
        let start = Instant::now();
        let outcomes = read_metadata_batch(&requests, workers, &|_, _| {});
        let elapsed = start.elapsed();
        let failed = outcomes
            .iter()
            .filter(|outcome| outcome.error.is_some())
            .count();
        for (request, outcome) in requests.iter().zip(&outcomes) {
            if let Some(error) = &outcome.error {
                eprintln!("失败：{}（{}）", request.path.display(), error);
            }
        }
        eprintln!(
            "元数据批读：文件={} 合计={:.1} MB 并行度={} 耗时={:.2} s 失败={}",
            requests.len(),
            total_bytes as f64 / (1024.0 * 1024.0),
            workers,
            elapsed.as_secs_f64(),
            failed
        );
        assert_eq!(outcomes.len(), requests.len());
    }
}
