//! Target-owned resource stamps survive watcher and application restarts.
use crate::diychef::thumbnail_cache::{self as disk, CacheDirectory};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

pub(super) struct ResourceRevisions {
    cache_directory: PathBuf,
    values: Mutex<HashMap<PathBuf, Revision>>,
}

#[derive(Default)]
struct Revision {
    value: u64,
    pending: Arc<AtomicBool>,
}

impl ResourceRevisions {
    pub(super) fn new(cache_directory: PathBuf) -> Self {
        Self {
            cache_directory,
            values: Mutex::new(HashMap::new()),
        }
    }

    pub(super) fn pending_flag(&self, root: &Path) -> Arc<AtomicBool> {
        self.values
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(root.to_path_buf())
            .or_default()
            .pending
            .clone()
    }

    pub(super) fn flush_pending(&self) {
        let roots: Vec<_> = self
            .values
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter(|(_, revision)| revision.pending.load(Ordering::Acquire))
            .map(|(root, _)| root.clone())
            .collect();
        for root in roots {
            self.current(&root);
        }
    }

    pub(super) fn current(&self, root: &Path) -> u64 {
        let mut values = self.values.lock().unwrap_or_else(|e| e.into_inner());
        let entry = values.entry(root.to_path_buf()).or_default();
        let previous = entry.value;
        let changed = entry.pending.swap(false, Ordering::AcqRel);
        if !changed && previous != 0 {
            return previous;
        }
        // Lock order is values -> disk access. Callers release both before
        // emitting an event or joining the directory worker.
        let revision = self
            .restore_or_advance(root, changed, previous)
            .unwrap_or_else(|error| {
                log::warn!(
                    "暖暖资源修订缓存不可用，使用当前进程修订：目录={}，原因={error}",
                    root.display()
                );
                fresh(previous)
            });
        // A raw event received during IO stays pending for the next read/flush.
        entry.value = revision;
        #[cfg(test)]
        super::tests::trace(
            root,
            format!("revision previous={previous} pending={changed} next={revision}"),
        );
        revision
    }

    #[cfg(test)]
    pub(super) fn has_pending(&self, root: &Path) -> bool {
        self.values
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(root)
            .is_some_and(|revision| revision.pending.load(Ordering::Acquire))
    }

    fn restore_or_advance(&self, root: &Path, changed: bool, previous: u64) -> Result<u64, String> {
        let _access = disk::lock_access()?;
        // ConfigService supplies its cache child; the shared helper validates
        // both that child and the fixed revision directory under the data root.
        let data_root = self
            .cache_directory
            .parent()
            .ok_or("diychef.previewUnavailable")?;
        let directory = disk::directory_under(data_root, CacheDirectory::ResourcesRevisions)?;
        // target::mod_root already returns the canonical backend-authorized
        // MOD root. Neither scan generation nor frontend target ID is an input.
        let file = directory.join(format!(
            "{:x}.bin",
            Sha256::digest(root.as_os_str().as_encoded_bytes())
        ));
        if let Some(info) = disk::regular_file(&file)? {
            if !changed {
                if info.len() == 8 {
                    let mut bytes = Vec::with_capacity(9);
                    fs::File::open(&file)
                        .and_then(|input| input.take(9).read_to_end(&mut bytes))
                        .map_err(|_| "diychef.previewUnavailable")?;
                    if let Ok(bytes) = <[u8; 8]>::try_from(bytes) {
                        let revision = u64::from_le_bytes(bytes);
                        if revision != 0 {
                            return Ok(revision);
                        }
                    }
                }
                log::warn!("暖暖资源修订缓存损坏，重新生成：{}", file.display());
            }
        }
        let revision = fresh(previous);
        disk::write_atomic(&directory, &file, |output| {
            output.write_all(&revision.to_le_bytes())
        })?;
        Ok(revision)
    }
}

fn fresh(previous: u64) -> u64 {
    loop {
        let revision = uuid::Uuid::new_v4().as_u128() as u64;
        if revision != 0 && revision != previous {
            return revision;
        }
    }
}
