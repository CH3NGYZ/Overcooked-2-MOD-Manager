//! Observe only files belonging to the backend-authorized selected DIYChef.
use crate::{directory_watch::DirectoryWatcher, models::ChefTarget};
use std::{
    path::{Component, Path},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

pub(crate) const FILES_CHANGED_EVENT: &str = "diychef-files-changed";

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct FilesChanged {
    pub target_id: String,
    pub generation: u64,
    pub resources_changed: bool,
}

#[derive(Default)]
pub(crate) struct ChefWatcher {
    directory: DirectoryWatcher<(String, u64)>,
    resources_revision: Arc<AtomicU64>,
}

impl ChefWatcher {
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
        let resources_revision = self.resources_revision.clone();
        self.directory.watch(
            key,
            vec![root.to_path_buf()],
            move |event| {
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
                // Catalog identity excludes OBJ/texture bytes. Advance before
                // notifying so the next read refreshes the preview session too.
                if payload.resources_changed {
                    resources_revision.fetch_add(1, Ordering::AcqRel);
                }
                emit(payload);
            },
        );
        self.resources_revision.load(Ordering::Acquire)
    }

    pub(crate) fn stop(&self) {
        self.directory.stop();
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
