//! Observe only files belonging to the backend-authorized selected DIYChef.
use crate::{directory_watch::DirectoryWatcher, models::ChefTarget};
use std::{
    path::{Component, Path, PathBuf},
    sync::{Arc, atomic::Ordering},
};

mod revisions;
use revisions::ResourceRevisions;

pub(crate) const FILES_CHANGED_EVENT: &str = "diychef-files-changed";

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct FilesChanged {
    pub target_id: String,
    pub generation: u64,
    pub resources_changed: bool,
}

pub(crate) struct ChefWatcher {
    directory: DirectoryWatcher<(String, u64)>,
    resources_revision: Arc<ResourceRevisions>,
}

impl ChefWatcher {
    pub(crate) fn new(cache_directory: PathBuf) -> Self {
        Self {
            directory: DirectoryWatcher::default(),
            resources_revision: Arc::new(ResourceRevisions::new(cache_directory)),
        }
    }

    /// Register before the first catalog/TXT read; the caller only supplies the
    /// path returned by target::mod_root, never a frontend-selected path.
    pub(crate) fn watch(
        &self,
        root: &Path,
        target: &ChefTarget,
        mut emit: impl FnMut(FilesChanged) + Send + 'static,
    ) -> u64 {
        let key = (target.id.clone(), target.generation);
        let payload = FilesChanged {
            target_id: key.0.clone(),
            generation: key.1,
            resources_changed: false,
        };
        let accept_root = root.to_path_buf();
        let emit_root = accept_root.clone();
        let pending = self.resources_revision.pending_flag(root);
        let resources_revision = self.resources_revision.clone();
        self.directory.watch(
            key,
            vec![root.to_path_buf()],
            move |event| {
                if event
                    .paths
                    .iter()
                    .any(|path| classify(&accept_root, path) == Some(true))
                {
                    // The debounced worker can be replaced before it emits.
                    // Keep receipt independently, without doing IO in notify.
                    pending.store(true, Ordering::Release);
                    return true;
                }
                event
                    .paths
                    .iter()
                    .any(|path| classify(&accept_root, path).is_some())
            },
            move |paths| {
                let mut payload = payload.clone();
                payload.resources_changed = paths
                    .iter()
                    .any(|path| classify(&emit_root, path) == Some(true));
                // Persist before notifying: metadata can remain unchanged, and
                // a restarted process must not revive the previous PNG key.
                if payload.resources_changed {
                    resources_revision.current(&emit_root);
                }
                emit(payload);
            },
        );
        // watch joins the old worker before this read; never hold the revision
        // lock across registration, since that worker may still be advancing.
        self.resources_revision.flush_pending();
        self.resources_revision.current(root)
    }

    pub(crate) fn stop(&self) {
        self.directory.stop();
        self.resources_revision.flush_pending();
    }
}

impl Drop for ChefWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

/// true means models or textures may have changed, even if catalog semantics
/// stayed equal. Whole target removal/rename invalidates those resources too.
fn classify(root: &Path, path: &Path) -> Option<bool> {
    let relative = path.strip_prefix(root).ok()?;
    let mut components = relative.components();
    let Some(component) = components.next() else {
        return Some(true);
    };
    let Component::Normal(first) = component else {
        return None;
    };
    let name = first.to_string_lossy();
    if name.eq_ignore_ascii_case("Resources") {
        return Some(true);
    }
    if components.next().is_none()
        && (name.eq_ignore_ascii_case("prefer.txt") || name.eq_ignore_ascii_case("prefer.txt.txt"))
    {
        return Some(false);
    }
    None
}

#[cfg(test)]
mod tests;
