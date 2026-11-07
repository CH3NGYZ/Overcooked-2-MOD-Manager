use super::paths::normalize_path;
use super::process::is_game_running;
use super::selection::{game_executable_exists, selection_root};
use super::steam::{steam_libraries_from_roots, steam_root_paths};
use crate::bepinex::{Package, detect_bepinex};
use crate::display_path::display_path;
use crate::models::{BepInExInfo, GameConfig, GameInfo, WinhttpState};
use std::path::{Path, PathBuf};

pub fn detect_game(config: &crate::models::AppConfig) -> GameInfo {
    detect_game_from_settings(&config.game)
}

pub(crate) fn detect_game_from_settings(config: &crate::models::GameConfig) -> GameInfo {
    log::debug!(
        "开始探测游戏状态：配置路径非空={}",
        !config.path.trim().is_empty()
    );
    let detected = find_game_path(config);
    let path = detected.as_ref().map(|location| location.game.clone());
    let game_exists = path
        .as_ref()
        .map(|p| game_executable_exists(p, config))
        .unwrap_or(false);
    let kitchen_running = path.as_ref().map(|p| is_game_running(p)).unwrap_or(false);
    let backup_root = crate::user_paths::backups_dir().ok();
    let bepinex = path
        .as_ref()
        .map(|value| detect_bepinex(value, backup_root.as_deref()))
        .unwrap_or_else(|| BepInExInfo {
            installed: false,
            has_residue: false,
            has_config: false,
            version: None,
            version_source: None,
            backups: Vec::new(),
        });
    let winhttp_state = path
        .as_ref()
        .map(|value| crate::bepinex::state(value, config))
        .unwrap_or(WinhttpState::Missing);
    /*
     * 送给前端的路径统一走 display_path（剥 Windows verbatim 前缀、去多余
     * 尾分隔符）。去掉前缀不改变指向，但**改变字符串形态**：`fs::canonicalize`
     * 返回的是 `\\?\E:\...`，本字段是 `E:\...`。任何拿它与 canonicalize 结果
     * 做 `strip_prefix`/`starts_with` 比较的地方都必须改用 `safe_game_path`
     * 的返回值做基准，否则前缀会静默失配（批量开关曾因此收集不到任何插件）。
     */
    let path_text = path.as_ref().map(|p| display_path(p));
    let steam_library = detected
        .as_ref()
        .map(|location| display_path(&location.library));
    let steam_root = detected
        .as_ref()
        .map(|location| display_path(&location.steam_root));
    let wine_prefix = detected
        .and_then(|location| location.wine_prefix)
        .map(|prefix| display_path(prefix));
    let result = GameInfo {
        path: path_text,
        steam_library,
        steam_root,
        wine_prefix,
        game_exists,
        bepinex,
        kitchen_running,
        winhttp_state,
    };
    log::info!(
        "游戏状态探测完成：已找到路径={}，游戏文件存在={}，BepInEx已安装={}，残留目录={}，游戏运行={}，WinHTTP={:?}",
        result.path.is_some(),
        result.game_exists,
        result.bepinex.installed,
        result.bepinex.has_residue,
        result.kitchen_running,
        result.winhttp_state
    );
    result
}

struct GameLocation {
    library: PathBuf,
    steam_root: PathBuf,
    game: PathBuf,
    wine_prefix: Option<PathBuf>,
}

fn location(
    library: PathBuf,
    game: PathBuf,
    steam_root: &Path,
    settings: &GameConfig,
) -> GameLocation {
    #[cfg(unix)]
    let wine_prefix = (!Package::current(settings).native())
        .then(|| {
            crate::wine::prefix_for_path(steam_root).or_else(|| crate::wine::prefix_for_path(&game))
        })
        .flatten();
    #[cfg(not(unix))]
    let wine_prefix = {
        let _ = (steam_root, settings);
        None
    };
    GameLocation {
        library,
        steam_root: steam_root.to_owned(),
        game,
        wine_prefix,
    }
}

fn find_game_path(config: &GameConfig) -> Option<GameLocation> {
    if !config.path.trim().is_empty() {
        /*
         * 配置里的路径可能是旧版本存下的原样字符串：带 `\\?\` 前缀、结尾
         * 斜杠、`~`，或 macOS 上直接指到 `Overcooked2.app` / `Contents/MacOS`。
         * 先收敛成游戏根并 canonicalize（解析符号链接：macOS 的
         * `/var` → `/private/var`），这样「用户选中」与「自动探测」落到同一个
         * 字符串；路径不存在时 canonicalize 失败，退回原样路径，与旧行为一致。
         */
        let stored = normalize_path(selection_root(
            Path::new(&config.path),
            Package::current(config),
        ));
        log::debug!("使用配置中的游戏路径：存在={}", stored.is_dir());
        // A Windows Steam library may live on a mapped drive outside drive_c.
        for steam in steam_root_paths(config) {
            for library in steam_libraries_from_roots(std::slice::from_ref(&steam)) {
                if stored.starts_with(normalize_path(&library)) {
                    return Some(location(library, stored, &steam, config));
                }
            }
        }
        return Some(location(stored.clone(), stored.clone(), &stored, config));
    }
    if let Ok(current) = std::env::current_exe() {
        if let Some(parent) = current.parent() {
            // 便携目录同样 canonicalize，避免 `\\?\` 与普通写法混用。
            let parent = normalize_path(parent);
            if game_executable_exists(&parent, config) {
                log::debug!("从程序目录探测到游戏");
                return Some(location(parent.clone(), parent.clone(), &parent, config));
            }
        }
    }
    for steam in steam_root_paths(config) {
        for library in steam_libraries_from_roots(std::slice::from_ref(&steam)) {
            let candidate = library.join("steamapps/common/Overcooked! 2");
            if game_executable_exists(&candidate, config) {
                let game = normalize_path(&candidate);
                log::debug!("从当前运行模式的 Steam 库探测到游戏");
                return Some(location(library, game, &steam, config));
            }
        }
    }
    log::warn!("未探测到当前运行模式的 Overcooked! 2 游戏目录");
    None
}

#[cfg(test)]
mod tests;
