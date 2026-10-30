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
use std::fs;
use std::io::{Read, Seek};
use std::path::{Component, Path, PathBuf};

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
                metadata: None,
                error: Some("levelSet.infoMissing".into()),
            };
        }
    };
    match fs::read(&info_path)
        .map_err(|error| error.to_string())
        .and_then(|bytes| asset_bundle::parse_level_set(&bytes).map_err(|error| error.to_string()))
    {
        Ok(metadata) => LevelSetOnDisk {
            dir_name,
            enabled,
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
    if dir_name.trim().is_empty()
        || Path::new(dir_name)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
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
