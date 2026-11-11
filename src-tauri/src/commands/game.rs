use crate::app_state::AppState;
use crate::game;
use crate::models::{GameConfig, GameInfo, OperationResult, WinhttpState};
use std::path::Path;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri::Emitter;
use tauri::State;

#[tauri::command]
pub fn detect_game(state: State<'_, AppState>) -> Result<GameInfo, String> {
    log::debug!("检测游戏目录和 BepInEx 状态");
    let config = state.config.load_or_default();
    let info = game::detect_game(&config);
    log::info!(
        "游戏检测完成：已找到游戏={}，BepInEx 已安装={}",
        info.path.is_some(),
        info.bepinex.installed
    );
    *state.game_state.lock().map_err(|error| error.to_string())? = Some(info.clone());
    Ok(info)
}

#[tauri::command]
pub fn check_crossover_running() -> bool {
    game::is_crossover_running()
}

#[tauri::command]
pub fn set_game_path(state: State<'_, AppState>, path: String) -> Result<GameInfo, String> {
    log::info!("设置游戏目录");
    let previous = state.config.load_or_default();
    let root = game::selected_game_root(Path::new(&path), &previous.game)?;
    if let Some(path) = game::detect_game(&previous).path {
        game::ensure_game_stopped(Path::new(&path))?;
    }
    let config = {
        let _guard = state
            .operation
            .lock()
            .map_err(|_| "lock.poisoned".to_string())?;
        let mut config = state.config.load_or_default();
        if config.game != previous.game {
            return Err("game.settingsChanged".into());
        }
        state.invalidate_local_mod_snapshot()?;
        config.game.path = root.to_string_lossy().to_string();
        state.config.save(&config).map_err(|e| e.to_string())?;
        config
    };
    log::info!("游戏目录设置完成");
    let info = game::detect_game(&config);
    *state.game_state.lock().map_err(|error| error.to_string())? = Some(info.clone());
    Ok(info)
}

#[tauri::command]
pub fn reset_game_path(state: State<'_, AppState>) -> Result<GameInfo, String> {
    log::info!("重置游戏目录");
    let config = {
        let _guard = state
            .operation
            .lock()
            .map_err(|_| "lock.poisoned".to_string())?;
        let mut config = state.config.load_or_default();
        state.invalidate_local_mod_snapshot()?;
        config.game.path.clear();
        state.config.save(&config).map_err(|e| e.to_string())?;
        config
    };
    log::info!("游戏目录已重置");
    let info = game::detect_game(&config);
    *state.game_state.lock().map_err(|error| error.to_string())? = Some(info.clone());
    Ok(info)
}

/// 立刻重算游戏状态并返回最新快照。
///
/// 观察器每秒推进一次、只在变化时推送，正常情况下前端不需要主动拉。但
/// 「进程刚退出」「用户从游戏切回管理器」「某次操作因为 `game.running` 被拒」
/// 这几个时刻，界面上显示的仍是上一轮的结果，用户会看到按钮卡在「停止游戏」。
/// 这条命令让前端当场把状态拉正，而不是等下一轮观察。
#[tauri::command]
pub fn refresh_game_state(app: AppHandle, state: State<'_, AppState>) -> Result<GameInfo, String> {
    // 触发点很少（切回窗口、点完停止、手动重算），记一行便于对账
    log::debug!("收到游戏状态重算请求");
    let info = refresh_game_snapshot(state.inner(), &app)?;
    if info.kitchen_running {
        log::debug!("游戏状态重算：仍在运行");
    }
    Ok(info)
}

/// 重算游戏快照，写回共享状态，变化时推送事件。
///
/// `get_dashboard` 会重新探测并覆盖快照，所以这里只更新运行状态与探测
/// 结果本身，不扫描 MOD —— 前端 `load()` 才做全量扫描。
fn refresh_game_snapshot(state: &AppState, app: &AppHandle) -> Result<GameInfo, String> {
    let config = state.config.load_or_default();
    let info = game::detect_game(&config);
    let changed = {
        let mut snapshot = state.game_state.lock().map_err(|error| error.to_string())?;
        let changed = snapshot.as_ref() != Some(&info);
        *snapshot = Some(info.clone());
        changed
    };
    if changed {
        if let Err(error) = app.emit(crate::game_state::GAME_STATE_EVENT, &info) {
            log::warn!("发送游戏状态变化事件失败：{error}");
        }
    }
    Ok(info)
}

