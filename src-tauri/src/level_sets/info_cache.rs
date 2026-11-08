//! info 解析的进程指纹与内容哈希磁盘缓存。

use crate::asset_bundle;
use crate::asset_bundle::LevelSetMetadata;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::UNIX_EPOCH;

/// 单个 `info` 文件的解析结果缓存条目。
pub(super) struct InfoCacheEntry {
    /// 解析时的文件长度。
    pub(super) len: u64,
    /// 解析时的修改时间（Unix 纳秒）；拿不到时记 0。
    pub(super) mtime_nanos: u128,
    /// 解析结果（成功或失败 tag）。
    pub(super) result: Result<LevelSetMetadata, String>,
    /// 这个 key 累计真正读盘解析的次数（缓存命中不增加）。
    pub(super) parses: u32,
}

/// 缓存条数上限。
///
/// 关卡集数量是几十的量级，512 已远超真实规模。超限直接清空重来 ——
/// 缓存只是加速，为它维护 LRU 淘汰反而多一份要出错的状态。
const INFO_CACHE_LIMIT: usize = 512;

pub(super) fn info_cache() -> &'static Mutex<HashMap<PathBuf, InfoCacheEntry>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, InfoCacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 缓存键：`info` 文件的绝对路径。
///
/// 用绝对路径而不是传入的原样路径：同一个 `info` 可能通过不同相对根被访问到，
/// 统一成绝对路径才能共用同一条缓存。`current_dir` 失败时退回原路径 ——
/// 那只可能发生在工作目录已被删除的极端情况，退化成不共享缓存也不会出错。
pub(super) fn info_cache_key(info_path: &Path) -> PathBuf {
    if info_path.is_absolute() {
        return info_path.to_path_buf();
    }
    match std::env::current_dir() {
        Ok(cwd) => cwd.join(info_path),
        Err(_) => info_path.to_path_buf(),
    }
}

/// 磁盘缓存目录名（位于数据根的 `cache/` 下）。
const INFO_DISK_CACHE_DIR: &str = "level-info";

/// 磁盘缓存格式版本。解析逻辑或字段有变动时递增，旧文件自然失效。
///
/// 2：LZMA 解码从纯 Rust 的 `lzma-rs` 换成 liblzma。换之前 liblzma 的头部
/// 参数写错，一批 `info` 解析失败并把**失败结果**写进了缓存；失败结果同样
/// 按内容哈希命中，不递增版本的话修好的实现也会一直读到那份旧的失败。
const INFO_DISK_CACHE_SCHEMA: u32 = 2;

/// 磁盘缓存条目。
#[derive(serde::Serialize, serde::Deserialize)]
struct InfoDiskEntry {
    schema: u32,
    /// 解析这份结果时文件内容的 SHA-256（十六进制）。
    ///
    /// 冗余存一份是为了自校验：文件名就是哈希，但文件被手工改过时能立刻发现。
    sha256: String,
    /// 解析结果（成功或失败 tag）。
    result: Result<LevelSetMetadata, String>,
}

/// 磁盘缓存目录；数据根不可用时返回 `None`（退化成只有内存缓存）。
fn info_disk_cache_dir() -> Option<PathBuf> {
    crate::user_paths::root()
        .ok()
        .map(|root| root.join("cache").join(INFO_DISK_CACHE_DIR))
}

/// 内容哈希：SHA-256 十六进制小写。
///
/// 为什么用内容哈希而不是 (len, mtime)：用户的需求是「文件没变就直接读缓存」。
/// mtime 会在复制、解压、同步工具 touch 之后变化，而内容完全一样；内容哈希
/// 是唯一与「文件到底变没变」等价的判据。
///
/// 为什么选 SHA-256 而不是 MD5：实测 40 MB 数据 SHA-256 285.8 MB/s、MD5
/// 408.6 MB/s（.NET 同栈对比），差距约 20 ms/40 MB；而 Rust 的 `sha2` 走
/// SHA-NI 硬件指令还要更快。为这点差距引入一个碰撞更弱的算法不划算。
pub(super) fn content_sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// 读磁盘缓存；不存在、格式不符或哈希对不上时返回 `None`。
///
/// `dir` 由调用方传入（而不是内部取数据根）：测试要用临时目录，
/// 不能往用户真实的数据根里写缓存文件。
pub(super) fn read_info_disk_cache(
    dir: &std::path::Path,
    sha256: &str,
) -> Option<Result<LevelSetMetadata, String>> {
    let path = dir.join(format!("{sha256}.json"));
    let bytes = fs::read(&path).ok()?;
    let entry: InfoDiskEntry = serde_json::from_slice(&bytes).ok()?;
    if entry.schema != INFO_DISK_CACHE_SCHEMA || entry.sha256 != sha256 {
        return None;
    }
    Some(entry.result)
}

/// 写磁盘缓存。失败静默忽略：缓存只是加速，写不进去不能影响解析结果。
pub(super) fn write_info_disk_cache(
    dir: &std::path::Path,
    sha256: &str,
    result: &Result<LevelSetMetadata, String>,
) {
    if fs::create_dir_all(dir).is_err() {
        return;
    }
    let entry = InfoDiskEntry {
        schema: INFO_DISK_CACHE_SCHEMA,
        sha256: sha256.to_string(),
        result: result.clone(),
    };
    let Ok(bytes) = serde_json::to_vec(&entry) else {
        return;
    };
    // 先写临时文件再改名：进程被杀在写一半时，不会留下一个「能读到但内容残缺」
    // 的缓存文件（那会让下一次解析直接拿到坏 JSON 而白白丢一次命中）。
    let target = dir.join(format!("{sha256}.json"));
    let temp = dir.join(format!("{sha256}.json.tmp"));
    if fs::write(&temp, &bytes).is_err() {
        return;
    }
    if fs::rename(&temp, &target).is_err() {
        let _ = fs::remove_file(&temp);
    }
}

