use crate::archive;
use crate::display_path::display_path;
use crate::game;
use chrono::Utc;
use std::borrow::Cow;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use uuid::Uuid;

/// 旧备份的固定相对目录；当前操作由扫描选中的 DLL 目录构造动态 spec。
pub(crate) const DIYLEVEL_DIR: &str = "BepInEx/plugins/OC2DIYLevel";

/// 列出已有的 DIYLevel 备份（按名称升序，与 BepInEx 备份同样的排列方式）。
pub(crate) fn list_diy_level_backups(backup_root: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(backup_root) else {
        return Vec::new();
    };
    let mut backups = entries
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(DIYLEVEL_BACKUP_PREFIX)
                && entry.path().is_dir()
        })
        .map(|entry| display_path(entry.path()))
        .collect::<Vec<_>>();
    backups.sort();
    backups
}

/// 备份目录名前缀：`DIYLevel.backup-<时间戳>-<uuid>`。
const DIYLEVEL_BACKUP_PREFIX: &str = "DIYLevel.backup-";

/// 备份里保存安装清单用的文件名（放在备份目录根部）。
///
/// 备份目录是框架目录的**完整可还原快照**，不只是文件副本：恢复后必须能
/// 再次卸载，而卸载依赖安装清单。所以清单要跟文件一起进备份，恢复时一并
/// 写回 `manifests/<id>.toml`。
///
/// 早期版本没备份清单，导致「卸载 → 恢复 → 再卸载」时报
/// 「没有安装清单，拒绝删除未知文件」，用户只能手动删目录。
const FRAMEWORK_BACKUP_MANIFEST: &str = "framework-manifest.toml";

/// 一次框架备份的内容描述。
///
/// 两个框架（BepInEx / DIYLevel）的备份范围不同，但「备份什么、恢复什么、
/// 清单放哪」完全同构，所以统一成这个结构，避免两套实现各自漂移：
///   · `directories` —— 需要整棵树拷贝的目录（游戏相对路径）
///   · `files`       —— 散落在游戏根目录的框架文件（游戏相对路径）
pub(crate) struct FrameworkBackupSpec<'a> {
    /// 备份目录名前缀，也是恢复/删除时校验用的标识。
    prefix: &'static str,
    /// 需要整目录拷贝的游戏相对路径。
    directories: Cow<'a, [Cow<'a, str>]>,
    /// 需要单文件拷贝的游戏相对路径（与 `directories` 互不重叠）。
    files: &'static [&'static str],
}

/// BepInEx 框架：`BepInEx/` 整棵树 + 游戏根目录的加载器文件。
///
/// 根目录那三个文件是关键：`winhttp.dll` 是实际的加载开关，`doorstop_config.ini`
/// 与 `run_bepinex.sh` 是 doorstop 的配置。早期备份只拷 `BepInEx/`，恢复后
/// 框架不完整 —— 判定「已安装」看的是 `BepInEx/core`，但 `winhttp.dll` 缺失
/// 会让框架显示为未启用，用户以为恢复失败。
pub(crate) const BEPINEX_BACKUP: FrameworkBackupSpec<'static> = FrameworkBackupSpec {
    prefix: "BepInEx.backup-",
    directories: Cow::Borrowed(&[Cow::Borrowed("BepInEx")]),
    files: &[
        "winhttp.dll",
        "winhttp.dll.disabled",
        "doorstop_config.ini",
        "run_bepinex.sh",
        "libdoorstop.so",
        "libdoorstop.dylib",
        "doorstop_libs/libdoorstop_x64.so",
        "doorstop_libs/libdoorstop_x64.dylib",
    ],
};

/// DIYLevel 旧备份的种类标识及兼容布局；当前备份使用动态目录 spec。
pub(crate) const DIYLEVEL_BACKUP: FrameworkBackupSpec<'static> = FrameworkBackupSpec {
    prefix: DIYLEVEL_BACKUP_PREFIX,
    directories: Cow::Borrowed(&[Cow::Borrowed(DIYLEVEL_DIR)]),
    files: &[],
};

