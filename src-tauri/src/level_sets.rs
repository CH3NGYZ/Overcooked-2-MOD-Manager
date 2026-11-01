//! DIYLevel 关卡集的扫描、开关与 ZIP 导入。
//!
//! ## 目录约定
//!
//! ```text
//! BepInEx/plugins/OC2DIYLevel/
//!   ├─ levels/      ← 启用的关卡集
//!   │    └─ <集名>/{info, s_*}
//!   └─ disabled/    ← 停用的关卡集（与 levels 平级）
//!        └─ <集名>/{info, s_*}
//! ```
//!
//! **开关就是移动整个目录**：`OC2DIYLevel.dll` 只枚举 `levels/*/`，它既不认识
//! `disabled` 这个名字，也没有「禁用」标志位（反编译它的字符串表可以确认：
//! `disable` 一次都没出现）。所以把目录移出去就等于卸载，移回来就等于重装，
//! 没有任何需要同步的状态。
//!
//! ## 为什么单关卡不做开关
//!
//! `info` 里的 `levelInfos` 是**整体声明**：它列出这个关卡集包含哪些关卡。
//! 只把某个 `s_*` 文件移走，声明与磁盘就对不上，游戏加载时会找不到资源而
//! 直接崩掉整张自定义地图 —— 比「少一个可玩关卡」严重得多。
//!
//! 所以粒度刻意停在**关卡集**这一层。
//!
//! ## ZIP 导入的形态
//!
//! 社区的关卡集都通过 ZIP 分发，有两种常见形态：
//!
//! 1. 根目录直接是 `info_xxx` + `s_xxx_*`（单关卡集）
//! 2. 根目录下有一个或多个子目录，每个子目录是一个关卡集
//!
//! 还有一种「多层包装」（ZIP 里套一层同名目录再放内容），也要认。
//! 判定方式是**按 `info` 文件定位**：一个 `info` 的父目录就是一个关卡集，
//! 与它同级的 `s_*` 属于它。
//!
//! 导入时**拒绝**下列情况，而不是猜：
//! - ZIP 里一个 `info` 都没有
//! - 同一关卡集在 ZIP 里出现两次
//! - 目标目录已存在同名关卡集（由调用方决定覆盖与否）
use crate::asset_bundle::{self, LevelSetMetadata};
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::UNIX_EPOCH;

/// DIYLevel 框架目录（游戏相对路径）。与 `commands.rs` 的 `DIYLEVEL_DIR` 同源，
/// 但这里是模块内的私有常量，避免两处互相依赖。
const DIYLEVEL_DIR: &str = "BepInEx/plugins/OC2DIYLevel";

/// 启用中的关卡集目录名。
const LEVELS_DIR: &str = "levels";

/// 停用的关卡集目录名。
///
/// 必须与 `levels` **平级**：DLL 只读 `levels/`，所以放在这里它就看不见。
/// 不要改成 `levels/.disabled` 之类的嵌套路径 —— 那会被 DLL 当作一个叫
/// 「.disabled」的关卡集去加载 `info`，找不到就刷一条错误。
const DISABLED_DIR: &str = "disabled";

/// 关卡集目录里声明「外部 runtime 最低版本」的文件。
///
/// 由关卡集导出器写出，内容是**纯版本号**（实测 `3.10.2`、`3.8.8`）。
const REQUIRES_FILE: &str = "requires.txt";

/// 外部 runtime（Web Runtime Loader）的插件 GUID。
///
/// 与官方地图源里 `relations.guid` 写的值**必须一致**：`requires.txt` 与线上
/// relation 是同一个要求的两处表述，guid 不同会让依赖表把它们当成两个插件，
/// 各算一遍。
const RUNTIME_WLOADER_GUID: &str = "oc2.oc2diylevelruntimewloader";

/// 游戏运行时是否允许「移动目录」这类关卡集操作。
///
/// ## 为什么安装可以、开关不行
///
/// 安装是**新增**：写一个游戏本次启动还没扫描过的新目录，即使游戏正在运行，
/// DIYLevel 也只会把它当成「下次启动才出现的地图」，不会读到半个文件。
///
/// 开关是**移动**：DLL 正在 `levels/` 下按目录枚举并加载 `info`，此时把目录
/// 挪到 `disabled/` 会让它正在读的路径消失。卸载（删除）同理。
///
/// ## 为什么做成常量而不是直接禁死
///
/// 这条约束**尚未实机验证**。用户会手动测试「游戏运行时把目录移到
/// `disabled/`」是否安全，确认没问题后把这个常量改成 `true` 即可放开。
///
/// 判定集中在一处，就不会出现「安装放开了、开关忘了拦」这种半放开状态。
pub(crate) const ALLOW_LEVEL_SET_TOGGLE_WHILE_RUNNING: bool = false;

/// 关卡集写操作的类型，决定游戏运行时是否放行。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelSetOperation {
    /// 安装 / 更新：新增内容，运行时允许。
    Install,
    /// 启用 / 停用：移动目录，运行时不安全（待实机验证）。
    Toggle,
}

impl LevelSetOperation {
    /// 该操作在游戏运行时是否允许。
    pub(crate) fn allowed_while_running(self) -> bool {
        match self {
            Self::Install => true,
            Self::Toggle => ALLOW_LEVEL_SET_TOGGLE_WHILE_RUNNING,
        }
    }
}

/// 按操作类型决定是否需要「游戏已停止」。
///
/// 所有关卡集写操作都必须走这里，不要在命令里各写一次 `ensure_game_stopped`
/// —— 策略一改就得改多处，早晚漏掉一处。
pub fn ensure_operation_allowed(
    game_root: &Path,
    operation: LevelSetOperation,
) -> Result<(), String> {
    if !operation.allowed_while_running() && crate::game::is_game_running(game_root) {
        log::warn!("关卡集操作被拒绝：游戏仍在运行，操作={operation:?}");
        return Err("levelSet.gameRunning".into());
    }
    Ok(())
}

/// 单个关卡集在磁盘上的状态。
#[derive(Debug, Clone)]
pub struct LevelSetOnDisk {
    /// 目录名（唯一键，也是开关时移动的目录名）。
    pub dir_name: String,
    /// `true` = 在 `levels/`，`false` = 在 `disabled/`。
    pub enabled: bool,
    /// 目录的绝对路径（启用集 `levels/<目录名>`，停用集 `disabled/<目录名>`）。
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
}

/// 扫描进度回调：`(已完成数, 总数, 当前读到的目录名)`。
///
/// 要求 `Sync` 而不是 `FnMut`：多份 `info` 会被不同线程同时解析，进度自然来自
/// 多个线程。实现方自己负责线程安全（命令层用 `AppHandle::emit`，它本身是线程安全的）。
pub type ScanProgress<'a> = &'a (dyn Fn(usize, usize, &str) + Sync);

/// 扫描一个游戏目录下的全部关卡集（启用与停用）。
pub fn scan_level_sets(game_root: &Path) -> Vec<LevelSetOnDisk> {
    scan_level_sets_with_progress(game_root, &|_, _, _| {})
}

/// 同上，但逐条上报进度。
///
/// **为什么并行**：解析一份 `info` 是「整份读入 → 算 SHA-256 → LZMA 解压整段
/// 数据 → 解对象表」的独立 CPU 密集工作，彼此没有共享状态。真实目录里 7 个地图
/// 合计 131 MB（最大一份 65 MB），串行时始终只有一个核心在干活 —— 冷扫描的
/// 13.6 s 里大部分时间其它核心是空的。按 CPU 核数分片后这些工作真正重叠。
///
/// **顺序仍然确定**：并行只改变谁先算完，结果按原下标归位；排序口径维持
/// 「先 `levels/` 再 `disabled/`，各自按目录名排序」，界面不会因为谁先跑完而重排。
pub fn scan_level_sets_with_progress(
    game_root: &Path,
    progress: ScanProgress<'_>,
) -> Vec<LevelSetOnDisk> {
    let base = game_root.join(DIYLEVEL_DIR);
    /*
     * 先把待解析的目标列全：并行之前必须先知道总数，才能给出「3/7」这种
     * 真实进度（转圈条下面要显示的就是它）。列目录本身只读元数据，很快。
     */
    let mut targets: Vec<(PathBuf, String, bool)> = Vec::new();
    for (dir_name, enabled) in [(LEVELS_DIR, true), (DISABLED_DIR, false)] {
        let Ok(entries) = fs::read_dir(base.join(dir_name)) else {
            continue;
        };
        let mut names: Vec<String> = entries
            .flatten()
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        for name in names {
            targets.push((base.join(dir_name).join(&name), name, enabled));
        }
    }

    let total = targets.len();
    progress(0, total, "");
    if total == 0 {
        log::debug!("关卡集扫描完成：数量=0，根={}", base.display());
        return Vec::new();
    }

    /*
     * 线程数取核数且不超过目标数：文件比核心少时多开线程只会多切换。
     * 用下标原子取号而不是预先切片 —— 各份 `info` 大小差几十倍（这里最大的
     * 65 MB、最小的 0.3 MB），切片会让拿到大文件的线程成为长尾，动态取号
     * 才能让先空闲的线程去接下一个。
     */
    let workers = std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(1)
        .min(total);
    let next = AtomicUsize::new(0);
    let finished = AtomicUsize::new(0);
    let parsed: Vec<(usize, LevelSetOnDisk)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut local: Vec<(usize, LevelSetOnDisk)> = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        if index >= total {
                            break;
                        }
                        let (path, name, enabled) = &targets[index];
                        local.push((index, read_level_set(path.clone(), name.clone(), *enabled)));
                        let done = finished.fetch_add(1, Ordering::Relaxed) + 1;
                        progress(done, total, name);
                    }
                    local
                })
            })
            .collect();
        handles
            .into_iter()
            // 某个线程 panic 时丢掉它那一片，而不是让整次扫描失败：
            // 少一张卡的元数据仍可显示目录名并开关，比整页报错好。
            .flat_map(|handle| handle.join().unwrap_or_default())
            .collect()
    });

    let mut ordered = parsed;
    ordered.sort_by_key(|(index, _)| *index);
    let sets: Vec<LevelSetOnDisk> = ordered.into_iter().map(|(_, set)| set).collect();
    log::debug!(
        "关卡集扫描完成：数量={}，根={}，并行度={}",
        sets.len(),
        base.display(),
        workers
    );
    sets
}

/// 读取单个关卡集目录。
///
/// `info` 的文件名有两种历史写法：固定名 `info`（DLL 认这个），以及导出工具
/// 带的 `info_<集名>`。先找前者，找不到再找唯一一个 `info_*` —— 两种都在
/// 真实资源里出现过。
fn read_level_set(directory: PathBuf, dir_name: String, enabled: bool) -> LevelSetOnDisk {
    // 与 info 无关，独立读取：info 读不出来时这个值照样能用
    let required_runtime = read_required_runtime(&directory);
    let info_path = match locate_info_file(&directory) {
        Some(path) => path,
        None => {
            return LevelSetOnDisk {
                dir_name,
                enabled,
                path: directory,
                metadata: None,
                error: Some("levelSet.infoMissing".into()),
                required_runtime,
            };
        }
    };
    match read_info_metadata(&info_path) {
        Ok(metadata) => LevelSetOnDisk {
            dir_name,
            enabled,
            path: directory,
            metadata: Some(metadata),
            error: None,
            required_runtime,
        },
        Err(error) => {
            log::warn!(
                "关卡集元数据解析失败：目录={}，文件={}，原因={}",
                dir_name,
                info_path.display(),
                error
            );
            LevelSetOnDisk {
                dir_name,
                enabled,
                path: directory,
                metadata: None,
                error: Some(error),
                required_runtime,
            }
        }
    }
}

