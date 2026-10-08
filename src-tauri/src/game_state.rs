use crate::commands::AppState;
use crate::config::ConfigService;
use crate::game;
use crate::models::{GameConfig, GameInfo};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use sysinfo::System;
use tauri::{AppHandle, Emitter, Manager};

const GAME_STATE_EVENT: &str = "game-state-changed";

/// A single backend observer replaces frontend IPC polling. Stable ticks only
/// inspect process metadata and a handful of filesystem timestamps; they do
/// not load configuration, read DLL versions, scan MODs, or write logs.
pub(crate) fn start_observer(app: AppHandle) {
    let config = app.state::<AppState>().config.clone();
    tauri::async_runtime::spawn(async move {
        let mut observer = GameStateObserver::new(config);
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
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
                Ok((_, Err(error))) => {
                    log::error!("游戏状态监视器停止：{error}");
                    break;
                }
                Err(error) => {
                    log::error!("游戏状态监视任务失败：{error}");
                    break;
                }
            }
        }
    });
}

struct GameStateObserver {
    config: ConfigService,
    config_stamp: Option<FileStamp>,
    game_settings: Option<GameConfig>,
    file_stamps: Vec<(PathBuf, Option<FileStamp>)>,
    processes: System,
    last_reported: Option<GameInfo>,
    missing_game_probe: GameDiscoveryObserver,
    portable_game_candidate: Option<PathBuf>,
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
            portable_game_candidate: std::env::current_exe()
                .ok()
                .and_then(|path| path.parent().map(|root| root.join("Overcooked2.exe"))),
        }
    }

    fn refresh(&mut self, state: &AppState) -> Result<Option<GameInfo>, String> {
        let cached = state.game_state.lock().map_err(|e| e.to_string())?.clone();
        let config_stamp = file_stamp(&self.config.config_path());
        let mut settings_changed = false;
        if self.game_settings.is_none() || config_stamp != self.config_stamp {
            let settings = self.config.load_or_default().game;
            settings_changed = self.game_settings.as_ref().is_none_or(|previous| {
                previous.path != settings.path || previous.auto_detect != settings.auto_detect
            });
            self.game_settings = Some(settings);
            // Loading may create or normalize the file. Remember its final stamp.
            self.config_stamp = file_stamp(&self.config.config_path());
        }
        let settings = self.game_settings.as_ref().ok_or("游戏设置尚未初始化")?;
        let file_stamps = game_file_stamps(cached.as_ref(), &self.config.backups_dir());
        // With no detected path, framework stamps are empty. Watch the
        // discovery inputs instead so a later game installation is noticed
        // without repeatedly scanning Steam or writing unchanged diagnostics.
        let discovery_changed =
            if settings.path.is_empty() && cached.as_ref().is_none_or(|info| info.path.is_none()) {
                self.missing_game_probe.refresh(
                    &game::steam_root_paths(),
                    self.portable_game_candidate.as_deref(),
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
            cached.clone().ok_or("游戏状态尚未初始化")?
        };
        next.kitchen_running = next.path.as_ref().is_some_and(|path| {
            game::is_game_running_with_system(Path::new(path), &mut self.processes)
        });

        let next_stamps = game_file_stamps(Some(&next), &self.config.backups_dir());
        let mut snapshot = state.game_state.lock().map_err(|e| e.to_string())?;
        // A user action may have returned a fresher snapshot during detection.
        // Never replace that result with an earlier background observation.
        if *snapshot != cached {
            return Ok(None);
        }
        self.file_stamps = next_stamps;
        *snapshot = Some(next.clone());
        Ok(changed_snapshot(&mut self.last_reported, next))
    }
}

#[derive(Default)]
struct GameDiscoveryObserver {
    steam_roots: Vec<PathBuf>,
    library_indexes: Vec<(PathBuf, Option<FileStamp>)>,
    portable_candidate: Option<PathBuf>,
    candidates: Vec<PathBuf>,
    candidate_stamps: Vec<(PathBuf, Option<FileStamp>)>,
}

