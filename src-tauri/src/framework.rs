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

struct FrameworkStep<'a> {
    stage: &'static str,
    source: &'a Path,
    target: &'a Path,
    staged: Option<&'a Path>,
}

impl<'a> FrameworkStep<'a> {
    fn new(stage: &'static str, source: &'a Path, target: &'a Path) -> Self {
        Self {
            stage,
            source,
            target,
            staged: None,
        }
    }

    fn with_staged(mut self, staged: Option<&'a Path>) -> Self {
        self.staged = staged;
        self
    }

    fn log(&self, reason: &str, path: Option<&Path>, error: Option<&std::io::Error>) {
        log::warn!(
            "框架备份或恢复失败：stage={} reason={reason} source={:?} target={:?} staged={:?} operation_path={:?} io_kind={:?} os_code={:?}",
            self.stage,
            crate::file_check::log_path(self.source),
            crate::file_check::log_path(self.target),
            self.staged.map(crate::file_check::log_path),
            path.map(crate::file_check::log_path),
            error.map(std::io::Error::kind),
            error.and_then(std::io::Error::raw_os_error),
        );
    }

    fn io<T>(&self, result: std::io::Result<T>, path: &Path) -> Result<T, String> {
        result.map_err(|error| {
            self.log("io", Some(path), Some(&error));
            error.to_string()
        })
    }

    fn checked<T>(&self, result: Result<T, String>, reason: &'static str) -> Result<T, String> {
        result.inspect_err(|error| {
            let code = crate::locale_tag::diagnostic_tag(error);
            log::warn!(
                "框架校验未通过：stage={} reason={reason} source={:?} target={:?} staged={:?} tag={code}",
                self.stage,
                crate::file_check::log_path(self.source),
                crate::file_check::log_path(self.target),
                self.staged.map(crate::file_check::log_path),
            );
        })
    }

    fn source_is_dir(&self) -> bool {
        match fs::metadata(self.source) {
            Ok(metadata) if metadata.is_dir() => true,
            Ok(_) => {
                self.log("source_not_directory", Some(self.source), None);
                false
            }
            Err(error) => {
                self.log("source_unreadable", Some(self.source), Some(&error));
                false
            }
        }
    }

    fn copy_file(&self) -> Result<(), String> {
        if let Some(parent) = self.target.parent() {
            self.io(fs::create_dir_all(parent), parent)?;
        }
        fs::copy(self.source, self.target)
            .map(|_| ())
            .map_err(|error| {
                self.log("copy_failed", None, Some(&error));
                error.to_string()
            })
    }
}

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
        let attempted = game_root.join(directory.as_ref());
        let source = FrameworkStep::new("backup/resolve_source", &attempted, backup_root).checked(
            game::safe_game_path(game_root, directory),
            "source_resolution_failed",
        )?;
        ensure_copy_paths_disjoint(&source, backup_root)?;
    }
    FrameworkStep::new("backup/create_root", game_root, backup_root)
        .io(fs::create_dir_all(backup_root), backup_root)?;
    let target = backup_root.join(format!(
        "{}{}-{}",
        spec.prefix,
        Utc::now().format("%Y%m%d%H%M%S"),
        Uuid::new_v4().simple()
    ));
    FrameworkStep::new("backup/create_snapshot", game_root, &target)
        .io(fs::create_dir_all(&target), &target)?;

    // 目录树：按原相对路径放回备份里，恢复时原样拷回游戏目录
    for directory in spec.directories.iter() {
        let destination = target.join(directory.as_ref());
        let attempted = game_root.join(directory.as_ref());
        let source = FrameworkStep::new("backup/resolve_source", &attempted, &destination)
            .checked(
                game::safe_game_path(game_root, directory),
                "source_resolution_failed",
            )?;
        copy_dir(&FrameworkStep::new(
            "backup/copy_tree",
            &source,
            &destination,
        ))?;
    }
    // 根目录的散文件：存在才拷，缺失不算错误（比如 macOS 没有 winhttp.dll）
    for relative in spec.files {
        let source = game_root.join(relative);
        if !source.is_file() {
            continue;
        }
        let destination = target.join(relative);
        FrameworkStep::new("backup/copy_file", &source, &destination).copy_file()?;
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
    FrameworkStep::new("backup/scope_manifest", manifests_dir, &target).checked(
        diy_level::scope_manifest(spec, &mut manifest),
        "invalid_manifest_scope",
    )?;
    write_backup_manifest(manifests_dir, &target, &manifest)?;
    FrameworkStep::new("backup/write_layout", game_root, &target).checked(
        diy_level::write_layout(&target, spec),
        "layout_write_failed",
    )?;
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
fn write_backup_manifest(
    manifests_dir: &Path,
    backup: &Path,
    manifest: &archive::InstallManifest,
) -> Result<(), String> {
    let path = backup.join(FRAMEWORK_BACKUP_MANIFEST);
    let temp = backup.join(format!("{FRAMEWORK_BACKUP_MANIFEST}.tmp"));
    let step = FrameworkStep::new("backup/write_manifest", manifests_dir, &path);
    let text = step.checked(
        toml::to_string_pretty(manifest).map_err(|error| error.to_string()),
        "manifest_serialization_failed",
    )?;
    step.io(fs::write(&temp, text), &temp)?;
    step.io(fs::rename(&temp, &path), &path)?;
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
    let path = backup.join(FRAMEWORK_BACKUP_MANIFEST);
    let step = FrameworkStep::new("restore/read_snapshot_manifest", &path, backup);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => {
            step.log("manifest_read_failed", Some(&path), Some(&error));
            return None;
        }
    };
    toml::from_str(&text)
        .inspect_err(|_| step.log("invalid_manifest", Some(&path), None))
        .ok()
}

