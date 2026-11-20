use crate::app_state::AppState;
use crate::archive;
use crate::game;
use crate::models::OperationResult;
use std::path::Path;
use tauri::State;

#[tauri::command]
pub fn open_external_page(url: String) -> Result<(), String> {
    crate::shell_open::open_web_page(&url, crate::shell_open::WebPageKind::External)
}

#[tauri::command]
pub fn open_devtools(window: tauri::WebviewWindow) -> Result<OperationResult, String> {
    log::info!("打开开发者工具");
    window.open_devtools();
    Ok(OperationResult {
        ok: true,
        message: "tools.devConsoleOpened".into(),
    })
}

#[tauri::command]
pub async fn exit_app() -> Result<(), String> {
    log::info!("收到退出应用请求");
    request_user_exit()?.await.map_err(|e| e.to_string())?
}

/// Both user exit paths retain the updater's exclusion through the final exit.
pub(crate) fn request_user_exit()
-> Result<tauri::async_runtime::JoinHandle<Result<(), String>>, String> {
    let exit = archive::begin_app_update()?;
    Ok(tauri::async_runtime::spawn_blocking(move || {
        let _exit = exit;
        crate::level_sets::recompress::shutdown().map_err(|error| {
            log::error!("地图资源恢复未完成，拒绝退出：{error}");
            "assetBundle.recoveryPending".to_string()
        })?;
        log::logger().flush();
        std::process::exit(0)
    }))
}

/// 当前进程的权限与可写性自检。
///
/// 应用不再启动即提权（否则 Windows UIPI 会阻止拖放、每次启动弹 UAC）。
/// 前端据此在设置页显示状态，并在真正遇到「拒绝访问」时提示用户
/// 以管理员身份重启。
#[derive(serde::Serialize)]
pub struct PermissionStatus {
    /// 当前进程是否具备管理员/root 权限。
    pub elevated: bool,
    /// 游戏 MOD 目录是否可写：需要提权的唯一真实判据。
    pub game_writable: bool,
    /// 管理器数据目录是否可写（用户文档 / Application Support 下的数据根）。
    pub data_writable: bool,
}

#[tauri::command]
pub fn permission_status(state: State<'_, AppState>) -> Result<PermissionStatus, String> {
    let config = state.config.load_or_default();
    let game_root = game::detect_game(&config).path;
    let game_writable = game_root
        .as_deref()
        .map(|path| probe_writable(&Path::new(path).join("BepInEx/plugins")))
        .unwrap_or(false);
    let data_writable = crate::user_paths::root()
        .map(|root| probe_writable(&root))
        .unwrap_or(false);
    Ok(PermissionStatus {
        elevated: crate::is_elevated(),
        game_writable,
        data_writable,
    })
}

/// 探测目录是否可写：创建并删除一个临时文件，不改动任何既有内容。
fn probe_writable(dir: &Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(format!(".oc2-write-probe-{}", uuid::Uuid::new_v4()));
    match std::fs::write(&probe, b"") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// 以管理员身份重启自己；只有新进程已确认接手时，当前进程才退出。
///
/// 只在文件访问真的被拒时由用户主动触发 —— 应用平时以普通权限运行，
/// 这样拖放可用、启动不弹 UAC、更新时能正常关闭自己。
#[tauri::command]
pub async fn relaunch_as_admin() -> Result<OperationResult, String> {
    if archive::app_update_in_progress() {
        return Err("update.inProgress".into());
    }
    log::info!("收到以管理员身份重启请求");
    if crate::is_elevated() {
        return Ok(OperationResult {
            ok: true,
            message: "update.alreadyElevated".into(),
        });
    }
    /*
     * 只有 Windows 有可用的提权重启路径（UAC runas）。macOS 没有 UAC，
     * 提权要经 sudo / Authorization Services，整进程重启这条路既做不到、
     * 也不该做：游戏目录和数据根都在用户目录下，本来就归当前用户所有；
     * 用 root 跑 GUI 会让 TCC 权限失效、配置与缓存变成 root 所有、更新器
     * 无法自替换。设置页在 macOS 上已不再显示这个按钮，这里兜住旧前端。
     */
    #[cfg(not(windows))]
    {
        log::info!("当前平台不需要也不支持提权重启");
        Ok(OperationResult {
            ok: false,
            message: "update.elevationUnsupported".into(),
        })
    }
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(|| relaunch_with(crate::elevation::launch))
            .await
            .map_err(|error| error.to_string())?
    }
}

#[cfg(windows)]
pub(crate) fn relaunch_with(
    launch: impl FnOnce() -> std::io::Result<()>,
) -> Result<OperationResult, String> {
    // Share the updater/data-migration gate while Windows is waiting for UAC.
    let _handoff = archive::begin_app_update()?;
    let _compression = crate::level_sets::recompress::quiesce_for_exit().map_err(|error| {
        log::error!("地图资源恢复未完成：{error}");
        "assetBundle.recoveryPending".to_string()
    })?;
    match launch() {
        Ok(()) => {
            log::info!("管理员实例已确认接手，退出当前进程以交接单实例锁");
            log::logger().flush();
            std::process::exit(0);
        }
        Err(error) => {
            log::warn!("管理员重启未交接，继续使用当前实例：{error}");
            Ok(OperationResult {
                ok: false,
                message: "update.elevationFailed".into(),
            })
        }
    }
}

/// Startup retries respect the persisted choice instead of re-detecting the OS.
#[tauri::command]
pub fn refresh_user_region(state: State<'_, AppState>) -> String {
    state.config.load_or_default().region
}

#[tauri::command]
pub async fn probe_startup_source(
    state: State<'_, AppState>,
) -> Result<crate::repository_sources::StartupProbe, String> {
    let config = state.config.clone();
    Ok(crate::repository_sources::probe_startup_source(&config).await)
}

#[tauri::command]
pub fn cancel_startup_probe() {
    crate::repository_sources::cancel_startup_probe();
}
