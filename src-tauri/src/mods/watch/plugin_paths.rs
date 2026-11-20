use crate::directory_watch::{RootScope, relative_path, retain_topology};
use notify::{
    Event, EventKind,
    event::{CreateKind, ModifyKind, RenameMode},
};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use walkdir::WalkDir;

use super::super::file_state::{
    is_in_map_tree, is_mod_file, is_ordinary_plugin, is_restore_staging_component,
};

/// Paths relative to the plugins root; built beside the metadata batch, never
/// by walking the game directory again under the operation lock.
#[derive(Default)]
pub(crate) struct ScannedPluginPaths {
    pub(crate) ordinary: Vec<PathBuf>,
    pub(crate) diy_level_packages: Vec<PathBuf>,
    pub(crate) registered: HashSet<String>,
}

#[derive(Default)]
pub(super) struct PluginPaths {
    scanned: Arc<ScannedPluginPaths>,
    known: BTreeSet<PathBuf>,
    diy_level_packages: HashSet<PathBuf>,
}

impl PluginPaths {
    pub(super) fn reconcile(&mut self, plugins: &Path, scanned: Arc<ScannedPluginPaths>) {
        if Arc::ptr_eq(&self.scanned, &scanned) {
            return;
        }
        self.known = scanned
            .ordinary
            .iter()
            .map(|path| index_key(&plugins.join(path)))
            .collect();
        self.diy_level_packages = scanned
            .diy_level_packages
            .iter()
            .map(|path| plugins.join(path))
            .collect();
        self.scanned = scanned;
    }

    pub(super) fn accept(&mut self, scope: &RootScope, event: &mut Event) -> bool {
        if retain_topology(event, std::slice::from_ref(scope)) {
            return true;
        }
        let mut map_info = HashMap::new();
        let mut index = 0;
        event.paths.retain_mut(|path| {
            let event_index = index;
            index += 1;
            let Some(relative) = scope.content(path) else {
                return false;
            };
            // Preserve actual endpoints for a late read-back echo check.
            let actual = scope.root.join(relative);
            let matched = self.changed(
                &actual,
                relative,
                &scope.root,
                event.kind,
                event_index,
                &mut map_info,
            );
            *path = actual;
            matched
        });
        !event.paths.is_empty()
    }

    fn changed(
        &mut self,
        path: &Path,
        relative: &Path,
        plugins: &Path,
        kind: EventKind,
        index: usize,
        map_info: &mut HashMap<PathBuf, bool>,
    ) -> bool {
        let missing = match kind {
            EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(RenameMode::From)) => true,
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => index == 0,
            EventKind::Modify(ModifyKind::Name(_)) | EventKind::Any | EventKind::Other => {
                !path.exists()
            }
            _ => false,
        };
        if missing {
            // NTFS remove/rename-from gives us no file type. Consult prior
            // ordinary DLLs, including files observed before the next scan.
            let key = index_key(path);
            let removed = self
                .known
                .range(key.clone()..)
                .take_while(|dll| dll.starts_with(&key))
                .cloned()
                .collect::<Vec<_>>();
            for dll in &removed {
                self.known.remove(dll);
            }
            return !removed.is_empty();
        }

        let in_map = |path: &Path, map_info: &mut HashMap<_, _>| {
            is_in_map_tree(path, plugins, &self.diy_level_packages, map_info)
        };
        let relevant_dll = |path: &Path, relative: &Path, map_info: &mut HashMap<_, _>| {
            is_mod_file(path)
                && is_ordinary_plugin(relative, &self.scanned.registered)
                && !in_map(path, map_info)
        };
        if relevant_dll(path, relative, map_info)
            && path.symlink_metadata().is_ok_and(|meta| meta.is_file())
        {
            self.known.insert(index_key(path));
            return true;
        }

        // A package can be moved in as a single directory event. Only walk
        // that new subtree, pruning known map trees and restore staging.
        if !matches!(
            kind,
            EventKind::Create(CreateKind::Folder | CreateKind::Any)
                | EventKind::Modify(ModifyKind::Name(_))
                | EventKind::Any
                | EventKind::Other
        ) || !path.symlink_metadata().is_ok_and(|meta| meta.is_dir())
            || relative
                .components()
                .any(|part| is_restore_staging_component(part.as_os_str()))
        {
            return false;
        }
        let entries = WalkDir::new(path)
            .follow_links(false)
            .follow_root_links(false)
            .into_iter()
            .filter_entry(|entry| {
                !entry.file_type().is_dir()
                    || (!is_restore_staging_component(entry.file_name())
                        && !in_map(entry.path(), map_info))
            });
        let mut accepted = false;
        for entry in entries.flatten() {
            if entry.file_type().is_file()
                && let Some(relative) = relative_path(entry.path(), plugins)
                && is_mod_file(entry.path())
                && is_ordinary_plugin(relative, &self.scanned.registered)
            {
                // Ancestors passed the same map-tree check as direct events.
                self.known.insert(index_key(entry.path()));
                accepted = true;
            }
        }
        accepted
    }
}

fn index_key(path: &Path) -> PathBuf {
    let key = path.to_path_buf();
    #[cfg(windows)]
    {
        let mut key = key;
        key.as_mut_os_str().make_ascii_lowercase();
        key
    }
    #[cfg(not(windows))]
    key
}