#[tauri::command]
pub async fn stop_game(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<OperationResult, String> {
    log::info!("收到停止厨房请求");
    // 结束进程不需要操作锁：锁是给配置/清单提交用的，
    // 而游戏退出期间的其他只读操作（例如状态刷新）应继续可用。
    // `State` 不能跨 await，先克隆出 `AppState`（克隆的是配置服务与各 `Arc` 句柄）。
    let state = state.inner().clone();
    let config = state.config.load_or_default();
    let info = game::detect_game(&config);
    let game_root = PathBuf::from(info.path.as_deref().ok_or("game.notFound")?);
    /*
     * 温和阶段预算 20 秒、强制后等待预算 6 秒；按窗口的同步兜底和进程扫描
     * 另计耗时，不是整个命令的硬截止。整段都要放在阻塞线程池里：
     * Tauri 的同步命令在 IPC 主线程内联执行，等待期间窗口会完全无响应。
     *
     * 停止失败时把「被判定成游戏」的进程名记下来：这类失败几乎都是同名的
     * 宿主进程或残留进程造成的，没有进程名就只剩「停止不了」这一句。
     */
    let result = tauri::async_runtime::spawn_blocking({
        let game_root = game_root.clone();
        move || game::stop_game(&game_root)
    })
    .await
    .map_err(|error| error.to_string())?;
    if let Err(error) = &result {
        log::warn!(
            "停止厨房失败：{error}；判定为游戏运行的进程={:?}",
            game::running_processes(&game_root)
        );
    }
    let stopped = result?;
    // 停完立刻重算并推送：用户点完按钮就该看到「启动游戏」，不再等观察器。
    refresh_game_snapshot(&state, &app)?;
    Ok(OperationResult {
        ok: true,
        message: if stopped == 0 {
            "game.notRunning".into()
        } else {
            "game.stopped".into()
        },
    })
}

#[tauri::command]
pub fn launch_normal(state: State<'_, AppState>) -> Result<OperationResult, String> {
    log::info!("启动厨房：正常 MOD 模式");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let info = game::detect_game(&config);
    let game_root = PathBuf::from(info.path.as_deref().ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;
    if config.launch.auto_restore_winhttp {
        if matches!(
            crate::bepinex::state(&game_root, &config.game),
            WinhttpState::ModDisabled | WinhttpState::Conflict
        ) {
            crate::bepinex::set_enabled(&game_root, &config.game, true)?;
        }
    }
    let result = open_steam(&config.game, &info);
    if result.is_ok() {
        log::info!("厨房启动请求已发送：正常 MOD 模式");
    } else if let Err(error) = &result {
        log::error!("厨房启动失败：正常 MOD 模式，原因={}", error);
    }
    result
}

#[tauri::command]
pub fn launch_no_mod(state: State<'_, AppState>) -> Result<OperationResult, String> {
    log::info!("启动厨房：无 MOD 模式");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let info = game::detect_game(&config);
    let game_root = PathBuf::from(info.path.as_deref().ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;
    if crate::bepinex::state(&game_root, &config.game) != WinhttpState::Missing {
        crate::bepinex::set_enabled(&game_root, &config.game, false)?;
    }
    let result = open_steam(&config.game, &info);
    if result.is_ok() {
        log::info!("厨房启动请求已发送：无 MOD 模式");
    } else if let Err(error) = &result {
        log::error!("厨房启动失败：无 MOD 模式，原因={}", error);
    }
    result
}

#[tauri::command]
pub fn restore_mod(state: State<'_, AppState>) -> Result<OperationResult, String> {
    log::info!("恢复框架状态");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;
    if crate::bepinex::state(&game_root, &config.game) == WinhttpState::ModDisabled {
        crate::bepinex::set_enabled(&game_root, &config.game, true)?;
    }
    log::info!("框架状态恢复完成");
    Ok(OperationResult {
        ok: true,
        message: "framework.stateRestored".into(),
    })
}

/// Switches the runtime's loader state without launching Steam. The overview
/// status card uses this command; actual game launch remains explicit.
#[tauri::command]
pub fn switch_winhttp(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<OperationResult, String> {
    log::info!("切换框架状态：启用={}", enabled);
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;
    /*
     * 未安装时直接拒绝。
     *
     * 没有核心程序集就没有可切换的加载器：禁用方向会去找
     * `winhttp.dll.disabled`（不存在），启用方向会去找 `winhttp.dll`
     * （也没有）。原生平台会去找 `run_bepinex.sh`。三种结果都是
     * 「未找到框架加载器」这种无从处理的报错。界面已经按状态隐藏开关，
     * 这里是防止 IPC 被直接调用的兜底。
     */
    if !crate::bepinex::installed(&game_root) {
        return Err("framework.notInstalledCannotToggle".into());
    }
    crate::bepinex::set_enabled(&game_root, &config.game, enabled)?;
    log::info!("框架状态切换完成：启用={}", enabled);
    Ok(OperationResult {
        ok: true,
        message: if enabled {
            "framework.enabledGameNotRunning".into()
        } else {
            "framework.disabledGameNotRunning".into()
        },
    })
}

fn open_steam(settings: &GameConfig, info: &GameInfo) -> Result<OperationResult, String> {
    if !info.game_exists {
        return Err("game.executableMissing".into());
    }
    game::steam_launch_command(settings, info)?;
    Ok(OperationResult {
        ok: true,
        message: "game.steamLaunchRequested".into(),
    })
}
