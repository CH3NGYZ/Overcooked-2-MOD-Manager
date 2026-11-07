use crate::app_state::AppState;
use crate::archive;
use crate::framework::DIYLEVEL_BACKUP;
use crate::framework::backup_framework;
use crate::framework::list_diy_level_backups;
use crate::framework::read_backup_manifest;
use crate::framework::restore_framework_backup;
use crate::framework::validate_framework_backup_in;
use crate::game;
use crate::models::{DiyLevelInfo, OperationResult};
use crate::mods;
use crate::mods::file_state::set_local_mod_file_state;
use crate::recycle_bin;
use crate::shell_open;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use tauri::State;

/// DIYLevel 自定义关卡框架的稳定 GUID。
///
/// 与仓库源里的 `plugin_guid`、安装清单、MOD 扫描用的是同一个标识 ——
/// 作者会改文件名和产品名，只有 GUID 稳定，所以状态判定只认它。
const DIYLEVEL_GUID: &str = "dev.gua.overcooked.diylevel";

/// 读取 DIYLevel 框架状态。
///
/// 复用 MOD 扫描结果，不单独扫盘：扫描已经处理了 `.dll.disabled` 命名、
/// 元数据解析和清单绑定，再写一套判定只会和 MOD 列表给出的结论不一致。
#[tauri::command]
pub fn get_diy_level_info(state: State<'_, AppState>) -> Result<DiyLevelInfo, String> {
    let config = state.config.load_or_default();
    let backups = list_diy_level_backups(&state.config.backups_dir());
    let Some(game_root) = game::detect_game(&config).path else {
        return Ok(DiyLevelInfo {
            installed: false,
            enabled: false,
            version: None,
            dll_file: None,
            has_levels_dir: false,
            level_packs: 0,
            backups,
        });
    };
    let root = PathBuf::from(&game_root);
    // 与 MOD 列表同源：同一份扫描、同一份清单，结论不会互相矛盾
    let installed =
        mods::scan_mods_with_manifests(&root, &config, Some(&state.config.manifests_dir()));
    let target = installed.iter().find(|item| {
        item.guid
            .as_deref()
            .is_some_and(|guid| guid.trim().eq_ignore_ascii_case(DIYLEVEL_GUID))
    });
    /*
     * 地图目录：`BepInEx/plugins/OC2DIYLevel/levels`。
     *
     * 目录名来自包里固定的相对结构（`OC2DIYLevel/levels/...`），不是用户
     * 可配的；这里只做「存在与否」的只读判断，不创建 —— 没装框架时凭空
     * 建一个空目录，会让人以为已经准备好放地图了。
     */
    let levels_dir = root.join("BepInEx/plugins/OC2DIYLevel/levels");
    Ok(DiyLevelInfo {
        installed: target.is_some(),
        enabled: target.is_some_and(|item| item.enabled),
        version: target.and_then(|item| item.version.clone()),
        dll_file: target.map(|item| item.dll_file.clone()),
        has_levels_dir: levels_dir.is_dir(),
        level_packs: count_level_packs(&levels_dir),
        backups,
    })
}

