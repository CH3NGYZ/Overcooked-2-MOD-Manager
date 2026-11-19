//! Watch changes in the selected game's plugins tree without reading Releases.
use super::file_state::{PLUGINS_DIR, is_mod_file, is_restore_staging_component};
use crate::{app_state::AppState, directory_watch::DirectoryWatcher, game, metadata_read};
use notify::{
    Event, EventKind,
    event::{CreateKind, ModifyKind, RemoveKind, RenameMode},
};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Default)]
pub(crate) struct LocalModWatcher {
    directory: DirectoryWatcher<PathBuf>,
}

impl LocalModWatcher {
    /// The caller holds `operation`; registration and a new scan's generation
    /// therefore refer to the same selected game and transaction boundary.
    pub(crate) fn watch_selected(&self, app: &AppHandle, root: Option<&Path>) {
        let Some(root) = root else {
            self.directory.stop();
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

    fn watch(&self, root: &Path, mut emit: impl FnMut(&Path, &Path, bool) + Send + 'static) {
        let Ok(plugins) = game::safe_game_path(root, PLUGINS_DIR) else {
            self.directory.stop();
            return;
        };
        // If a link resolves inside the game, watch its actual content and the
        // original names. Never register a recursive root outside the game.
        let logical = root.join(PLUGINS_DIR);
        let guards = [Some(root), logical.parent(), plugins.parent()]
            .into_iter()
            .flatten()
            .filter(|path| {
                game::resolve_existing_ancestor(path)
                    .is_ok_and(|resolved| resolved.starts_with(root))
            })
            .map(Path::to_path_buf)
            .collect::<Vec<_>>();
        let filters = [logical, plugins.clone()].map(|path| {
            let parent_exists = path.parent().is_some_and(Path::is_dir);
            let plugins_exist = path.is_dir();
            (path, parent_exists, plugins_exist)
        });
        let root = root.to_path_buf();
        let rebuilt = self.directory.watch(
            root.clone(),
            vec![plugins.clone()],
            &guards,
            move |event| accept_event(&filters, event),
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

fn accept_event(filters: &[(PathBuf, bool, bool)], event: &mut Event) -> bool {
    // Project a batch to either content or topology. Its accumulated path set
    // has at most one marker, regardless of how many files were copied.
    let mut accepted = event.need_rescan();
    let mut topology = accepted;
    for path in &event.paths {
        for (plugins, parent_exists, plugins_exist) in filters {
            if let Some(relative) = relative_path(path, plugins) {
                if relative.as_os_str().is_empty() {
                    topology |= root_change(event.kind, *plugins_exist);
                } else if !relative
                    .components()
                    .any(|part| is_restore_staging_component(part.as_os_str()))
                {
                    accepted |= is_mod_file(relative) || tree_change(event.kind, path);
                }
            } else if let Some(parent) = plugins.parent() {
                if relative_path(path, parent)
                    .is_some_and(|relative| relative.as_os_str().is_empty())
                {
                    topology |= root_change(event.kind, *parent_exists);
                }
            }
        }
    }
    event.paths.clear();
    if topology {
        event.paths.push(PathBuf::new());
    }
    accepted || topology
}

fn relative_path<'a>(path: &'a Path, root: &Path) -> Option<&'a Path> {
    if let Ok(relative) = path.strip_prefix(root) {
        return Some(relative);
    }
    #[cfg(windows)]
    {
        // A Windows parent watch reports entry names with on-disk casing.
        let mut parts = path.components();
        for part in root.components() {
            if !parts
                .next()?
                .as_os_str()
                .eq_ignore_ascii_case(part.as_os_str())
            {
                return None;
            }
        }
        return Some(parts.as_path());
    }
    #[cfg(not(windows))]
    None
}

fn root_change(kind: EventKind, was_present: bool) -> bool {
    // FSEvents can report a synthetic Create(Folder) for a root that was
    // already present. Real replacement also reports Remove or Name change.
    match kind {
        EventKind::Create(_) => cfg!(windows) || !was_present,
        EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(_)) => true,
        _ => false,
    }
}

fn tree_change(kind: EventKind, path: &Path) -> bool {
    match kind {
        EventKind::Create(CreateKind::Folder) | EventKind::Remove(RemoveKind::Folder) => true,
        EventKind::Create(CreateKind::Any)
        | EventKind::Modify(ModifyKind::Name(RenameMode::To)) => path.is_dir(),
        // On Windows the name of a disappeared entry has no type attached.
        // It may have been a directory containing cached DLLs.
        EventKind::Remove(RemoveKind::Any) | EventKind::Modify(ModifyKind::Name(_)) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests;
