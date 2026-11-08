mod api_rate_gate;
mod app_state;
mod app_update;
mod archive;
// DIYLevel 关卡集的 info 是 Unity AssetBundle，关卡集名/作者/版本只能从里面解析
mod asset_bundle;
mod avatars;
mod bepinex;
mod commands;
mod community;
mod config;
mod direct_mod;
// 展示用路径归一化：送给前端的路径只从这里出口，不参与 I/O
mod display_path;
mod download;
mod file_metadata;
mod framework;
mod game;
mod game_state;
mod gitee_community;
mod gitee_webview;
mod github_proxies;
mod github_proxy_runtime;
mod install_guard;
// 地图（DIYLevel 关卡集）的扫描、开关与 ZIP 导入
mod level_sets;
// 关卡集目录的外部变更监听：资源管理器里增删目录后让列表自动刷新
mod level_watch;
mod locale_tag;
// 地图仓库源（与 repository_sources 对称：源树、相对路径、Release 刷新）
mod map_sources;
// Release note 里的地图元数据块（`<!-- oc2-map ... -->`）：一个仓库放多张地图时
// 每张地图的信息跟着自己的 Release 走，不再需要一份集中式 TOML
mod map_note;
// 社区地图发布：GitHub App 网页授权、安装范围查询与 Release / 附件写入
mod map_publish;
// 元数据并行读取的中间层：MOD 的 DLL 与地图的 info 都走它编排
mod metadata_read;
mod mod_config;
mod mod_preview;
mod models;
mod mods;
mod recycle_bin;
mod release_cache;
mod releases;
mod repository_readme;
mod repository_sources;
// 系统 Shell 交互：打开目录/URL 与在文件管理器中定位（不硬编码 explorer.exe）
mod shell_open;
mod update_migration;
mod user_paths;
// 版本区间语法（`version = ">=1.8,<2"`）：std-only，工具 `tools/oc2-info` 也复用
#[cfg(any(target_os = "macos", test))]
mod user_region;
mod version_range;
#[cfg(any(unix, test))]
mod wine;

