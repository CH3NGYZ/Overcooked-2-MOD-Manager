use crate::app_state::AppState;
#[cfg(test)]
use crate::config::ConfigService;
use crate::game;
use crate::models::{AppConfig, OperationResult};
use crate::mods;
use crate::shell_open;
use std::path::Path;
use std::path::PathBuf;
use tauri::State;

fn resolve_mod_file_for_location(game_root: &Path, dll_file: &str) -> Result<PathBuf, String> {
    let relative = mods::canonical_dll_name(&dll_file.replace('\\', "/"));
    let normalized = relative.to_ascii_lowercase();
    if !normalized.starts_with("bepinex/plugins/") || !normalized.ends_with(".dll") {
        return Err("mods.onlyPluginsLocatable".into());
    }
    let plugins_root = game::safe_game_path(game_root, "BepInEx/plugins")?;
    for candidate in [relative.clone(), mods::disabled_dll_file(&relative)] {
        let path = game::safe_game_path(game_root, &candidate)?;
        if !path.starts_with(&plugins_root) {
            return Err("mods.onlyPluginsLocatable".into());
        }
        if path.is_file() {
            return Ok(path);
        }
    }
    Err("mods.dllFileMissing".into())
}

#[tauri::command]
pub fn open_mod_file_location(
    state: State<'_, AppState>,
    dll_file: String,
) -> Result<OperationResult, String> {
    log::debug!("打开 MOD 文件位置：文件={}", dll_file);
    let config = {
        let _guard = state
            .operation
            .lock()
            .map_err(|_| "lock.poisoned".to_string())?;
        state.config.load_or_default()
    };
    let game_root = game::detect_game(&config).path.ok_or("game.notFound")?;
    let path = resolve_mod_file_for_location(Path::new(&game_root), &dll_file)?;
    shell_open::reveal_in_file_manager(&path)?;
    Ok(OperationResult {
        ok: true,
        message: "mods.fileLocationOpened".into(),
    })
}

#[tauri::command]
pub fn open_mod_logs_directory(
    state: State<'_, AppState>,
    dll_file: String,
) -> Result<OperationResult, String> {
    log::debug!("打开 MOD 日志目录：文件={}", dll_file);
    let (game_root, guid, _) = resolve_mod_config(&state, &dll_file)?;
    let path = crate::mod_config::checked_logs_dir(&game_root, &guid)?;
    if !path.is_dir() {
        return Err("mods.logsDirMissing".into());
    }
    shell_open::reveal_in_file_manager(&path)?;
    Ok(OperationResult {
        ok: true,
        message: "mods.logsDirOpened".into(),
    })
}

#[tauri::command]
pub fn read_mod_config(
    state: State<'_, AppState>,
    dll_file: String,
) -> Result<crate::mod_config::ModConfigDocument, String> {
    let (game_root, guid, _) = resolve_mod_config(&state, &dll_file)?;
    log::debug!("读取 MOD 配置：GUID={}", guid);
    crate::mod_config::read(&game_root, &guid)
}

fn resolve_mod_config(
    state: &AppState,
    dll_file: &str,
) -> Result<(PathBuf, String, String), String> {
    let (root, config) = resolve_config_game(state)?;
    resolve_mod_file_for_location(&root, dll_file)?;
    let installed = mods::scan_mods_with_manifests(&root, &config, None);
    let key = mods::canonical_dll_name(&dll_file.replace('\\', "/"));
    let guid = installed
        .into_iter()
        .find(|item| item.dll_file.eq_ignore_ascii_case(&key))
        .and_then(|item| item.guid)
        .ok_or("mods.guidUnrecognized")?;
    Ok((root, guid, config.game.path))
}

fn resolve_config_game(state: &AppState) -> Result<(PathBuf, AppConfig), String> {
    let config = {
        let _guard = state.operation.lock().map_err(|_| "lock.poisoned")?;
        state.config.load_or_default()
    };
    let root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    Ok((root, config))
}

#[tauri::command]
pub fn save_mod_config(
    state: State<'_, AppState>,
    dll_file: String,
    revision: String,
    content: String,
) -> Result<crate::mod_config::ModConfigDocument, String> {
    let (game_root, guid, configured_path) = resolve_mod_config(&state, &dll_file)?;
    save_config_document(
        &state,
        &game_root,
        &guid,
        &configured_path,
        &revision,
        &content,
    )
}

#[tauri::command]
pub fn read_bepinex_config(
    state: State<'_, AppState>,
) -> Result<crate::mod_config::ModConfigDocument, String> {
    let (game_root, _) = resolve_config_game(&state)?;
    crate::mod_config::read(&game_root, "BepInEx")
}

#[tauri::command]
pub fn save_bepinex_config(
    state: State<'_, AppState>,
    revision: String,
    content: String,
) -> Result<crate::mod_config::ModConfigDocument, String> {
    let (game_root, config) = resolve_config_game(&state)?;
    save_config_document(
        &state,
        &game_root,
        "BepInEx",
        &config.game.path,
        &revision,
        &content,
    )
}

fn save_config_document(
    state: &AppState,
    game_root: &Path,
    guid: &str,
    configured_path: &str,
    revision: &str,
    content: &str,
) -> Result<crate::mod_config::ModConfigDocument, String> {
    let _guard = state.operation.lock().map_err(|_| "lock.poisoned")?;
    let config = state.config.load_or_default();
    if config.game.path != configured_path {
        return Err("game.directoryChangedReopenConfig".into());
    }
    game::ensure_game_stopped(game_root)?;
    let result = crate::mod_config::save(game_root, guid, revision, content);
    log::info!("保存配置：标识={}，成功={}", guid, result.is_ok());
    result
}

#[cfg(test)]
mod mod_file_location_tests;
