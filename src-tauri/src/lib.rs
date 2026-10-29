mod api_rate_gate;
mod app_update;
mod archive;
mod avatars;
mod bepinex;
mod commands;
mod community;
mod config;
mod direct_mod;
mod download;
mod game;
mod game_state;
mod gitee_community;
mod gitee_webview;
mod github_proxies;
mod install_guard;
mod mod_config;
mod mod_preview;
mod models;
mod mods;
mod recycle_bin;
mod releases;
mod repository_readme;
mod repository_sources;
mod update_migration;
mod user_paths;
#[cfg(any(target_os = "macos", test))]
mod user_region;
#[cfg(any(unix, test))]
mod wine;

use chrono::Utc;
use std::fs::{self, OpenOptions};
use std::io::Write;
use tauri::Manager;
use tauri_plugin_log::{RotationStrategy, Target, TargetKind};

pub fn run() {
    install_guard::run_if_requested();
    update_migration::run_if_requested();
    install_panic_logger();
    /*
     * 不再在启动时无条件自提权。
     *
     * 提权会给整个应用带来三个副作用：
     *   1. Windows UIPI 阻止跨完整性级别的 OLE 拖放 —— 提权窗口收不到任何
     *      文件拖放事件（资源管理器是 Medium，本进程是 High）；
     *   2. 每次启动都弹 UAC；
     *   3. 退出时普通权限无法结束自己，更新前必须先让用户手动关闭。
     *
     * 现在改为按需提权：安装器默认 perUser 装到用户目录，游戏 MOD 目录
     * 通常也可直接写入；只有真的遇到「拒绝访问」时，才由前端提示用户
     * 以管理员身份重启（见 `request_elevation` 命令）。
     */
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|_, _, _| {}))
        .plugin(tauri_plugin_dialog::init())
        .on_window_event(gitee_webview::close_with_main)
        .plugin(
            tauri_plugin_updater::Builder::new()
                .header("User-Agent", releases::APP_USER_AGENT)
                .expect("valid updater User-Agent")
                .build(),
        )
        .setup(|app| {
            let log_dir = user_paths::logs_dir()?;
            fs::create_dir_all(&log_dir)?;
            let log_path = log_dir.clone();
            app.handle().plugin(
                tauri_plugin_log::Builder::default()
                    .targets([
                        Target::new(TargetKind::Stdout),
                        Target::new(TargetKind::Folder {
                            path: log_dir,
                            file_name: Some("rust".into()),
                        }),
                    ])
                    // Keep trace-level stage details in rust.log so diagnostics can
                    // reconstruct a complete operation without exposing secrets.
                    .level(log::LevelFilter::Trace)
                    .level_for("tao", log::LevelFilter::Info)
                    // The plugin's 40 KiB/KeepOne defaults discard startup JS
                    // requests quickly when detailed runtime logging is on.
                    .max_file_size(8 * 1024 * 1024)
                    .rotation_strategy(RotationStrategy::KeepSome(5))
                    .build(),
            )?;
            let state = commands::AppState::new().map_err(|error| {
                log::error!("初始化应用状态失败：{error}");
                std::io::Error::other(error.to_string())
            })?;
            app.manage(state);
            log::info!(
                "OC2 MOD Manager started; version={}",
                env!("CARGO_PKG_VERSION")
            );
            if let Ok(executable) = std::env::current_exe() {
                log::info!(
                    "当前运行程序={}，Rust 日志目录={}",
                    executable.display(),
                    log_path.display()
                );
            }
            let region = config::current_user_region();
            match repository_sources::github_proxy_sources(region) {
                Ok(sources) => {
                    let scripts = sources.iter().filter(|source| source.format == github_proxies::ProxySourceFormat::UserScript).collect::<Vec<_>>();
                    log::info!("GitHub 代理 JS 启动同步已排队：地区={}，JS 来源数={}，等待界面初始化及教程结束", region, scripts.len());
                    for source in scripts {
                        log::info!("GitHub 代理 JS 待请求地址={}", repository_sources::safe_log_address(&source.url));
                    }
                }
                Err(error) => log::error!("GitHub 代理 JS 启动配置读取失败：错误类别={}", repository_sources::error_category(&error)),
            }
            set_initial_window_size(app);
            present_startup_window(app);
            game_state::start_observer(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_dashboard,
            commands::get_config,
            commands::get_install_language,
            commands::read_rust_log,
            commands::read_frontend_log,
            commands::read_crash_log,
            commands::write_frontend_log,
            commands::save_mod_note,
            commands::install_local_mod,
            direct_mod::install_online_mod,
            mod_preview::analyze_mod_source,
            mod_preview::confirm_mod_install,
            mod_preview::cancel_mod_preview,
            commands::uninstall_local_mod,
            commands::uninstall_mod_dll,
            commands::get_mod_recycle_bin,
            commands::restore_recycled_mod,
            commands::delete_recycled_mod,
            commands::delete_all_recycled_mods,
            commands::toggle_local_mod,
            commands::toggle_local_mod_package,
            commands::set_all_local_mods_enabled,
            commands::save_settings,
            commands::save_github_proxies,
            commands::refresh_github_proxies,
            commands::save_tokens,
            commands::detect_game,
            commands::check_crossover_running,
            commands::set_game_path,
            commands::reset_game_path,
            commands::refresh_repository,
            community::refresh_community_repositories,
            community::cancel_community_search,
            repository_readme::get_repository_readme,
            repository_readme::cancel_repository_readme,
            commands::get_repository_releases,
            commands::install_bepinex,
            commands::repair_native_bepinex_overlay,
            commands::uninstall_bepinex,
            commands::backup_bepinex,
            commands::restore_bepinex,
            commands::delete_bepinex_backup,
            commands::install_repository,
            commands::cancel_download,
            commands::uninstall_repository,
            commands::export_repositories,
            commands::export_repositories_to_file,
            commands::copy_repositories,
            commands::read_clipboard,
            commands::copy_text,
            commands::push_colorful_name,
            commands::get_builtin_tutorial,
            commands::get_builtin_tutorial_repositories,
            commands::parse_tutorial_config,
            commands::serialize_tutorial_config,
            commands::read_text_file,
            commands::preview_repository_import,
            commands::import_repositories,
            commands::clear_repository_source_cache,
            commands::import_repository_source,
            commands::save_repository_source,
            commands::save_repository_sources,
            commands::delete_repository_source,
            commands::reorder_repository_sources,
            commands::save_repository,
            commands::delete_repository,
            commands::launch_normal,
            commands::stop_game,
            commands::launch_no_mod,
            commands::restore_mod,
            commands::switch_winhttp,
            commands::open_game_directory,
            commands::open_mod_directory,
            commands::open_bepinex_directory,
            commands::get_repository_avatars,
            commands::clear_repository_avatars,
            commands::get_diy_level_info,
            commands::set_diy_level_enabled,
            commands::open_diy_level_directory,
            commands::backup_diy_level,
            commands::restore_diy_level,
            commands::delete_diy_level_backup,
            commands::uninstall_diy_level,
            commands::open_mod_file_location,
            commands::open_mod_logs_directory,
            commands::read_mod_config,
            commands::save_mod_config,
            commands::read_bepinex_config,
            commands::save_bepinex_config,
            commands::open_backup_directory,
            commands::open_release_page,
            commands::open_update_repository,
            commands::exit_app,
            commands::open_devtools,
            commands::reset_ui,
            commands::permission_status,
            commands::relaunch_as_admin,
            commands::check_app_update,
            commands::install_app_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while building tauri application");
}

/// Start the manager at 80% of the current monitor's work area while keeping
/// the configured 1280x720 minimum. This runs once during startup; later UI
/// zoom changes are handled inside the webview and never resize the window.
fn set_initial_window_size<R: tauri::Runtime>(app: &tauri::App<R>) {
    let Some(window) = app.get_webview_window("main") else {
        log::warn!("Unable to find the main window while applying startup size");
        return;
    };
    if window.is_maximized().unwrap_or(false) {
        return;
    }
    let Ok(Some(monitor)) = window.current_monitor() else {
        log::warn!("Unable to detect the current monitor while applying startup size");
        return;
    };
    let work_area = monitor.work_area();
    let scale_factor = monitor.scale_factor().max(1.0);
    let width = (f64::from(work_area.size.width) * 0.8 / scale_factor).max(1280.0);
    let height = (f64::from(work_area.size.height) * 0.8 / scale_factor).max(720.0);
    if let Err(error) = window.set_size(tauri::LogicalSize::new(width, height)) {
        log::warn!("Unable to apply startup window size: {error}");
        return;
    }
    if let Err(error) = window.center() {
        log::warn!("Unable to center startup window: {error}");
    }
}

/// Bring the manager to the foreground once during startup without changing
/// its normal z-order. `show` and `set_focus` only request foreground focus;
/// the window is never made always-on-top.
fn present_startup_window<R: tauri::Runtime>(app: &tauri::App<R>) {
    let Some(window) = app.get_webview_window("main") else {
        log::warn!("Unable to find the main window while presenting it at startup");
        return;
    };
    if let Err(error) = window.unminimize() {
        log::debug!("Unable to restore the main window before startup focus: {error}");
    }
    if let Err(error) = window.show() {
        log::warn!("Unable to show the main window at startup: {error}");
    }
    if let Err(error) = window.set_focus() {
        log::warn!("Unable to focus the main window at startup: {error}");
    }
}

fn install_panic_logger() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        previous(panic);
        let Ok(log_dir) = user_paths::logs_dir() else {
            return;
        };
        if fs::create_dir_all(&log_dir).is_err() {
            return;
        }
        let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_dir.join("crash.log"))
        else {
            return;
        };
        let _ = writeln!(
            file,
            "[{}][panic] {}",
            Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
            panic
        );
    }));
}

