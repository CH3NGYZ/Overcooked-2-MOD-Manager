//! Watch changes in the selected game's plugins tree without reading Releases.
use super::file_state::PLUGINS_DIR;
use crate::{
    app_state::AppState,
    directory_watch::{DirectoryWatcher, RootScope},
    game, metadata_read,
};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};

mod plugin_paths;
use plugin_paths::PluginPaths;
pub(crate) use plugin_paths::ScannedPluginPaths;

#[derive(Default)]
pub(crate) struct LocalModWatcher {
    directory: DirectoryWatcher<PathBuf>,
    scanned: Arc<Mutex<Option<(PathBuf, Arc<ScannedPluginPaths>)>>>,
}

impl LocalModWatcher {
    /// The caller holds `operation`; registration and a new scan's generation
    /// therefore refer to the same selected game and transaction boundary.
    pub(crate) fn watch_selected(&self, app: &AppHandle, root: Option<&Path>) {
        let Some(root) = root else {
            self.directory.stop();
            self.scanned
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take();
            return;
        };
        let app = app.clone();
        self.watch(root, move |root, plugins, topology_changed| {
            let app = app.clone();
            let root = root.to_path_buf();
            let plugins = plugins.to_path_buf();
            // Registration joins its old debounce worker. Do not acquire the
            // operation lock or re-register from that worker.
            tauri::async_runtime::spawn_blocking(move || {
                let Some(state) = app.try_state::<AppState>() else {
                    return;
                };
                let Ok(_operation) = state.operation.lock() else {
                    return;
                };
                if !invalidate_if_selected(&state, &root, &plugins) {
                    return;
                }
                if topology_changed {
                    // Windows silently stops the recursive handle if the directory
                    // is removed; a same-name replacement needs a new handle.
                    state.local_mod_watch.directory.stop();
                    state.local_mod_watch.watch_selected(&app, Some(&root));
                }
                drop(_operation);
                if let Err(error) = app.emit("local-mods-changed", ()) {
                    log::debug!("发送本地 MOD 变化事件失败：{error}");
                }
            });
        });
    }

    /// Reuse the validated scan; directory deletion notifications from Windows
    /// don't identify whether the disappeared entry was a file or a folder.
    pub(crate) fn publish_paths(&self, root: &Path, scanned: ScannedPluginPaths) {
        *self.scanned.lock().unwrap_or_else(|e| e.into_inner()) =
            Some((root.to_path_buf(), Arc::new(scanned)));
    }

    fn watch(&self, root: &Path, mut emit: impl FnMut(&Path, &Path, bool) + Send + 'static) {
        let Ok(plugins) = game::safe_game_path(root, PLUGINS_DIR) else {
            self.directory.stop();
            return;
        };
        // If a link resolves inside the game, watch its actual content and the
        // original names. Never register a recursive root outside the game.
        let scope = RootScope::new(root, root.join(PLUGINS_DIR), plugins.clone());
        let guards = scope.guards().collect::<Vec<_>>();
        let scanned = self.scanned.clone();
        let mut paths = PluginPaths::default();
        let root = root.to_path_buf();
        let event_root = root.clone();
        let rebuilt = self.directory.watch(
            root.clone(),
            vec![plugins.clone()],
            &guards,
            move |event| {
                let latest = scanned
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .as_ref()
                    .filter(|(root, _)| root == &event_root)
                    .map(|(_, snapshot)| snapshot.clone());
                // Index and directory probes belong to the OS callback; the
                // scanner only swaps the snapshot and never waits for disk.
                if let Some(latest) = latest {
                    paths.reconcile(&scope.root, latest);
                }
                paths.accept(&scope, event)
            },
            {
                let plugins = plugins.clone();
                move |paths| emit(&root, &plugins, !paths.is_empty())
            },
        );
        if rebuilt {
            // A→B→A can happen while A is edited off-watch with its old mtime.
            metadata_read::invalidate_dll_details_under(&plugins);
        }
    }
}

/// Called under `operation`, beside scan begin/publish and transaction writes.
fn invalidate_if_selected(state: &AppState, root: &Path, plugins: &Path) -> bool {
    let selected = game::detect_game(&state.config.load_or_default())
        .path
        .and_then(|path| game::resolve_existing_ancestor(Path::new(&path)).ok());
    if selected.as_deref() != Some(root) {
        return false;
    }
    metadata_read::invalidate_dll_details_under(plugins);
    state.invalidate_local_mod_snapshot().is_ok()
}

#[cfg(test)]
mod tests;
