//! Lazy metadata is immutable and shared even when workers discover it later.
use super::Document;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

type Entry = Arc<OnceLock<Result<Arc<Document>, String>>>;
#[derive(Clone, Default)]
pub(super) struct Documents {
    shared: Arc<Mutex<State>>,
}
#[derive(Default)]
struct State {
    entries: HashMap<String, Entry>,
    bytes: usize,
}
impl Documents {
    #[cfg(test)]
    pub(super) fn stats(&self) -> (usize, usize) {
        let state = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        (state.bytes, state.entries.len())
    }

    pub(super) fn load(
        &self,
        name: &str,
        load: impl FnOnce() -> Result<Document, String>,
    ) -> Result<Arc<Document>, String> {
        let key = name.to_ascii_lowercase();
        let entry = self
            .shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entries
            .entry(key.clone())
            .or_default()
            .clone();
        let result = entry.get_or_init(|| {
            // OnceLock coordinates only this document; other metadata and block
            // reads do not wait on the document-directory mutex.
            let document = load()?;
            let mut state = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            let bytes = document.file.data.len();
            if state.bytes + bytes > 128 * 1024 * 1024 {
                return Err("diychef.previewLimit".into());
            }
            state.bytes += bytes;
            Ok(Arc::new(document))
        });
        if result.is_err() {
            // A transient IO error remains retryable for the same resource stamp.
            let mut state = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            if state
                .entries
                .get(&key)
                .is_some_and(|current| Arc::ptr_eq(current, &entry))
            {
                state.entries.remove(&key);
            }
        }
        result.clone()
    }
}
