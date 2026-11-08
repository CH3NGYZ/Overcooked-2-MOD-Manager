use crate::app_state::AppState;
use crate::archive;
use crate::game;
use crate::models::OperationResult;
use std::path::Path;
use tauri::State;

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
pub fn exit_app() -> Result<(), String> {
    if archive::app_update_in_progress() {
        return Err("update.inProgress".into());
    }
    log::info!("收到退出应用请求");
    std::process::exit(0)
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

/// 以管理员身份重启自己；成功后本进程立即退出。
///
/// 只在文件访问真的被拒时由用户主动触发 —— 应用平时以普通权限运行，
/// 这样拖放可用、启动不弹 UAC、更新时能正常关闭自己。
#[tauri::command]
pub fn relaunch_as_admin() -> Result<OperationResult, String> {
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
    if !cfg!(windows) {
        log::info!("当前平台不需要也不支持提权重启");
        return Ok(OperationResult {
            ok: false,
            message: "update.elevationUnsupported".into(),
        });
    }
    if crate::relaunch_elevated() {
        log::info!("已启动管理员实例，退出当前进程");
        // 给新进程一点时间接管
        std::thread::sleep(std::time::Duration::from_millis(200));
        std::process::exit(0);
    }
    Ok(OperationResult {
        ok: false,
        message: "update.elevationFailed".into(),
    })
}

/// Recheck after the tutorial rather than reuse the startup config snapshot.
#[tauri::command]
pub fn refresh_user_region() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        crate::user_region::refresh()
    }
    #[cfg(not(target_os = "macos"))]
    {
        crate::config::current_user_region()
    }
}

#[tauri::command]
pub async fn probe_startup_source() -> crate::repository_sources::StartupProbe {
    crate::repository_sources::probe_startup_source().await
}

#[tauri::command]
pub fn cancel_startup_probe() {
    crate::repository_sources::cancel_startup_probe();
}
