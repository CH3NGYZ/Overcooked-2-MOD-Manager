use crate::app_state::AppState;
use crate::archive;
use crate::bepinex::install::BEPINEX_MANIFEST_ID;
use crate::bepinex::install::framework_manifest_files;
use crate::bepinex::install::install_framework_archive;
use crate::download::DownloadContext;
use crate::download::planning::asset_proxy_urls;
use crate::download::planning::plan_asset_download_candidates;
use crate::download::try_download_candidates as try_asset_download_candidates;
use crate::framework::BEPINEX_BACKUP;
use crate::framework::backup_framework;
use crate::framework::ensure_copy_paths_disjoint;
use crate::framework::restore_framework_backup;
use crate::framework::validate_framework_backup_in;
use crate::game;
use crate::models::{BepInExInstallResult, GameRuntime, OperationResult, Provider, ProxyMode};
use crate::recycle_bin;
use crate::releases;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri::State;

#[tauri::command]
pub async fn install_bepinex(
    app: AppHandle,
    state: State<'_, AppState>,
    reinstall: bool,
    provider: Provider,
) -> Result<BepInExInstallResult, String> {
    archive::start_download();
    log::info!(
        "开始安装 BepInEx：平台={:?}，重新安装={}",
        provider,
        reinstall
    );
    let config = state.config.load_or_default();
    let package = crate::bepinex::Package::current(&config.game);
    let info = game::detect_game(&config);
    if !info.game_exists {
        return Err("game.executableMissing".into());
    }
    let game_root = info.path.as_deref().ok_or("game.notFound")?;
    let game_root = PathBuf::from(game_root);
    game::ensure_game_stopped(&game_root)?;
    if info.bepinex.installed && !reinstall {
        return Err("framework.alreadyInstalled".into());
    }
    let client = releases::build_download_client(config.network.timeout_seconds)
        .map_err(|e| e.to_string())?;
    let download_url = package.url(&provider);
    log::debug!(
        "开始下载 BepInEx 安装包：平台={:?}，URL={}",
        provider,
        download_url
    );
    let proxy_urls = if provider == Provider::Github {
        asset_proxy_urls(
            &config.github_proxies.release_proxies,
            download_url,
            false,
            "",
            "",
            ProxyMode::AutoProxy,
            None,
        )
    } else {
        Vec::new()
    };
    let candidates =
        plan_asset_download_candidates(ProxyMode::AutoDirect, download_url, proxy_urls)?;
    let cache_dir = state.config.cache_dir();
    let temp = try_asset_download_candidates(
        candidates,
        archive::is_download_cancelled,
        |candidate, reporter| {
            let client = &client;
            let cache_dir = &cache_dir;
            async move {
                let temp = archive::download_to_temp_with_threads_and_first_byte_timeout(
                    client,
                    &candidate.url,
                    cache_dir,
                    "BepInEx",
                    &reporter,
                    1,
                    Some(archive::FIRST_BYTE_TIMEOUT),
                )
                .await?;
                if let Err(error) =
                    archive::validate_download_file(&temp, None, Some(package.digest()))
                        .and_then(|_| archive::validate_zip(&temp))
                {
                    let _ = fs::remove_file(&temp);
                    return Err(error);
                }
                Ok(temp)
            }
        },
        Some(DownloadContext {
            execution: None,
            app: Some(&app),
            label: "BepInEx",
            expected_size: None,
        }),
    )
    .await?;
    let installation = (|| {
        let _guard = state
            .operation
            .lock()
            .map_err(|_| "lock.poisoned".to_string())?;
        if state.config.load_or_default().game != config.game {
            return Err("game.settingsChanged".into());
        }
        if archive::is_download_cancelled() {
            return Err("download.cancelled".into());
        }
        game::ensure_game_stopped(&game_root)?;
        install_framework_archive(
            &temp,
            &game_root,
            &state.config.backups_dir(),
            &state.config.manifests_dir(),
            package,
            reinstall,
            crate::bepinex::PACKAGE_VERSION,
        )
    })();
    let _ = fs::remove_file(&temp);
    match installation {
        Ok(()) => {
            log::info!(
                "BepInEx 安装完成：渠道={:?}，游戏包={:?}",
                provider,
                package
            );
            #[cfg(unix)]
            let override_result = if !package.native() {
                let prefix = info.wine_prefix.map(PathBuf::from);
                Some(
                    tauri::async_runtime::spawn_blocking(move || {
                        let prefix = prefix.ok_or("wine.prefixUnknown")?;
                        crate::wine::configure_winhttp(&prefix)
                    })
                    .await
                    .unwrap_or_else(|_| Err("wine.winhttpFailed".into())),
                )
            } else {
                None
            };
            #[cfg(not(unix))]
            let override_result: Option<Result<(), String>> = None;
            let wine_override_applied = override_result.as_ref().is_some_and(Result::is_ok);
            let wine_override_error = override_result.and_then(Result::err);
            if let Some(error) = &wine_override_error {
                log::warn!("BepInEx 已安装，Wine 配置需手动完成：{error}");
            }
            Ok(BepInExInstallResult {
                operation: OperationResult {
                    ok: true,
                    message: "framework.bepInExInstalled".into(),
                },
                wine_override_applied,
                wine_override_error,
                macos_overlay_repaired: package == crate::bepinex::Package::MacOS,
            })
        }
        Err(error) => {
            log::error!("BepInEx 安装失败：原因={}", error);
            Err(crate::locale_tag::with_params(
                "framework.installFailed",
                &[("reason", &error)],
            ))
        }
    }
}