/// 恢复一个框架备份：先删现有内容，再整体还原，最后写回清单。
///
/// 与用户要求一致：**先删再恢复**，得到的是备份那一刻的完整快照。
///
/// ## 为什么 `*.before-restore-*` 必须删掉
///
/// 删除前会把现有内容另存一份。恢复成功后这个目录必须
/// 立刻删除：它就在游戏根目录下（`BepInEx.before-restore-<ts>`、
/// `OC2DIYLevel.before-restore-<ts>`），MOD 扫描会把里面的 DLL 当成插件，
/// 于是用户看到「刚恢复完就冒出一堆重复/低版本 MOD」，甚至出现同 GUID 冲突。
///
/// 它不是备份列表里的快照；恢复中断时可能留下，供后续排查或手动恢复。
pub(crate) fn restore_framework_backup(
    game_root: &Path,
    backup: &Path,
    spec: &FrameworkBackupSpec<'_>,
    manifests_dir: &Path,
    manifest_id: &str,
) -> Result<(), String> {
    if !FrameworkStep::new("restore/check_snapshot", backup, game_root).source_is_dir() {
        return Err("backup.notFound".into());
    }
    FrameworkStep::new("restore/check_ownership", backup, manifests_dir).checked(
        diy_level::verify_restore_ownership(spec, manifests_dir, manifest_id),
        "ownership_not_verified",
    )?;
    // 恢复前的另存目录：放在备份根的同级，避开被还原的目录树
    let stamp = Utc::now().timestamp();
    // Validate every destination and backup source before removing anything.
    let mut directories = spec
        .directories
        .iter()
        .map(|directory| {
            let backup_path = backup.join(directory.as_ref());
            let game_path = game_root.join(directory.as_ref());
            let resolve = FrameworkStep::new("restore/resolve_paths", &backup_path, &game_path);
            let destination = resolve.checked(
                game::safe_game_path(game_root, directory),
                "target_resolution_failed",
            )?;
            let source = resolve.checked(
                game::safe_game_path(backup, directory),
                "source_resolution_failed",
            )?;
            if !FrameworkStep::new("restore/check_snapshot_tree", &source, &destination)
                .source_is_dir()
            {
                return Err("backup.notFound".to_string());
            }
            ensure_copy_paths_disjoint(&source, &destination)?;
            Ok((directory.as_ref(), source, destination, None))
        })
        .collect::<Result<Vec<_>, String>>()?;
    for (directory, source, destination, staged) in &mut directories {
        if !destination.exists() {
            continue;
        }
        let relative = format!(
            "{directory}.before-restore-{stamp}-{}",
            Uuid::new_v4().simple()
        );
        let staged_path = game_root.join(&relative);
        let previous = FrameworkStep::new("restore/resolve_staged_path", source, destination)
            .with_staged(Some(&staged_path))
            .checked(
                game::safe_game_path(game_root, &relative),
                "staged_path_resolution_failed",
            )?;
        stage_existing_tree("restore/stage_existing_tree", destination, &previous)?;
        FrameworkStep::new("restore/remove_existing_tree", source, destination)
            .with_staged(Some(&previous))
            .io(fs::remove_dir_all(&*destination), destination)?;
        log::info!("恢复前已另存：路径={}", previous.display());
        *staged = Some(previous);
    }
    for relative in spec.files {
        let destination = game_root.join(relative);
        if destination.exists() {
            if let Err(error) = fs::remove_file(&destination) {
                FrameworkStep::new(
                    "restore/remove_existing_file",
                    &backup.join(relative),
                    &destination,
                )
                .log(
                    "continuing_after_remove_failed",
                    Some(&destination),
                    Some(&error),
                );
            }
        }
    }

    // 整体还原
    for (_, source, destination, staged) in &directories {
        copy_dir(
            &FrameworkStep::new("restore/copy_tree", source, destination)
                .with_staged(staged.as_deref()),
        )?;
    }
    for relative in spec.files {
        let source = backup.join(relative);
        if !source.is_file() {
            continue;
        }
        let destination = game_root.join(relative);
        FrameworkStep::new("restore/copy_file", &source, &destination).copy_file()?;
    }

    /*
     * 文件还原后清理另存目录。
     *
     * 删不掉不算恢复失败（目录已经正确还原了），但必须留日志并如实告知：
     * 残留目录会被 MOD 扫描当成插件，用户需要自己删。
     */
    let mut leftovers = Vec::new();
    for (_, source, destination, staged) in directories {
        let Some(previous) = staged else {
            continue;
        };
        let step = FrameworkStep::new("restore/cleanup_staged_tree", &source, &destination)
            .with_staged(Some(&previous));
        step.checked(
            archive::clear_readonly(&previous),
            "staged_clear_readonly_failed",
        )?;
        match fs::remove_dir_all(&previous) {
            Ok(()) => log::info!("已清理恢复前暂存：路径={}", previous.display()),
            Err(error) => {
                step.log("staged_remove_failed", Some(&previous), Some(&error));
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
    FrameworkStep::new("restore/scope_manifest", backup, manifests_dir).checked(
        diy_level::scope_manifest(spec, &mut manifest),
        "invalid_manifest_scope",
    )?;
    FrameworkStep::new("restore/write_manifest", backup, manifests_dir).checked(
        archive::write_manifest(manifests_dir, &manifest),
        "manifest_write_failed",
    )?;
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
    let step = FrameworkStep::new("validate/backup_location", backup_path, backup_root);
    let target = fs::canonicalize(backup_path).map_err(|error| {
        step.log("backup_resolution_failed", Some(backup_path), Some(&error));
        "backup.notFound".to_string()
    })?;
    let root = fs::canonicalize(backup_root).map_err(|error| {
        step.log(
            "backup_root_resolution_failed",
            Some(backup_root),
            Some(&error),
        );
        "backup.unavailable".to_string()
    })?;
    let name = target
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if !name.starts_with(spec.prefix) || !target.is_dir() {
        step.log("wrong_kind", Some(&target), None);
        return Err("backup.wrongKind".into());
    }
    if target.parent() != Some(root.as_path()) {
        step.log("outside_backup_root", Some(&target), None);
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
    let source = game_root.join("BepInEx");
    ensure_copy_paths_disjoint(&source, backup_root)?;
    FrameworkStep::new("backup_directory/create_root", &source, backup_root)
        .io(fs::create_dir_all(backup_root), backup_root)?;
    let target = backup_root.join(format!(
        "BepInEx.backup-{}-{}",
        Utc::now().format("%Y%m%d%H%M%S"),
        Uuid::new_v4().simple()
    ));
    copy_dir(&FrameworkStep::new(
        "backup_directory/copy_tree",
        &source,
        &target,
    ))?;
    Ok(target)
}

pub(crate) fn restore_preserved_dirs(backup: &Path, game_root: &Path) -> Result<(), String> {
    for name in ["plugins", "config"] {
        let source = backup.join(name);
        let destination = game_root.join("BepInEx").join(name);
        if source.exists() {
            copy_dir(&FrameworkStep::new(
                "restore_preserved/copy_tree",
                &source,
                &destination,
            ))?;
        }
    }
    Ok(())
}

/// 把整个 `BepInEx/` 换成备份内容（安装失败时的回滚）。
///
/// 与 `restore_framework_backup` 一样，回滚素材用完即删：`BepInEx.before-restore-*`
/// 留在游戏根目录下会被 MOD 扫描当成插件，表现为「刚回滚完就多出一批重复 MOD」。
pub(crate) fn restore_backup_dir(backup: &Path, game_root: &Path) -> Result<(), String> {
    let destination = game_root.join("BepInEx");
    if !FrameworkStep::new("restore_directory/check_snapshot", backup, &destination).source_is_dir()
    {
        return Err("backup.notFound".into());
    }
    ensure_copy_paths_disjoint(backup, &destination)?;
    let mut previous = None;
    if destination.exists() {
        let staged = game_root.join(format!("BepInEx.before-restore-{}", Utc::now().timestamp()));
        stage_existing_tree(
            "restore_directory/stage_existing_tree",
            &destination,
            &staged,
        )?;
        FrameworkStep::new(
            "restore_directory/remove_existing_tree",
            backup,
            &destination,
        )
        .with_staged(Some(&staged))
        .io(fs::remove_dir_all(&destination), &destination)?;
        previous = Some(staged);
    }
    copy_dir(
        &FrameworkStep::new("restore_directory/copy_tree", backup, &destination)
            .with_staged(previous.as_deref()),
    )?;

    // 删除失败不影响回滚结果，但必须留痕：残留目录会污染 MOD 扫描
    if let Some(staged) = previous {
        let step = FrameworkStep::new(
            "restore_directory/cleanup_staged_tree",
            backup,
            &destination,
        )
        .with_staged(Some(&staged));
        step.checked(
            archive::clear_readonly(&staged),
            "staged_clear_readonly_failed",
        )?;
        match fs::remove_dir_all(&staged) {
            Ok(()) => log::info!("已清理回滚前暂存：路径={}", staged.display()),
            Err(error) => step.log("staged_remove_failed", Some(&staged), Some(&error)),
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
    let step = FrameworkStep::new("validate/disjoint_paths", source, destination);
    let source = step.checked(
        game::resolve_existing_ancestor(&step.io(std::path::absolute(source), source)?),
        "source_resolution_failed",
    )?;
    let destination = step.checked(
        game::resolve_existing_ancestor(&step.io(std::path::absolute(destination), destination)?),
        "target_resolution_failed",
    )?;
    #[cfg(windows)]
    let (source, destination) = (
        PathBuf::from(source.to_string_lossy().to_lowercase()),
        PathBuf::from(destination.to_string_lossy().to_lowercase()),
    );
    if source == destination {
        step.log("same_directory", Some(&destination), None);
        return Err("backup.sameDirectory".into());
    }
    if source.starts_with(&destination) || destination.starts_with(&source) {
        step.log("overlapping_directories", Some(&destination), None);
        return Err("backup.overlap".into());
    }
    Ok(())
}

fn stage_existing_tree(stage: &'static str, existing: &Path, staged: &Path) -> Result<(), String> {
    let step = FrameworkStep::new(stage, existing, staged).with_staged(Some(staged));
    step.checked(
        archive::clear_readonly(existing),
        "source_clear_readonly_failed",
    )?;
    copy_dir(&step)
}

fn copy_dir(step: &FrameworkStep<'_>) -> Result<(), String> {
    let (source, destination) = (step.source, step.target);
    if !step.source_is_dir() {
        return Err(crate::locale_tag::with_params(
            "mods.directoryMissing",
            &[("path", &source.display().to_string())],
        ));
    }
    step.checked(
        ensure_copy_paths_disjoint(source, destination),
        "copy_paths_not_verified",
    )?;
    step.io(fs::create_dir_all(destination), destination)?;
    for entry in walkdir::WalkDir::new(source).follow_links(false) {
        let entry = entry.map_err(|error| {
            step.log(
                "walk_source_failed",
                Some(error.path().unwrap_or(source)),
                error.io_error(),
            );
            error.to_string()
        })?;
        if entry.file_type().is_symlink() {
            step.log("source_link", Some(entry.path()), None);
            return Err("archive.pathEscape".into());
        }
        let relative = entry.path().strip_prefix(source).map_err(|error| {
            step.log("outside_source", Some(entry.path()), None);
            error.to_string()
        })?;
        let target = destination.join(relative);
        if entry.file_type().is_dir() {
            step.io(fs::create_dir_all(&target), &target)?;
        } else {
            FrameworkStep {
                source: entry.path(),
                target: &target,
                ..*step
            }
            .copy_file()?;
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
