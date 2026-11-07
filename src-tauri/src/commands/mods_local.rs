use crate::app_state::AppState;
use crate::archive;
use crate::game;
use crate::models::{OperationResult, UninstallLocalModResult};
use crate::mods;
use crate::mods::file_state::{set_local_mod_file_state, toggle_local_mod_file};
use crate::mods::install::install_manual_mod;
use crate::recycle_bin;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use tauri::State;

#[tauri::command]
pub fn save_mod_note(
    state: State<'_, AppState>,
    dll_file: String,
    note: String,
) -> Result<OperationResult, String> {
    log::info!("保存 MOD 备注：文件={}", dll_file);
    let config = state.config.load_or_default();
    let game_root = game::detect_game(&config).path.ok_or("game.notFound")?;
    let installed = mods::scan_mods_with_manifests(
        Path::new(&game_root),
        &config,
        Some(&state.config.manifests_dir()),
    );
    let selected = installed
        .iter()
        .find(|item| item.dll_file == dll_file)
        .ok_or("mods.installedNotFound")?;
    let note_key = mods::note_key(selected);
    // Scan outside the configuration transaction, then modify the latest
    // snapshot so a concurrent source/proxy refresh is preserved.
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let mut config = state.config.load_or_default();
    state.config.set_mod_note(&mut config, &note_key, &note);
    state.config.save(&config).map_err(|e| e.to_string())?;
    log::info!(
        "MOD 备注保存完成：文件={}，已清空={}",
        dll_file,
        note.trim().is_empty()
    );
    Ok(OperationResult {
        ok: true,
        message: if note.trim().is_empty() {
            "mods.noteCleared".into()
        } else {
            "mods.noteSaved".into()
        },
    })
}