/// 备份一个框架：目录树 + 根文件 + 清单快照。
///
/// 返回备份目录路径。清单快照写在备份根部的 `framework-manifest.toml`，
/// 恢复时由 `restore_framework_backup` 写回管理器清单目录。
pub(crate) fn backup_framework(
    game_root: &Path,
    backup_root: &Path,
    spec: &FrameworkBackupSpec<'_>,
    manifests_dir: &Path,
    manifest_id: &str,
) -> Result<PathBuf, String> {
    for directory in spec.directories.iter() {
        let source = game::safe_game_path(game_root, directory)?;
        ensure_copy_paths_disjoint(&source, backup_root)?;
    }
    fs::create_dir_all(backup_root).map_err(|e| e.to_string())?;
    let target = backup_root.join(format!(
        "{}{}-{}",
        spec.prefix,
        Utc::now().format("%Y%m%d%H%M%S"),
        Uuid::new_v4().simple()
    ));
    fs::create_dir_all(&target).map_err(|e| e.to_string())?;

    // 目录树：按原相对路径放回备份里，恢复时原样拷回游戏目录
    for directory in spec.directories.iter() {
        copy_dir(
            &game::safe_game_path(game_root, directory)?,
            &target.join(directory.as_ref()),
        )?;
    }
    // 根目录的散文件：存在才拷，缺失不算错误（比如 macOS 没有 winhttp.dll）
    for relative in spec.files {
        let source = game_root.join(relative);
        if !source.is_file() {
            continue;
        }
        let destination = target.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::copy(&source, &destination).map_err(|e| e.to_string())?;
    }

    /*
     * 清单快照。
     *
     * 有现成清单就原样存一份（保留原始安装信息，恢复后卸载行为与安装时
     * 完全一致）。没有清单（手工装的框架）就按当前实际文件现造一份 ——
     * 这样恢复后依然能正常卸载，不会落回「拒绝删除未知文件」。
     */
    let mut manifest = archive::read_manifest(manifests_dir, manifest_id).unwrap_or_else(|| {
        let files = framework_files_on_disk(game_root, spec);
        archive::new_manifest(
            manifest_id,
            crate::bepinex::PACKAGE_VERSION,
            archive::single_language("DIYLevel"),
            Default::default(),
            files,
            Vec::new(),
        )
    });
    diy_level::scope_manifest(spec, &mut manifest)?;
    write_backup_manifest(&target, &manifest)?;
    diy_level::write_layout(&target, spec)?;
    log::info!(
        "框架备份完成：前缀={}，路径={}，清单ID={}，清单文件数={}",
        spec.prefix,
        target.display(),
        manifest_id,
        manifest.files.len()
    );
    Ok(target)
}

/// 把清单写进备份目录根部（与 `archive::write_manifest` 同样的原子写法）。
fn write_backup_manifest(backup: &Path, manifest: &archive::InstallManifest) -> Result<(), String> {
    let text = toml::to_string_pretty(manifest).map_err(|e| e.to_string())?;
    let path = backup.join(FRAMEWORK_BACKUP_MANIFEST);
    let temp = backup.join(format!("{FRAMEWORK_BACKUP_MANIFEST}.tmp"));
    fs::write(&temp, text).map_err(|e| e.to_string())?;
    fs::rename(&temp, &path).map_err(|e| e.to_string())?;
    Ok(())
}

