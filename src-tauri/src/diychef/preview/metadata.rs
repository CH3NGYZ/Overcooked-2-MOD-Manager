//! Exact per-request metadata, shared only while a scan is in flight.
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    fs::Metadata,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex, OnceLock},
};

const MAX_FILES: usize = 16_384;
const MAX_PATH_BYTES: usize = 2 * 1024 * 1024;

pub(super) struct Snapshot {
    pub paths: Vec<PathBuf>,
    pub manifest: String,
    pub stamp: String,
}

type ResultSnapshot = Result<Arc<Snapshot>, String>;
type Key = (u64, PathBuf);

#[derive(Default)]
struct Flight {
    result: Mutex<Option<ResultSnapshot>>,
    ready: Condvar,
}

#[derive(Default)]
struct Flights {
    active: Mutex<HashMap<Key, Arc<Flight>>>,
}

impl Flights {
    fn run(&self, key: Key, read: impl FnOnce() -> ResultSnapshot) -> ResultSnapshot {
        let (flight, owner) = {
            let mut active = self
                .active
                .lock()
                .map_err(|_| "diychef.previewUnavailable")?;
            match active.get(&key) {
                Some(flight) => (flight.clone(), false),
                None => {
                    let flight = Arc::new(Flight::default());
                    active.insert(key.clone(), flight.clone());
                    (flight, true)
                }
            }
        };
        if owner {
            // A failed/panicking enumerator must release every concurrent caller.
            let completed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(read));
            let result = completed
                .as_ref()
                .map_or_else(|_| Err("diychef.previewUnavailable".into()), Clone::clone);
            {
                let mut active = self.active.lock().unwrap_or_else(|e| e.into_inner());
                let mut slot = flight.result.lock().unwrap_or_else(|e| e.into_inner());
                if slot.is_none() {
                    *slot = Some(result);
                }
                if active
                    .get(&key)
                    .is_some_and(|current| Arc::ptr_eq(current, &flight))
                {
                    // No finished snapshot is cached: the next request always
                    // enumerates names/size/mtime again, even one instant later.
                    active.remove(&key);
                }
                flight.ready.notify_all();
            }
            if let Err(panic) = completed {
                std::panic::resume_unwind(panic);
            }
        }
        let mut result = flight
            .result
            .lock()
            .map_err(|_| "diychef.previewUnavailable")?;
        while result.is_none() {
            result = flight
                .ready
                .wait(result)
                .map_err(|_| "diychef.previewUnavailable")?;
        }
        result.as_ref().unwrap().clone()
    }

    fn clear(&self) {
        let retired = std::mem::take(&mut *self.active.lock().unwrap_or_else(|e| e.into_inner()));
        for flight in retired.into_values() {
            *flight.result.lock().unwrap_or_else(|e| e.into_inner()) =
                Some(Err("diychef.previewUnavailable".into()));
            flight.ready.notify_all();
        }
    }
}

fn flights() -> &'static Flights {
    static FLIGHTS: OnceLock<Flights> = OnceLock::new();
    FLIGHTS.get_or_init(Flights::default)
}

pub(super) fn clear() {
    flights().clear();
}

pub(super) fn game(game: &Path) -> ResultSnapshot {
    let epoch = super::cache_epoch();
    super::require_epoch(epoch)?;
    let snapshot = flights().run((epoch, game.to_owned()), || {
        let rows = source_metadata(game)?;
        let manifest = super::fingerprint::metadata_files(&rows)?;
        let stamp = format!("{:x}", Sha256::digest(manifest.as_bytes()));
        Ok(Arc::new(Snapshot {
            paths: rows.into_iter().map(|(path, _)| path).collect(),
            manifest,
            stamp,
        }))
    })?;
    super::require_epoch(epoch)?;
    Ok(snapshot)
}

pub(super) fn source_metadata(game: &Path) -> Result<Vec<(PathBuf, Metadata)>, String> {
    let sources = crate::unity_assets::game_source_metadata(game, MAX_FILES, MAX_PATH_BYTES)
        .map_err(|_| "diychef.previewLimit")?;
    let mut sources: BTreeMap<_, _> = sources.into_iter().collect();
    let mut path_bytes: usize = sources
        .keys()
        .map(|path| path.as_os_str().as_encoded_bytes().len())
        .sum();
    for relative in [
        "Overcooked2_Data/resources.assets",
        "Overcooked2.app/Contents/Resources/Data/resources.assets",
        "Contents/Resources/Data/resources.assets",
    ] {
        let Ok(path) = crate::game::safe_game_path(game, relative) else {
            continue;
        };
        let Ok(path) = path.canonicalize() else {
            continue;
        };
        let Ok(info) = path.metadata() else { continue };
        if !info.is_file() {
            continue;
        }
        let stream = path.with_file_name("resources.assets.resS");
        for (path, info) in std::iter::once((path, info)).chain(
            stream
                .metadata()
                .ok()
                .filter(Metadata::is_file)
                .map(|info| (stream, info)),
        ) {
            if !sources.contains_key(&path) {
                path_bytes = path_bytes.saturating_add(path.as_os_str().as_encoded_bytes().len());
                if sources.len() >= MAX_FILES || path_bytes > MAX_PATH_BYTES {
                    return Err("diychef.previewLimit".into());
                }
            }
            sources.insert(path, info);
        }
    }
    Ok(sources.into_iter().collect())
}

#[cfg(test)]
mod tests;
