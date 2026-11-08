use crate::app_state::AppState;
use crate::config::ConfigService;
use crate::game;
use crate::models::{GameConfig, GameInfo};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use sysinfo::System;
use tauri::{AppHandle, Emitter, Manager};

pub(crate) const GAME_STATE_EVENT: &str = "game-state-changed";

/// A single backend observer replaces frontend IPC polling. Stable ticks only
/// inspect process metadata and a handful of filesystem timestamps; they do
/// not load configuration, read DLL versions, scan MODs, or write logs.
pub(crate) fn start_observer(app: AppHandle) {
    let config = app.state::<AppState>().config.clone();
    tauri::async_runtime::spawn(async move {
        let mut observer = GameStateObserver::new(config);
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        /*
         * 单轮失败不再直接结束监听：观察器一停，界面就永远停在最后一次快照上
         * （「游戏已经退出了，按钮还是停止游戏」）。锁中毒这类不可恢复的错误
         * 会一直失败，所以连续失败到上限才退出，避免刷日志又保持可见。
         */
        let mut failures = 0_u32;
        loop {
            interval.tick().await;
            if app.get_webview_window("main").is_none() {
                break;
            }
            let handle = app.clone();
            let result = tauri::async_runtime::spawn_blocking(move || {
                let change = observer.refresh(&handle.state::<AppState>());
                (observer, change)
            })
            .await;
            match result {
                Ok((next, Ok(change))) => {
                    observer = next;
                    failures = 0;
                    if let Some(info) = change {
                        log::info!(
                            "游戏状态变化：已找到游戏={}，BepInEx已安装={}，游戏运行={}，WinHTTP={:?}",
                            info.game_exists,
                            info.bepinex.installed,
                            info.kitchen_running,
                            info.winhttp_state
                        );
                        if let Err(error) = app.emit(GAME_STATE_EVENT, &info) {
                            log::warn!("发送游戏状态变化事件失败：{error}");
                        }
                    }
                }
                Ok((next, Err(error))) => {
                    observer = next;
                    failures += 1;
                    log::error!("游戏状态监视器本轮失败（第 {failures} 次）：{error}");
                    if failures >= MAX_CONSECUTIVE_FAILURES {
                        log::error!("游戏状态监视器连续失败，停止监听");
                        break;
                    }
                }
                Err(error) => {
                    log::error!("游戏状态监视任务失败：{error}");
                    break;
                }
            }
        }
    });
}

/// 观察器连续失败多少次后放弃（锁中毒这类错误不会自愈）。
const MAX_CONSECUTIVE_FAILURES: u32 = 10;

struct GameStateObserver {
    config: ConfigService,
    config_stamp: Option<FileStamp>,
    game_settings: Option<GameConfig>,
    file_stamps: Vec<(PathBuf, Option<FileStamp>)>,
    processes: System,
    last_reported: Option<GameInfo>,
    missing_game_probe: GameDiscoveryObserver,
    portable_game_root: Option<PathBuf>,
}

impl GameStateObserver {
    fn new(config: ConfigService) -> Self {
        Self {
            config,
            config_stamp: None,
            game_settings: None,
            file_stamps: Vec::new(),
            processes: System::new(),
            last_reported: None,
            missing_game_probe: GameDiscoveryObserver::default(),
            portable_game_root: std::env::current_exe()
                .ok()
                .and_then(|path| path.parent().map(Path::to_path_buf)),
        }
    }

    fn refresh(&mut self, state: &AppState) -> Result<Option<GameInfo>, String> {
        let cached = state.game_state.lock().map_err(|e| e.to_string())?.clone();
        let config_stamp = file_stamp(&self.config.config_path());
        let mut settings_changed = false;
        if self.game_settings.is_none() || config_stamp != self.config_stamp {
            let settings = self.config.load_or_default().game;
            settings_changed = self.game_settings.as_ref() != Some(&settings);
            self.game_settings = Some(settings);
            // Loading may create or normalize the file. Remember its final stamp.
            self.config_stamp = file_stamp(&self.config.config_path());
        }
        let settings = self
            .game_settings
            .as_ref()
            .ok_or("game.settingsNotInitialized")?;
        let file_stamps = game_file_stamps(cached.as_ref(), &self.config.backups_dir());
        // With no detected path, framework stamps are empty. Watch the
        // discovery inputs instead so a later game installation is noticed
        // without repeatedly scanning Steam or writing unchanged diagnostics.
        let executable_name = crate::bepinex::Package::current(settings).executable();
        let portable_candidate = self
            .portable_game_root
            .as_ref()
            .map(|root| root.join(executable_name));
        let discovery_changed =
            if settings.path.is_empty() && cached.as_ref().is_none_or(|info| info.path.is_none()) {
                self.missing_game_probe.refresh(
                    &game::steam_root_paths(settings),
                    portable_candidate.as_deref(),
                    executable_name,
                )
            } else {
                false
            };
        let mut next = if settings_changed
            || cached.is_none()
            || file_stamps != self.file_stamps
            || discovery_changed
        {
            game::detect_game_from_settings(settings)
        } else {
            cached.clone().ok_or("game.stateNotInitialized")?
        };
        next.kitchen_running = next.path.as_ref().is_some_and(|path| {
            game::is_game_running_with_system(Path::new(path), &mut self.processes)
        });

        let next_stamps = game_file_stamps(Some(&next), &self.config.backups_dir());
        let mut snapshot = state.game_state.lock().map_err(|e| e.to_string())?;
        /*
         * 检测期间可能有别的命令（`get_dashboard`、`save_settings` 等）写入更新的快照。
         * 旧实现遇到这种情况直接 `return Ok(None)` 丢弃整轮，于是：
         *   - `last_reported` 永远不推进，事件再也不发；
         *   - `file_stamps` 不更新，下一轮又走全量探测，形成「一直探测、从不通知」。
         * 现在改为：以外部快照为新基线继续本轮，只把本轮探测到的
         * `kitchen_running` 合并进去；若合并后与外部快照相同则不发事件。
         */
        let baseline = snapshot.clone().unwrap_or_else(|| next.clone());
        let mut merged = baseline;
        merged.kitchen_running = next.kitchen_running;
        let external_newer = *snapshot != cached;
        if external_newer && *snapshot == Some(merged.clone()) {
            // 外部快照已经是同一份内容，无需再写或通知
            self.file_stamps = next_stamps;
            return Ok(None);
        }
        self.file_stamps = next_stamps;
        *snapshot = Some(merged.clone());
        Ok(changed_snapshot(&mut self.last_reported, merged))
    }
}

