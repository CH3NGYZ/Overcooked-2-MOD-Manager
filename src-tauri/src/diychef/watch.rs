//! Observe only files belonging to the backend-authorized selected DIYChef.
use crate::{directory_watch::DirectoryWatcher, models::ChefTarget};
use std::{
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
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
        #[cfg(test)]
        tests::trace(root, format!("watch key={key:?}"));
        let payload = FilesChanged {
            target_id: key.0.clone(),
            generation: key.1,
            resources_changed: false,
        };
        let accept_root = root.to_path_buf();
        let emit_root = accept_root.clone();
        let root_was_present = root.is_dir();
        let pending = self.resources_revision.pending_flag(root);
        let resources_revision = self.resources_revision.clone();
        self.directory.watch(
            key,
            vec![root.to_path_buf()],
            move |event| {
                #[cfg(test)]
                tests::trace(
                    &accept_root,
                    format!(
                        "raw event={event:?} classified={:?}",
                        event
                            .paths
                            .iter()
                            .map(|path| classify(&accept_root, path))
                            .collect::<Vec<_>>()
                    ),
                );
                let accepted = accept_event(&accept_root, root_was_present, event, &pending);
                #[cfg(test)]
                tests::trace(
                    &accept_root,
                    format!("accepted={accepted} retained={:?}", event.paths),
                );
                accepted
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
        let revision = self.resources_revision.current(root);
        #[cfg(test)]
        tests::trace(root, format!("watch returned revision={revision}"));
        revision
    }

    pub(crate) fn stop(&self) {
        self.directory.stop();
        self.resources_revision.flush_pending();
    }

    /// Return the latest revision observed for a backend-authorized target.
    ///
    /// Mutating commands use this at their commit boundary to reject a stale
    /// editor session after an external Resources edit. The watcher owns the
    /// revision store, so callers never provide a cache path or arbitrary root.
    pub(crate) fn resources_revision(&self, root: &Path) -> u64 {
        self.resources_revision.flush_pending();
        self.resources_revision.current(root)
    }

    /// Mark a backend-owned resource mutation immediately. Filesystem
    /// notifications are asynchronous, so a command returning a fresh snapshot
    /// must advance the revision before the watcher eventually reports it.
    pub(crate) fn mark_resources_changed(&self, root: &Path) -> u64 {
        self.resources_revision.mark_changed(root);
        self.resources_revision.current(root)
    }
}

impl Drop for ChefWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

fn accept_event(
    root: &Path,
    root_was_present: bool,
    event: &mut notify::Event,
    pending: &AtomicBool,
) -> bool {
    // FSEvents can deliver the existing root's Create(Folder) after watch
    // registration. It says nothing about Resources; retain child creations,
    // removals, renames and rescan/unknown root events normally. Filtering before
    // debounce also prevents a mixed root+prefer event becoming a resource edit.
    // Initially missing roots still need their creation signal from the shared
    // ancestor watch. This snapshot does not close the registration TOCTOU race.
    let root_creation = root_was_present
        && matches!(event.kind, notify::EventKind::Create(_))
        && !event.need_rescan();
    event
        .paths
        .retain(|path| !(root_creation && path == root) && classify(root, path).is_some());
    if event
        .paths
        .iter()
        .any(|path| classify(root, path) == Some(true))
    {
        // The debounced worker can be replaced before it emits.
        // Keep receipt independently, without doing IO in notify.
        pending.store(true, Ordering::Release);
        return true;
    }
    !event.paths.is_empty()
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
