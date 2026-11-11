use crate::app_state::AppState;
use crate::archive;
use crate::game;
use crate::level_sets::view::bind_map_relation_snapshot;
use crate::level_sets::view::level_set_to_info;
use crate::models::{LevelSetFlattenReport, LevelSetInfo, LevelSetScanProgress, OperationResult};
use crate::shell_open;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri::Emitter;
use tauri::State;

/// 清空地图（关卡集）`info` 的解析缓存，返回删除的磁盘缓存文件数。
///
/// 同步命令：只删文件与清一个内存映射，没有解析也没有 IO 等待，
/// 放进 `spawn_blocking` 反而多一次线程切换。
#[tauri::command]
pub fn clear_level_set_cache() -> Result<usize, String> {
    crate::level_sets::clear_info_cache()
}

/// 列出全部关卡集（启用与停用）。
///
/// 只读取地图列表；转换由本地 MOD 扫描与安装完成分别入队。
/// 未找到游戏目录时返回空列表而不是报错：地图页在没有厨房的情况下也该能打开。
#[tauri::command]
pub async fn get_level_sets(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<LevelSetInfo>, String> {
    /*
     * 必须离开 IPC 主线程再扫描。
     *
     * 同步命令由 Tauri 直接在调用线程上内联执行（`tauri-macros` 的同步包装
     * 就是 `let result = handler(..)`），而扫描要逐个解压 `info` 容器：debug
     * 构建下冷扫描 7 个关卡集实测 13.6 s，期间整个窗口不响应。改成 async +
     * `spawn_blocking` 后耗时不变，但界面可以继续转圈、可以取消。
     *
     * `State` 不能跨 await，所以在同步段把需要的东西都取出来再 move 进闭包。
     */
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let prepared = prepare_level_set_scan(&state)?;
        let Some((config, snapshot, roots)) = prepared else {
            /*
             * 游戏目录没了：旧监听还指着已经消失的路径，先停掉（空 roots 在
             * `LevelWatcher::watch` 里就是「停掉现有监听」），下次检测到目录再建。
             */
            state.level_watch.watch(&app, Vec::new());
            crate::api_scheduler::set_installed("map", Vec::new());
            return Ok(Vec::new());
        };
        /*
         * 目录监听跟着游戏目录走：`watch` 幂等，目标没变直接返回，游戏路径被
         * 用户改掉时重建。这里仍在同步段，`State` 不能跨 await，先把它用掉。
         */
        crate::level_sets::recompress::watch_selected(&state, &app);
        let manifests_dir = state.config.manifests_dir();
        // 并行度在同步段算好再 move 进闭包（`State` 不能跨 await）
        let info_workers = crate::concurrency::workers_from(config.map_info_concurrency);
        /*
         * 进度事件逐份发：解析发生在多个线程里，`emit` 本身线程安全。
         * 发送失败（窗口已关、前端还没挂监听）不影响扫描结果，忽略即可。
         */
        let progress_app = app.clone();
        let sets = crate::level_sets::scan_level_sets_with_progress(
            &roots,
            Some(&manifests_dir),
            info_workers,
            &move |done, total, current| {
                let _ = progress_app.emit(
                    "level-set-scan-progress",
                    LevelSetScanProgress {
                        done,
                        total,
                        current: current.to_string(),
                    },
                );
            },
        );
        // 快照只读一次：逐条关卡集都去读文件会变成几十次 IO
        let snapshots = archive::read_map_relation_snapshots(&manifests_dir);
        log::debug!("关卡集列表读取完成：数量={}", sets.len());
        let infos: Vec<LevelSetInfo> = sets
            .into_iter()
            .map(|set| {
                let mut info = level_set_to_info(set);
                bind_map_relation_snapshot(&mut info, &config, &snapshots);
                info
            })
            .collect();
        // Parsing happens outside the operation lock. A scan superseded by a
        // game/root change must neither publish its result nor reset priorities.
        let _operation = state
            .operation
            .lock()
            .map_err(|_| "lock.poisoned".to_string())?;
        state
            .local_mod_snapshot
            .lock()
            .map_err(|_| "lock.poisoned".to_string())?
            .get(snapshot.generation, roots.game_root())?;
        crate::api_scheduler::set_installed(
            "map",
            config
                .map_repositories
                .iter()
                .filter(|map| {
                    infos
                        .iter()
                        .any(|info| info.map_id.as_deref() == Some(map.id.as_str()))
                })
                .map(|map| {
                    crate::api_scheduler::repository_key(
                        &map.provider,
                        &map.username,
                        &map.repository,
                    )
                }),
        );
        Ok(infos)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Migration is the only write associated with scanning. Select and migrate
/// while the operation lock prevents a new scan or game setting from intervening.
fn prepare_level_set_scan(
    state: &AppState,
) -> Result<
    Option<(
        crate::models::AppConfig,
        crate::models::LocalModSnapshot,
        crate::level_sets::LevelSetRoots,
    )>,
    String,
> {
    let _operation = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let Some(game_root) = game::detect_game(&config).path else {
        return Ok(None);
    };
    let (snapshot, roots) =
        crate::mods::snapshot::level_roots(state, std::path::Path::new(&game_root))?;
    if roots.package_root().is_some() {
        crate::level_sets::migrate_legacy_disabled_root(&roots);
    }
    Ok(Some((config, snapshot, roots)))
}

#[cfg(test)]
#[path = "levels/scan_tests.rs"]
mod scan_tests;

/// 启用或停用一个关卡集（在 `levels/` 与 `BepInEx/cache/modmanager_disabled_maps/` 之间移动目录）。
///
/// 地图启停允许游戏运行；路径校验、操作锁与目录冲突保护仍由既有链路负责。
#[tauri::command]
pub fn set_level_set_enabled(
    state: State<'_, AppState>,
    dir_name: String,
    enabled: bool,
) -> Result<OperationResult, String> {
    log::info!("切换关卡集状态：目录={}，启用={}", dir_name, enabled);
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    let (_, roots) = crate::mods::snapshot::level_roots(&state, &game_root)?;
    crate::level_sets::set_level_set_enabled(&roots, &dir_name, enabled)?;
    Ok(OperationResult {
        ok: true,
        message: if enabled {
            "message.levelSet.enabled".into()
        } else {
            "message.levelSet.disabled".into()
        },
    })
}

/// 批量切换全部关卡集（地图管理页的「全部启用 / 全部禁用」）。
///
/// 逐目录切换本身只是改名，但前端逐条发 IPC 时每条都要重新读配置、探测
/// 游戏目录并抢一次操作锁 —— 地图一多就是一串重复开销。核心扫描与回滚在
/// `level_sets::set_all_level_sets_enabled`（有测试），命令层只负责
/// 加锁、定位游戏目录和把结果翻译成提示。
#[tauri::command]
pub fn set_all_level_sets_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<OperationResult, String> {
    log::info!("批量切换全部关卡集：启用={}", enabled);
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    let (_, roots) = crate::mods::snapshot::level_roots(&state, &game_root)?;
    let changed = crate::level_sets::set_all_level_sets_enabled(&roots, enabled)?;
    if changed.is_empty() {
        return Ok(OperationResult {
            ok: true,
            message: "message.levelSet.nothingToToggle".into(),
        });
    }
    log::info!(
        "批量切换关卡集完成：启用={}，变更数量={}",
        enabled,
        changed.len()
    );
    Ok(OperationResult {
        ok: true,
        message: crate::locale_tag::with_params(
            if enabled {
                "message.levelSet.bulkEnabled"
            } else {
                "message.levelSet.bulkDisabled"
            },
            &[("count", &changed.len().to_string())],
        ),
    })
}

/// 在系统文件管理器中打开一个关卡集目录。
///
/// 只读：不创建目录、不要求游戏已停止。前端只传目录名与「启用与否」，实际
/// 路径由后端选择当前最高版本包的 `levels` 或停用根
/// `BepInEx/cache/modmanager_disabled_maps` 后拼出。参数不能指定任意路径，
/// `safe_game_path` 与对应根目录的归属检查都在 `level_sets` 里做。
#[tauri::command]
pub fn open_level_set_directory(
    state: State<'_, AppState>,
    dir_name: String,
    enabled: bool,
) -> Result<OperationResult, String> {
    log::debug!("打开关卡集目录：目录={}，启用={}", dir_name, enabled);
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    let (_, roots) = crate::mods::snapshot::level_roots(&state, &game_root)?;
    let path = crate::level_sets::locate_level_set_directory(&roots, &dir_name, enabled)?;
    shell_open::open_path(&path)?;
    Ok(OperationResult {
        ok: true,
        message: "message.levelSet.dirOpened".into(),
    })
}

/// 删除一个关卡集（默认移入回收站，跟随 `recycle_deleted_mods`）。
///
/// 纯地图删除允许游戏运行；实际涉及随包 DLL 时，领域实现保留 MOD 门禁。
#[tauri::command]
pub async fn delete_level_set(
    state: State<'_, AppState>,
    dir_name: String,
    enabled: bool,
) -> Result<OperationResult, String> {
    // 回收/删除要移动整个集目录（可能有上千个文件），不能占着 IPC 线程
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        delete_level_set_blocking(&state, &dir_name, enabled)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// [`delete_level_set`] 的同步实现（在阻塞线程池里跑）。
fn delete_level_set_blocking(
    state: &AppState,
    dir_name: &str,
    enabled: bool,
) -> Result<OperationResult, String> {
    log::info!("删除关卡集：目录={}，启用={}", dir_name, enabled);
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    let (_, roots) = crate::mods::snapshot::level_roots(&state, &game_root)?;
    crate::level_sets::remove_level_set(
        &state.config.mod_recycle_bin_dir(),
        &state.config.manifests_dir(),
        &roots,
        &dir_name,
        enabled,
        config.recycle_deleted_mods,
    )?;
    Ok(OperationResult {
        ok: true,
        message: if config.recycle_deleted_mods {
            "message.levelSet.movedToRecycleBin".into()
        } else {
            "message.levelSet.deleted".into()
        },
    })
}

/// 一键拍平：把 `levels/` 下嵌套了几层的关卡集整理成扫描认得的规范目录。
///
/// `dry_run=true` 只探测候选、不写盘（页头按钮据此显示「有几个目录可以整理」）。
/// 真正搬移占用操作锁，游戏运行时也允许执行。
#[tauri::command]
pub async fn flatten_level_sets(
    state: State<'_, AppState>,
    dry_run: bool,
) -> Result<LevelSetFlattenReport, String> {
    // 搬移可能涉及上千个文件，不能占着 IPC 线程
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || flatten_level_sets_blocking(&state, dry_run))
        .await
        .map_err(|error| error.to_string())?
}

/// [`flatten_level_sets`] 的同步实现（在阻塞线程池里跑）。
fn flatten_level_sets_blocking(
    state: &AppState,
    dry_run: bool,
) -> Result<LevelSetFlattenReport, String> {
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    let recycle_root = state.config.mod_recycle_bin_dir();
    let manifests_dir = state.config.manifests_dir();
    if dry_run {
        let (_, roots) = crate::mods::snapshot::level_roots(state, &game_root)?;
        // 探测只读：不锁操作、不要求游戏停止
        return crate::level_sets::flatten_level_sets(&roots, &recycle_root, &manifests_dir, true);
    }
    log::info!("开始整理地图目录：游戏目录={}", game_root.display());
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let (_, roots) = crate::mods::snapshot::level_roots(state, &game_root)?;
    crate::level_sets::flatten_level_sets(&roots, &recycle_root, &manifests_dir, false)
}

/// 预览仅接收已安装关卡集坐标，不接收任意文件路径，也不要求停止游戏。
#[tauri::command]
pub async fn get_level_set_preview(
    state: State<'_, AppState>,
    dir_name: String,
    enabled: bool,
) -> Result<Vec<crate::level_sets::preview_reader::Level>, String> {
    let config = state.config.load_or_default();
    let root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    let (_, roots) = crate::mods::snapshot::level_roots(&state, &root)?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::level_sets::preview::catalog(&roots, &dir_name, enabled)
    })
    .await
    .map_err(|_| "assetBundle.previewUnavailable".to_string())?
}
#[tauri::command]
pub async fn get_level_preview_detail(
    state: State<'_, AppState>,
    dir_name: String,
    enabled: bool,
    index: usize,
    part: String,
) -> Result<crate::level_sets::preview::Detail, String> {
    let config = state.config.load_or_default();
    let root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    let (_, roots) = crate::mods::snapshot::level_roots(&state, &root)?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::level_sets::preview::detail(&roots, &dir_name, enabled, index, &part)
    })
    .await
    .map_err(|_| "assetBundle.previewUnavailable".to_string())?
}

#[tauri::command]
pub fn get_level_compression_progress() -> crate::level_sets::recompress::Progress {
    crate::level_sets::recompress::snapshot()
}