/// 地图集数量 = `levels/` 下的文件夹个数。
///
/// 只数目录：`levels/` 里直接放的散文件（说明文档、缩略图、配置）不属于
/// 任何地图集。目录不存在或不可读都返回 0 —— 这是只读的展示信息，
/// 不该因为权限问题让整个面板报错。
fn count_level_packs(levels_dir: &Path) -> u32 {
    let Ok(entries) = std::fs::read_dir(levels_dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .count() as u32
}

/// 切换 DIYLevel 框架的启用状态。
///
/// 走与 MOD 列表相同的 `.dll` / `.dll.disabled` 改名逻辑（`set_local_mod_file_state`），
/// 不新增一套开关实现 —— 否则两处对「禁用」的定义迟早分叉。
#[tauri::command]
pub fn set_diy_level_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<OperationResult, String> {
    log::info!("切换 DIYLevel 框架状态：启用={enabled}");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;
    let installed =
        mods::scan_mods_with_manifests(&game_root, &config, Some(&state.config.manifests_dir()));
    let Some(target) = installed.iter().find(|item| {
        item.guid
            .as_deref()
            .is_some_and(|guid| guid.trim().eq_ignore_ascii_case(DIYLEVEL_GUID))
    }) else {
        return Err("framework.diyLevelMissing".into());
    };
    if target.enabled == enabled {
        return Ok(OperationResult {
            ok: true,
            message: if enabled {
                "mods.stateEnabled"
            } else {
                "mods.stateDisabled"
            }
            .into(),
        });
    }
    set_local_mod_file_state(&game_root, &target.dll_file, enabled)?;
    Ok(OperationResult {
        ok: true,
        message: if enabled {
            "message.framework.diyLevelEnabled".into()
        } else {
            "message.framework.diyLevelDisabled".into()
        },
    })
}

/// 卸载 DIYLevel 框架（先自动备份，再按安装清单回收）。
///
/// 与 `uninstall_bepinex` 同一套顺序：**先整目录备份，再回收文件**。
/// 回收站只带得走清单里登记的文件，用户自己放进 `OC2DIYLevel` 的地图集、
/// 改过的配置不在清单里 —— 只有目录备份能保证还能找回。
#[tauri::command]
pub fn uninstall_diy_level(state: State<'_, AppState>) -> Result<OperationResult, String> {
    log::info!("开始卸载 DIYLevel");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;

    let installed =
        mods::scan_mods_with_manifests(&game_root, &config, Some(&state.config.manifests_dir()));
    let Some(target) = installed.iter().find(|item| {
        item.guid
            .as_deref()
            .is_some_and(|guid| guid.trim().eq_ignore_ascii_case(DIYLEVEL_GUID))
    }) else {
        return Err("framework.diyLevelMissing".into());
    };
    let dll_file = target.dll_file.clone();
    let package_id = target.package_id.clone().unwrap_or_else(|| "manual".into());

    // 先备份整个 OC2DIYLevel 目录（含清单快照），再动文件
    let backup = backup_framework(
        &game_root,
        &state.config.backups_dir(),
        &DIYLEVEL_BACKUP,
        &state.config.manifests_dir(),
        &package_id,
    )?;
    log::info!("DIYLevel 卸载前备份完成：路径={}", backup.display());

    let manifests_dir = state.config.manifests_dir();
    let recycle_root = state.config.mod_recycle_bin_dir();
    let manifest_id = Some(package_id.clone());

    // 有安装清单就整包回收；没有（手工装的）就只回收这个 DLL
    let has_manifest = manifest_id
        .as_deref()
        .is_some_and(|id| archive::read_manifest(&manifests_dir, id).is_some());
    let result = if let Some(id) = manifest_id.as_deref().filter(|_| has_manifest) {
        recycle_bin::recycle_package(&recycle_root, &manifests_dir, &game_root, id, None).map(
            |entry| {
                crate::locale_tag::with_params(
                    "message.framework.diyLevelMovedToRecycleBin",
                    &[
                        ("count", &entry.file_count.to_string()),
                        ("path", &backup.display().to_string()),
                    ],
                )
            },
        )
    } else if config.recycle_deleted_mods {
        recycle_bin::recycle_mod_dll(
            &recycle_root,
            &manifests_dir,
            &game_root,
            manifest_id.as_deref().unwrap_or("manual"),
            &dll_file,
            &installed,
        )
        .map(|_| {
            crate::locale_tag::with_params(
                "message.framework.diyLevelMovedToRecycleBinNoCount",
                &[("path", &backup.display().to_string())],
            )
        })
    } else {
        recycle_bin::purge_mod_dll(
            &recycle_root,
            &manifests_dir,
            &game_root,
            manifest_id.as_deref().unwrap_or("manual"),
            &dll_file,
            &installed,
        )
        .map(|_| {
            crate::locale_tag::with_params(
                "message.framework.diyLevelDeleted",
                &[("path", &backup.display().to_string())],
            )
        })
    };

    match result {
        Ok(message) => {
            log::info!("DIYLevel 卸载完成：{message}");
            Ok(OperationResult { ok: true, message })
        }
        Err(error) => {
            log::error!("DIYLevel 回收失败：原因={error}，备份={}", backup.display());
            Err(crate::locale_tag::with_params(
                "backup.diyLevelRecycleFailed",
                &[
                    ("reason", &error.to_string()),
                    ("path", &backup.display().to_string()),
                ],
            ))
        }
    }
}

/// 打开 DIYLevel 的地图目录（放在框架所在目录层）。
///
/// 与 `open_bepinex_directory` 同一套约束：只允许游戏目录内的固定子路径，
/// 且**不自动创建**——目录不存在说明框架没装或版本结构不同，创建空目录
/// 只会误导用户。
#[tauri::command]
pub fn open_diy_level_directory(state: State<'_, AppState>) -> Result<OperationResult, String> {
    let config = state.config.load_or_default();
    let game_root = game::detect_game(&config).path.ok_or("game.notFound")?;
    let root = Path::new(&game_root);
    // 优先地图目录；框架装在别处时退回插件目录，至少能给用户一个能看的落点
    let levels = game::safe_game_path(root, "BepInEx/plugins/OC2DIYLevel/levels")?;
    let package = game::safe_game_path(root, "BepInEx/plugins/OC2DIYLevel")?;
    let target = if levels.is_dir() {
        levels
    } else if package.is_dir() {
        package
    } else {
        return Err("error.framework.diyLevelDirMissing?path=BepInEx/plugins/OC2DIYLevel".into());
    };
    shell_open::open_path(&target)?;
    Ok(OperationResult {
        ok: true,
        message: "message.framework.diyLevelDirOpened".into(),
    })
}

/// 备份 DIYLevel 目录（只含 `BepInEx/plugins/OC2DIYLevel`）+ 它的安装清单快照。
#[tauri::command]
pub fn backup_diy_level(state: State<'_, AppState>) -> Result<OperationResult, String> {
    log::info!("开始备份 DIYLevel");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;
    // 清单 ID 是动态的（仓库 id 或 manual-<uuid>），从当前扫描结果取
    let installed =
        mods::scan_mods_with_manifests(&game_root, &config, Some(&state.config.manifests_dir()));
    let manifest_id = installed
        .iter()
        .find(|item| {
            item.guid
                .as_deref()
                .is_some_and(|guid| guid.trim().eq_ignore_ascii_case(DIYLEVEL_GUID))
        })
        .and_then(|item| item.package_id.clone())
        .ok_or("framework.diyLevelMissing")?;
    let backup = backup_framework(
        &game_root,
        &state.config.backups_dir(),
        &DIYLEVEL_BACKUP,
        &state.config.manifests_dir(),
        &manifest_id,
    )?;
    log::info!("DIYLevel 备份完成：路径={}", backup.display());
    Ok(OperationResult {
        ok: true,
        message: crate::locale_tag::with_params(
            "message.backup.created",
            &[("path", &backup.display().to_string())],
        ),
    })
}

/// 恢复 DIYLevel 备份。
///
/// 按用户要求：**先删掉现有的 DIYLevel 目录，再整体还原备份**。这与覆盖式
/// 恢复不同 —— 备份之后新增的地图集不会被留下来，得到的就是备份那一刻的
/// 完整快照，不会出现「新旧文件混在一起」的半新半旧状态。
///
/// 与 BepInEx 恢复一致，删除前先把当前目录另存一份 `DIYLevel.before-restore-*`，
/// 用户误操作时还有一次后悔的机会。
#[tauri::command]
pub fn restore_diy_level(
    state: State<'_, AppState>,
    backup_path: String,
) -> Result<OperationResult, String> {
    log::info!("开始恢复 DIYLevel 备份：路径={backup_path}");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;

    let backup = validate_framework_backup_in(
        Path::new(&backup_path),
        &state.config.backups_dir(),
        &DIYLEVEL_BACKUP,
    )?;
    /*
     * 清单 ID：优先用备份里快照自带的（那是安装时的真实 ID），
     * 没有快照的旧备份退回扫描当前安装结果，最后兜底 "manual"。
     */
    let manifest_id = read_backup_manifest(&backup)
        .map(|manifest| manifest.repository_id)
        .or_else(|| {
            let installed = mods::scan_mods_with_manifests(
                &game_root,
                &config,
                Some(&state.config.manifests_dir()),
            );
            installed
                .iter()
                .find(|item| {
                    item.guid
                        .as_deref()
                        .is_some_and(|guid| guid.trim().eq_ignore_ascii_case(DIYLEVEL_GUID))
                })
                .and_then(|item| item.package_id.clone())
        })
        .unwrap_or_else(|| "manual".into());
    restore_framework_backup(
        &game_root,
        &backup,
        &DIYLEVEL_BACKUP,
        &state.config.manifests_dir(),
        &manifest_id,
    )?;
    log::info!("DIYLevel 备份恢复完成：清单ID={manifest_id}");
    Ok(OperationResult {
        ok: true,
        message: "message.backup.diyLevelRestored".into(),
    })
}

/// 删除一个 DIYLevel 备份。
#[tauri::command]
pub fn delete_diy_level_backup(
    state: State<'_, AppState>,
    backup_path: String,
) -> Result<OperationResult, String> {
    log::info!("开始删除 DIYLevel 备份：路径={backup_path}");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let target = validate_framework_backup_in(
        Path::new(&backup_path),
        &state.config.backups_dir(),
        &DIYLEVEL_BACKUP,
    )?;
    archive::clear_readonly(&target)?;
    fs::remove_dir_all(&target).map_err(|e| e.to_string())?;
    log::info!("DIYLevel 备份删除完成");
    Ok(OperationResult {
        ok: true,
        message: "message.backup.deleted".into(),
    })
}

#[cfg(test)]
mod diy_level_packs_tests;
