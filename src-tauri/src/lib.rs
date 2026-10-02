mod archive;
mod commands;
mod config;
mod game;
mod models;
mod mods;
mod releases;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    request_elevation();
    let state = commands::AppState::new().expect("无法初始化 OC2 Mod Manager 配置服务");
    tauri::Builder::default()
        .manage(state)
        .plugin(tauri_plugin_single_instance::init(|_, _, _| {}))
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_updater::Builder::new()
                .header("User-Agent", releases::APP_USER_AGENT)
                .expect("valid updater User-Agent")
                .build(),
        )
        .setup(|app| {
            app.handle().plugin(
                tauri_plugin_log::Builder::default()
                    .level(if cfg!(debug_assertions) {
                        log::LevelFilter::Debug
                    } else {
                        log::LevelFilter::Info
                    })
                    .build(),
            )?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_dashboard,
            commands::get_config,
            commands::save_mod_note,
            commands::install_local_mod,
            commands::uninstall_local_mod,
            commands::save_settings,
            commands::save_tokens,
            commands::detect_game,
            commands::set_game_path,
            commands::refresh_repository,
            commands::get_repository_releases,
            commands::install_bepinex,
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
            commands::read_text_file,
            commands::preview_repository_import,
            commands::import_repositories,
            commands::save_repository,
            commands::delete_repository,
            commands::launch_normal,
            commands::launch_no_mod,
            commands::restore_mod,
            commands::switch_winhttp,
            commands::open_game_directory,
            commands::open_backup_directory,
            commands::open_release_page,
            commands::exit_app,
            commands::open_devtools,
            commands::reset_ui,
            commands::check_app_update,
            commands::install_app_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while building tauri application");
}

#[cfg(windows)]
fn request_elevation() {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null_mut;
    use winapi::um::shellapi::ShellExecuteW;
    use winapi::um::winuser::SW_SHOWNORMAL;
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn IsUserAnAdmin() -> i32;
    }
    let elevated = unsafe { IsUserAnAdmin() != 0 };
    if elevated {
        return;
    }
    let exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(_) => std::process::exit(1),
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
    if (result as usize) > 32 {
        std::process::exit(0);
    }
    std::process::exit(1);
}

#[cfg(not(windows))]
fn request_elevation() {}
