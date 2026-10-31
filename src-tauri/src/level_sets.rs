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
}

/// 扫描一个游戏目录下的全部关卡集（启用与停用）。
///
/// 顺序：先 `levels/` 再 `disabled/`，各自按目录名排序 —— 排序结果稳定，
/// 界面不会因为在两次扫描之间插入了一个新目录就整片重排。
pub fn scan_level_sets(game_root: &Path) -> Vec<LevelSetOnDisk> {
    let base = game_root.join(DIYLEVEL_DIR);
    let mut sets = Vec::new();
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
            sets.push(read_level_set(
                base.join(dir_name).join(&name),
                name,
                enabled,
            ));
        }
    }
    log::debug!("关卡集扫描完成：数量={}，根={}", sets.len(), base.display());
    sets
}

/// 读取单个关卡集目录。
///
/// `info` 的文件名有两种历史写法：固定名 `info`（DLL 认这个），以及导出工具
/// 带的 `info_<集名>`。先找前者，找不到再找唯一一个 `info_*` —— 两种都在
/// 真实资源里出现过。
fn read_level_set(directory: PathBuf, dir_name: String, enabled: bool) -> LevelSetOnDisk {
    let info_path = match locate_info_file(&directory) {
        Some(path) => path,
        None => {
            return LevelSetOnDisk {
                dir_name,
                enabled,
                path: directory,
                metadata: None,
                error: Some("levelSet.infoMissing".into()),
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
            }
        }
    }
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
    let result = fs::read(&key)
        .map_err(|error| error.to_string())
        .and_then(|bytes| asset_bundle::parse_level_set(&bytes).map_err(|error| error.to_string()));
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

/// 两个关卡集身份是否相同：`level_set_name` 与 `author` 都 trim 后忽略大小写比较。
///
/// 空名视为解析失败，永不匹配 —— 否则所有读不出 `info` 的目录会抢同一张地图。
/// 作者允许为空（官方导出的 `info` 常不写作者），空对空算相同。
pub fn level_set_identity_matches(
    level_set_name: &str,
    author: &str,
    other_level_set_name: &str,
    other_author: &str,
) -> bool {
    let name = level_set_name.trim();
    let other = other_level_set_name.trim();
    !name.is_empty()
        && !other.is_empty()
        && name.eq_ignore_ascii_case(other)
        && author.trim().eq_ignore_ascii_case(other_author.trim())
}

#[cfg(test)]
mod identity_tests {
    use super::level_set_identity_matches;

    #[test]
    fn identity_matches_ignore_case_and_surrounding_whitespace() {
        assert!(level_set_identity_matches(
            "RW_LevelSet",
            "RW",
            " rw_levelset ",
            " rw "
        ));
    }

    #[test]
    fn identity_requires_a_non_empty_level_set_name() {
        // 空名 = info 没解析出来，不能把所有坏目录都算成同一张地图
        assert!(!level_set_identity_matches("", "", "", ""));
        assert!(!level_set_identity_matches("  ", "RW", "RW_LevelSet", "RW"));
        assert!(!level_set_identity_matches("RW_LevelSet", "RW", "", "RW"));
    }

    #[test]
    fn identity_compares_the_author_too() {
        assert!(!level_set_identity_matches(
            "RW_LevelSet",
            "RW",
            "RW_LevelSet",
            "Other"
        ));
        // 官方导出的 info 常不写作者：两边都空算相同
        assert!(level_set_identity_matches(
            "RW_LevelSet",
            "",
            "RW_LevelSet",
            ""
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
    let mut info_entries: BTreeMap<String, usize> = BTreeMap::new();
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
            info_entries.insert(directory, index);
        }
    }

    if info_entries.is_empty() {
        return Err("levelSet.zipWithoutInfo".into());
    }

    let mut imported = Vec::with_capacity(info_entries.len());
    for (directory, index) in info_entries {
        let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        let metadata = asset_bundle::parse_level_set(&bytes).map_err(|error| {
            crate::locale_tag::with_params(
                "levelSet.zipInfoInvalid",
                &[("reason", &error.to_string())],
            )
        })?;

        /*
         * 关卡集目录名：用 info 里的英文集名，而不是 ZIP 里的目录名。
         *
         * 后者常是作者的临时命名（`新建文件夹`、`levels`、甚至根目录为空串），
         * 而集名是作者在编辑器里真正填的名字，落到磁盘后还能和元数据对上。
         * 清理掉不能做目录名的字符。
         */
        let dir_name = sanitize_dir_name(&metadata.name)
            .or_else(|| sanitize_dir_name(&directory.rsplit('/').next().unwrap_or("")))
            .ok_or_else(|| "levelSet.invalidName".to_string())?;

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

    // 同名关卡集出现两次（例如 ZIP 里有 `a/info` 和 `b/info` 但集名相同）
    let mut seen = std::collections::HashSet::new();
    for set in &imported {
        if !seen.insert(set.dir_name.clone()) {
            return Err(crate::locale_tag::with_params(
                "levelSet.zipDuplicate",
                &[("name", &set.dir_name)],
            ));
        }
    }
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
    for set in &targets {
        let destination = levels_dir.join(&set.dir_name);
        if destination.exists() {
            return Err(crate::locale_tag::with_params(
                "levelSet.targetExists",
                &[("name", &set.dir_name)],
            ));
        }
    }

    let mut installed = Vec::new();
    for set in targets {
        write_level_set(zip_path, game_root, set)?;
        installed.push(set.dir_name.clone());
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
        let name_matches = set
            .metadata
            .name
            .eq_ignore_ascii_case(&expected.level_set_name);
        let author_matches = set.metadata.author.eq_ignore_ascii_case(&expected.author);
        if !name_matches || !author_matches {
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
    let destination = levels_dir.join(&set.dir_name);

    /*
     * 已存在就回收 —— 在线更新、重复安装、本地更新三条路径都落在这一处。
     *
     * 同时检查 `disabled/` 下的同名目录：用户可能先停用了旧版本再装新的，
     * 那个旧目录仍然是他的数据，也该一并回收，否则启用时会撞名。
     */
    let disabled_dir = game_root.join(DIYLEVEL_DIR).join(DISABLED_DIR);
    let mut recycled: Vec<String> = Vec::new();
    for candidate in [destination.clone(), disabled_dir.join(&set.dir_name)] {
        if !candidate.is_dir() {
            continue;
        }
        let existing_meta = read_level_set(candidate.clone(), set.dir_name.clone(), true).metadata;
        if let Some(record_id) = recycle_existing_level_set(
            recycle_root,
            manifests_dir,
            game_root,
            &candidate,
            &set.dir_name,
            existing_meta.as_ref(),
        )? {
            recycled.push(record_id);
        }
    }

    // 写入新内容；失败时把刚才回收的旧版本放回去
    let write_result = write_level_set(zip_path, game_root, set);
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
        dir_name: set.dir_name.clone(),
        metadata: set.metadata.clone(),
        replaced: !recycled.is_empty(),
    })
}

/// 调用方声明的关卡集标识（来自地图仓库配置）。
#[derive(Debug, Clone)]
pub struct ExpectedLevelSet {
    pub level_set_name: String,
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
    set: &ImportedLevelSet,
) -> Result<(), String> {
    let destination = game_root
        .join(DIYLEVEL_DIR)
        .join(LEVELS_DIR)
        .join(&set.dir_name);
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
            author: "RW".into(),
        };
        let installed =
            install_level_set(&recycle, &manifests, &zip, &game, Some(expected)).unwrap();
        assert_eq!(installed.metadata.version, "0.73");

        std::fs::remove_dir_all(&root).unwrap();
    }
}