/// 读 `requires.txt`：第一行有效内容就是要求的外部 runtime 最低版本。
///
/// 允许空行与 `#` 注释行（同目录的 `stub_levels.txt` 就带说明行）。文件缺失、
/// 全是空行注释、或读不出来时返回 `None` —— 那种情况下界面不显示关系卡片。
fn read_required_runtime(directory: &Path) -> Option<String> {
    let text = fs::read_to_string(directory.join(REQUIRES_FILE)).ok()?;
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
}

/// 把 `requires.txt` 的版本号做成一条 `requires` 关系。
///
/// guid 与地图源里写的完全一致，所以依赖表判定、缺依赖提示、「去安装」跳转
/// 全部复用已有管线，不需要为本地包另写一套。
pub fn runtime_relation(min_version: &str) -> Option<crate::models::RepositoryRelation> {
    let min_version = min_version.trim();
    if min_version.is_empty() {
        return None;
    }
    Some(crate::models::RepositoryRelation {
        kind: crate::models::RelationKind::Requires,
        guid: RUNTIME_WLOADER_GUID.to_string(),
        min_version: min_version.to_string(),
        max_version: String::new(),
        prerelease: None,
        optional: false,
    })
}

/// 找到关卡集的 `info` 文件。
///
/// 优先固定名 `info`；否则取唯一一个 `info_*`。有多个 `info_*` 时不猜，
/// 返回 `None` 让上层报「缺少 info」—— 与其加载错的那一个，不如明确失败。
fn locate_info_file(directory: &Path) -> Option<PathBuf> {
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

// ===== info 解析缓存 =====
//
// ## 为什么需要
//
// 地图管理页每次进入（以及每次开关、每次导入后刷新）都会重新扫描全部关卡集，
// 而每个 `info` 都是一个 UnityFS 容器：解压数据块 + 走一遍 SerializedFile
// 类型树。单集 1~3 ms 看着不多，但几十个集就是几十到上百毫秒，而这期间页面
// 是空白的。文件内容只在「安装 / 替换关卡集」时才变，重复解析同一份字节
// 纯属浪费。
//
// ## 为什么用 (len, mtime) 当指纹
//
// 不需要内容哈希：文件被替换时长度或修改时间几乎必然变化，而哈希要把整个
// 文件读一遍 —— 那正是缓存想省掉的成本。指纹不一致就重解，一致就直接用。
//
// ## 为什么错误也缓存
//
// 坏掉的 `info` 在用户修好之前每次扫描都会失败。失败同样是解析的产物，
// 缓存它不会让错误更隐蔽（指纹一变就重解），却能让「列表里有一个坏关卡集」
// 这种常见状态不再反复付出解析代价。

/// 单个 `info` 文件的解析结果缓存条目。
struct InfoCacheEntry {
    /// 解析时的文件长度。
    len: u64,
    /// 解析时的修改时间（Unix 纳秒）；拿不到时记 0。
    mtime_nanos: u128,
    /// 解析结果（成功或失败 tag）。
    result: Result<LevelSetMetadata, String>,
    /// 这个 key 累计**真正读盘解析**的次数（缓存命中不增加）。
    ///
    /// 只有测试读它：命中与否从返回值上看不出来（命中与不命中结果相同），
    /// 而「第二次没有再解析」是缓存唯一的可断言证据。
    parses: u32,
}

/// 缓存条数上限。
///
/// 关卡集数量是几十的量级，512 已远超真实规模。超限直接清空重来 ——
/// 缓存只是加速，为它维护 LRU 淘汰反而多一份要出错的状态。
const INFO_CACHE_LIMIT: usize = 512;

fn info_cache() -> &'static Mutex<HashMap<PathBuf, InfoCacheEntry>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, InfoCacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 缓存键：`info` 文件的绝对路径。
///
/// 用绝对路径而不是传入的原样路径：同一个 `info` 可能通过不同相对根被访问到，
/// 统一成绝对路径才能共用同一条缓存。`current_dir` 失败时退回原路径 ——
/// 那只可能发生在工作目录已被删除的极端情况，退化成不共享缓存也不会出错。
fn info_cache_key(info_path: &Path) -> PathBuf {
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
fn content_sha256(bytes: &[u8]) -> String {
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
fn read_info_disk_cache(
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
fn write_info_disk_cache(
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
fn clear_info_disk_cache(dir: &Path) -> Result<usize, String> {
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
fn read_info_metadata(info_path: &Path) -> Result<LevelSetMetadata, String> {
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

/// 缓存里某个 `info` 已经真正解析过几次（测试用）。
#[cfg(test)]
fn info_cache_parses(info_path: &Path) -> Option<u32> {
    let cache = info_cache().lock().ok()?;
    cache
        .get(&info_cache_key(info_path))
        .map(|entry| entry.parses)
}

/// 目录名是否是**单一路径片段**（不含分隔符、`..`、盘符、根）。
///
/// 关卡集目录名来自扫描结果或前端传参。允许 `..` 或分隔符就能把游戏目录之外
/// 的目录搬进来 / 删掉，所以所有「按目录名定位」的地方都先过这里 ——
/// 集中一处判定，不会出现某个入口忘了校验。
///
/// 判定用 `components()` 而不是查字符串里的 `/`、`\`：Windows 同时认两种分隔符，
/// 而且 `C:`、`.`、`..` 这些写法必须先拆成组件才看得清。这里要求**恰好一个**
/// `Normal` 组件，`a/b` 这种多段写法也会被拒。
fn is_safe_dir_name(dir_name: &str) -> bool {
    if dir_name.trim().is_empty() {
        return false;
    }
    let mut components = Path::new(dir_name).components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

/// 两个关卡集是否是同一张地图。
///
/// 身份由三段构成：`levelSetName`（英文集名）+ `levelSetNameZH`（中文名）+
/// `author`。只用前两段不够 —— 实测「胡闹糕中」与「胡闹达学」是同一个作者的
/// 两张不同地图，英文集名与作者都写成 `GAODA / GAODA`，只有中文名不同。
///
/// 比较规则：
/// - 英文集名：trim 后忽略大小写，**空名永不匹配**（空名说明 `info` 没解析
///   出来，不能把所有坏目录都算成同一张图）。
/// - 作者：trim 后忽略大小写，空对空算相同（官方导出的 `info` 常不写作者）。
/// - 中文名：**两边都非空才比较**。地图源 TOML 可以只写英文集名与作者（老源
///   都没有中文名），此时退回前两段的判定，与加入中文名之前的行为一致。
pub fn level_set_identity_matches(
    level_set_name: &str,
    name_zh: Option<&str>,
    author: &str,
    other_level_set_name: &str,
    other_name_zh: Option<&str>,
    other_author: &str,
) -> bool {
    let name = level_set_name.trim();
    let other = other_level_set_name.trim();
    if name.is_empty() || other.is_empty() {
        return false;
    }
    if !name.eq_ignore_ascii_case(other) {
        return false;
    }
    if !author.trim().eq_ignore_ascii_case(other_author.trim()) {
        return false;
    }
    match (
        name_zh.map(str::trim).filter(|value| !value.is_empty()),
        other_name_zh
            .map(str::trim)
            .filter(|value| !value.is_empty()),
    ) {
        (Some(left), Some(right)) => left.eq_ignore_ascii_case(right),
        _ => true,
    }
}

#[cfg(test)]
mod identity_tests {
    use super::level_set_identity_matches;

    #[test]
    fn identity_matches_ignore_case_and_surrounding_whitespace() {
        assert!(level_set_identity_matches(
            "RW_LevelSet",
            None,
            "RW",
            " rw_levelset ",
            None,
            " rw "
        ));
    }

    #[test]
    fn identity_requires_a_non_empty_level_set_name() {
        // 空名 = info 没解析出来，不能把所有坏目录都算成同一张地图
        assert!(!level_set_identity_matches("", None, "", "", None, ""));
        assert!(!level_set_identity_matches(
            "  ",
            None,
            "RW",
            "RW_LevelSet",
            None,
            "RW"
        ));
        assert!(!level_set_identity_matches(
            "RW_LevelSet",
            None,
            "RW",
            "",
            None,
            "RW"
        ));
    }

    #[test]
    fn identity_compares_the_author_too() {
        assert!(!level_set_identity_matches(
            "RW_LevelSet",
            None,
            "RW",
            "RW_LevelSet",
            None,
            "Other"
        ));
        // 官方导出的 info 常不写作者：两边都空算相同
        assert!(level_set_identity_matches(
            "RW_LevelSet",
            None,
            "",
            "RW_LevelSet",
            None,
            ""
        ));
    }

    #[test]
    fn identity_separates_maps_that_only_differ_in_the_chinese_name() {
        // 实测「胡闹糕中」与「胡闹达学」：英文集名与作者完全相同，只有中文名不同
        assert!(!level_set_identity_matches(
            "GAODA",
            Some("胡闹糕中"),
            "GAODA",
            "GAODA",
            Some("胡闹达学"),
            "GAODA"
        ));
        // 同一张图升级：三段都不变，仍应判为同一张
        assert!(level_set_identity_matches(
            "GAODA",
            Some("胡闹糕中"),
            "GAODA",
            "GAODA",
            Some("胡闹糕中"),
            "GAODA"
        ));
        // 地图源没写中文名（老源）时退回前两段，不能因为缺字段就绑不上
        assert!(level_set_identity_matches(
            "GAODA",
            Some("胡闹糕中"),
            "GAODA",
            "GAODA",
            None,
            "GAODA"
        ));
    }
}

/// 启用或停用一个关卡集：在 `levels/` 与 `disabled/` 之间移动整个目录。
///
/// 返回值是移动后的新路径。目标位置已有同名目录时**拒绝**并返回错误 ——
/// 覆盖会静默丢掉用户已有的那个关卡集，而这两个目录都是用户数据。
pub fn set_level_set_enabled(
    game_root: &Path,
    dir_name: &str,
    enabled: bool,
) -> Result<PathBuf, String> {
    // 目录名必须是单一路径片段：允许 `..` 或分隔符就能把任意目录搬进游戏目录
    if !is_safe_dir_name(dir_name) {
        log::warn!("拒绝不安全的关卡集目录名：{}", dir_name);
        return Err("levelSet.invalidName".into());
    }
    let base = game_root.join(DIYLEVEL_DIR);
    let (from_dir, to_dir) = if enabled {
        (DISABLED_DIR, LEVELS_DIR)
    } else {
        (LEVELS_DIR, DISABLED_DIR)
    };
    let source = base.join(from_dir).join(dir_name);
    let destination = base.join(to_dir).join(dir_name);
    if !source.is_dir() {
        log::warn!(
            "关卡集不在预期位置：目录={}，期望={}",
            dir_name,
            source.display()
        );
        return Err("levelSet.notFound".into());
    }
    if destination.exists() {
        log::warn!(
            "目标位置已有同名关卡集，拒绝覆盖：目录={}，目标={}",
            dir_name,
            destination.display()
        );
        return Err(crate::locale_tag::with_params(
            "levelSet.targetExists",
            &[("name", dir_name)],
        ));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::rename(&source, &destination).map_err(|error| {
        log::error!(
            "移动关卡集失败：目录={}，从={}，到={}，原因={}",
            dir_name,
            source.display(),
            destination.display(),
            error
        );
        crate::locale_tag::with_params("levelSet.moveFailed", &[("reason", &error.to_string())])
    })?;
    log::info!(
        "关卡集已{}：目录={}，新路径={}",
        if enabled { "启用" } else { "停用" },
        dir_name,
        destination.display()
    );
    Ok(destination)
}

/// 批量启用 / 停用全部关卡集，返回**实际**移动过的目录名。
///
/// 逐条调用 `set_level_set_enabled` 时每次都重新读目录，地图一多就是重复
/// 遍历；这里先扫一次盘、只处理需要变的目录。中途失败按**逆序回滚**已经
/// 移动的目录，保证列表不会停在半开半关的状态（与 MOD 的批量开关同语义）。
///
/// 空列表表示「本来就已经是目标状态」，调用方据此给出「无需切换」提示。
pub fn set_all_level_sets_enabled(game_root: &Path, enabled: bool) -> Result<Vec<String>, String> {
    let targets: Vec<String> = scan_level_sets(game_root)
        .into_iter()
        .filter(|set| set.enabled != enabled)
        .map(|set| set.dir_name)
        .collect();
    let mut changed: Vec<String> = Vec::new();
    for dir_name in &targets {
        match set_level_set_enabled(game_root, dir_name, enabled) {
            Ok(_) => changed.push(dir_name.clone()),
            Err(error) => {
                log::error!(
                    "批量切换关卡集失败，开始回滚：目录={}，原因={}",
                    dir_name,
                    error
                );
                for previous in changed.iter().rev() {
                    if let Err(rollback) = set_level_set_enabled(game_root, previous, !enabled) {
                        log::error!("批量切换回滚失败：目录={}，原因={}", previous, rollback);
                    }
                }
                return Err(error);
            }
        }
    }
    Ok(changed)
}

/// 定位一个关卡集目录，并确认它确实是 `OC2DIYLevel` 下的真实目录。
///
/// `enabled` 决定去 `levels/` 还是 `disabled/` 找：同名目录可能只存在于其中
/// 一边，猜错就会操作到另一个关卡集（打开错目录、删错文件）。
///
/// 「打开目录」与「删除」都先走这里，路径校验只写一遍 —— 两个入口不会出现
/// 一个严一个松，也不会因为某个入口将来漏改而让前端参数变成任意路径。
pub fn locate_level_set_directory(
    game_root: &Path,
    dir_name: &str,
    enabled: bool,
) -> Result<PathBuf, String> {
    if !is_safe_dir_name(dir_name) {
        log::warn!("拒绝不安全的关卡集目录名：{}", dir_name);
        return Err("levelSet.invalidName".into());
    }
    // 先确认框架目录本身在游戏内，再拼关卡集目录：safe_game_path 会解析最近的
    // 已存在祖先，目录联接因此不能把目标指到游戏目录之外
    let base = crate::game::safe_game_path(game_root, DIYLEVEL_DIR)?;
    let relative = format!(
        "{DIYLEVEL_DIR}/{}/{dir_name}",
        if enabled { LEVELS_DIR } else { DISABLED_DIR }
    );
    let path = crate::game::safe_game_path(game_root, &relative)?;
    if !path.starts_with(&base) || !path.is_dir() {
        log::warn!(
            "关卡集目录不存在或不在 DIYLevel 下：目录={}，期望={}",
            dir_name,
            path.display()
        );
        return Err("levelSet.notFound".into());
    }
    Ok(path)
}

/// 删除一个关卡集：跟随回收开关，要么整目录移进回收站，要么直接删除。
///
/// ## 为什么默认进回收站
///
/// 关卡集里常是作者精心调过的场景文件，删掉没有版本控制可回退。回收站按
/// **文件**记账（恢复能逐路径还原、失败能逐文件回滚），所以这里先把目录展开
/// 成文件列表再交给 `recycle_bin::stage` —— 与「安装时回收旧版本」走同一条
/// 代码路径，回收站记录格式不会分叉。
///
/// ## 为什么空目录不进回收站
///
/// `stage` 会为它产生一条 0 文件的记录，用户点恢复什么也拿不回来。
/// 直接移除更诚实。
pub fn remove_level_set(
    recycle_root: &Path,
    manifests_dir: &Path,
    game_root: &Path,
    dir_name: &str,
    enabled: bool,
    recycle: bool,
) -> Result<(), String> {
    let set_dir = locate_level_set_directory(game_root, dir_name, enabled)?;
    if !recycle {
        // 用户关掉了「删除 MOD 时优先放入回收站」：路径归属已校验，直接删
        fs::remove_dir_all(&set_dir).map_err(|error| {
            crate::locale_tag::with_params(
                "levelSet.deleteFailed",
                &[("reason", &error.to_string())],
            )
        })?;
        log::info!("关卡集已删除（未进回收站）：目录={}", dir_name);
        return Ok(());
    }
    let files = collect_files(game_root, &set_dir)?;
    if files.is_empty() {
        fs::remove_dir_all(&set_dir).map_err(|error| {
            crate::locale_tag::with_params(
                "levelSet.deleteFailed",
                &[("reason", &error.to_string())],
            )
        })?;
        log::info!("关卡集目录为空，已直接移除：目录={}", dir_name);
        return Ok(());
    }
    // 回收记录用集名显示（界面语言下的名字）；info 读不出来时退回目录名
    let name = read_level_set(set_dir.clone(), dir_name.to_string(), enabled)
        .metadata
        .map(|metadata| metadata.name)
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| dir_name.to_string());
    crate::recycle_bin::stage(
        recycle_root,
        manifests_dir,
        game_root,
        name,
        None,
        None,
        Vec::new(),
        files,
        None,
        &mut || Ok(()),
    )?;
    /*
     * 回收只搬文件，目录壳会留在原地。不清掉它，扫描会把这个空目录当成
     * 一个「缺 info」的坏关卡集继续显示在列表里。
     */
    remove_empty_directories(&set_dir);
    log::info!("关卡集已移入回收站：目录={}", dir_name);
    Ok(())
}

/// 自底向上删除空目录；非空目录保留。
///
/// 回收只搬文件、不搬目录，所以删完 / 回收完要清掉空壳。**只删空目录**：
/// 里面还有没被搬走的文件（目录联接、被占用而搬不动的文件）时保留现场 ——
/// 宁可留一个空壳让用户自己看，也不能静默丢掉文件。
fn remove_empty_directories(path: &Path) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            remove_empty_directories(&entry.path());
        }
    }
    // 非空时 remove_dir 会失败，正是我们要的「保留现场」
    let _ = fs::remove_dir(path);
}

/// ZIP 里识别出的一个关卡集。
#[derive(Debug, Clone)]
pub struct ImportedLevelSet {
    /// 导入后使用的目录名。
    pub dir_name: String,
    /// 该关卡集的 `info` 文件内容解析结果。
    pub metadata: LevelSetMetadata,
    /// ZIP 内该关卡集的文件：条目名 → 去掉关卡集前缀后的相对路径。
    pub files: Vec<(String, String)>,
}

/// 读一个 ZIP 文件，按 `info` 识别出其中的关卡集。
///
/// 只读条目清单与 `info` 内容，**不解压其它文件** —— 调用方拿到
/// [`ImportedLevelSet::files`] 后按需写入，避免为了识别而先落盘一份临时副本。
pub fn inspect_zip(zip_path: &Path) -> Result<Vec<ImportedLevelSet>, String> {
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
    let mut info_candidates: BTreeMap<String, Vec<usize>> = BTreeMap::new();
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
        by_directory
            .entry(directory.clone())
            .or_default()
            .push(name.clone());
        if is_info_file(&file_name) {
            info_candidates.entry(directory).or_default().push(index);
        }
    }

    if info_candidates.is_empty() {
        return Err("levelSet.zipWithoutInfo".into());
    }

    /*
     * 分两阶段：先解析出每个目录的元数据与「基础目录名」，再统一分配目录名。
     *
     * 不能边解析边分配：同一批里共用同一个集名的几张图，谁先谁后决定谁拿到
     * 不带后缀的名字，而顺序又由 ZIP 条目与 BTreeMap 键序决定 —— 同样的包
     * 两次导入可能得到不同目录名。
     */
    let mut parsed_sets: Vec<(String, LevelSetMetadata, String)> = Vec::new();
    for (directory, candidates) in info_candidates {
        let mut metadata: Option<LevelSetMetadata> = None;
        let mut last_error = String::new();
        for index in candidates {
            let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
            let mut bytes = Vec::with_capacity(entry.size() as usize);
            entry
                .read_to_end(&mut bytes)
                .map_err(|error| error.to_string())?;
            match asset_bundle::parse_level_set(&bytes) {
                Ok(parsed) => {
                    metadata = Some(parsed);
                    break;
                }
                Err(error) => {
                    log::debug!(
                        "候选 info 不是关卡集，继续试下一个：目录={}，条目序号={}，原因={}",
                        directory,
                        index,
                        error
                    );
                    last_error = error.to_string();
                }
            }
        }
        let Some(metadata) = metadata else {
            return Err(crate::locale_tag::with_params(
                "levelSet.zipInfoInvalid",
                &[("reason", &last_error)],
            ));
        };

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
        let files = by_directory
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
        imported.push(ImportedLevelSet {
            dir_name,
            metadata,
            files,
        });
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
        "ZIP 关卡集识别完成：文件={}，关卡集={}",
        zip_path.display(),
        imported.len()
    );
    Ok(imported)
}

/// 把选定的关卡集从 ZIP 写入 `levels/`。
///
/// 只写入 `selected` 里列出的条目；已存在的目标目录一律拒绝，由调用方先
/// 决定是否覆盖（移动旧目录到 `disabled/` 或先删除）。这样这个函数不需要
/// 猜测「用户想要覆盖还是想并存」。
pub fn install_from_zip(
    zip_path: &Path,
    game_root: &Path,
    selected: &[String],
) -> Result<Vec<String>, String> {
    let sets = inspect_zip(zip_path)?;
    let targets: Vec<&ImportedLevelSet> = sets
        .iter()
        .filter(|set| selected.iter().any(|name| name == &set.dir_name))
        .collect();
    if targets.is_empty() {
        return Err("levelSet.nothingSelected".into());
    }

    let levels_dir = game_root.join(DIYLEVEL_DIR).join(LEVELS_DIR);
    let disabled_dir = game_root.join(DIYLEVEL_DIR).join(DISABLED_DIR);

    /*
     * 目标目录名下已经有东西时：
     *
     * - **身份相同** → 仍按原规则拒绝，覆盖与否交给调用方（它可能想先移动
     *   旧目录到 `disabled/`，或干脆让用户自己决定）。
     * - **身份不同** → 换一个目录名并存。两个不同地图共用同一个英文集名与
     *   作者是真实存在的（实测「胡闹糕中」与「胡闹达学」都是 GAODA/GAODA），
     *   整批拒绝只会让用户既装不上新的、也不知道该删谁。
     * - `disabled/` 下有同名目录也算占用：那是用户停用的数据，直接覆盖会丢。
     */
    let mut resolved: Vec<(String, &ImportedLevelSet)> = Vec::new();
    let mut taken = collect_dir_names(&levels_dir);
    taken.extend(collect_dir_names(&disabled_dir));
    for set in &targets {
        let mut dir_name = set.dir_name.clone();
        if taken.contains(&dir_name.to_lowercase()) {
            let enabled_path = levels_dir.join(&dir_name);
            let existing = read_level_set(enabled_path, dir_name.clone(), true);
            /*
             * 读不出 info 的目录（空目录、损坏、手动塞进来的东西）不算「另一张
             * 地图」，而是「无法判断」—— 这种情况不替用户换名绕过，按原规则
             * 拒绝，让他先处理掉那个目录。换名只用于**确证身份不同**的并存。
             */
            let Some(existing_meta) = existing.metadata.as_ref() else {
                return Err(crate::locale_tag::with_params(
                    "levelSet.targetExists",
                    &[("name", &dir_name)],
                ));
            };
            let same_identity = level_set_identity_matches(
                &existing_meta.name,
                existing_meta.name_zh.as_deref(),
                &existing_meta.author,
                &set.metadata.name,
                set.metadata.name_zh.as_deref(),
                &set.metadata.author,
            );
            if same_identity {
                return Err(crate::locale_tag::with_params(
                    "levelSet.targetExists",
                    &[("name", &dir_name)],
                ));
            }
            dir_name = allocate_dir_name(&dir_name, &set.metadata, &mut taken, false);
            log::info!(
                "导入的关卡集与已有目录不同身份，改用新目录名：原目录={}，新目录={}",
                set.dir_name,
                dir_name
            );
        } else {
            taken.insert(dir_name.to_lowercase());
        }
        resolved.push((dir_name, *set));
    }

    let mut installed = Vec::new();
    for (dir_name, set) in resolved {
        write_level_set(zip_path, game_root, &dir_name, set)?;
        installed.push(dir_name);
    }
    Ok(installed)
}

/// 是否是 macOS 打包产生的垃圾条目。
fn is_junk_entry(name: &str) -> bool {
    name.starts_with("__MACOSX/")
        || name.rsplit('/').next().is_some_and(|file| {
            file == ".DS_Store" || file.starts_with("._") || file == "Thumbs.db"
        })
}

/// 是否是关卡集的 `info` 文件（`info` 或 `info_<集名>`）。
fn is_info_file(name: &str) -> bool {
    name == "info" || name.starts_with("info_")
}

/// 把 ZIP 条目名拆成 `(目录, 文件名)`。根目录返回空字符串。
fn split_entry(name: &str) -> (String, String) {
    match name.rsplit_once('/') {
        Some((directory, file)) => (directory.to_string(), file.to_string()),
        None => (String::new(), name.to_string()),
    }
}

/// 相对路径是否安全（非绝对、无 `..`、无 Windows 盘符与数据流冒号）。
fn is_safe_relative(relative: &str) -> bool {
    if relative.is_empty() || relative.contains(':') {
        return false;
    }
    let path = Path::new(relative);
    !path.is_absolute()
        && !path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::RootDir))
}

/// 把集名变成合法目录名。
///
/// 去掉 Windows 不允许的字符与空白；结果为空时返回 `None`，由调用方换用
/// 下一个候选（ZIP 目录名 / 报错），不静默落一个空名字。
fn sanitize_dir_name(name: &str) -> Option<String> {
    let cleaned: String = name
        .trim()
        .chars()
        .map(|ch| match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            ch if ch.is_control() => '_',
            ch => ch,
        })
        .collect();
    let cleaned = cleaned.trim_matches(['.', ' ']).to_string();
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

/// 单个目录名的字符数上限。
///
/// 关卡集目录位于 `<游戏>/BepInEx/plugins/OC2DIYLevel/levels/` 下，Windows
/// 默认 260 字符的路径上限还要留给里面的文件名（Unity 的 `.resS` 常与主文件同名同长）。
/// 正常集名远低于这个值，这里只拦病态长名。
const MAX_DIR_NAME_CHARS: usize = 48;

/// 给关卡集挑一个在本批里不重名的目录名。
///
/// 只用英文集名不够：不同地图可能共用同一个集名（实测「胡闹糕中」与
/// 「胡闹达学」都写 `GAODA`，作者也都是 `GAODA`）。撞车时按可读性依次追加
/// 中文名、作者、uid 前缀，最后才退到数字后缀。
///
/// 中文名排第一是因为它区分度最高且**不随版本变化** —— 同一张图升级时目录名
/// 保持不变，回收覆盖才能命中同一处。`uid` 是 `MD5(baseUID + version)`，每发
/// 一版就变，只能当最后的兜底。
///
/// `allow_base` 为 `false` 时跳过基础名本身。同一批里有多张图共用这个基础名
/// 时，调用方会对**每一张**都置 `false`，让它们一起带后缀，而不是「第一个
/// 占住干净名字、其余加后缀」—— 后者会让目录名取决于条目顺序。
fn allocate_dir_name(
    base: &str,
    metadata: &LevelSetMetadata,
    taken: &mut std::collections::HashSet<String>,
    allow_base: bool,
) -> String {
    let shorten = |value: &str| -> Option<String> {
        let cleaned = sanitize_dir_name(value)?;
        let shortened: String = cleaned.chars().take(MAX_DIR_NAME_CHARS).collect();
        let shortened = shortened.trim_matches(['.', ' ']).to_string();
        if shortened.is_empty() {
            None
        } else {
            Some(shortened)
        }
    };

    let base = shorten(base).unwrap_or_else(|| "LevelSet".to_string());
    let mut candidates: Vec<String> = Vec::new();
    if allow_base {
        candidates.push(base.clone());
    }
    if let Some(name_zh) = metadata.name_zh.as_deref().and_then(shorten) {
        candidates.push(format!("{base}_{name_zh}"));
    }
    if let Some(author) = shorten(&metadata.author) {
        candidates.push(format!("{base}_{author}"));
    }
    let uid: String = metadata.uid.chars().take(8).collect();
    if !uid.trim().is_empty() {
        candidates.push(format!("{base}_{uid}"));
    }

    /*
     * 按小写比较：Windows 与 macOS 的默认文件系统都不区分大小写，只比原串会
     * 放过 `GAODA` 与 `gaoda` 这种「看起来不同、实际撞同一个目录」的组合。
     */
    for candidate in candidates {
        if taken.insert(candidate.to_lowercase()) {
            return candidate;
        }
    }
    for suffix in 2u32.. {
        let candidate = format!("{base}_{suffix}");
        if taken.insert(candidate.to_lowercase()) {
            return candidate;
        }
    }
    unreachable!("数字后缀必然能找到一个空位")
}

/// 收集一个目录下所有子目录名（转小写），用于避开已占用的目录名。
fn collect_dir_names(directory: &Path) -> std::collections::HashSet<String> {
    let mut names = std::collections::HashSet::new();
    let Ok(entries) = fs::read_dir(directory) else {
        return names;
    };
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            names.insert(entry.file_name().to_string_lossy().to_lowercase());
        }
    }
    names
}

