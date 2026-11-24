use crate::app_state::AppState;
use crate::app_update::staged::UpdateOnExit;
use crate::archive;
use crate::game;
use crate::models::OperationResult;
use std::path::Path;
use std::time::Duration;
use tauri::{AppHandle, State};

const UPDATE_DOWNLOAD_EXIT_TIMEOUT: Duration = Duration::from_secs(2);

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
pub async fn exit_app(
    app: AppHandle,
    state: State<'_, AppState>,
    install_update: bool,
) -> Result<(), String> {
    log::info!("收到退出应用请求");
    let update = if install_update {
        UpdateOnExit::Install
    } else {
        UpdateOnExit::Skip
    };
    request_user_exit(app, state.inner().clone(), ExitAction::Update(update)).await
}

#[tauri::command]
pub async fn apply_staged_app_update(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    request_user_exit(
        app,
        state.inner().clone(),
        ExitAction::Update(UpdateOnExit::Restart),
    )
    .await
}

pub(super) enum ExitAction {
    Update(UpdateOnExit),
    OpenRecoveryPage(String),
}

pub(super) async fn request_user_exit(
    app: AppHandle,
    state: AppState,
    action: ExitAction,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _exit = archive::begin_app_update()?;
        let closing = state.app_updates.closing()?;
        let compression = crate::level_sets::recompress::quiesce_for_exit();
        let _operation = state.operation.lock().map_err(|_| "lock.poisoned")?;
        restore_map_compression_for_exit(&compression, &state)?;
        if !closing.wait_for_download(UPDATE_DOWNLOAD_EXIT_TIMEOUT)? {
            log::warn!("更新暂存取消未在退出期限内完成，继续退出");
        }
        match action {
            ExitAction::Update(update) => {
                if update != UpdateOnExit::Skip && closing.has_ready()? {
                    crate::mod_changes::guard::ensure_recovered(&state)?;
                }
                let restart = update == UpdateOnExit::Restart;
                if let Some(staged) = closing.take_ready(update)?
                    && let Err(error) = staged.handoff(&app, restart)
                {
                    if restart {
                        return Err(error);
                    }
                    log::error!("退出时未能交接或应用更新：{error}");
                }
                log::info!("管理器退出条件已满足：应用更新后重启={restart}");
            }
            ExitAction::OpenRecoveryPage(url) => {
                crate::shell_open::open_web_page(&url, crate::shell_open::WebPageKind::Release)?;
                log::info!("已交接更新恢复网页，关闭当前管理器");
            }
        }
        exit_process(&app)
    })
    .await
    .map_err(|error| error.to_string())?
}

fn exit_process(app: &AppHandle) -> ! {
    tauri_plugin_single_instance::destroy(app);
    log::logger().flush();
    std::process::exit(0)
}

fn restore_map_compression_for_exit(
    pause: &crate::level_sets::recompress::ExitPause,
    state: &AppState,
) -> Result<(), String> {
    pause.restore(state).map_err(|error| {
        log::error!("地图资源恢复未完成，拒绝退出：{error}");
        "assetBundle.recoveryPending".into()
    })
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
        .map(|path| probe_writable(&Path::new(path).join("BepInEx").join("plugins")))
        .unwrap_or(false);
    let data_root = crate::user_paths::root();
    let data_writable = match &data_root {
        Ok(root) => probe_writable(root),
        Err(error) => {
            log::warn!("[permission] 无法确定数据目录，未探测可写性：原因={error}");
            false
        }
    };
    let status = PermissionStatus {
        elevated: crate::is_elevated(),
        game_writable,
        data_writable,
    };
    log::info!(
        "[permission] 可写性探测：管理员权限={}，游戏目录={game_root:?}，插件目录可写={}，数据目录={:?}，数据目录可写={}",
        status.elevated,
        status.game_writable,
        data_root.as_ref().ok(),
        status.data_writable,
    );
    Ok(status)
}

/// 探测目录是否可写：创建并删除一个临时文件，不改动任何既有内容。
fn probe_writable(dir: &Path) -> bool {
    if let Err(error) = std::fs::create_dir_all(dir) {
        log_permission_probe_error("create_directory", dir, &error);
        return false;
    }
    let probe = dir.join(format!(".oc2-write-probe-{}", uuid::Uuid::new_v4()));
    match std::fs::write(&probe, b"") {
        Ok(()) => {
            if let Err(error) = std::fs::remove_file(&probe) {
                log_permission_probe_error("remove_probe", &probe, &error);
            }
            true
        }
        Err(error) => {
            log_permission_probe_error("write_probe", &probe, &error);
            false
        }
    }
}

fn log_permission_probe_error(stage: &str, path: &Path, error: &std::io::Error) {
    log::warn!(
        "[permission] 可写性探测遇到文件操作错误：stage={stage}，path={path:?}，kind={:?}，os_code={:?}，reason={error}",
        error.kind(),
        error.raw_os_error(),
    );
}

#[cfg(test)]
mod permission_diagnostics_tests {
    use super::*;

    #[test]
    fn permission_probe_reports_the_exact_blocked_directory() {
        let root = tempfile::tempdir().unwrap();
        let blocked = root.path().join("plugins-as-file");
        std::fs::write(&blocked, "private fixture contents").unwrap();
        let needle = root.path().file_name().unwrap().to_str().unwrap();
        let capture =
            crate::http_diagnostics::capture_test_logs("app_lib::commands::system", needle);
        assert!(!probe_writable(&blocked));
        let messages = capture.messages().join("\n");
        assert!(messages.contains("stage=create_directory"), "{messages}");
        assert!(messages.contains("plugins-as-file"), "{messages}");
        assert!(messages.contains("os_code=Some("), "{messages}");
        assert!(!messages.contains("private fixture contents"));
    }
}

/// 以管理员身份重启自己；只有新进程已确认接手时，当前进程才退出。
///
/// 只在文件访问真的被拒时由用户主动触发 —— 应用平时以普通权限运行，
/// 这样拖放可用、启动不弹 UAC、更新时能正常关闭自己。
#[tauri::command]
pub async fn relaunch_as_admin(state: State<'_, AppState>) -> Result<OperationResult, String> {
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
        let _ = state;
        log::info!("当前平台不需要也不支持提权重启");
        Ok(OperationResult {
            ok: false,
            message: "update.elevationUnsupported".into(),
        })
    }
    #[cfg(windows)]
    {
        let state = state.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            relaunch_with(&state, crate::elevation::launch)
        })
        .await
        .map_err(|error| error.to_string())?
    }
}

#[cfg(windows)]
pub(crate) fn relaunch_with(
    state: &AppState,
    launch: impl FnOnce() -> std::io::Result<()>,
) -> Result<OperationResult, String> {
    // Share the updater/data-migration gate while Windows is waiting for UAC.
    let _handoff = archive::begin_app_update()?;
    let compression = crate::level_sets::recompress::quiesce_for_exit();
    let _operation = state.operation.lock().map_err(|_| "lock.poisoned")?;
    restore_map_compression_for_exit(&compression, state)?;
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