#[tauri::command]
pub fn repair_native_bepinex_overlay(
    state: State<'_, AppState>,
) -> Result<OperationResult, String> {
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    if std::env::consts::OS != "macos" || config.game.runtime != GameRuntime::Native {
        return Err("framework.overlayMacNativeOnly".into());
    }
    let info = game::detect_game(&config);
    if !info.game_exists {
        return Err("game.executableMissing".into());
    }
    let game_root = PathBuf::from(info.path.ok_or("game.directoryMissing")?);
    game::ensure_game_stopped(&game_root)?;
    crate::bepinex::repair_macos_overlay(&game_root)?;
    log::info!("macOS Steam Overlay 注入修复完成");
    Ok(OperationResult {
        ok: true,
        message: "framework.overlayRepaired".into(),
    })
}

#[tauri::command]
pub async fn uninstall_bepinex(state: State<'_, AppState>) -> Result<OperationResult, String> {
    // 卸载前要把整个 BepInEx 目录快照到备份：几百 MB 的复制，不能占 IPC 线程
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || uninstall_bepinex_blocking(&state))
        .await
        .map_err(|error| error.to_string())?
}

/// [`uninstall_bepinex`] 的同步实现（在阻塞线程池里跑）。
fn uninstall_bepinex_blocking(state: &AppState) -> Result<OperationResult, String> {
    log::info!("开始卸载 BepInEx");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let info = game::detect_game(&config);
    let game_root = PathBuf::from(info.path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;
    let source = game_root.join("BepInEx");
    /*
     * 必须按「核心程序集存在」判断，不能按目录存在。
     *
     * 目录在、核心不在 = 只有 MOD 的空壳。此时如果继续走删除流程，
     * 没有框架清单就会删掉整个 `BepInEx` —— 把用户所有 MOD 一起删掉，
     * 而用户只是点了一下「卸载框架」。
     */
    if !crate::bepinex::installed(&game_root) {
        return Err(if source.exists() {
            "framework.bepinexOnlyResidue".into()
        } else {
            "framework.bepinexMissing".to_string()
        });
    }
    ensure_copy_paths_disjoint(&source, &state.config.backups_dir())?;

    /*
     * 卸载前先备份框架目录。
     *
     * 回收站保留的是「按清单移走的文件」，而备份是**整目录快照**：
     * 用户自己往 BepInEx 里放的东西（手工装的插件、改过的 config、日志）
     * 不在清单里，回收站不会带走它们 —— 只有备份能保证还能找回。
     * 两者职责不同，都要保留。
     */
    let backup = backup_framework(
        &game_root,
        &state.config.backups_dir(),
        &BEPINEX_BACKUP,
        &state.config.manifests_dir(),
        BEPINEX_MANIFEST_ID,
    )?;
    log::info!("BepInEx 卸载前备份完成：路径={}", backup.display());

    let manifests_dir = state.config.manifests_dir();
    let recycle_root = state.config.mod_recycle_bin_dir();
    /*
     * 优先按安装清单回收。
     *
     * 框架 ZIP 不只包含 `BepInEx/`：`winhttp.dll`、`doorstop_config.ini`、
     * `run_bepinex.sh` 都直接解压在游戏根目录。只 `remove_dir_all(BepInEx)`
     * 会把这些文件留在原地 —— 用户以为卸载干净了，实际游戏仍被 doorstop 接管。
     *
     * 清单里记的就是当初解压出的全部游戏相对路径，所以直接复用回收站的
     * `recycle_package`（它内部走 `stage`，负责共享文件保护、清单快照、
     * scratch 目录与回滚），不另写一套移动逻辑。
     */
    if archive::read_manifest(&manifests_dir, BEPINEX_MANIFEST_ID).is_some() {
        match recycle_bin::recycle_package(
            &recycle_root,
            &manifests_dir,
            &game_root,
            BEPINEX_MANIFEST_ID,
            None,
        ) {
            Ok(entry) => {
                log::info!(
                    "BepInEx 卸载完成（已回收）：文件数={}，回收记录={}，备份={}",
                    entry.file_count,
                    entry.id,
                    backup.display()
                );
                return Ok(OperationResult {
                    ok: true,
                    message: crate::locale_tag::with_params(
                        "message.framework.movedToRecycleBin",
                        &[
                            ("count", &entry.file_count.to_string()),
                            ("path", &backup.display().to_string()),
                        ],
                    ),
                });
            }
            Err(error) => {
                /*
                 * 回收失败就中止，不退回「直接删目录」：
                 * 那种做法只能删掉一部分，留下散落的 doorstop/winhttp 文件，
                 * 用户会看到一个既装不上又卸不掉的半残状态。
                 */
                log::error!(
                    "BepInEx 回收失败，已中止卸载：原因={}，备份={}",
                    error,
                    backup.display()
                );
                return Err(crate::locale_tag::with_params(
                    "backup.recycleFailed",
                    &[
                        ("reason", &error.to_string()),
                        ("path", &backup.display().to_string()),
                    ],
                ));
            }
        }
    }

    /*
     * 没有清单（旧版本装的框架）：按「框架已知文件」现造一份清单再回收。
     *
     * 以前这里直接 `remove_dir_all(BepInEx)`，会把 `plugins/`、`config/`
     * 里的用户 MOD 和配置一起删掉 —— 而用户点的是「卸载框架」。
     * 现在只收框架自己的东西（core、doorstop 库、根目录的加载器文件），
     * 装 MOD 产生的内容原地保留；回收记录仍可在回收站恢复。
     */
    log::warn!("未找到 BepInEx 安装清单，按框架已知文件重建清单后回收");
    let files = framework_manifest_files(&game_root);
    if files.is_empty() {
        return Err(crate::locale_tag::with_params(
            "backup.noReclaimableFiles",
            &[("path", &backup.display().to_string())],
        ));
    }
    let fallback = archive::new_manifest(
        BEPINEX_MANIFEST_ID,
        crate::bepinex::PACKAGE_VERSION,
        archive::single_language("BepInEx"),
        Default::default(),
        files,
        Vec::new(),
    );
    archive::write_manifest(&manifests_dir, &fallback)?;
    match recycle_bin::recycle_package(
        &recycle_root,
        &manifests_dir,
        &game_root,
        BEPINEX_MANIFEST_ID,
        None,
    ) {
        Ok(entry) => {
            log::info!(
                "BepInEx 卸载完成（旧安装按已知文件回收）：文件数={}，回收记录={}，备份={}",
                entry.file_count,
                entry.id,
                backup.display()
            );
            Ok(OperationResult {
                ok: true,
                message: crate::locale_tag::with_params(
                    "message.framework.movedToRecycleBin",
                    &[
                        ("count", &entry.file_count.to_string()),
                        ("path", &backup.display().to_string()),
                    ],
                ),
            })
        }
        Err(error) => Err(crate::locale_tag::with_params(
            "backup.recycleFailed",
            &[
                ("reason", &error.to_string()),
                ("path", &backup.display().to_string()),
            ],
        )),
    }
}

#[tauri::command]
pub async fn backup_bepinex(state: State<'_, AppState>) -> Result<OperationResult, String> {
    // 整目录快照是重活：同步命令会把窗口冻在「备份中」那一刻
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || backup_bepinex_blocking(&state))
        .await
        .map_err(|error| error.to_string())?
}