/// 供 ZIP 读取测试用：把内存字节包成可 seek 的读取器。
#[allow(dead_code)]
trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsafe_directory_names() {
        let root = std::env::temp_dir().join(format!("oc2-lv-name-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join(DIYLEVEL_DIR).join(LEVELS_DIR)).unwrap();

        // 目录名必须是单一路径片段：这些都应该被拒绝
        for name in ["..", "../evil", "a/b", "a\\b", ""] {
            let result = set_level_set_enabled(&root, name, false);
            assert!(result.is_err(), "应拒绝目录名 {name:?}");
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn toggling_moves_the_whole_directory_between_levels_and_disabled() {
        let root = std::env::temp_dir().join(format!("oc2-lv-move-{}", uuid::Uuid::new_v4()));
        let levels = root.join(DIYLEVEL_DIR).join(LEVELS_DIR);
        let set = levels.join("MySet");
        std::fs::create_dir_all(&set).unwrap();
        std::fs::write(set.join("info_MySet"), b"placeholder").unwrap();

        // 停用：整个目录移到 disabled/
        let moved = set_level_set_enabled(&root, "MySet", false).unwrap();
        assert!(!set.exists(), "原目录应已移走");
        assert!(moved.is_dir(), "应出现在 disabled/");
        assert!(moved.join("info_MySet").is_file(), "目录内容要跟着走");

        // 启用：移回 levels/
        let back = set_level_set_enabled(&root, "MySet", true).unwrap();
        assert_eq!(back, set);
        assert!(back.join("info_MySet").is_file());

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn bulk_toggle_moves_every_set_and_skips_noop_ones() {
        let root = std::env::temp_dir().join(format!("oc2-lv-bulk-{}", uuid::Uuid::new_v4()));
        let base = root.join(DIYLEVEL_DIR);
        for name in ["A", "B"] {
            std::fs::create_dir_all(base.join(LEVELS_DIR).join(name)).unwrap();
        }
        std::fs::create_dir_all(base.join(DISABLED_DIR).join("C")).unwrap();

        // 停用：只移动当前启用的 A、B，已经在 disabled 的 C 不参与
        let changed = set_all_level_sets_enabled(&root, false).unwrap();
        assert_eq!(changed, vec!["A".to_string(), "B".to_string()]);
        for name in ["A", "B", "C"] {
            assert!(
                base.join(DISABLED_DIR).join(name).is_dir(),
                "{name} 应在 disabled"
            );
        }

        // 全部已是禁用态：再关一次不产生移动
        assert!(set_all_level_sets_enabled(&root, false).unwrap().is_empty());

        // 启用：三个都回到 levels
        let back = set_all_level_sets_enabled(&root, true).unwrap();
        assert_eq!(back.len(), 3);
        for name in ["A", "B", "C"] {
            assert!(
                base.join(LEVELS_DIR).join(name).is_dir(),
                "{name} 应在 levels"
            );
        }

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn refuses_to_overwrite_an_existing_set_at_the_target() {
        let root = std::env::temp_dir().join(format!("oc2-lv-clash-{}", uuid::Uuid::new_v4()));
        let base = root.join(DIYLEVEL_DIR);
        std::fs::create_dir_all(base.join(LEVELS_DIR).join("Dup")).unwrap();
        std::fs::create_dir_all(base.join(DISABLED_DIR).join("Dup")).unwrap();

        // 两边同名：覆盖会静默丢掉用户已有的那个，必须拒绝
        let result = set_level_set_enabled(&root, "Dup", false);
        assert!(result.is_err(), "目标已存在时应拒绝");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn locates_fixed_info_before_prefixed_variants() {
        let root = std::env::temp_dir().join(format!("oc2-lv-info-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        // 固定名优先：DLL 只认 `info`
        std::fs::write(root.join("info"), b"a").unwrap();
        std::fs::write(root.join("info_Other"), b"b").unwrap();
        assert_eq!(locate_info_file(&root), Some(root.join("info")));

        // 只有带前缀的唯一一个时才用它
        std::fs::remove_file(root.join("info")).unwrap();
        assert_eq!(locate_info_file(&root), Some(root.join("info_Other")));

        // 多个带前缀的：不猜
        std::fs::write(root.join("info_Another"), b"c").unwrap();
        assert_eq!(locate_info_file(&root), None);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn sanitizes_level_set_names_into_directory_names() {
        assert_eq!(
            sanitize_dir_name("RW_LevelSet").as_deref(),
            Some("RW_LevelSet")
        );
        assert_eq!(sanitize_dir_name("a/b:c").as_deref(), Some("a_b_c"));
        assert_eq!(sanitize_dir_name("  "), None);
        assert_eq!(sanitize_dir_name("..."), None);
    }

    #[test]
    fn junk_and_info_detection() {
        assert!(is_junk_entry("__MACOSX/x/._info"));
        assert!(is_junk_entry("a/.DS_Store"));
        assert!(!is_junk_entry("levels/a/info"));
        assert!(is_info_file("info"));
        assert!(is_info_file("info_rw"));
        assert!(!is_info_file("s_rw_1"));
    }

    #[test]
    fn rejects_paths_that_escape_the_set_directory() {
        assert!(!is_safe_relative("../evil"));
        assert!(!is_safe_relative("/abs/path"));
        assert!(!is_safe_relative("C:/windows"));
        assert!(!is_safe_relative(""));
        assert!(is_safe_relative("s_rw_1"));
        assert!(is_safe_relative("sub/dir/file.dll"));
    }
}

/// ZIP 导入的端到端测试。
///
/// 用真实分发的 RW 地图 ZIP（`.tmp/rw.zip`）跑一遍：识别关卡集 → 解析
/// info → 写进 `levels/`。文件不在时跳过，避免 CI 依赖网络。
#[cfg(test)]
mod zip_tests {
    use super::*;

    fn rw_zip() -> Option<PathBuf> {
        // cargo test 的 cwd 是 src-tauri/，用 manifest 目录回到项目根
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.tmp/rw.zip");
        path.is_file().then_some(path)
    }

    #[test]
    #[ignore = "需要 .tmp/rw.zip，用 --ignored 单独跑"]
    fn inspects_the_published_rw_zip() {
        let Some(zip) = rw_zip() else {
            eprintln!("跳过：没有 .tmp/rw.zip");
            return;
        };
        let sets = inspect_zip(&zip).expect("应能识别 RW 地图 ZIP");
        assert_eq!(sets.len(), 1, "RW 的 ZIP 只有一个关卡集");
        let set = &sets[0];
        assert_eq!(set.metadata.name, "RW_LevelSet");
        assert_eq!(set.metadata.author, "RW");
        assert_eq!(set.metadata.version, "0.73");
        assert_eq!(set.metadata.levels.len(), 10);
        // 目录名来自集名而不是 ZIP 结构
        assert_eq!(set.dir_name, "RW_LevelSet");
        // 文件数 = info + 10 个关卡
        assert_eq!(set.files.len(), 11);
        // info 在 ZIP 里叫 info_rw，落盘时要变成固定名 `info`
        assert!(
            set.files
                .iter()
                .any(|(entry, relative)| { entry == "info_rw" && relative == "info_rw" })
        );
    }

    #[test]
    #[ignore = "需要 .tmp/rw.zip，用 --ignored 单独跑"]
    fn installs_the_zip_into_levels_with_fixed_info_name() {
        let Some(zip) = rw_zip() else {
            eprintln!("跳过：没有 .tmp/rw.zip");
            return;
        };
        let root = std::env::temp_dir().join(format!("oc2-lv-zip-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join(DIYLEVEL_DIR)).unwrap();

        let installed =
            install_from_zip(&zip, &root, &["RW_LevelSet".to_string()]).expect("应能安装");
        assert_eq!(installed, vec!["RW_LevelSet".to_string()]);

        let destination = root.join(DIYLEVEL_DIR).join(LEVELS_DIR).join("RW_LevelSet");
        // DLL 只认固定名 `info`，不能保留 ZIP 里的 `info_rw`
        assert!(destination.join("info").is_file(), "info 应落成固定名");
        assert!(
            !destination.join("info_rw").exists(),
            "不应保留带前缀的名字"
        );
        // 关卡文件保持原名：游戏按 sceneName 找它们
        assert!(destination.join("s_rw_1").is_file());
        assert!(destination.join("s_rw_10").is_file());

        // 装完之后再扫一遍，元数据应能读回来
        let sets = scan_level_sets(&root);
        assert_eq!(sets.len(), 1);
        assert!(sets[0].enabled);
        assert_eq!(sets[0].metadata.as_ref().unwrap().name, "RW_LevelSet");
        assert_eq!(sets[0].metadata.as_ref().unwrap().levels.len(), 10);

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    #[ignore = "需要 .tmp/rw.zip，用 --ignored 单独跑"]
    fn refuses_to_install_over_an_existing_set() {
        let Some(zip) = rw_zip() else {
            eprintln!("跳过：没有 .tmp/rw.zip");
            return;
        };
        let root = std::env::temp_dir().join(format!("oc2-lv-dup-{}", uuid::Uuid::new_v4()));
        let existing = root.join(DIYLEVEL_DIR).join(LEVELS_DIR).join("RW_LevelSet");
        std::fs::create_dir_all(&existing).unwrap();

        // 覆盖会静默丢掉用户已有的关卡集（可能是自己改过的版本）
        let result = install_from_zip(&zip, &root, &["RW_LevelSet".to_string()]);
        assert!(result.is_err(), "目标已存在时应拒绝安装");
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// 只用于目录名分配测试的元数据。
    fn metadata(name: &str, name_zh: Option<&str>, author: &str, uid: &str) -> LevelSetMetadata {
        LevelSetMetadata {
            name: name.into(),
            name_zh: name_zh.map(str::to_string),
            author: author.into(),
            version: "1.0.0".into(),
            uid: uid.into(),
            base_uid: None,
            levels: Vec::new(),
        }
    }

    #[test]
    fn allocate_dir_name_disambiguates_with_the_chinese_name() {
        /*
         * 真实样本：同一个作者的两张地图，info 里英文集名与作者都写 GAODA，
         * 只有中文名不同（胡闹糕中 / 胡闹达学）。它们必须落进不同目录，
         * 否则后装的会把先装的当旧版本回收掉。
         */
        let mut taken = std::collections::HashSet::new();
        let first = allocate_dir_name(
            "GAODA",
            &metadata("GAODA", Some("胡闹糕中"), "GAODA", "6040602f-aaaa"),
            &mut taken,
            false,
        );
        let second = allocate_dir_name(
            "GAODA",
            &metadata("GAODA", Some("胡闹达学"), "GAODA", "4b519ea0-bbbb"),
            &mut taken,
            false,
        );
        // 同一批共用一个集名时，两张都带后缀，谁也不独占「干净」名字
        assert_eq!(first, "GAODA_胡闹糕中");
        assert_eq!(second, "GAODA_胡闹达学");
        assert_ne!(first.to_lowercase(), second.to_lowercase());
    }

    #[test]
    fn allocate_dir_name_treats_case_only_differences_as_collisions() {
        // Windows 与 macOS 的默认文件系统都不区分大小写：`GAODA` 与 `gaoda`
        // 会落进同一个目录，只比原串会漏判。
        let mut taken = std::collections::HashSet::new();
        let first = allocate_dir_name("GAODA", &metadata("GAODA", None, "", ""), &mut taken, true);
        let second = allocate_dir_name("gaoda", &metadata("gaoda", None, "", ""), &mut taken, true);
        assert_eq!(first, "GAODA");
        assert_ne!(second.to_lowercase(), first.to_lowercase());
    }

    #[test]
    fn allocate_dir_name_falls_back_to_author_then_uid_then_number() {
        let mut taken = std::collections::HashSet::new();
        // 第一个直接拿到基础名
        assert_eq!(
            allocate_dir_name(
                "S",
                &metadata("S", None, "A1", "aaaa1111-2222"),
                &mut taken,
                true
            ),
            "S"
        );
        // 没中文名 → 用作者
        assert_eq!(
            allocate_dir_name(
                "S",
                &metadata("S", None, "A1", "bbbb2222-3333"),
                &mut taken,
                true
            ),
            "S_A1"
        );
        // 作者也撞了 → 用 uid 前 8 位
        assert_eq!(
            allocate_dir_name(
                "S",
                &metadata("S", None, "A1", "cccc3333-4444"),
                &mut taken,
                true
            ),
            "S_cccc3333"
        );
        // 三段全撞 → 数字兜底
        assert_eq!(
            allocate_dir_name(
                "S",
                &metadata("S", None, "A1", "cccc3333-4444"),
                &mut taken,
                true
            ),
            "S_2"
        );
    }

    #[test]
    fn allocate_dir_name_keeps_names_usable_as_directories() {
        // 集名里的非法字符要先清理，超长要截断，否则建不目录
        let mut taken = std::collections::HashSet::new();
        let name = allocate_dir_name(
            "a/b:c",
            &metadata("a/b:c", Some("x/y"), "aut hor", ""),
            &mut taken,
            true,
        );
        assert_eq!(name, "a_b_c");
        let long = "x".repeat(200);
        let mut taken = std::collections::HashSet::new();
        let name = allocate_dir_name(&long, &metadata(&long, None, "", ""), &mut taken, true);
        assert!(name.chars().count() <= MAX_DIR_NAME_CHARS);
    }

    #[test]
    fn collects_existing_directory_names_for_collision_checks() {
        let root = std::env::temp_dir().join(format!("oc2-lv-names-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("GAODA")).unwrap();
        std::fs::create_dir_all(root.join("Other")).unwrap();
        std::fs::write(root.join("file.txt"), b"x").unwrap();
        let names = collect_dir_names(&root);
        assert!(names.contains("gaoda"), "应收集到目录名并转小写");
        assert!(names.contains("other"));
        assert!(!names.contains("file.txt"), "文件不算占用目录名");
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// 造一个 ZIP：`entries` 是 (条目名, 内容) 列表。
    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        use std::io::Write as _;
        let file = std::fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        for (name, bytes) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap();
    }

    /// 从 `.tmp/rw.zip` 里读出一份真实 `info` 的字节。
    fn real_info_bytes() -> Option<Vec<u8>> {
        let zip = rw_zip()?;
        let file = std::fs::File::open(zip).ok()?;
        let mut archive = zip::ZipArchive::new(file).ok()?;
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).ok()?;
            if is_info_file(entry.name()) {
                let mut bytes = Vec::new();
                entry.read_to_end(&mut bytes).ok()?;
                return Some(bytes);
            }
        }
        None
    }

    #[test]
    #[ignore = "需要 .tmp/rw.zip，用 --ignored 单独跑"]
    fn picks_the_parsable_info_when_a_directory_has_several() {
        /*
         * 回归：「04_前置地图合集」的 `宜/` 目录里 `info_yi` 是真关卡集，
         * `info_yil` 只是随包带进来的资源（没有 LevelSetInfoSO 对象）。
         * 旧实现按目录只留一个 info，后者覆盖前者，于是整包报
         * missingLevelSetInfo。现在必须逐个试解析，取能读出来的那个。
         */
        let Some(info) = real_info_bytes() else {
            eprintln!("跳过：没有 .tmp/rw.zip");
            return;
        };
        let zip_path =
            std::env::temp_dir().join(format!("oc2-lv-multi-{}.zip", uuid::Uuid::new_v4()));
        // 坏 info 故意排在后面：旧实现取最后一个，正好会踩到这个
        write_zip(
            &zip_path,
            &[
                ("Set/info_good", &info),
                ("Set/info_bogus", b"not a unity bundle"),
                ("Set/scene1", b"scene"),
            ],
        );

        let sets =
            inspect_zip(&zip_path).expect("同目录多个 info 时应取能解析的那个，而不是整包失败");
        assert_eq!(sets.len(), 1);
        assert_eq!(sets[0].metadata.name, "RW_LevelSet");
        assert_eq!(sets[0].dir_name, "RW_LevelSet");
        assert_eq!(sets[0].files.len(), 3, "同目录的文件都要归到这个关卡集");

        std::fs::remove_file(&zip_path).unwrap();
    }

    #[test]
    #[ignore = "需要 .tmp/rw.zip，用 --ignored 单独跑"]
    fn reports_the_reason_when_every_info_in_a_directory_fails() {
        // 全部候选都解析不出关卡集时才报错，而且错误里要有解析原因
        let zip_path =
            std::env::temp_dir().join(format!("oc2-lv-bad-{}.zip", uuid::Uuid::new_v4()));
        write_zip(
            &zip_path,
            &[("Set/info_a", b"junk-a"), ("Set/info_b", b"junk-b")],
        );
        let error = inspect_zip(&zip_path).expect_err("全都解析不了时应失败");
        assert!(error.starts_with("levelSet.zipInfoInvalid"), "实际={error}");
        std::fs::remove_file(&zip_path).unwrap();
    }

    /// 用真实地图 ZIP 做端到端验证，路径由环境变量给出（仓库里不放几百 MB 的包）。
    ///
    /// `OC2_LEVEL_SET_REAL_ZIP=<zip 路径> cargo test --lib real_zip -- --ignored --nocapture`
    #[test]
    #[ignore = "需要 OC2_LEVEL_SET_REAL_ZIP 指向真实地图 ZIP"]
    fn inspects_a_real_zip_from_the_environment() {
        let Some(path) = std::env::var_os("OC2_LEVEL_SET_REAL_ZIP").map(PathBuf::from) else {
            eprintln!("跳过：没有 OC2_LEVEL_SET_REAL_ZIP");
            return;
        };
        let sets = inspect_zip(&path).expect("应能识别这个 ZIP");
        assert!(!sets.is_empty(), "至少识别出一个关卡集");
        let mut seen = std::collections::HashSet::new();
        for set in &sets {
            eprintln!(
                "目录={} 集名={} 中文={:?} 作者={} 版本={} 关卡={} 文件={}",
                set.dir_name,
                set.metadata.name,
                set.metadata.name_zh,
                set.metadata.author,
                set.metadata.version,
                set.metadata.levels.len(),
                set.files.len()
            );
            assert!(
                seen.insert(set.dir_name.to_lowercase()),
                "同一批里的目录名必须互不相同：{}",
                set.dir_name
            );
        }
    }
}

// ===== requires.txt：本地包自带的外部 runtime 要求 =====
#[cfg(test)]
mod requires_tests {
    use super::*;

    #[test]
    fn reads_the_runtime_requirement_from_requires_txt() {
        let root = std::env::temp_dir().join(format!("oc2-lv-req-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();

        // 没有文件 = 没有外部 runtime 要求，界面不显示关系卡片
        assert_eq!(read_required_runtime(&root), None);

        // 实测的真实内容：纯版本号（eon_v0.2.1 / tutu / mianhua_v1.4.2 都是它）
        std::fs::write(root.join(REQUIRES_FILE), "3.10.2\n").unwrap();
        assert_eq!(read_required_runtime(&root), Some("3.10.2".into()));

        // 空行与 `#` 注释行要跳过（同目录的 stub_levels.txt 就带说明行）
        std::fs::write(
            root.join(REQUIRES_FILE),
            "# 需要 Web Runtime\n\n  3.8.8  \n",
        )
        .unwrap();
        assert_eq!(read_required_runtime(&root), Some("3.8.8".into()));

        // 全是注释 → 等同于没有要求
        std::fs::write(root.join(REQUIRES_FILE), "# nothing\n\n  \n").unwrap();
        assert_eq!(read_required_runtime(&root), None);

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn runtime_relation_matches_the_online_declaration_shape() {
        /*
         * 必须与地图源里那条 `guid = "oc2.oc2diylevelruntimewloader"` +
         * `min_version = "3.10.2"` 完全同形：依赖表判定、缺依赖提示与跳转
         * 都靠 guid 匹配，形状不同会让两份声明各自算一遍。
         */
        let relation = runtime_relation("3.10.2").expect("有版本号就该产出关系");
        assert_eq!(relation.kind, crate::models::RelationKind::Requires);
        assert_eq!(relation.guid, "oc2.oc2diylevelruntimewloader");
        assert_eq!(relation.min_version, "3.10.2");
        assert!(relation.max_version.is_empty(), "只声明下限");
        assert!(!relation.optional, "缺依赖要提示，不能静默");
        assert!(relation.prerelease.is_none());

        // 空版本号不能造出一条「不限版本」的关系
        assert!(runtime_relation("   ").is_none());
    }

    #[test]
    fn scanning_reads_requires_txt_even_when_info_is_broken() {
        // info 坏掉时开关照样要用，外观要求也不该因此丢掉
        let game = std::env::temp_dir().join(format!("oc2-lv-req-scan-{}", uuid::Uuid::new_v4()));
        let set = game.join(DIYLEVEL_DIR).join(LEVELS_DIR).join("Broken");
        std::fs::create_dir_all(&set).unwrap();
        std::fs::write(set.join("info_broken"), b"not a bundle").unwrap();
        std::fs::write(set.join(REQUIRES_FILE), "3.10.2").unwrap();

        let sets = scan_level_sets(&game);
        assert_eq!(sets.len(), 1);
        assert!(sets[0].metadata.is_none(), "info 本身读不出来");
        assert_eq!(sets[0].required_runtime.as_deref(), Some("3.10.2"));

        std::fs::remove_dir_all(&game).unwrap();
    }
}

// ===== 安装 / 更新：先回收已有的关卡集 =====

/// 把一个关卡集目录展开成「游戏相对路径」列表。
///
/// 回收站按文件记账（这样恢复能逐路径还原、失败能逐文件回滚），所以目录要先
/// 展开。只收文件，不收目录本身 —— 空目录由回收后的清理逻辑处理。
fn collect_files(game_root: &Path, set_dir: &Path) -> Result<Vec<String>, String> {
    let canonical_root = game_root
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(set_dir).follow_links(false) {
        let entry = entry.map_err(|error| error.to_string())?;
        if !entry.file_type().is_file() {
            continue;
        }
        let absolute = entry
            .path()
            .canonicalize()
            .map_err(|error| error.to_string())?;
        // 目录联接可能把真实文件指到游戏目录之外，逐文件确认归属
        let relative = absolute
            .strip_prefix(&canonical_root)
            .map_err(|_| "archive.pathEscape".to_string())?;
        files.push(relative.to_string_lossy().replace('\\', "/"));
    }
    Ok(files)
}

/// 回收一个已存在的关卡集（移进回收站，而不是直接删）。
///
/// 三种情况都要走这里：
/// 1. 在线更新（Release 新版本覆盖旧版本）
/// 2. 本地重复安装（同一个 ZIP 再装一次）
/// 3. 本地更新（手动导入的新版本）
///
/// 直接 `remove_dir_all` 会让用户丢掉自己改过的关卡集，而关卡集往往包含
/// 作者精心调过的场景文件，没有版本控制可回退。
fn recycle_existing_level_set(
    recycle_root: &Path,
    manifests_dir: &Path,
    game_root: &Path,
    set_dir: &Path,
    dir_name: &str,
    metadata: Option<&LevelSetMetadata>,
) -> Result<Option<String>, String> {
    let files = collect_files(game_root, set_dir)?;
    if files.is_empty() {
        /*
         * 空目录没有可回收的内容。直接移除即可 —— 让 `stage` 处理会产生一条
         * 0 文件的回收记录，用户点恢复什么也拿不回来。
         */
        fs::remove_dir_all(set_dir).map_err(|error| error.to_string())?;
        log::info!("关卡集目录为空，已直接移除：目录={}", dir_name);
        return Ok(None);
    }
    let name = metadata
        .map(|meta| meta.name.clone())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| dir_name.to_string());
    let record = crate::recycle_bin::stage(
        recycle_root,
        manifests_dir,
        game_root,
        name,
        None,
        None,
        Vec::new(),
        files,
        None,
        &mut || Ok(()),
    )?;
    // 返回记录 id，写入失败时用它把旧版本恢复回去
    Ok(Some(record.id))
}

/// 安装或更新一个关卡集的完整流程。
///
/// ## 顺序是刻意的
///
/// 1. **先校验 ZIP**：识别不出关卡集就立刻失败，此时磁盘还没有任何改动。
/// 2. **回收旧的**：把已有目录整目录移进回收站（不是删除）。
/// 3. **写入新的**：失败时把回收的记录恢复回去。
///
/// 反过来做（先写后删）会在写入中途失败时同时损失新旧两份。
///
/// `expected` 是调用方声明的关卡集标识（在线安装来自仓库配置的
/// `author + levelSetName`）。**声明与 ZIP 实际内容不一致时直接拒绝**，
/// 不做「装进去再说」—— 那会让仓库与本地绑定错位，更新检测从此失效。
pub fn install_level_set(
    recycle_root: &Path,
    manifests_dir: &Path,
    zip_path: &Path,
    game_root: &Path,
    expected: Option<ExpectedLevelSet>,
) -> Result<InstalledLevelSet, String> {
    let sets = inspect_zip(zip_path)?;

    /*
     * 单集安装：ZIP 里必须恰好有一个关卡集。
     *
     * 一个仓库对应一个关卡集（见项目约定），多集的 ZIP 无法判断该装哪个，
     * 只能让用户改用「导入本地 ZIP」自己挑。
     */
    if expected.is_some() && sets.len() != 1 {
        log::warn!(
            "在线安装的 ZIP 应只含一个关卡集，实际={}，文件={}",
            sets.len(),
            zip_path.display()
        );
        return Err(crate::locale_tag::with_params(
            "levelSet.zipMultipleForRepository",
            &[("count", &sets.len().to_string())],
        ));
    }
    let set = &sets[0];

    /*
     * 声明与实际内容必须一致。
     *
     * 绑定靠 `author + levelSetName`，两者任一不符就说明这个仓库指向的资源
     * 变了（作者改了名，或仓库配错了）。此时装进去会让本地关卡集与仓库
     * 对不上，后续所有更新检测都失效 —— 与其埋这个雷，不如当场拒绝。
     */
    if let Some(expected) = &expected {
        let matches = level_set_identity_matches(
            &set.metadata.name,
            set.metadata.name_zh.as_deref(),
            &set.metadata.author,
            &expected.level_set_name,
            expected.level_set_name_zh.as_deref(),
            &expected.author,
        );
        if !matches {
            log::warn!(
                "关卡集与仓库声明不一致：期望={} / {}，实际={} / {}",
                expected.level_set_name,
                expected.author,
                set.metadata.name,
                set.metadata.author
            );
            return Err(crate::locale_tag::with_params(
                "levelSet.zipMismatch",
                &[
                    (
                        "expected",
                        &format!("{} / {}", expected.level_set_name, expected.author),
                    ),
                    (
                        "actual",
                        &format!("{} / {}", set.metadata.name, set.metadata.author),
                    ),
                ],
            ));
        }
    }

    let levels_dir = game_root.join(DIYLEVEL_DIR).join(LEVELS_DIR);
    let disabled_dir = game_root.join(DIYLEVEL_DIR).join(DISABLED_DIR);

    /*
     * 目标目录名可能已被**另一张**地图占用。
     *
     * 典型场景是两个不同地图的 `levelSetName` 与 `author` 完全相同（实测
     * 「胡闹糕中」与「胡闹达学」都写 GAODA / GAODA）。目录名由集名派生，
     * 后装的那个会把先装的当成旧版本回收掉 —— 界面上看起来只是「更新」，
     * 实际丢了一张图。身份不同就必须换目录名，让两张真正并存。
     */
    let mut dir_name = set.dir_name.clone();
    let mut destination = levels_dir.join(&dir_name);
    if destination.is_dir() {
        let existing = read_level_set(destination.clone(), dir_name.clone(), true);
        let same_identity = existing.metadata.as_ref().is_some_and(|existing_meta| {
            level_set_identity_matches(
                &existing_meta.name,
                existing_meta.name_zh.as_deref(),
                &existing_meta.author,
                &set.metadata.name,
                set.metadata.name_zh.as_deref(),
                &set.metadata.author,
            )
        });
        if !same_identity {
            let mut taken = collect_dir_names(&levels_dir);
            taken.extend(collect_dir_names(&disabled_dir));
            dir_name = allocate_dir_name(&dir_name, &set.metadata, &mut taken, false);
            destination = levels_dir.join(&dir_name);
            log::info!(
                "目标目录已被另一张关卡集占用，改用新目录名并存：原目录={}，新目录={}",
                set.dir_name,
                dir_name
            );
        }
    }

    /*
     * 已存在就回收 —— 在线更新、重复安装、本地更新三条路径都落在这一处。
     *
     * 同时检查 `disabled/` 下的同名目录：用户可能先停用了旧版本再装新的，
     * 那个旧目录仍然是他的数据，也该一并回收，否则启用时会撞名。
     */
    let mut recycled: Vec<String> = Vec::new();
    for candidate in [destination.clone(), disabled_dir.join(&dir_name)] {
        if !candidate.is_dir() {
            continue;
        }
        let existing_meta = read_level_set(candidate.clone(), dir_name.clone(), true).metadata;
        if let Some(record_id) = recycle_existing_level_set(
            recycle_root,
            manifests_dir,
            game_root,
            &candidate,
            &dir_name,
            existing_meta.as_ref(),
        )? {
            recycled.push(record_id);
        }
    }

    // 写入新内容；失败时把刚才回收的旧版本放回去
    let write_result = write_level_set(zip_path, game_root, &dir_name, set);
    if let Err(error) = write_result {
        log::error!("关卡集写入失败，尝试恢复已回收的旧版本：原因={}", error);
        for record_id in &recycled {
            /*
             * 只做尽力恢复：即使恢复失败，回收记录仍在回收站里，
             * 用户可以手动恢复，数据不会丢。
             */
            if let Err(restore) =
                crate::recycle_bin::restore(recycle_root, manifests_dir, game_root, record_id)
            {
                log::warn!(
                    "自动恢复旧关卡集失败，记录保留在回收站：记录={}，原因={}",
                    record_id,
                    restore
                );
            }
        }
        return Err(error);
    }

    log::info!(
        "关卡集安装完成：集={}，版本={}，作者={}，目录={}",
        set.metadata.name,
        set.metadata.version,
        set.metadata.author,
        destination.display()
    );
    Ok(InstalledLevelSet {
        dir_name: dir_name.clone(),
        metadata: set.metadata.clone(),
        replaced: !recycled.is_empty(),
    })
}

/// 调用方声明的关卡集标识（来自地图仓库配置）。
#[derive(Debug, Clone)]
pub struct ExpectedLevelSet {
    pub level_set_name: String,
    /// 声明的中文名；老地图源不写这个字段，比对时退回英文集名 + 作者。
    pub level_set_name_zh: Option<String>,
    pub author: String,
}

/// 安装结果。
#[derive(Debug, Clone)]
pub struct InstalledLevelSet {
    pub dir_name: String,
    pub metadata: LevelSetMetadata,
    /// 是否替换掉了已有版本（更新或重复安装）。
    pub replaced: bool,
}

/// 把单个关卡集从 ZIP 写入 `levels/`。
fn write_level_set(
    zip_path: &Path,
    game_root: &Path,
    dir_name: &str,
    set: &ImportedLevelSet,
) -> Result<(), String> {
    let destination = game_root.join(DIYLEVEL_DIR).join(LEVELS_DIR).join(dir_name);
    fs::create_dir_all(&destination).map_err(|error| error.to_string())?;

    let file = fs::File::open(zip_path).map_err(|error| error.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| error.to_string())?;
    /*
     * 先建「条目名 → 索引」表，再用 `by_index` 取内容。
     *
     * `by_name` 每次都从头线性扫描中央目录；一个含 11 个文件的 ZIP 上不明显，
     * 但多关卡集（几十个文件）会明显变慢，而这是用户点一次「安装」就要等的。
     */
    let mut index_of = std::collections::HashMap::new();
    for index in 0..archive.len() {
        if let Ok(entry) = archive.by_index_raw(index) {
            index_of.insert(entry.name().replace('\\', "/"), index);
        }
    }

    for (entry_name, relative) in &set.files {
        if !is_safe_relative(relative) {
            log::warn!(
                "拒绝不安全的 ZIP 条目：条目={}，相对路径={}",
                entry_name,
                relative
            );
            // 已创建的目录要清掉，避免留下半个关卡集
            let _ = fs::remove_dir_all(&destination);
            return Err("archive.pathEscape".into());
        }
        /*
         * `info_<集名>` 落盘成固定名 `info`：DLL 只认这个。`s_*` 原样保留 ——
         * 游戏按 `sceneName` 找场景，改名就找不到。
         */
        let disk_name = if is_info_file(relative) {
            "info".to_string()
        } else {
            relative.clone()
        };
        let target = destination.join(&disk_name);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let Some(index) = index_of.get(entry_name) else {
            let _ = fs::remove_dir_all(&destination);
            return Err(crate::locale_tag::with_params(
                "levelSet.zipEntryMissing",
                &[("entry", entry_name)],
            ));
        };
        let mut entry = archive
            .by_index(*index)
            .map_err(|error| error.to_string())?;
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        fs::write(&target, &bytes).map_err(|error| {
            crate::locale_tag::with_params(
                "levelSet.writeFailed",
                &[("reason", &error.to_string())],
            )
        })?;
    }
    Ok(())
}

/// 关卡集管理新增行为的回归测试：info 解析缓存、目录定位、删除与回收。
///
/// 全部用例都在 `std::env::temp_dir()` 下自建临时目录，不依赖本机游戏目录，
/// 任何机器上都能跑；缓存用例只关心「解析次数」和「结果是否跟着文件变」，
/// 与真实关卡集内容无关。
#[cfg(test)]
mod map_management_tests {
    use super::*;

    /// 每个用例独立根目录，避免并行测试互相踩；用例结束自己删掉。
    fn workspace(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("oc2-lv-{tag}-{}", uuid::Uuid::new_v4()));
        let game = root.join("game");
        let recycle = root.join("recycle");
        let manifests = root.join("manifests");
        std::fs::create_dir_all(game.join(DIYLEVEL_DIR).join(LEVELS_DIR)).unwrap();
        std::fs::create_dir_all(game.join(DIYLEVEL_DIR).join(DISABLED_DIR)).unwrap();
        std::fs::create_dir_all(&recycle).unwrap();
        std::fs::create_dir_all(&manifests).unwrap();
        (root, game, recycle)
    }

    /// 缓存的命中与失效：同一文件第二次扫描不再解析，内容变了必须重新解析。
    ///
    /// 只有命中没有失效就是脏数据 —— 用户替换关卡集后界面还显示旧名字。
    /// 所以这里同时验证：命中时计数不涨，文件换掉后计数 +1 且错误内容跟着变。
    #[test]
    fn info_cache_parses_once_until_the_file_changes() {
        let (root, game, _recycle) = workspace("cache");
        let set_dir = game.join(DIYLEVEL_DIR).join(LEVELS_DIR).join("Cached");
        std::fs::create_dir_all(&set_dir).unwrap();
        let info = set_dir.join("info");
        std::fs::write(&info, b"not a bundle").unwrap();

        let first = scan_level_sets(&game);
        assert_eq!(first.len(), 1);
        let first_error = first[0].error.clone().expect("非 AssetBundle 应报错");
        assert_eq!(
            info_cache_parses(&info),
            Some(1),
            "首次扫描必须真正解析一次"
        );

        let second = scan_level_sets(&game);
        assert_eq!(second[0].error.as_deref(), Some(first_error.as_str()));
        assert_eq!(info_cache_parses(&info), Some(1), "文件没变时不该重复解析");

        // 换掉内容且长度不同：len/mtime 指纹失效，必须重新解析并拿到新结果。
        std::fs::write(&info, b"UnityFS\0\0\0\0\0\0\0\0\0\0\0\0").unwrap();
        let third = scan_level_sets(&game);
        assert_ne!(
            third[0].error.as_deref(),
            Some(first_error.as_str()),
            "文件被替换后应重新解析并拿到新结果"
        );
        assert_eq!(info_cache_parses(&info), Some(2), "指纹变化应再解析一次");

        std::fs::remove_dir_all(&root).unwrap();
    }

    /// 扫描要带回关卡集目录的真实绝对路径。
    ///
    /// 详情面板把它与目录名合成「路径」一行；停用集必须指向 `disabled/`，
    /// 否则用户照抄出来的路径在磁盘上根本不存在。坏掉的关卡集（info 解析
    /// 失败）同样要带路径 —— 认不出元数据不等于找不到目录。
    #[test]
    fn scan_reports_the_absolute_directory_path_for_both_sides() {
        let (root, game, _recycle) = workspace("path");
        let enabled_dir = game.join(DIYLEVEL_DIR).join(LEVELS_DIR).join("EnabledSet");
        let disabled_dir = game
            .join(DIYLEVEL_DIR)
            .join(DISABLED_DIR)
            .join("DisabledSet");
        std::fs::create_dir_all(&enabled_dir).unwrap();
        std::fs::create_dir_all(&disabled_dir).unwrap();
        std::fs::write(enabled_dir.join("info"), b"not a bundle").unwrap();
        std::fs::write(disabled_dir.join("info"), b"not a bundle").unwrap();

        let sets = scan_level_sets(&game);
        assert_eq!(sets.len(), 2);
        let enabled = sets
            .iter()
            .find(|set| set.dir_name == "EnabledSet")
            .expect("应扫到启用集");
        let disabled = sets
            .iter()
            .find(|set| set.dir_name == "DisabledSet")
            .expect("应扫到停用集");

        assert!(enabled.enabled);
        assert_eq!(enabled.path, enabled_dir, "启用集应指向 levels/<目录名>");
        assert!(enabled.path.is_absolute(), "路径必须是绝对路径");
        assert!(
            enabled
                .path
                .ends_with(Path::new(LEVELS_DIR).join("EnabledSet"))
        );

        assert!(!disabled.enabled);
        assert_eq!(
            disabled.path, disabled_dir,
            "停用集应指向 disabled/<目录名>"
        );
        assert!(disabled.path.is_absolute(), "停用集路径也必须是绝对路径");
        assert!(
            disabled
                .path
                .ends_with(Path::new(DISABLED_DIR).join("DisabledSet"))
        );

        std::fs::remove_dir_all(&root).unwrap();
    }

    /// 定位必须跟随 `enabled` 决定去 `levels/` 还是 `disabled/` 找。
    ///
    /// 传错一侧就会「打开了别的目录」或「删掉了本该保留的副本」，所以这条
    /// 用同名目录只存在于一侧来验证，而不是只看是否成功。
    #[test]
    fn locating_respects_the_enabled_side() {
        let (root, game, _recycle) = workspace("locate");
        let enabled_dir = game.join(DIYLEVEL_DIR).join(LEVELS_DIR).join("Same");
        std::fs::create_dir_all(&enabled_dir).unwrap();

        let located = locate_level_set_directory(&game, "Same", true).unwrap();
        assert_eq!(located, enabled_dir.canonicalize().unwrap());
        assert_eq!(
            locate_level_set_directory(&game, "Same", false).unwrap_err(),
            "levelSet.notFound",
            "停用侧没有同名目录时必须报 notFound"
        );

        std::fs::remove_dir_all(&root).unwrap();
    }

    /// 开着回收站时，删除是「搬进回收站」而不是真删：记录里要能看到这两个文件。
    #[test]
    fn deleting_moves_the_set_into_the_recycle_bin() {
        let (root, game, recycle) = workspace("recycle");
        let manifests = root.join("manifests");
        let set = game.join(DIYLEVEL_DIR).join(LEVELS_DIR).join("MySet");
        std::fs::create_dir_all(&set).unwrap();
        std::fs::write(set.join("info"), b"placeholder").unwrap();
        std::fs::write(set.join("s_myset_1"), b"level").unwrap();

        remove_level_set(&recycle, &manifests, &game, "MySet", true, true).unwrap();

        assert!(!set.exists(), "关卡集目录应已移走");
        let records = crate::recycle_bin::list(&recycle).unwrap();
        assert_eq!(records.len(), 1, "应产生一条回收记录");
        assert_eq!(records[0].file_count, 2, "两个文件都应进回收记录");

        std::fs::remove_dir_all(&root).unwrap();
    }

    /// 关掉回收站时直接删干净，且不留下空回收记录。
    #[test]
    fn deleting_without_recycling_removes_the_directory() {
        let (root, game, recycle) = workspace("purge");
        let manifests = root.join("manifests");
        let set = game.join(DIYLEVEL_DIR).join(DISABLED_DIR).join("MySet");
        std::fs::create_dir_all(&set).unwrap();
        std::fs::write(set.join("info"), b"placeholder").unwrap();

        // enabled=false：在 disabled/ 下找
        remove_level_set(&recycle, &manifests, &game, "MySet", false, false).unwrap();

        assert!(!set.exists(), "关卡集目录应已删除");
        assert!(
            crate::recycle_bin::list(&recycle).unwrap().is_empty(),
            "关闭回收时不该产生记录"
        );

        std::fs::remove_dir_all(&root).unwrap();
    }

    /// 不安全的目录名必须在碰盘之前就被拒；不存在的目录报 notFound 而不是报别的错。
    #[test]
    fn deleting_rejects_unsafe_names_and_missing_sets() {
        let (root, game, recycle) = workspace("guard");
        let manifests = root.join("manifests");

        for name in ["", "..", "../escape", "a/b", "C:evil"] {
            let result = remove_level_set(&recycle, &manifests, &game, name, true, true);
            assert!(result.is_err(), "应拒绝不安全的目录名 {name:?}");
        }
        assert_eq!(
            remove_level_set(&recycle, &manifests, &game, "Missing", true, true).unwrap_err(),
            "levelSet.notFound"
        );

        std::fs::remove_dir_all(&root).unwrap();
    }
}

/// 「已存在就回收」的回归测试。
///
/// 用户明确要求：在线更新、本地重复安装、本地更新三条路径都要回收旧关卡集，
/// 而不是直接删除。关卡集常含作者精心调过的场景文件，没有版本控制可回退。
#[cfg(test)]
mod recycle_on_install_tests {
    use super::*;

    fn rw_zip() -> Option<PathBuf> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.tmp/rw.zip");
        path.is_file().then_some(path)
    }

    fn workspace(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("oc2-lv-{tag}-{}", uuid::Uuid::new_v4()));
        let game = root.join("game");
        let recycle = root.join("recycle");
        let manifests = root.join("manifests");
        std::fs::create_dir_all(game.join(DIYLEVEL_DIR).join(LEVELS_DIR)).unwrap();
        std::fs::create_dir_all(&recycle).unwrap();
        std::fs::create_dir_all(&manifests).unwrap();
        (root, game, recycle)
    }

    #[test]
    #[ignore = "需要 .tmp/rw.zip，用 --ignored 单独跑"]
    fn reinstalling_the_same_set_recycles_the_old_one_first() {
        let Some(zip) = rw_zip() else {
            eprintln!("跳过：没有 .tmp/rw.zip");
            return;
        };
        let (root, game, recycle) = workspace("dup");
        let manifests = root.join("manifests");

        // 第一次安装：没有旧版本，不该产生回收记录
        let first = install_level_set(&recycle, &manifests, &zip, &game, None).unwrap();
        assert!(!first.replaced, "首次安装不应标记为替换");
        assert!(
            crate::recycle_bin::list(&recycle).unwrap().is_empty(),
            "首次安装不该产生回收记录"
        );

        // 第二次：必须先把旧关卡集回收，再写入新的
        let second = install_level_set(&recycle, &manifests, &zip, &game, None).unwrap();
        assert!(second.replaced, "重复安装应标记为替换");
        let records = crate::recycle_bin::list(&recycle).unwrap();
        assert_eq!(records.len(), 1, "旧关卡集应进回收站，而不是被删掉");
        assert!(
            records[0].file_count > 0,
            "回收记录必须带文件，否则恢复出来是空的"
        );

        // 新版本仍然完整可用
        let sets = scan_level_sets(&game);
        assert_eq!(sets.len(), 1);
        assert_eq!(sets[0].metadata.as_ref().unwrap().levels.len(), 10);

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    #[ignore = "需要 .tmp/rw.zip，用 --ignored 单独跑"]
    fn recycling_also_clears_a_disabled_copy_with_the_same_name() {
        let Some(zip) = rw_zip() else {
            eprintln!("跳过：没有 .tmp/rw.zip");
            return;
        };
        let (root, game, recycle) = workspace("disabled");
        let manifests = root.join("manifests");

        /*
         * 用户先停用了旧版本（在 disabled/），再装新的。
         * 那个旧目录仍是他的数据，必须一起回收 —— 否则启用时会撞名。
         */
        let stale = game
            .join(DIYLEVEL_DIR)
            .join(DISABLED_DIR)
            .join("RW_LevelSet");
        std::fs::create_dir_all(&stale).unwrap();
        std::fs::write(stale.join("info"), b"stale").unwrap();

        let installed = install_level_set(&recycle, &manifests, &zip, &game, None).unwrap();
        assert!(installed.replaced, "停用目录也算已有版本");
        assert!(!stale.exists(), "停用的同名目录应被移走");
        assert_eq!(crate::recycle_bin::list(&recycle).unwrap().len(), 1);

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    #[ignore = "需要 .tmp/rw.zip，用 --ignored 单独跑"]
    fn a_mismatched_declaration_is_rejected_without_touching_the_disk() {
        let Some(zip) = rw_zip() else {
            eprintln!("跳过：没有 .tmp/rw.zip");
            return;
        };
        let (root, game, recycle) = workspace("mismatch");
        let manifests = root.join("manifests");

        // 仓库声明的是另一个关卡集：装进去会让绑定错位，后续更新检测全失效
        let wrong = ExpectedLevelSet {
            level_set_name: "SomethingElse".into(),
            level_set_name_zh: None,
            author: "RW".into(),
        };
        let result = install_level_set(&recycle, &manifests, &zip, &game, Some(wrong));
        assert!(result.is_err(), "声明与实际不一致时应拒绝");
        // 拒绝发生在任何磁盘改动之前
        assert!(
            scan_level_sets(&game).is_empty(),
            "被拒绝时不该留下任何关卡集"
        );
        assert!(crate::recycle_bin::list(&recycle).unwrap().is_empty());

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    #[ignore = "需要 .tmp/rw.zip，用 --ignored 单独跑"]
    fn a_matching_declaration_is_accepted() {
        let Some(zip) = rw_zip() else {
            eprintln!("跳过：没有 .tmp/rw.zip");
            return;
        };
        let (root, game, recycle) = workspace("match");
        let manifests = root.join("manifests");

        let expected = ExpectedLevelSet {
            level_set_name: "RW_LevelSet".into(),
            level_set_name_zh: None,
            author: "RW".into(),
        };
        let installed =
            install_level_set(&recycle, &manifests, &zip, &game, Some(expected)).unwrap();
        assert_eq!(installed.metadata.version, "0.73");

        std::fs::remove_dir_all(&root).unwrap();
    }

    /// 磁盘缓存往返：同一份内容写进去再读出来必须一模一样。
    ///
    /// 这条覆盖的是「跨进程命中」的核心机制 —— 进程内缓存用 (len, mtime)，
    /// 重启就没了；磁盘这份用内容 SHA-256，重启后仍然命中，这正是用户要的
    /// 「文件没变就直接读缓存」。哈希算法选 SHA-256 的依据见 `content_sha256`。
    #[test]
    fn info_disk_cache_round_trips_by_content_hash() {
        let dir = std::env::temp_dir().join(format!(
            "oc2-info-disk-cache-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);

        let metadata = LevelSetMetadata {
            name: "RoundTrip".into(),
            name_zh: Some("往返测试".into()),
            author: "Lead".into(),
            version: "1.0".into(),
            uid: "uid-1".into(),
            base_uid: Some("base-1".into()),
            levels: vec![crate::asset_bundle::LevelMetadata {
                name: "L1".into(),
                name_zh: None,
                scene_name: "Scene".into(),
                min_order_count: 1,
                max_order_count: 4,
            }],
        };
        let payload = Result::<LevelSetMetadata, String>::Ok(metadata.clone());
        let sha = content_sha256(b"payload");

        assert!(
            read_info_disk_cache(&dir, &sha).is_none(),
            "空目录必须未命中"
        );
        write_info_disk_cache(&dir, &sha, &payload);
        assert_eq!(
            read_info_disk_cache(&dir, &sha),
            Some(payload),
            "写入后应能读回同一份结果"
        );

        // 哈希对不上（文件被改过）时不认
        assert!(read_info_disk_cache(&dir, "deadbeef").is_none());

        // 失败结果同样缓存：坏 info 不该每次扫描都重付解析代价
        let failure = Result::<LevelSetMetadata, String>::Err("assetBundle.notABundle".into());
        write_info_disk_cache(&dir, &sha, &failure);
        assert_eq!(read_info_disk_cache(&dir, &sha), Some(failure));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 清缓存只清本模块的文件，别把 `cache/` 里邻居的缓存一起删了。
    ///
    /// 这条同时钉住返回值语义：前端拿它显示「已清除 N 份」，数字必须等于
    /// 真正删掉的缓存份数（`*.json` 与写一半的 `*.json.tmp`），不含目录里的
    /// 其它文件。
    #[test]
    fn clear_info_disk_cache_removes_only_our_files() {
        let dir = std::env::temp_dir().join(format!(
            "oc2-info-clear-cache-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);

        let payload = Result::<LevelSetMetadata, String>::Err("assetBundle.notABundle".into());
        let first = content_sha256(b"first");
        let second = content_sha256(b"second");
        write_info_disk_cache(&dir, &first, &payload);
        write_info_disk_cache(&dir, &second, &payload);
        // 模拟进程被杀在写一半时留下的临时文件
        std::fs::write(dir.join("halfwritten.json.tmp"), b"{").unwrap();
        // 邻居模块的缓存：不能被顺手删掉
        std::fs::write(dir.join("keep-me.bin"), b"neighbour").unwrap();

        let removed = clear_info_disk_cache(&dir).unwrap();
        assert_eq!(removed, 3, "应删掉两份缓存与一个写一半的临时文件");
        assert!(
            read_info_disk_cache(&dir, &first).is_none(),
            "清完之后必须未命中"
        );
        assert!(read_info_disk_cache(&dir, &second).is_none());
        assert!(
            dir.join("keep-me.bin").is_file(),
            "目录里其它模块的文件不能删"
        );

        // 目录已空追删不是错误，返回 0
        assert_eq!(clear_info_disk_cache(&dir).unwrap(), 0);
        // 目录不存在同样返回 0，不报错
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(clear_info_disk_cache(&dir).unwrap(), 0);
    }

    /// 并行扫描必须保持串行时的顺序，进度必须是「真实完成数」。
    ///
    /// 真实目录里各份 `info` 大小差几十倍（0.3 MB 与 65 MB 并存），谁先算完
    /// 完全不确定 —— 结果按原下标归位这件事只能靠断言钉住。这里不往目录里放
    /// `info`：解析失败的集照样要出现在列表里（卡片仍能按目录名开关），顺带
    /// 覆盖「坏集不阻塞扫描」。
    #[test]
    fn parallel_scan_keeps_order_and_reports_progress() {
        let root = std::env::temp_dir().join(format!(
            "oc2-level-scan-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let base = root.join(DIYLEVEL_DIR);
        // 刻意乱序创建：目录项的读取顺序不代表扫描输出的顺序
        let plan: [(&str, &[&str]); 2] = [
            (LEVELS_DIR, &["zeta", "alpha", "mike"]),
            (DISABLED_DIR, &["beta", "yankee"]),
        ];
        for (side, names) in plan {
            for &name in names {
                std::fs::create_dir_all(base.join(side).join(name)).unwrap();
            }
        }

        let seen: Mutex<Vec<(usize, usize, String)>> = Mutex::new(Vec::new());
        let sets = scan_level_sets_with_progress(&root, &|done, total, current| {
            seen.lock()
                .unwrap()
                .push((done, total, current.to_string()));
        });

        let order: Vec<&str> = sets.iter().map(|set| set.dir_name.as_str()).collect();
        assert_eq!(
            order,
            vec!["alpha", "mike", "zeta", "beta", "yankee"],
            "并行不能打乱「先 levels 再 disabled、各自按名排序」"
        );
        assert!(
            sets.iter()
                .all(|set| set.error.as_deref() == Some("levelSet.infoMissing")),
            "没有 info 的集要保留在列表里并带上原因"
        );

        let progress = seen.lock().unwrap();
        assert_eq!(
            progress
                .first()
                .map(|(done, total, current)| (*done, *total, current.as_str())),
            Some((0, 5, "")),
            "第一句进度是「还没开始，共 5 份」"
        );
        assert_eq!(
            progress.last().map(|(done, total, _)| (*done, *total)),
            Some((5, 5)),
            "最后一句进度必须等于总数"
        );
        let mut done_values: Vec<usize> = progress.iter().map(|(done, _, _)| *done).collect();
        done_values.sort_unstable();
        assert_eq!(
            done_values,
            vec![0, 1, 2, 3, 4, 5],
            "完成数必须逐份递增，不能跳号也不能重复"
        );

        std::fs::remove_dir_all(&root).unwrap();
    }
}