#[derive(Default)]
struct GameDiscoveryObserver {
    steam_roots: Vec<PathBuf>,
    library_indexes: Vec<(PathBuf, Option<FileStamp>)>,
    portable_candidate: Option<PathBuf>,
    executable_name: String,
    candidates: Vec<PathBuf>,
    candidate_stamps: Vec<(PathBuf, Option<FileStamp>)>,
}

impl GameDiscoveryObserver {
    fn refresh(
        &mut self,
        steam_roots: &[PathBuf],
        portable_candidate: Option<&Path>,
        executable_name: &str,
    ) -> bool {
        let library_indexes = steam_roots
            .iter()
            .map(|root| {
                let index = root.join("steamapps/libraryfolders.vdf");
                let stamp = file_stamp(&index);
                (index, stamp)
            })
            .collect::<Vec<_>>();
        let inputs_changed = self.steam_roots != steam_roots
            || self.library_indexes != library_indexes
            || self.portable_candidate.as_deref() != portable_candidate
            || self.executable_name != executable_name;
        if inputs_changed {
            self.steam_roots = steam_roots.to_vec();
            self.library_indexes = library_indexes;
            self.portable_candidate = portable_candidate.map(Path::to_path_buf);
            self.executable_name = executable_name.into();
            self.candidates = game::steam_libraries_from_roots(steam_roots)
                .into_iter()
                .map(|root| {
                    root.join("steamapps/common/Overcooked! 2")
                        .join(executable_name)
                })
                .chain(self.portable_candidate.clone())
                .collect();
            self.candidates.sort();
            self.candidates.dedup();
        }
        let candidate_stamps = self
            .candidates
            .iter()
            .map(|path| (path.clone(), file_stamp(path)))
            .collect::<Vec<_>>();
        let changed = inputs_changed || self.candidate_stamps != candidate_stamps;
        self.candidate_stamps = candidate_stamps;
        changed
    }
}

fn changed_snapshot(previous: &mut Option<GameInfo>, next: GameInfo) -> Option<GameInfo> {
    if previous.as_ref() == Some(&next) {
        return None;
    }
    *previous = Some(next.clone());
    Some(next)
}

#[derive(PartialEq, Eq)]
struct FileStamp {
    modified: Option<SystemTime>,
    length: u64,
}

fn file_stamp(path: &Path) -> Option<FileStamp> {
    fs::metadata(path).ok().map(|metadata| FileStamp {
        modified: metadata.modified().ok(),
        length: metadata.len(),
    })
}

fn game_file_stamps(info: Option<&GameInfo>, backups: &Path) -> Vec<(PathBuf, Option<FileStamp>)> {
    let Some(root) = info.and_then(|info| info.path.as_deref()).map(Path::new) else {
        return Vec::new();
    };
    [
        root.to_path_buf(),
        root.join("Overcooked2.exe"),
        root.join("Overcooked2.app"),
        root.join("Overcooked2.app/Contents/MacOS"),
        root.join("BepInEx"),
        root.join("BepInEx/core"),
        root.join("BepInEx/core/BepInEx.dll"),
        root.join("BepInEx/core/BepInEx.Core.dll"),
        root.join("BepInEx/config"),
        root.join("BepInEx/config/BepInEx.cfg"),
        root.join("winhttp.dll"),
        root.join("winhttp.dll.disabled"),
        root.join("run_bepinex.sh"),
        root.join("libdoorstop.so"),
        root.join("libdoorstop.dylib"),
        root.join("doorstop_libs/libdoorstop_x64.so"),
        root.join("doorstop_libs/libdoorstop_x64.dylib"),
        backups.to_path_buf(),
    ]
    .into_iter()
    .map(|path| {
        let stamp = file_stamp(&path);
        (path, stamp)
    })
    .collect()
}

#[cfg(test)]
mod tests;