impl GameDiscoveryObserver {
    fn refresh(&mut self, steam_roots: &[PathBuf], portable_candidate: Option<&Path>) -> bool {
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
            || self.portable_candidate.as_deref() != portable_candidate;
        if inputs_changed {
            self.steam_roots = steam_roots.to_vec();
            self.library_indexes = library_indexes;
            self.portable_candidate = portable_candidate.map(Path::to_path_buf);
            self.candidates = game::steam_libraries_from_roots(steam_roots)
                .into_iter()
                .map(|root| root.join("steamapps/common/Overcooked! 2/Overcooked2.exe"))
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
        root.join("BepInEx"),
        root.join("BepInEx/core"),
        root.join("BepInEx/core/BepInEx.dll"),
        root.join("BepInEx/core/BepInEx.Core.dll"),
        root.join("winhttp.dll"),
        root.join("winhttp.dll.disabled"),
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
mod tests {
    use super::*;
    use crate::models::{BepInExInfo, WinhttpState};
    use std::io::Write;

    fn snapshot() -> GameInfo {
        GameInfo {
            path: Some("game".into()),
            steam_library: None,
            game_exists: true,
            bepinex: BepInExInfo {
                installed: false,
                complete: false,
                version: None,
                version_source: None,
                backups: Vec::new(),
            },
            kitchen_running: false,
            winhttp_state: WinhttpState::Missing,
        }
    }

    #[test]
    fn stable_game_state_does_not_repeat_events() {
        let info = snapshot();
        let mut previous = None;
        assert_eq!(
            changed_snapshot(&mut previous, info.clone()),
            Some(info.clone())
        );
        for _ in 0..10 {
            assert!(changed_snapshot(&mut previous, info.clone()).is_none());
        }
    }

    #[test]
    fn game_start_stop_and_restart_each_publish_once() {
        let mut info = snapshot();
        let mut previous = Some(info.clone());
        for running in [true, false, true] {
            info.kitchen_running = running;
            assert_eq!(
                changed_snapshot(&mut previous, info.clone()),
                Some(info.clone())
            );
            assert!(changed_snapshot(&mut previous, info.clone()).is_none());
        }
    }

    #[test]
    fn framework_changes_publish_without_losing_game_fields() {
        let mut info = snapshot();
        let mut previous = Some(info.clone());
        info.bepinex.installed = true;
        info.bepinex.complete = true;
        info.winhttp_state = WinhttpState::ModEnabled;
        let event = changed_snapshot(&mut previous, info).unwrap();
        assert_eq!(event.path.as_deref(), Some("game"));
        assert!(event.bepinex.complete);
        assert_eq!(event.winhttp_state, WinhttpState::ModEnabled);
        assert!(changed_snapshot(&mut previous, event).is_none());
    }

    #[test]
    fn framework_file_changes_invalidate_the_snapshot() {
        let root = std::env::temp_dir().join(format!("oc2-game-state-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Overcooked2.exe"), []).unwrap();
        let settings = GameConfig {
            path: root.to_string_lossy().into_owned(),
            auto_detect: false,
        };
        let before = game::detect_game_from_settings(&settings);
        let backups = root.join("backups");
        let original_stamps = game_file_stamps(Some(&before), &backups);
        assert!(original_stamps == game_file_stamps(Some(&before), &backups));
        fs::create_dir_all(root.join("BepInEx/core")).unwrap();
        fs::write(root.join("BepInEx/core/BepInEx.dll"), []).unwrap();
        assert!(original_stamps != game_file_stamps(Some(&before), &backups));
        let after = game::detect_game_from_settings(&settings);
        assert!(after.bepinex.installed && after.bepinex.complete);
        let mut previous = Some(before);
        assert!(changed_snapshot(&mut previous, after).is_some());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn framework_enable_disable_renames_invalidate_and_publish_once() {
        let root =
            std::env::temp_dir().join(format!("oc2-winhttp-observer-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Overcooked2.exe"), []).unwrap();
        fs::write(root.join("winhttp.dll"), b"loader").unwrap();
        let settings = GameConfig {
            path: root.to_string_lossy().into_owned(),
            auto_detect: false,
        };
        let backups = root.join("backups");
        let enabled = game::detect_game_from_settings(&settings);
        assert_eq!(enabled.winhttp_state, WinhttpState::ModEnabled);
        let mut previous = Some(enabled.clone());
        let mut stamps = game_file_stamps(Some(&enabled), &backups);
        assert!(
            stamps
                .iter()
                .any(|(path, _)| path == &root.join("winhttp.dll.disabled"))
        );

        for (source, destination, expected) in [
            (
                "winhttp.dll",
                "winhttp.dll.disabled",
                WinhttpState::ModDisabled,
            ),
            (
                "winhttp.dll.disabled",
                "winhttp.dll",
                WinhttpState::ModEnabled,
            ),
        ] {
            fs::rename(root.join(source), root.join(destination)).unwrap();
            let next = game::detect_game_from_settings(&settings);
            let next_stamps = game_file_stamps(Some(&next), &backups);
            assert!(stamps != next_stamps);
            assert_eq!(next.winhttp_state, expected);
            assert_eq!(
                changed_snapshot(&mut previous, next.clone()),
                Some(next.clone())
            );
            assert!(changed_snapshot(&mut previous, next.clone()).is_none());
            assert!(next_stamps == game_file_stamps(Some(&next), &backups));
            stamps = next_stamps;
        }

        fs::remove_file(root.join("Overcooked2.exe")).unwrap();
        fs::remove_file(root.join("winhttp.dll")).unwrap();
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn appending_game_logs_does_not_trigger_framework_detection() {
        let root = std::env::temp_dir().join(format!("oc2-game-log-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("BepInEx/core")).unwrap();
        fs::write(root.join("Overcooked2.exe"), []).unwrap();
        fs::write(root.join("BepInEx/core/BepInEx.dll"), []).unwrap();
        let log = root.join("BepInEx/LogOutput.log");
        fs::write(&log, "existing game log\n").unwrap();
        let mut info = snapshot();
        info.path = Some(root.to_string_lossy().into_owned());
        let backups = root.join("backups");
        let original = game_file_stamps(Some(&info), &backups);
        fs::OpenOptions::new()
            .append(true)
            .open(log)
            .unwrap()
            .write_all(b"next game log record\n")
            .unwrap();
        assert!(original == game_file_stamps(Some(&info), &backups));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_game_probe_notices_a_later_installation_without_repeating_changes() {
        let root = std::env::temp_dir().join(format!("oc2-discovery-{}", uuid::Uuid::new_v4()));
        let steam_roots = vec![root.clone()];
        let executable = root.join("steamapps/common/Overcooked! 2/Overcooked2.exe");
        let mut observer = GameDiscoveryObserver::default();
        assert!(observer.refresh(&steam_roots, None));
        for _ in 0..10 {
            assert!(!observer.refresh(&steam_roots, None));
        }
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::write(&executable, []).unwrap();
        assert!(observer.refresh(&steam_roots, None));
        assert!(!observer.refresh(&steam_roots, None));
        fs::remove_file(&executable).unwrap();
        assert!(observer.refresh(&steam_roots, None));
        assert!(!observer.refresh(&steam_roots, None));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_game_probe_notices_a_new_library_index() {
        let root = std::env::temp_dir().join(format!("oc2-library-{}", uuid::Uuid::new_v4()));
        let steam = root.join("Steam");
        let library = root.join("additional-library");
        let executable = library.join("steamapps/common/Overcooked! 2/Overcooked2.exe");
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::write(&executable, []).unwrap();
        let steam_roots = vec![steam.clone()];
        let mut observer = GameDiscoveryObserver::default();
        assert!(observer.refresh(&steam_roots, None));
        assert!(!observer.refresh(&steam_roots, None));
        fs::create_dir_all(steam.join("steamapps")).unwrap();
        let escaped_library = library.to_string_lossy().replace('\\', "\\\\");
        fs::write(
            steam.join("steamapps/libraryfolders.vdf"),
            format!("\"libraryfolders\" {{ \"0\" {{ \"path\" \"{escaped_library}\" }} }}"),
        )
        .unwrap();
        assert!(observer.refresh(&steam_roots, None));
        assert!(
            observer
                .candidates
                .contains(&fs::canonicalize(&executable).unwrap())
        );
        assert!(!observer.refresh(&steam_roots, None));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn configured_missing_directory_notices_a_later_game_executable() {
        let root = std::env::temp_dir().join(format!("oc2-configured-{}", uuid::Uuid::new_v4()));
        let settings = GameConfig {
            path: root.to_string_lossy().into_owned(),
            auto_detect: false,
        };
        let before = game::detect_game_from_settings(&settings);
        assert!(!before.game_exists);
        let before_stamps = game_file_stamps(Some(&before), &root.join("backups"));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Overcooked2.exe"), []).unwrap();
        assert!(before_stamps != game_file_stamps(Some(&before), &root.join("backups")));
        let after = game::detect_game_from_settings(&settings);
        assert!(after.game_exists);
        assert!(changed_snapshot(&mut Some(before), after).is_some());
        fs::remove_dir_all(root).unwrap();
    }
}