/// [`backup_bepinex`] 的同步实现（在阻塞线程池里跑）。
fn backup_bepinex_blocking(state: &AppState) -> Result<OperationResult, String> {
    log::info!("开始备份 BepInEx");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;
    let backup = backup_framework(
        &game_root,
        &state.config.backups_dir(),
        &BEPINEX_BACKUP,
        &state.config.manifests_dir(),
        BEPINEX_MANIFEST_ID,
    )?;
    log::info!("BepInEx 备份完成：路径={}", backup.display());
    Ok(OperationResult {
        ok: true,
        message: crate::locale_tag::with_params(
            "message.backup.created",
            &[("path", &backup.display().to_string())],
        ),
    })
}

#[tauri::command]
pub async fn restore_bepinex(
    state: State<'_, AppState>,
    backup_path: String,
) -> Result<OperationResult, String> {
    // 把整份备份写回游戏目录同样是重活：不能占 IPC 线程
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || restore_bepinex_blocking(&state, &backup_path))
        .await
        .map_err(|error| error.to_string())?
}

/// [`restore_bepinex`] 的同步实现（在阻塞线程池里跑）。
fn restore_bepinex_blocking(
    state: &AppState,
    backup_path: &str,
) -> Result<OperationResult, String> {
    log::info!("开始恢复 BepInEx 备份");
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
        &BEPINEX_BACKUP,
    )?;
    restore_framework_backup(
        &game_root,
        &backup,
        &BEPINEX_BACKUP,
        &state.config.manifests_dir(),
        BEPINEX_MANIFEST_ID,
    )?;
    log::info!("BepInEx 备份恢复完成");
    Ok(OperationResult {
        ok: true,
        message: "message.backup.bepInExRestored".into(),
    })
}