#[tauri::command]
pub async fn install_local_mod(
    state: State<'_, AppState>,
    source_path: String,
) -> Result<OperationResult, String> {
    // 解压 ZIP 落盘是重活：同步命令在 IPC 线程上跑，界面会整段冻住
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        install_manual_mod(&state, &PathBuf::from(source_path), None)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// 是否需要先让用户确认「连整个 MOD 包一起删」。
///
/// 三个条件同时成立才需要：包内还有其他 MOD、没有同 GUID 的其他启用副本
/// （那种情况走单 DLL 回收分支），并且用户点的**不是**整包删除
/// （`allow_unmanaged = false`）。
///
/// 抽成函数是为了能被单测直接覆盖：命令本身要真实游戏目录和锁，测不到。
fn needs_package_confirmation(
    package_mod_names: &[String],
    has_other_enabled_same_guid: bool,
    allow_unmanaged: bool,
) -> bool {
    !package_mod_names.is_empty() && !has_other_enabled_same_guid && !allow_unmanaged
}

#[tauri::command]
pub async fn uninstall_local_mod(
    state: State<'_, AppState>,
    dll_file: String,
    allow_unmanaged: bool,
) -> Result<UninstallLocalModResult, String> {
    // 判定「要不要整包确认」要先全量扫描已安装 MOD，再加上回收站的移动：重活
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        uninstall_local_mod_blocking(&state, &dll_file, allow_unmanaged)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// [`uninstall_local_mod`] 的同步实现（在阻塞线程池里跑）。
fn uninstall_local_mod_blocking(
    state: &AppState,
    dll_file: &str,
    allow_unmanaged: bool,
) -> Result<UninstallLocalModResult, String> {
    log::info!(
        "开始卸载本地 MOD：文件={}，允许非管理器文件={}",
        dll_file,
        allow_unmanaged
    );
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;
    let manifest_dir = state.config.manifests_dir();
    let manifests = archive::read_manifests(&manifest_dir);
    let relative = Path::new(&dll_file);
    let safe_relative = mods::canonical_dll_name(&relative.to_string_lossy().replace('\\', "/"));
    let manifest = manifests
        .into_iter()
        .filter(|manifest| {
            manifest
                .files
                .iter()
                .any(|file| mods::canonical_dll_name(file).eq_ignore_ascii_case(&safe_relative))
        })
        .max_by(|left, right| left.installed_at.cmp(&right.installed_at));
    if let Some(manifest) = manifest {
        let installed = mods::scan_mods_with_manifests(&game_root, &config, Some(&manifest_dir));
        let target_mod = installed.iter().find(|item| {
            mods::canonical_dll_name(&item.dll_file).eq_ignore_ascii_case(&safe_relative)
        });
        let has_other_enabled_same_guid = target_mod
            .and_then(|target| target.guid.as_deref())
            .map(|guid| {
                installed.iter().any(|item| {
                    item.enabled
                        && !item.dll_file.eq_ignore_ascii_case(&safe_relative)
                        && item.guid.as_deref().is_some_and(|peer_guid| {
                            peer_guid.trim().eq_ignore_ascii_case(guid.trim())
                        })
                })
            })
            .unwrap_or(false);
        let package_mod_names = installed
            .iter()
            .filter(|item| item.package_id.as_deref() == Some(manifest.repository_id.as_str()))
            .filter(|item| !item.dll_file.eq_ignore_ascii_case(&safe_relative))
            .map(|item| item.name.clone())
            .collect::<Vec<_>>();
        /*
         * 包确认只在「用户点的是单个 MOD」时需要。
         *
         * `allow_unmanaged = true` 有两个来源：一是用户已在「整包删除」二级
         * 确认里点过确定；二是冲突卸载/安装计划里本来就按整包语义处理。
         * 两种情况都已经表达过「我要连包一起删」，再返回一次
         * requires_package_confirmation 只会让包标题上的「删除」按钮
         * 又弹一遍「当前 MOD 与其他 MOD 一并安装」——真实 bug。
         */
        if needs_package_confirmation(
            &package_mod_names,
            has_other_enabled_same_guid,
            allow_unmanaged,
        ) {
            log::warn!(
                "卸载本地 MOD 需要确认整个 MOD 包：文件={}，包={}，同包项目数={}",
                safe_relative,
                manifest.repository_id,
                package_mod_names.len()
            );
            return Ok(UninstallLocalModResult {
                ok: false,
                message: "mods.packageConfirmation".into(),
                requires_confirmation: false,
                requires_package_confirmation: true,
                package_id: Some(manifest.repository_id),
                package_mod_names,
            });
        }
        if has_other_enabled_same_guid {
            /*
             * 用户关闭「删除 MOD 时优先放入回收站」后走 purge_* 系列：
             * 同一套定位与共享文件保护逻辑，只是不留回收记录。
             */
            if config.recycle_deleted_mods {
                recycle_bin::recycle_mod_dll(
                    &state.config.mod_recycle_bin_dir(),
                    &manifest_dir,
                    &game_root,
                    &manifest.repository_id,
                    &safe_relative,
                    &installed,
                )?;
            } else {
                recycle_bin::purge_mod_dll(
                    &state.config.mod_recycle_bin_dir(),
                    &manifest_dir,
                    &game_root,
                    &manifest.repository_id,
                    &safe_relative,
                    &installed,
                )?;
            }
            log::info!(
                "已回收 MOD 包中的单个 DLL：文件={}，包={}，进回收站={}",
                safe_relative,
                manifest.repository_id,
                config.recycle_deleted_mods
            );
            return Ok(UninstallLocalModResult {
                ok: true,
                message: if config.recycle_deleted_mods {
                    "message.mods.movedToRecycleBin".into()
                } else {
                    "message.mods.deleted".into()
                },
                requires_confirmation: false,
                requires_package_confirmation: false,
                package_id: Some(manifest.repository_id),
                package_mod_names,
            });
        }
        if config.recycle_deleted_mods {
            recycle_bin::recycle_selected_mod(
                &state.config.mod_recycle_bin_dir(),
                &manifest_dir,
                &game_root,
                &manifest.repository_id,
                &safe_relative,
                true,
            )?;
        } else {
            recycle_bin::purge_selected_mod(
                &state.config.mod_recycle_bin_dir(),
                &manifest_dir,
                &game_root,
                &manifest.repository_id,
                &safe_relative,
                true,
            )?;
        }
        log::info!(
            "已卸载 MOD 包：包={}，文件={}，进回收站={}",
            manifest.repository_id,
            safe_relative,
            config.recycle_deleted_mods
        );
        return Ok(UninstallLocalModResult {
            ok: true,
            message: if config.recycle_deleted_mods {
                "message.mods.packageMovedToRecycleBin".into()
            } else {
                "message.mods.packageDeleted".into()
            },
            requires_confirmation: false,
            requires_package_confirmation: false,
            package_id: None,
            package_mod_names: Vec::new(),
        });
    }
    if !allow_unmanaged {
        log::warn!(
            "发现未由管理器安装的 MOD，等待用户确认：文件={}",
            safe_relative
        );
        return Ok(UninstallLocalModResult {
            ok: false,
            message: "mods.unmanagedConfirmation".into(),
            requires_confirmation: true,
            requires_package_confirmation: false,
            package_id: None,
            package_mod_names: Vec::new(),
        });
    }
    if config.recycle_deleted_mods {
        recycle_bin::recycle_unmanaged(
            &state.config.mod_recycle_bin_dir(),
            &game_root,
            &safe_relative,
        )?;
    } else {
        recycle_bin::purge_unmanaged(
            &state.config.mod_recycle_bin_dir(),
            &game_root,
            &safe_relative,
        )?;
    }
    log::info!(
        "已删除未托管 MOD DLL：文件={}，进回收站={}",
        safe_relative,
        config.recycle_deleted_mods
    );
    Ok(UninstallLocalModResult {
        ok: true,
        message: if config.recycle_deleted_mods {
            "message.mods.movedToRecycleBin".into()
        } else {
            "message.mods.deleted".into()
        },
        requires_confirmation: false,
        requires_package_confirmation: false,
        package_id: None,
        package_mod_names: Vec::new(),
    })
}

#[tauri::command]
pub async fn uninstall_mod_dll(
    state: State<'_, AppState>,
    dll_file: String,
    package_id: String,
) -> Result<OperationResult, String> {
    // 与 `uninstall_local_mod` 同一条规则：扫描 + 回收是重活，别占 IPC 线程
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        uninstall_mod_dll_blocking(&state, &dll_file, &package_id)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// [`uninstall_mod_dll`] 的同步实现（在阻塞线程池里跑）。
fn uninstall_mod_dll_blocking(
    state: &AppState,
    dll_file: &str,
    package_id: &str,
) -> Result<OperationResult, String> {
    log::info!(
        "开始删除 MOD 包内 DLL：包={}，文件={}",
        package_id,
        dll_file
    );
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;
    let manifest_dir = state.config.manifests_dir();
    let manifest = archive::read_manifest(&manifest_dir, &package_id).ok_or("mods.unknownFiles")?;
    let safe_relative =
        mods::canonical_dll_name(&Path::new(&dll_file).to_string_lossy().replace('\\', "/"));
    if !manifest
        .files
        .iter()
        .any(|file| mods::canonical_dll_name(file).eq_ignore_ascii_case(&safe_relative))
    {
        return Err("mods.dllNotInPackage".into());
    }
    let installed = mods::scan_mods_with_manifests(&game_root, &config, Some(&manifest_dir));
    let package_mod_count = installed
        .iter()
        .filter(|item| item.package_id.as_deref() == Some(package_id))
        .count();
    let safe_duplicate = installed
        .iter()
        .find(|item| mods::canonical_dll_name(&item.dll_file).eq_ignore_ascii_case(&safe_relative))
        .is_some_and(|target| {
            installed
                .iter()
                .any(|peer| mods::is_safe_replacement_mod(target, peer))
        });
    if package_mod_count <= 1 && !safe_duplicate {
        if config.recycle_deleted_mods {
            recycle_bin::recycle_selected_mod(
                &state.config.mod_recycle_bin_dir(),
                &manifest_dir,
                &game_root,
                &package_id,
                &safe_relative,
                true,
            )?;
        } else {
            recycle_bin::purge_selected_mod(
                &state.config.mod_recycle_bin_dir(),
                &manifest_dir,
                &game_root,
                &package_id,
                &safe_relative,
                true,
            )?;
        }
        log::info!(
            "已删除 MOD 包最后一个 DLL：包={}，进回收站={}",
            package_id,
            config.recycle_deleted_mods
        );
        return Ok(OperationResult {
            ok: true,
            message: if config.recycle_deleted_mods {
                "message.mods.packageMovedToRecycleBin".into()
            } else {
                "message.mods.packageDeleted".into()
            },
        });
    }
    if config.recycle_deleted_mods {
        recycle_bin::recycle_selected_mod(
            &state.config.mod_recycle_bin_dir(),
            &manifest_dir,
            &game_root,
            &package_id,
            &safe_relative,
            false,
        )?;
    } else {
        recycle_bin::purge_selected_mod(
            &state.config.mod_recycle_bin_dir(),
            &manifest_dir,
            &game_root,
            &package_id,
            &safe_relative,
            false,
        )?;
    }
    log::info!(
        "已删除 MOD 包内 DLL：包={}，进回收站={}",
        package_id,
        config.recycle_deleted_mods
    );
    Ok(OperationResult {
        ok: true,
        message: if config.recycle_deleted_mods {
            "message.mods.movedToRecycleBin".into()
        } else {
            "message.mods.deleted".into()
        },
    })
}

#[tauri::command]
pub fn toggle_local_mod(
    state: State<'_, AppState>,
    dll_file: String,
    enabled: bool,
) -> Result<OperationResult, String> {
    log::info!("切换本地 MOD：文件={}，启用={}", dll_file, enabled);
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;
    let relative = mods::canonical_dll_name(&dll_file.replace('\\', "/"));
    toggle_local_mod_file(&game_root, &relative, enabled)?;
    log::info!("本地 MOD 状态切换完成：文件={}，启用={}", relative, enabled);
    Ok(OperationResult {
        ok: true,
        message: if enabled {
            "mods.enabled".into()
        } else {
            "mods.disabled".into()
        },
    })
}

#[tauri::command]
pub fn toggle_local_mod_package(
    state: State<'_, AppState>,
    package_id: String,
    enabled: bool,
) -> Result<OperationResult, String> {
    log::info!("切换 MOD 包：包={}，启用={}", package_id, enabled);
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;
    let manifest = archive::read_manifest(&state.config.manifests_dir(), &package_id)
        .ok_or("mods.packageManifestMissing")?;
    let mut dll_files = manifest
        .files
        .iter()
        .map(|file| mods::canonical_dll_name(file))
        .filter(|file| {
            file.to_ascii_lowercase().starts_with("bepinex/plugins/")
                && Path::new(file)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("dll"))
        })
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    dll_files.sort_by_key(|file| file.to_ascii_lowercase());
    if dll_files.is_empty() {
        return Err("mods.packageNoToggleableDll".into());
    }
    let mut changed = Vec::new();
    for dll_file in dll_files {
        match set_local_mod_file_state(&game_root, &dll_file, enabled) {
            Ok(true) => changed.push(dll_file),
            Ok(false) => {}
            Err(error) => {
                log::error!(
                    "切换 MOD 包失败，开始回滚：包={}，原因={}",
                    package_id,
                    error
                );
                for previous in changed.into_iter().rev() {
                    let _ = toggle_local_mod_file(&game_root, &previous, !enabled);
                }
                return Err(error);
            }
        }
    }
    log::info!(
        "MOD 包状态切换完成：包={}，启用={}，变更文件数={}",
        package_id,
        enabled,
        changed.len()
    );
    Ok(OperationResult {
        ok: true,
        message: if enabled {
            "mods.packageEnabled".into()
        } else {
            "mods.packageDisabled".into()
        },
    })
}

/// 一次性启用或禁用**全部**已安装的插件 DLL。
///
/// 与单包切换共用 `set_local_mod_file_state`（负责路径归属校验、
/// `.disabled` 成对检查与只读位清理），因此安全边界完全一致。
///
/// 失败时回滚已经改过的文件：批量操作必须要么全成、要么全不动。
/// 不回滚会留下「一半启用一半禁用」的中间态，用户看到的是随机结果。
///
/// 不切换 `BepInEx/plugins` 之外的 DLL（`patchers`、`core` 里的管理器自身
/// 组件等）：那些不是 MOD，改了会破坏框架。
#[tauri::command]
pub fn set_all_local_mods_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<OperationResult, String> {
    log::info!("批量切换全部本地 MOD：启用={}", enabled);
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;

    /*
     * 直接从磁盘扫描，不依赖 Dashboard 快照：
     * 快照可能过期，而用户点「全部启用」时期望覆盖磁盘上真实存在的每一个插件。
     */
    let dll_files = mods::plugin_dll_files(&game_root, Some(&state.config.manifests_dir()))?;
    if dll_files.is_empty() {
        return Err("mods.noPluginDll".into());
    }

    let mut changed = Vec::new();
    for dll_file in dll_files {
        // 统一成启用态路径再交给状态机：它自己会按 `.disabled` 找源文件
        let relative = dll_file.trim_end_matches(".disabled").to_string();
        match set_local_mod_file_state(&game_root, &relative, enabled) {
            Ok(true) => changed.push(relative),
            Ok(false) => {}
            Err(error) => {
                log::error!("批量切换失败，开始回滚：文件={}，原因={}", relative, error);
                for previous in changed.into_iter().rev() {
                    let _ = toggle_local_mod_file(&game_root, &previous, !enabled);
                }
                return Err(crate::locale_tag::with_params(
                    "mods.toggleFailed",
                    &[
                        ("relative", &relative.to_string()),
                        ("error", &error.to_string()),
                    ],
                ));
            }
        }
    }
    log::info!(
        "批量切换完成：启用={}，变更文件数={}",
        enabled,
        changed.len()
    );
    Ok(OperationResult {
        ok: true,
        message: crate::locale_tag::with_params(
            if enabled {
                "message.mods.bulkToggleEnabled"
            } else {
                "message.mods.bulkToggleDisabled"
            },
            &[("count", &changed.len().to_string())],
        ),
    })
}

#[cfg(test)]
mod package_confirmation_tests;
