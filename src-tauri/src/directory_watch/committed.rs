//! A successful transaction's actual endpoints, used only to identify OS echoes.
use crate::file_metadata::Stamp;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};

#[derive(Default)]
pub(crate) struct CommittedWatchPaths(Mutex<Arc<CommittedPaths>>);

impl CommittedWatchPaths {
    /// Publish or revoke under the operation lock. No disk IO under this mutex.
    pub(crate) fn publish(&self, paths: CommittedPaths) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = Arc::new(paths);
    }

    pub(crate) fn classify<'a>(
        &self,
        operation: &'a Mutex<()>,
        paths: Vec<PathBuf>,
    ) -> Option<(MutexGuard<'a, ()>, Vec<PathBuf>)> {
        // Wait for Applying to finish before deciding what the commit covers.
        let operation_guard = operation.lock().ok()?;
        let recorded = self.0.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if recorded.roots.is_empty() {
            return Some((operation_guard, paths));
        }
        // Native metadata calls need not block the next writer. Recheck the
        // selected record after getting the lock back, before acting on it.
        drop(operation_guard);
        let (remaining, mismatched) = recorded.external(&paths);
        let operation_guard = operation.lock().ok()?;
        let mut current = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if !Arc::ptr_eq(&recorded, &current) {
            return Some((operation_guard, paths));
        }
        if !mismatched.is_empty() {
            // A changed path must not stay suppressed if later restored to the
            // state left by this commit.
            drop(recorded);
            let current = Arc::make_mut(&mut current);
            for path in mismatched {
                current.roots.remove(&path);
            }
        }
        Some((operation_guard, remaining))
    }
}

/// Cover only paths for which the durable transaction knows the final state.
/// Unrelated paths and any unreadable or changed endpoint stay external events.
/// No polling or expiry window: a late notification is safe to skip only while
/// its current content matches the state already announced by the commit.
#[derive(Clone, Debug, Default)]
pub(crate) struct CommittedPaths {
    roots: BTreeMap<PathBuf, Endpoint>,
}

#[derive(Clone, Debug)]
enum Endpoint {
    Present(Stamp),
    Missing { include_children: bool },
}

impl CommittedPaths {
    pub(crate) fn present(&mut self, path: &Path, stamp: Stamp) {
        if matches!(stamp, Stamp::Present(_)) {
            self.roots
                .insert(path.to_path_buf(), Endpoint::Present(stamp));
        }
    }

    pub(crate) fn missing(&mut self, path: PathBuf, include_children: bool) {
        self.roots
            .insert(path, Endpoint::Missing { include_children });
    }

    fn external(&self, paths: &[PathBuf]) -> (Vec<PathBuf>, BTreeSet<PathBuf>) {
        // At most one metadata lookup per touched entry. A surviving map tree
        // covers only exact verified entries, never an externally added child.
        let mut verified = BTreeMap::new();
        let paths = paths
            .iter()
            .filter(|path| {
                !path.ancestors().any(|ancestor| {
                    let Some((actual_path, endpoint)) = self.find(ancestor) else {
                        return false;
                    };
                    match endpoint {
                        Endpoint::Missing {
                            include_children: true,
                        } => {}
                        _ if ancestor == path.as_path() => {}
                        _ => return false,
                    }
                    *verified.entry(actual_path).or_insert_with(|| {
                        Stamp::capture(actual_path).is_ok_and(|actual| match endpoint {
                            Endpoint::Present(expected) => &actual == expected,
                            Endpoint::Missing { .. } => actual == Stamp::Missing,
                        })
                    })
                })
            })
            .cloned()
            .collect();
        let mismatched = verified
            .into_iter()
            .filter_map(|(path, matches)| (!matches).then(|| path.clone()))
            .collect();
        (paths, mismatched)
    }

    fn find(&self, path: &Path) -> Option<(&PathBuf, &Endpoint)> {
        self.roots.get_key_value(path).or_else(|| {
            if cfg!(windows) {
                self.roots.iter().find(|(endpoint, _)| {
                    super::relative_path(path, endpoint)
                        .is_some_and(|relative| relative.as_os_str().is_empty())
                })
            } else {
                None
            }
        })
    }
}