/// 扫描一个框架在磁盘上实际存在的全部文件（游戏相对路径）。
///
/// 只在「没有现成安装清单」时用来现造清单：手工安装的框架没有清单，
/// 备份/恢复后必须能再次卸载，否则会落回「拒绝删除未知文件」。
fn framework_files_on_disk(game_root: &Path, spec: &FrameworkBackupSpec<'_>) -> Vec<String> {
    let mut files = Vec::new();
    for directory in spec.directories.iter() {
        for entry in walkdir::WalkDir::new(game_root.join(directory.as_ref()))
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            if let Ok(relative) = entry.path().strip_prefix(game_root) {
                files.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    for relative in spec.files {
        if game_root.join(relative).is_file() {
            files.push((*relative).to_string());
        }
    }
    files
}

/// 读取备份里的清单快照；旧备份没有这个文件时返回 `None`。
pub(crate) fn read_backup_manifest(backup: &Path) -> Option<archive::InstallManifest> {
    let text = fs::read_to_string(backup.join(FRAMEWORK_BACKUP_MANIFEST)).ok()?;
    toml::from_str(&text).ok()
}

/// 恢复一个框架备份：先删现有内容，再整体还原，最后写回清单。
///
/// 与用户要求一致：**先删再恢复**，得到的是备份那一刻的完整快照。
///
/// ## 为什么 `*.before-restore-*` 必须删掉
///
/// 删除前会把现有内容另存一份，失败时用来回滚。但恢复成功后这个目录必须
/// 立刻删除：它就在游戏根目录下（`BepInEx.before-restore-<ts>`、
/// `OC2DIYLevel.before-restore-<ts>`），MOD 扫描会把里面的 DLL 当成插件，
/// 于是用户看到「刚恢复完就冒出一堆重复/低版本 MOD」，甚至出现同 GUID 冲突。
///
/// 也就是说它不是「给用户后悔用的备份」，只是**本次操作的回滚素材**：
/// 真正的后悔入口是备份列表里的快照。所以成功即删，失败才用。
pub(crate) fn restore_framework_backup(
    game_root: &Path,
    backup: &Path,
    spec: &FrameworkBackupSpec<'_>,
    manifests_dir: &Path,
    manifest_id: &str,
) -> Result<(), String> {
    if !backup.is_dir() {
        return Err("backup.notFound".into());
    }
    diy_level::verify_restore_ownership(spec, manifests_dir, manifest_id)?;
    // 恢复前的另存目录：放在备份根的同级，避开被还原的目录树
    let stamp = Utc::now().timestamp();
    let mut staged = Vec::new();
    // Validate every destination and backup source before removing anything.
    let directories = spec
        .directories
        .iter()
        .map(|directory| {
            let destination = game::safe_game_path(game_root, directory)?;
            let source = game::safe_game_path(backup, directory)?;
            if !source.is_dir() {
                return Err("backup.notFound".to_string());
            }
            ensure_copy_paths_disjoint(&source, &destination)?;
            Ok((directory.as_ref(), source, destination))
        })
        .collect::<Result<Vec<_>, String>>()?;
    for (directory, _, destination) in &directories {
        if !destination.exists() {
            continue;
        }
        let previous = game::safe_game_path(
            game_root,
            &format!(
                "{directory}.before-restore-{stamp}-{}",
                Uuid::new_v4().simple()
            ),
        )?;
        archive::clear_readonly(destination)?;
        copy_dir(destination, &previous)?;
        staged.push(previous.clone());
        fs::remove_dir_all(destination).map_err(|e| e.to_string())?;
        log::info!("恢复前已另存：路径={}", previous.display());
    }
    for relative in spec.files {
        let destination = game_root.join(relative);
        if destination.exists() {
            let _ = fs::remove_file(&destination);
        }
    }

    // 整体还原
    for (_, source, destination) in &directories {
        copy_dir(source, destination)?;
    }
    for relative in spec.files {
        let source = backup.join(relative);
        if !source.is_file() {
            continue;
        }
        let destination = game_root.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::copy(&source, &destination).map_err(|e| e.to_string())?;
    }

    /*
     * 还原完成，回滚素材用完即弃。
     *
     * 删不掉不算恢复失败（目录已经正确还原了），但必须留日志并如实告知：
     * 残留目录会被 MOD 扫描当成插件，用户需要自己删。
     */
    let mut leftovers = Vec::new();
    for previous in staged {
        archive::clear_readonly(&previous)?;
        match fs::remove_dir_all(&previous) {
            Ok(()) => log::info!("已清理恢复前暂存：路径={}", previous.display()),
            Err(error) => {
                log::warn!(
                    "恢复前暂存清理失败：路径={}，原因={}",
                    previous.display(),
                    error
                );
                leftovers.push(previous.display().to_string());
            }
        }
    }
    if !leftovers.is_empty() {
        return Err(crate::locale_tag::with_params(
            "backup.restoreLeftoverCleanupFailed",
            &[("paths", &leftovers.join(", "))],
        ));
    }

    /*
     * 写回安装清单。
     *
     * 这是「恢复后还能正常卸载」的关键：卸载依赖清单来判断哪些文件属于
     * 框架。旧备份没有清单快照时，退回按当前实际文件现造一份 —— 至少保证
     * 不会再报「没有安装清单，拒绝删除未知文件」。
     */
    let mut manifest = read_backup_manifest(backup).unwrap_or_else(|| {
        let files = framework_files_on_disk(game_root, spec);
        archive::new_manifest(
            manifest_id,
            crate::bepinex::PACKAGE_VERSION,
            archive::single_language("DIYLevel"),
            Default::default(),
            files,
            Vec::new(),
        )
    });
    diy_level::scope_manifest(spec, &mut manifest)?;
    archive::write_manifest(manifests_dir, &manifest)?;
    log::info!(
        "框架备份恢复完成：前缀={}，路径={}，清单文件数={}",
        spec.prefix,
        game_root.display(),
        manifest.files.len()
    );
    Ok(())
}

/// 校验一个路径确实是本管理器创建的某个框架备份目录。
pub(crate) fn validate_framework_backup_in(
    backup_path: &Path,
    backup_root: &Path,
    spec: &FrameworkBackupSpec<'_>,
) -> Result<PathBuf, String> {
    let target = fs::canonicalize(backup_path).map_err(|_| "backup.notFound".to_string())?;
    let root = fs::canonicalize(backup_root).map_err(|_| "backup.unavailable".to_string())?;
    let name = target
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if !name.starts_with(spec.prefix) || !target.is_dir() {
        return Err("backup.wrongKind".into());
    }
    if target.parent() != Some(root.as_path()) {
        return Err("backup.outsideRoot".into());
    }
    Ok(target)
}

pub(crate) fn backup_bepinex_dir(game_root: &Path, backup_root: &Path) -> Result<PathBuf, String> {
    /*
     * 只检查「框架目录」与「备份目录」互不包含，这才是递归复制的来源。
     *
     * 这里曾经还有一段「管理器安装目录不能与备份目录重叠」的自检：当年数据根
     * 放在 `install_dir()/OC2ModManager`，备份目录必然是管理器安装目录的子目录，
     * 这个判断恒真，结果所有备份与卸载都被它拦死，用户只看到「请把管理器安装
     * 到游戏目录外」这种无从处理的提示 —— 而管理器本来就不在游戏目录里。
     *
     * 现在数据根在用户文档目录，除非把管理器装进 BepInEx（安装校验已经拒绝），
     * 备份源与备份目标天然不相交。保留这条检查纯属防御：将来的调用方如果把
     * 备份目标指到游戏目录内，这里仍然会拦住递归复制。
     */
    ensure_copy_paths_disjoint(&game_root.join("BepInEx"), backup_root)?;
    let target = bepinex_backup_path(backup_root)?;
    copy_dir(&game_root.join("BepInEx"), &target)?;
    Ok(target)
}

fn bepinex_backup_path(backup_root: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(backup_root).map_err(|e| e.to_string())?;
    Ok(backup_root.join(format!(
        "BepInEx.backup-{}-{}",
        Utc::now().format("%Y%m%d%H%M%S"),
        Uuid::new_v4().simple()
    )))
}

pub(crate) fn restore_preserved_dirs(backup: &Path, game_root: &Path) -> Result<(), String> {
    for name in ["plugins", "config"] {
        let source = backup.join(name);
        let destination = game_root.join("BepInEx").join(name);
        if source.exists() {
            copy_dir(&source, &destination)?;
        }
    }
    Ok(())
}

/// 把整个 `BepInEx/` 换成备份内容（安装失败时的回滚）。
///
/// 与 `restore_framework_backup` 一样，回滚素材用完即删：`BepInEx.before-restore-*`
/// 留在游戏根目录下会被 MOD 扫描当成插件，表现为「刚回滚完就多出一批重复 MOD」。
pub(crate) fn restore_backup_dir(backup: &Path, game_root: &Path) -> Result<(), String> {
    if !backup.is_dir() {
        return Err("backup.notFound".into());
    }
    let destination = game_root.join("BepInEx");
    ensure_copy_paths_disjoint(backup, &destination)?;
    let mut previous = None;
    if destination.exists() {
        let staged = game_root.join(format!("BepInEx.before-restore-{}", Utc::now().timestamp()));
        archive::clear_readonly(&destination)?;
        copy_dir(&destination, &staged)?;
        previous = Some(staged);
        fs::remove_dir_all(&destination).map_err(|e| e.to_string())?;
    }
    copy_dir(backup, &destination)?;

    // 删除失败不影响回滚结果，但必须留痕：残留目录会污染 MOD 扫描
    if let Some(staged) = previous {
        archive::clear_readonly(&staged)?;
        match fs::remove_dir_all(&staged) {
            Ok(()) => log::info!("已清理回滚前暂存：路径={}", staged.display()),
            Err(error) => log::warn!(
                "回滚前暂存清理失败：路径={}，原因={}",
                staged.display(),
                error
            ),
        }
    }
    Ok(())
}

/// 备份与源目录必须互不包含。
///
/// 两个目录互相包含时，复制会把自己正在写入的内容再复制进去，
/// 表现为无限递归、磁盘被瞬间填满。数据目录改为跟随管理器安装目录后，
/// 这个风险变大：只要管理器装在游戏目录内，备份目录就可能落在
/// `BepInEx` 里面（或反之）。
///
/// 检查用「最近存在的祖先」解析，因此目标目录尚未创建时也能识别出
/// 目录联接（junction）造成的逻辑包含关系。
pub(crate) fn ensure_copy_paths_disjoint(source: &Path, destination: &Path) -> Result<(), String> {
    let source =
        game::resolve_existing_ancestor(&std::path::absolute(source).map_err(|e| e.to_string())?)?;
    let destination = game::resolve_existing_ancestor(
        &std::path::absolute(destination).map_err(|e| e.to_string())?,
    )?;
    #[cfg(windows)]
    let (source, destination) = (
        PathBuf::from(source.to_string_lossy().to_lowercase()),
        PathBuf::from(destination.to_string_lossy().to_lowercase()),
    );
    if source == destination {
        return Err("backup.sameDirectory".into());
    }
    if source.starts_with(&destination) || destination.starts_with(&source) {
        return Err("backup.overlap".into());
    }
    Ok(())
}

fn copy_dir(source: &Path, destination: &Path) -> Result<(), String> {
    if !source.is_dir() {
        return Err(crate::locale_tag::with_params(
            "mods.directoryMissing",
            &[("path", &source.display().to_string())],
        ));
    }
    ensure_copy_paths_disjoint(source, destination)?;
    fs::create_dir_all(destination).map_err(|e| e.to_string())?;
    for entry in walkdir::WalkDir::new(source).follow_links(false) {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry.file_type().is_symlink() {
            return Err("archive.pathEscape".into());
        }
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|e| e.to_string())?;
        let target = destination.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(target).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::copy(entry.path(), target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

mod diy_level;
pub(crate) use diy_level::{diy_level_backup_spec, diy_level_restore_spec};

#[cfg(test)]
mod test_support;
#[cfg(test)]
use test_support::validate_diy_level_backup_in;

#[cfg(test)]
mod backup_safety_tests;

#[cfg(test)]
mod diy_level_backup_tests;

#[cfg(test)]
mod framework_backup_roundtrip_tests;
