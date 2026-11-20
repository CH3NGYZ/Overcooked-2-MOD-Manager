//! 关卡集目录的外部变更监听。
//!
//! 地图管理页的列表来自磁盘扫描，但用户会直接在资源管理器里增删 `levels/`
//! 下的目录、替换 `info`；只靠页面自己的重读看不到这些变化。这里用系统的
//! 目录变更通知（Windows `ReadDirectoryChangesW`、macOS FSEvents）代替轮询：
//! 轮询要么太慢（1 秒级）要么太贵（每次都要重扫全部 `info`）。
//!
//! 事件合并路径后将变化地图集加入后台转换队列，并提醒前端重读；扫描口径（两个根的第一层）、
//! 排序与身份判定都在 `level_sets` 里，前端重读一次就能拿到完整一致的结果。

use crate::{
    app_state::AppState,
    directory_watch::{
        DirectoryWatcher, RootScope, relative_path, retain_topology, topology_changed,
    },
    game, level_sets,
};
use notify::Event;
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::{AppHandle, Emitter, Manager};

const LEVEL_SETS_CHANGED_EVENT: &str = "level-sets-changed";

/// AppState owns the same shared OS watcher used by DIYChef files.
#[derive(Default)]
pub struct LevelWatcher {
    directory: DirectoryWatcher<Vec<PathBuf>>,
}

impl LevelWatcher {
    pub(crate) fn watch(
        &self,
        app: &AppHandle,
        selected: Option<&level_sets::LevelSetRoots>,
        reattach: bool,
    ) {
        let Some(selected) = selected.cloned() else {
            self.directory.stop();
            return;
        };
        let scopes = level_sets::recompress::watch_roots(&selected);
        if scopes.is_empty() {
            self.directory.stop();
            return;
        }
        let emit_app = app.clone();
        self.watch_paths(scopes, reattach, move |active, roots, paths| {
            let app = emit_app.clone();
            let active = active.clone();
            let roots = roots.to_vec();
            let selected = selected.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let Some(state) = app.try_state::<AppState>() else {
                    return;
                };
                let reattach = topology_changed(&roots, &paths);
                let Some((_operation, external)) = state
                    .committed_watch_paths
                    .classify(&state.operation, paths)
                else {
                    return;
                };
                let game = game::detect_game(&state.config.load_or_default())
                    .path
                    .and_then(|path| game::resolve_existing_ancestor(Path::new(&path)).ok());
                // A MOD notice may already have revoked the scan. In that
                // case keep the map notice for the same game: the new MOD scan
                // will own reattachment and the refreshed map read.
                if !active.load(Ordering::Acquire) || game.as_deref() != Some(selected.game_root())
                {
                    return;
                }
                if let Ok((_, current)) =
                    crate::mods::snapshot::level_roots(&state, selected.game_root())
                {
                    if current != selected {
                        return;
                    }
                    if reattach {
                        // Even a covered transaction may have created a root;
                        // the parent-only registration then needs to follow it.
                        state.level_watch.watch(&app, Some(&current), true);
                    }
                }
                if external.is_empty() {
                    return;
                }
                if let Ok(mut sessions) = state.mod_changes.lock() {
                    sessions.invalidate();
                }
                drop(_operation);
                let names = changed_sets(&roots, &external);
                if let Err(error) = app.emit(LEVEL_SETS_CHANGED_EVENT, ()) {
                    log::debug!("发送关卡集变化事件失败：{error}");
                }
                crate::level_sets::recompress::enqueue_installed(&state, &app, names.as_deref());
            });
        });
    }

    pub(crate) fn watch_paths(
        &self,
        scopes: Vec<RootScope>,
        reattach: bool,
        mut emit: impl FnMut(&Arc<AtomicBool>, &[PathBuf], Vec<PathBuf>) + Send + 'static,
    ) {
        if reattach {
            self.directory.stop();
        }
        let paths = scopes
            .iter()
            .map(|scope| scope.root.clone())
            .collect::<Vec<_>>();
        let guards = scopes
            .iter()
            .flat_map(|scope| scope.guards())
            .collect::<Vec<_>>();
        self.directory.watch(
            scopes.iter().map(|scope| scope.logical.clone()).collect(),
            paths.clone(),
            &guards,
            move |event| accept_event(&scopes, event),
            move |active, changed| emit(active, &paths, changed),
        );
    }
}

fn accept_event(scopes: &[RootScope], event: &mut Event) -> bool {
    if retain_topology(event, scopes) {
        return true;
    }
    event.paths.retain_mut(|path| {
        for scope in scopes {
            if let Some(relative) = scope.content(path) {
                // Project the set name only after checking exact write paths.
                *path = scope.root.join(relative);
                return true;
            }
        }
        false
    });
    !event.paths.is_empty()
}

/// Only changed top-level sets are queued; ancestor notifications recheck all candidates.
fn changed_sets(roots: &[PathBuf], paths: &[PathBuf]) -> Option<Vec<String>> {
    let mut names = std::collections::BTreeSet::new();
    for path in paths {
        let mut matched = false;
        for root in roots {
            if let Some(relative) = relative_path(path, root) {
                if let Some(std::path::Component::Normal(name)) = relative.components().next() {
                    names.insert(name.to_string_lossy().into_owned());
                    matched = true;
                } else {
                    return None;
                }
            }
        }
        if !matched {
            return None;
        }
    }
    Some(names.into_iter().collect())
}

#[cfg(test)]
mod tests;