#[tauri::command]
pub async fn delete_bepinex_backup(
    state: State<'_, AppState>,
    backup_path: String,
) -> Result<OperationResult, String> {
    // 删整目录按文件数计：大备份能到几秒，搬到阻塞线程池
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        delete_bepinex_backup_blocking(&state, &backup_path)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// [`delete_bepinex_backup`] 的同步实现（在阻塞线程池里跑）。
fn delete_bepinex_backup_blocking(
    state: &AppState,
    backup_path: &str,
) -> Result<OperationResult, String> {
    log::info!("开始删除 BepInEx 备份");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;

    let target = fs::canonicalize(&backup_path).map_err(|_| "backup.notFound".to_string())?;
    let name = target
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if !name.starts_with("BepInEx.backup-") || !target.is_dir() {
        return Err("backup.wrongKindBepInEx".into());
    }

    let game_root = fs::canonicalize(&game_root).map_err(|e| e.to_string())?;
    let manager_backup_path = state.config.backups_dir();
    fs::create_dir_all(&manager_backup_path).map_err(|e| e.to_string())?;
    let manager_backup_root = fs::canonicalize(manager_backup_path).map_err(|e| e.to_string())?;
    let allowed = target.parent() == Some(game_root.as_path())
        || target.parent() == Some(manager_backup_root.as_path());
    if !allowed {
        return Err("backup.outsideRoot".into());
    }

    archive::clear_readonly(&target)?;
    fs::remove_dir_all(&target).map_err(|e| e.to_string())?;
    log::info!("BepInEx 备份删除完成");
    Ok(OperationResult {
        ok: true,
        message: "message.backup.deleted".into(),
    })
}
