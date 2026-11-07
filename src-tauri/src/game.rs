//! 游戏目录、平台启动、进程判定、停止与路径安全的真实领域入口。

mod discovery;
mod paths;
mod process;
mod selection;
mod steam;
mod stop;

pub(crate) use discovery::{detect_game, detect_game_from_settings};
pub(crate) use paths::{resolve_existing_ancestor, safe_game_path};
pub(crate) use process::{
    ensure_game_stopped, is_crossover_running, is_game_running, is_game_running_with_system,
    running_processes,
};
pub(crate) use selection::selected_game_root;
pub(crate) use steam::{steam_launch_command, steam_libraries_from_roots, steam_root_paths};
pub(crate) use stop::stop_game;
