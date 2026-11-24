// Cargo's rustc-link-arg-tests covers integration tests, not this unit-test harness.
// Reuse Tauri's generated resource (and native search path) only for this harness;
// application binaries already link it through tauri_build.
#[cfg(all(test, target_os = "windows", target_env = "msvc"))]
#[link(name = "resource", kind = "static")]
unsafe extern "C" {}

mod api_rate_gate;
mod api_scheduler;
mod app_state;
mod app_update;
mod archive;
// DIYLevel 关卡集的 info 是 Unity AssetBundle，关卡集名/作者/版本只能从里面解析
mod asset_bundle;
mod avatars;
mod bepinex;
mod commands;
mod community;
mod concurrency;
mod config;
mod direct_mod;
mod diychef;
// 展示用路径归一化：送给前端的路径只从这里出口，不参与 I/O
mod display_path;
mod download;
#[cfg(windows)]
mod elevation;
mod file_check;
mod file_metadata;
mod framework;
mod game;
mod game_state;
mod gitee_community;
mod gitee_webview;
mod github_proxies;
mod github_proxy_runtime;
mod http_diagnostics;
mod http_user_agent;
mod install_guard;
// 地图（DIYLevel 关卡集）的扫描、开关与 ZIP 导入
mod level_sets;
// 关卡集目录的外部变更监听：资源管理器里增删目录后让列表自动刷新
mod directory_watch;
mod level_watch;
mod locale_tag;
mod main_webview;
mod manager_data;
// 地图仓库源（与 repository_sources 对称：源树、相对路径、Release 刷新）
mod map_sources;
// Release note 里的地图元数据块（`<!-- oc2-map ... -->`）：一个仓库放多张地图时
// 每张地图的信息跟着自己的 Release 走，不再需要一份集中式 TOML
mod map_note;
// 社区地图发布：GitHub App 网页授权、安装范围查询与 Release / 附件写入
mod map_publish;
// 元数据并行读取的中间层：MOD 的 DLL 与地图的 info 都走它编排
mod metadata_read;
mod mod_changes;
mod mod_config;
mod mod_preview;
mod models;
mod mods;
mod network_policy;
mod providers;
mod recycle_bin;
mod release_cache;
mod releases;
mod repository_readme;
mod repository_sources;
// 系统 Shell 交互：打开目录/URL 与在文件管理器中定位（不硬编码 explorer.exe）
mod shell_open;
mod unity_assets;
mod update_migration;
mod user_paths;
mod webview_download;
// 版本区间语法（`version = ">=1.8,<2"`）：std-only，工具 `tools/oc2-info` 也复用
#[cfg(any(target_os = "macos", test))]
mod user_region;
mod version_range;
#[cfg(any(unix, test))]
mod wine;

use tauri::{Emitter, Manager};
use tauri_plugin_log::{Target, TargetKind};

macro_rules! audited_handler {
    ($($command:path),* $(,)?) => {
        commands::logs::trace_ipc(tauri::generate_handler![$($command),*], &[$(stringify!($command)),*])
    };
}