use chrono::Utc;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
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
     * 以管理员身份重启（见 `relaunch_as_admin` 命令）。
     */
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|_, _, _| {}))
        .plugin(tauri_plugin_dialog::init())
        .on_window_event(|window, event| {
            if window.label() == "main" && archive::app_update_in_progress() {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                }
            }
            gitee_webview::close_with_main(window, event);
        })
        .plugin(
            tauri_plugin_updater::Builder::new()
                .header("User-Agent", releases::APP_USER_AGENT)
                .expect("valid updater User-Agent")
                .build(),
        )
        .setup(|app| {
            // 旧版本把数据放在安装目录里；先搬走再建日志目录，
            // 这样同一卷上一次 rename 就能完成迁移。
            let migration = user_paths::migrate_legacy_root();
            let data_root = user_paths::root()?;
            let log_dir = data_root.join("logs");
            fs::create_dir_all(&log_dir)
                .map_err(|error| format!("{}: {error}", log_dir.display()))?;
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
            let state = app_state::AppState::new().map_err(|error| {
                log::error!("初始化应用状态失败：{error}");
                std::io::Error::other(error.to_string())
            })?;
            let proxy_config = state.config.clone();
            app.manage(state);
            tauri::async_runtime::spawn(async move {
                if proxy_config.refresh_proxies().await.is_err() {
                    log::warn!("启动线上代理清单不可用，继续使用随包清单");
                }
            });
            // 迁移发生在日志插件初始化之前，结果在这里补记。
            match migration {
                user_paths::LegacyMigration::Moved { legacy, target } => log::info!(
                    "已将旧数据目录迁移到新位置：旧={}，新={}",
                    legacy.display(),
                    target.display()
                ),
                user_paths::LegacyMigration::Failed {
                    legacy,
                    target,
                    error,
                } => log::warn!(
                    "旧数据目录迁移未完成，继续使用新位置：旧={}，新={}，原因={}",
                    legacy.display(),
                    target.display(),
                    error
                ),
                user_paths::LegacyMigration::None => {}
            }
            log::info!(
                "OC2 MOD Manager started; version={}",
                env!("CARGO_PKG_VERSION")
            );
            if let Ok(executable) = std::env::current_exe() {
                log::info!(
                    "当前运行程序={}，数据根={}，Rust 日志目录={}",
                    executable.display(),
                    data_root.display(),
                    log_path.display()
                );
            }
            set_initial_window_size(app);
            present_startup_window(app);
            game_state::start_observer(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::dashboard::get_dashboard,
            commands::settings::get_config,
            commands::settings::get_install_language,
            commands::logs::read_rust_log,
            commands::logs::read_frontend_log,
            commands::logs::read_crash_log,
            commands::logs::clear_log,
            commands::logs::write_frontend_log,
            commands::mods_local::save_mod_note,
            commands::mods_local::install_local_mod,
            direct_mod::install_online_mod,
            mod_preview::commands::analyze_mod_source,
            mod_preview::commands::confirm_mod_install,
            mod_preview::commands::cancel_mod_preview,
            commands::mods_local::uninstall_local_mod,
            commands::mods_local::uninstall_mod_dll,
            commands::recycle::get_mod_recycle_bin,
            commands::recycle::restore_recycled_mod,
            commands::recycle::delete_recycled_mod,
            commands::recycle::delete_all_recycled_mods,
            commands::mods_local::toggle_local_mod,
            commands::mods_local::toggle_local_mod_package,
            commands::mods_local::set_all_local_mods_enabled,
            commands::settings::save_settings,
            commands::settings::refresh_github_proxies,
            commands::settings::save_tokens,
            commands::game::detect_game,
            commands::game::check_crossover_running,
            commands::game::set_game_path,
            commands::game::reset_game_path,
            commands::repository_releases::refresh_repository,
            commands::relations::sync_dependency_snapshots,
            community::commands::refresh_community_repositories,
            community::commands::cancel_community_search,
            map_publish::authorization::start_github_authorization,
            map_publish::authorization::poll_github_authorization,
            map_publish::authorization::cancel_github_authorization,
            map_publish::session::map_publish_state,
            map_publish::session::check_publish_session,
            map_publish::session::sign_out_map_publish,
            map_publish::repositories::list_publish_repositories,
            map_publish::releases::list_publish_releases,
            map_publish::releases::save_publish_release,
            map_publish::releases::delete_publish_release,
            map_publish::assets::upload_publish_asset,
            map_publish::assets::delete_publish_asset,
            map_publish::zip::inspect_publish_zip,
            map_publish::zip::stage_publish_zip,
            map_publish::repositories::publish_repository_topics,
            repository_readme::get_repository_readme,
            repository_readme::cancel_repository_readme,
            commands::repository_releases::get_repository_releases,
            commands::bepinex::install_bepinex,
            commands::bepinex::repair_native_bepinex_overlay,
            commands::bepinex::uninstall_bepinex,
            commands::bepinex::backup_bepinex,
            commands::bepinex::restore_bepinex,
            commands::bepinex::delete_bepinex_backup,
            commands::repository_install::install_repository,
            commands::download::cancel_download,
            commands::repository_install::uninstall_repository,
            commands::repositories::export_repositories,
            commands::repositories::export_repositories_to_file,
            commands::repositories::copy_repositories,
            commands::clipboard::read_clipboard,
            commands::clipboard::copy_text,
            commands::colorful_name::push_colorful_name,
            commands::repositories::preview_repository_import,
            commands::repositories::import_repositories,
            commands::repository_sources::clear_repository_source_cache,
            commands::repository_sources::import_repository_source,
            commands::repository_sources::save_repository_source,
            commands::repository_sources::save_repository_sources,
            commands::repository_sources::delete_repository_source,
            commands::repository_sources::reorder_repository_sources,
            commands::repositories::save_repository,
            commands::repositories::delete_repository,
            commands::game::launch_normal,
            commands::game::stop_game,
            commands::game::refresh_game_state,
            commands::game::launch_no_mod,
            commands::game::restore_mod,
            commands::game::switch_winhttp,
            commands::paths::open_game_directory,
            commands::paths::open_mod_directory,
            commands::paths::open_bepinex_directory,
            commands::avatars::get_repository_avatars,
            commands::avatars::clear_repository_avatars,
            commands::diy_level::get_diy_level_info,
            commands::levels::get_level_sets,
            commands::levels::set_level_set_enabled,
            commands::levels::set_all_level_sets_enabled,
            commands::drop_source::classify_drop_source,
            commands::map_install::import_level_sets,
            commands::levels::open_level_set_directory,
            commands::levels::delete_level_set,
            commands::levels::flatten_level_sets,
            commands::levels::clear_level_set_cache,
            commands::maps::get_maps,
            commands::maps::refresh_map_sources,
            commands::maps::refresh_map_releases,
            commands::map_install::stage_map_release,
            commands::map_install::stage_map_source,
            commands::map_install::discard_map_source,
            commands::diy_level::set_diy_level_enabled,
            commands::diy_level::open_diy_level_directory,
            commands::diy_level::backup_diy_level,
            commands::diy_level::restore_diy_level,
            commands::diy_level::delete_diy_level_backup,
            commands::diy_level::uninstall_diy_level,
            commands::mod_settings::open_mod_file_location,
            commands::mod_settings::open_mod_logs_directory,
            commands::mod_settings::read_mod_config,
            commands::mod_settings::save_mod_config,
            commands::mod_settings::read_bepinex_config,
            commands::mod_settings::save_bepinex_config,
            commands::paths::open_backup_directory,
            commands::update::open_release_page,
            commands::update::open_update_repository,
            commands::system::exit_app,
            commands::system::open_devtools,
            commands::settings::reset_ui,
            commands::system::permission_status,
            commands::system::relaunch_as_admin,
            commands::update::check_app_update,
            commands::update::install_app_update,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_, event| {
            if let tauri::RunEvent::ExitRequested {
                code: None, api, ..
            } = event
            {
                if archive::app_update_in_progress() {
                    api.prevent_exit();
                }
            }
        });
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
        let message = panic.to_string();
        // 主路径：数据根里的 logs/crash.log。
        if let Ok(log_dir) = user_paths::logs_dir() {
            if append_panic_line(&log_dir.join("crash.log"), &message).is_ok() {
                return;
            }
        }
        // 兜底：数据目录本身不可用时，至少把崩溃写进临时目录。
        // 旧实现在这里直接 `return`，于是 macOS 上「启动即崩、没有任何日志」。
        let fallback = std::env::temp_dir().join(PANIC_LOG_FALLBACK);
        if let Err(error) = append_panic_line(&fallback, &message) {
            eprintln!("panic 日志写入失败：{}（{error}）", fallback.display());
        }
    }));
}

/// 数据目录不可用时的崩溃日志文件名（位于系统临时目录）。
const PANIC_LOG_FALLBACK: &str = "oc2-modmanager-crash.log";

/// 追加一条 panic 记录，必要时创建父目录。
fn append_panic_line(path: &Path, message: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(
        file,
        "[{}][panic] {}",
        Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
        message
    )
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
        // Rust 2024 要求 extern 块自身标 unsafe（旧写法只在 Windows 之外编译，
        // 本机 Windows 检查不到，macOS 目标第一次跑 CI 就会红）。
        unsafe extern "C" {
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

#[cfg(test)]
#[path = "lib/tests.rs"]
mod tests;