/// 当前进程是否已具备管理员权限。
#[cfg(windows)]
pub(crate) fn is_elevated() -> bool {
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn IsUserAnAdmin() -> i32;
    }
    unsafe { IsUserAnAdmin() != 0 }
}

#[cfg(not(windows))]
pub(crate) fn is_elevated() -> bool {
    // 非 Windows 没有 UAC；用 euid 判断是否 root。
    #[cfg(unix)]
    {
        extern "C" {
            fn geteuid() -> u32;
        }
        unsafe { geteuid() == 0 }
    }
    #[cfg(not(unix))]
    {
        false
    }
}

/// 以管理员身份重启自己（仅 Windows 的 UAC 提权有意义）。
///
/// 返回 true 表示新进程已启动、本进程应当退出；false 表示用户取消了 UAC
/// 或启动失败，调用方应保留当前窗口并提示用户。
#[cfg(windows)]
pub(crate) fn relaunch_elevated() -> bool {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null_mut;
    use winapi::um::shellapi::ShellExecuteW;
    use winapi::um::winuser::SW_SHOWNORMAL;
    if is_elevated() {
        return false;
    }
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let wide = |value: &std::ffi::OsStr| {
        value
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<u16>>()
    };
    let exe_w = wide(exe.as_os_str());
    let verb_w = wide(std::ffi::OsStr::new("runas"));
    let result = unsafe {
        ShellExecuteW(
            null_mut(),
            verb_w.as_ptr(),
            exe_w.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    (result as usize) > 32
}

#[cfg(not(windows))]
pub(crate) fn relaunch_elevated() -> bool {
    false
}