/// 清空磁盘缓存目录，返回删除的文件数。只删本模块写出的文件。
///
/// `dir` 由调用方传入的理由与读写一致：测试要用临时目录，不能动用户真实数据根。
/// 目录里只认 `*.json` 与写一半的 `*.json.tmp` —— `cache/` 由多个模块共用，
/// 不按文件名过滤会连带删掉别人的缓存。
pub(super) fn clear_info_disk_cache(dir: &Path) -> Result<usize, String> {
    if !dir.is_dir() {
        return Ok(0);
    }
    let mut removed = 0usize;
    for entry in fs::read_dir(dir).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let ours = matches!(
            path.extension().and_then(|ext| ext.to_str()),
            Some("json") | Some("tmp")
        );
        if ours && path.is_file() && fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

/// 清空 `info` 的两级解析缓存，返回删除的磁盘缓存文件数。
///
/// 两层都要清：只清磁盘的话，本进程的 `(len, mtime)` 命中仍会返回旧结果；
/// 只清内存的话，下一次解析立刻从磁盘读回同一份 —— 两种都让用户觉得
/// 「点了没反应」。这与头像缓存的处理是同一个理由。
///
/// 为什么需要手动清：缓存的键是**内容哈希**，只要文件内容不变就永远命中。
/// 解析逻辑出过错（旧版本把某个字段读错）时，修好的版本面对同一份文件
/// 仍会拿到那份错误结果，只有清掉缓存才会重新解析。
pub fn clear_info_cache() -> Result<usize, String> {
    match info_cache().lock() {
        Ok(mut cache) => cache.clear(),
        /*
         * 锁中毒说明持锁线程 panic 过。这里与读路径的处理相反：读路径放弃写入
         * （反正下次还会重新解析），而清空正是用户要的结果 —— 取回内部数据
         * 继续清，否则按钮点了等于没点。
         */
        Err(poison) => poison.into_inner().clear(),
    }
    match info_disk_cache_dir() {
        Some(dir) => clear_info_disk_cache(&dir),
        None => Ok(0),
    }
}

/// 读 `info` 并解析，命中缓存时不再读盘。
pub(crate) fn read_info_metadata(info_path: &Path) -> Result<LevelSetMetadata, String> {
    let key = info_cache_key(info_path);
    let (len, mtime_nanos) = match fs::metadata(&key) {
        Ok(metadata) => (
            metadata.len(),
            metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|delta| delta.as_nanos())
                .unwrap_or(0),
        ),
        // 元数据读不到就无从判断缓存是否仍然有效，直接报错 ——
        // 与「不缓存时读文件失败」的行为保持一致，不悄悄返回旧结果
        Err(error) => return Err(error.to_string()),
    };
    if let Ok(cache) = info_cache().lock() {
        if let Some(entry) = cache.get(&key) {
            if entry.len == len && entry.mtime_nanos == mtime_nanos {
                return entry.result.clone();
            }
        }
    }
    /*
     * 两级缓存：
     *   1. 进程内 (len, mtime) 指纹 —— 命中不读盘，是最快的路径；
     *   2. 磁盘内容哈希 —— 跨进程有效，重启管理器后仍然命中。
     *
     * 第 2 级要先把文件读进来算哈希（几十 MB / 几十毫秒），但这仍比重新解析
     * 便宜一到两个数量级，而且只有「这个进程第一次见到这份内容」时才付。
     */
    let bytes = match fs::read(&key) {
        Ok(bytes) => bytes,
        Err(error) => return Err(error.to_string()),
    };
    let sha256 = content_sha256(&bytes);
    let disk_dir = info_disk_cache_dir();
    let cached = disk_dir
        .as_deref()
        .and_then(|dir| read_info_disk_cache(dir, &sha256));
    let result = match cached {
        Some(cached) => cached,
        None => {
            let parsed = asset_bundle::parse_level_set(&bytes).map_err(|error| error.to_string());
            if let Some(dir) = disk_dir.as_deref() {
                write_info_disk_cache(dir, &sha256, &parsed);
            }
            parsed
        }
    };
    /*
     * 锁只在这几行里持有：解析本身（可能几十毫秒）在锁外做。
     * 锁中毒（持锁线程 panic）时放弃写入缓存 —— 解析结果照常返回，
     * 下一次扫描会重新解析，正确性不受影响。
     */
    if let Ok(mut cache) = info_cache().lock() {
        if cache.len() >= INFO_CACHE_LIMIT {
            cache.clear();
        }
        let previous_parses = cache.get(&key).map(|entry| entry.parses).unwrap_or(0);
        cache.insert(
            key,
            InfoCacheEntry {
                len,
                mtime_nanos,
                result: result.clone(),
                parses: previous_parses.saturating_add(1),
            },
        );
    }
    result
}

#[cfg(test)]
mod test_support;
#[cfg(test)]
pub(super) use test_support::info_cache_parses;
