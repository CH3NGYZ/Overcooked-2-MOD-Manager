use crate::app_state::AppState;
use crate::game;
use crate::models::OperationResult;
use crate::shell_open;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use tauri::State;

#[tauri::command]
pub fn open_game_directory(state: State<'_, AppState>) -> Result<OperationResult, String> {
    log::debug!("打开游戏目录");
    let config = state.config.load_or_default();
    let path = game::detect_game(&config).path.ok_or("game.notFound")?;
    shell_open::open_path(Path::new(&path))?;
    Ok(OperationResult {
        ok: true,
        message: "tools.gameDirOpened".into(),
    })
}

#[tauri::command]
pub fn open_mod_directory(state: State<'_, AppState>) -> Result<OperationResult, String> {
    log::debug!("打开 MOD 目录");
    let config = state.config.load_or_default();
    let game_root = game::detect_game(&config).path.ok_or("game.notFound")?;
    let plugins = PathBuf::from(game_root).join("BepInEx/plugins");
    fs::create_dir_all(&plugins).map_err(|e| e.to_string())?;
    shell_open::open_path(&plugins)?;
    Ok(OperationResult {
        ok: true,
        message: "tools.modsDirOpened".into(),
    })
}

#[tauri::command]
pub fn open_bepinex_directory(state: State<'_, AppState>) -> Result<OperationResult, String> {
    log::debug!("打开 BepInEx 目录");
    let config = state.config.load_or_default();
    let game_root = game::detect_game(&config).path.ok_or("game.notFound")?;
    let framework = game::safe_game_path(Path::new(&game_root), "BepInEx")?;
    if !framework.is_dir() {
        return Err("framework.bepInExDirMissing".into());
    }
    shell_open::open_path(&framework)?;
    Ok(OperationResult {
        ok: true,
        message: "framework.bepInExDirOpened".into(),
    })
}

#[tauri::command]
pub fn open_backup_directory(
    state: State<'_, AppState>,
    backup_path: Option<String>,
) -> Result<OperationResult, String> {
    let path = match backup_path {
        Some(path) => {
            let path = PathBuf::from(path);
            if !path.is_dir() {
                return Err("backup.notFound".into());
            }
            path
        }
        None => {
            let path = state.config.backups_dir();
            fs::create_dir_all(&path).map_err(|e| e.to_string())?;
            path
        }
    };
    log::debug!("打开备份目录：路径={}", path.display());
    shell_open::open_path(&path)?;
    Ok(OperationResult {
        ok: true,
        message: "tools.backupDirOpened".into(),
    })
}