pub fn run() {
    #[cfg(windows)]
    app_update::staged::windows::run_if_requested();
    install_guard::run_if_requested();
    update_migration::run_if_requested();
    manager_data::run_if_requested();
    #[cfg(windows)]
    if !elevation::complete_handoff() {
        return;
    }
    let _data_session = match manager_data::initialize() {
        Ok(session) => session,
        Err(error) => {
            manager_data::show_startup_error(&error);
            return;
        }
    };
    let skip_legacy_migration = _data_session.skip_legacy_migration;
    let data_startup = _data_session.startup_diagnostics.clone();
    let data_operation_completed = _data_session.completed_operation_diagnostics.clone();
    let data_operation_failure = _data_session.failure_diagnostics.clone();
    let data_helper_failure = _data_session.helper_failure_diagnostics.clone();
    commands::logs::install_panic_logger();
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
    let context = tauri::generate_context!();
    let preference_reset =
        manager_data::webview_reset_script(_data_session.completed_reset_token, context.config());
    let mut builder = tauri::Builder::default()
        .append_invoke_initialization_script(preference_reset)
        .plugin(tauri_plugin_single_instance::init(|_, _, _| {}))
        .plugin(tauri_plugin_dialog::init())
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                }
                if let tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) =
                    event
                {
                    commands::logs::trace_native_drop(paths);
                }
            }
            // Keep the main window alive even before the frontend close owner
            // has registered. App confirms drafts, then calls exit_app; that
            // command restores pending map files before allowing process exit.
            gitee_webview::close_with_main(window, event);
            webview_download::close_with_main(window, event);
        });
    // An MSIX package must never expose updater plugin IPCs that can launch an
    // independent installer. Keep the same policy as check/install commands.
    match app_update::distribution::current() {
        Ok(models::AppDistribution::Standalone) => {
            builder = builder.plugin(
                tauri_plugin_updater::Builder::new()
                    .header("User-Agent", http_user_agent::app_user_agent())
                    .expect("valid updater User-Agent")
                    .build(),
            );
        }
        Ok(models::AppDistribution::MicrosoftStore) => {}
        Err(error) => eprintln!("Independent updater disabled: {error}"),
    }
    builder
        .setup(move |app| {
            // 旧版本把数据放在安装目录里；先搬走再建日志目录，
            // 这样同一卷上一次 rename 就能完成迁移。
            let migration = if skip_legacy_migration {
                user_paths::LegacyMigration::None
            } else {
                user_paths::migrate_legacy_root()
            };
            let data_root = user_paths::root()?;
            let logs_directory = user_paths::logs_dir()?;
            let history_target = commands::logs::start(&logs_directory, &data_root)?;
            app.handle().plugin(
                tauri_plugin_log::Builder::default()
                    .targets([Target::new(TargetKind::Stdout), history_target])
                    .level(log::LevelFilter::Trace)
                    .level_for("tao", log::LevelFilter::Info)
                    .level_for("tauri_plugin_updater", log::LevelFilter::Off)
                    .build(),
            )?;
            log::info!("{data_startup}");
            if let Some(diagnostic) = &data_operation_completed {
                log::info!("{diagnostic}");
            }
            if let Some(diagnostic) = &data_operation_failure {
                log::warn!("{diagnostic}");
            }
            if let Some(diagnostic) = &data_helper_failure {
                log::warn!("{diagnostic}");
            }
            let state = app_state::AppState::new().map_err(|error| {
                log::error!(
                    "初始化应用状态失败：原因={}",
                    crate::locale_tag::diagnostic_tag(&error)
                );
                std::io::Error::other(error.to_string())
            })?;
            let proxy_config = state.config.clone();
            // Bind native downloads before exposing state to startup commands.
            proxy_config.set_source_browser(app.handle().clone());
            let mut provider_changes = state.config.data_selection_changes();
            let provider_events = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while provider_changes.changed().await.is_ok() {
                    let _ = provider_events.emit("data-provider-changed", ());
                }
            });
            app.manage(state.clone());
            main_webview::create(app)?;
            #[cfg(target_os = "macos")]
            if let Err(error) = app_update::staged::mac_quit::install(app.handle()) {
                log::warn!("无法为 macOS 应用菜单接入安全退出：{error}");
            }
            tauri::async_runtime::spawn(async move {
                if let Err(error) = proxy_config.refresh_proxies().await {
                    log::warn!(
                        "启动代理策略刷新未完成：原因={}，当前会话来源={:?}",
                        crate::locale_tag::diagnostic_tag(&error),
                        proxy_config.data_selection().0,
                    );
                }
            });
            // 迁移发生在日志插件初始化之前，结果在这里补记。
            match migration {
                user_paths::LegacyMigration::Moved { legacy, target } => log::info!(
                    "已将旧数据目录迁移到新位置：旧={:?}，新={:?}",
                    crate::file_check::log_path(&legacy),
                    crate::file_check::log_path(&target)
                ),
                user_paths::LegacyMigration::Failed {
                    legacy,
                    target,
                    error,
                } => log::warn!(
                    "旧数据目录迁移未完成，继续使用新位置：旧={:?}，新={:?}，分类={:?}，OS码={:?}",
                    crate::file_check::log_path(&legacy),
                    crate::file_check::log_path(&target),
                    error.kind(),
                    error.raw_os_error()
                ),
                user_paths::LegacyMigration::None => {}
            }
            log::info!(
                "OC2 MOD Manager started; version={}",
                env!("CARGO_PKG_VERSION")
            );
            if let Ok(executable) = std::env::current_exe() {
                log::info!(
                    "当前运行程序={:?}，数据根={:?}，Rust 日志目录={:?}",
                    crate::file_check::log_path(&executable),
                    crate::file_check::log_path(&data_root),
                    crate::file_check::log_path(&logs_directory)
                );
            }
            set_initial_window_size(app);
            present_startup_window(app);
            game_state::start_observer(app.handle().clone());
            Ok(())
        })
        .invoke_handler(audited_handler![
            commands::mod_changes::evaluate_mod_changes,
            commands::mod_changes::plan_mod_changes,
            commands::mod_changes::prepare_mod_changes,
            commands::mod_changes::execute_mod_changes,
            commands::mod_changes::cancel_mod_changes,
            commands::dashboard::get_dashboard,
            commands::dashboard::get_local_mods,
            commands::diychef_editor::open_diychef_editor,
            commands::diychef_editor::migrate_diychef_resources,
            commands::diychef_editor::initialize_diychef_config,
            commands::diychef_editor::validate_diychef_draft,
            commands::diychef_editor::save_diychef_config,
            commands::diychef_editor::delete_diychef_resource,
            commands::diychef_preview::get_diychef_preview,
            commands::diychef_preview::read_diychef_preview_asset,
            commands::diychef_preview::clear_diychef_cache,
            commands::diychef_thumbnail::diychef_thumbnail_cache,
            commands::diychef_thumbnail::get_diychef_preview_fingerprint,
            commands::dashboard::rebind_local_mods,
            commands::dashboard::get_mod_updates,
            commands::settings::get_config,
            commands::settings::get_install_language,
            commands::manager_data::get_manager_data_directory,
            commands::manager_data::clear_all_config,
            commands::manager_data::migrate_manager_data,
            commands::logs::list_log_files,
            commands::logs::read_log,
            commands::logs::copy_log_file,
            commands::logs::clear_log,
            commands::logs::write_frontend_log,
            commands::logs::record_ipc_failure,
            commands::logs::record_file_drop,
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
            commands::mod_repository_releases::refresh_mod_repository,
            commands::relations::sync_dependency_snapshots,
            community::commands::refresh_community_repositories,
            community::commands::cancel_community_search,
            map_publish::authorization::start_publish_authorization,
            map_publish::authorization::poll_publish_authorization,
            map_publish::authorization::cancel_publish_authorization,
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
            repository_readme::get_repository_readme,
            repository_readme::cancel_repository_readme,
            commands::mod_repository_releases::get_repository_releases,
            commands::bepinex::install_bepinex,
            commands::bepinex::repair_native_bepinex_overlay,
            commands::bepinex::uninstall_bepinex,
            commands::bepinex::backup_bepinex,
            commands::bepinex::restore_bepinex,
            commands::bepinex::delete_bepinex_backup,
            commands::mod_repository_install::install_mod_repository,
            commands::download::cancel_download,
            commands::download::control_download_race,
            commands::mod_repository_install::uninstall_repository,
            commands::mod_repositories::export_mod_repositories,
            commands::mod_repositories::export_mod_repositories_to_file,
            commands::mod_repositories::copy_mod_repositories,
            commands::clipboard::read_clipboard,
            commands::clipboard::copy_text,
            commands::clipboard::copy_image,
            commands::colorful_name::push_colorful_name,
            commands::mod_repositories::preview_mod_repository_import,
            commands::mod_repositories::import_mod_repositories,
            commands::repository_sources::clear_repository_source_cache,
            commands::repository_sources::import_repository_source,
            commands::repository_sources::save_repository_source,
            commands::repository_sources::save_repository_sources,
            commands::repository_sources::delete_repository_source,
            commands::repository_sources::reorder_repository_sources,
            commands::mod_repositories::save_mod_repository,
            commands::mod_repositories::delete_mod_repository,
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
            commands::levels::get_level_compression_progress,
            commands::levels::get_level_set_preview,
            commands::levels::get_level_preview_detail,
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
            commands::system::open_external_page,
            commands::update::open_release_page,
            commands::update::get_app_distribution,
            commands::update::open_update_repository,
            commands::system::exit_app,
            commands::system::open_devtools,
            commands::settings::reset_ui,
            commands::system::permission_status,
            commands::system::refresh_user_region,
            commands::system::probe_startup_source,
            commands::system::cancel_startup_probe,
            commands::system::relaunch_as_admin,
            commands::update::get_pending_app_update_recovery,
            commands::update::check_app_update,
            commands::update::stage_app_update,
            commands::update::finish_app_update_recovery,
            commands::system::apply_staged_app_update,
        ])
        .build(context)
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested {
                code: None, api, ..
            } = event
            {
                api.prevent_exit();
                let _ = main_webview::request_quit(app);
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
