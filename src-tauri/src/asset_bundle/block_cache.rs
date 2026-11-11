//! A preview context and its worker forks share one decoded-block budget.
use super::{BundleLayout, ParseError};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
};

const BUDGET: usize = 192 * 1024 * 1024;
type BlockKey = (PathBuf, usize, usize, usize, u16);

/// Clone shares immutable layouts/blocks, rather than multiplying the budget.
#[derive(Clone, Default)]
pub(crate) struct BundleBlockCache {
    shared: Arc<Shared>,
}
struct Shared {
    state: Mutex<State>,
    changed: Condvar,
    budget: usize,
}
impl Default for Shared {
    fn default() -> Self {
        Self {
            state: Mutex::new(State::default()),
            changed: Condvar::new(),
            budget: BUDGET,
        }
    }
}
#[derive(Default)]
struct State {
    layouts: HashMap<PathBuf, Arc<BundleLayout>>,
    entries: VecDeque<(BlockKey, Arc<Vec<u8>>)>,
    loading: HashSet<LoadKey>,
    bytes: usize,
}
#[derive(Clone, Eq, Hash, PartialEq)]
enum LoadKey {
    Layout(PathBuf),
    Block(BlockKey),
}

/// A failed read or unwinding decoder releases waiters without caching failure.
struct Loading<'a> {
    shared: &'a Shared,
    key: LoadKey,
}
impl Drop for Loading<'_> {
    fn drop(&mut self) {
        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        state.loading.remove(&self.key);
        self.shared.changed.notify_all();
    }
}

impl BundleBlockCache {
    pub(super) fn layout(
        &self,
        path: &Path,
        load: impl FnOnce() -> Result<BundleLayout, ParseError>,
    ) -> Result<Arc<BundleLayout>, ParseError> {
        let key = LoadKey::Layout(path.to_path_buf());
        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if let Some(layout) = state.layouts.get(path) {
                return Ok(layout.clone());
            }
            if state.loading.insert(key.clone()) {
                break;
            }
            state = self
                .shared
                .changed
                .wait(state)
                .unwrap_or_else(|e| e.into_inner());
        }
        drop(state);
        let _loading = Loading {
            shared: &self.shared,
            key,
        };
        let layout = Arc::new(load()?);
        self.shared
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .layouts
            .insert(path.to_path_buf(), layout.clone());
        Ok(layout)
    }

    pub(super) fn block(
        &self,
        key: BlockKey,
        load: impl FnOnce() -> Result<Vec<u8>, ParseError>,
    ) -> Result<Arc<Vec<u8>>, ParseError> {
        let loading_key = LoadKey::Block(key.clone());
        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if let Some((_, bytes)) = state.entries.iter().find(|(entry, _)| *entry == key) {
                return Ok(bytes.clone());
            }
            if state.loading.insert(loading_key.clone()) {
                break;
            }
            state = self
                .shared
                .changed
                .wait(state)
                .unwrap_or_else(|e| e.into_inner());
        }
        drop(state);
        let _loading = Loading {
            shared: &self.shared,
            key: loading_key,
        };
        // File IO and decompression run without the shared cache lock. Different
        // blocks progress together; the same 119MiB LZMA block is decoded once.
        // Arc<[u8]>::from(Vec) copies the entire allocation. Retain the decoder's
        // Vec so a large LZMA block does not briefly occupy two full buffers.
        let bytes = Arc::new(load()?);
        if bytes.len() <= self.shared.budget {
            let mut retired = Vec::new();
            let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
            while state.bytes + bytes.len() > self.shared.budget {
                let Some((_, old)) = state.entries.pop_front() else {
                    break;
                };
                state.bytes -= old.len();
                retired.push(old);
            }
            state.bytes += bytes.len();
            state.entries.push_back((key, bytes.clone()));
            drop(state);
            drop(retired);
        }
        Ok(bytes)
    }

    #[cfg(test)]
    pub(crate) fn stats(&self) -> (usize, usize, usize) {
        let state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        (state.bytes, state.entries.len(), state.layouts.len())
    }

    #[cfg(test)]
    pub(crate) fn shares_storage_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.shared, &other.shared)
    }
}

#[cfg(test)]
#[path = "block_cache_tests.rs"]
mod tests;
