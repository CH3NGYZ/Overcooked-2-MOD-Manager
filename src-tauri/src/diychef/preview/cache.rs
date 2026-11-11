use super::{assets, custom, geometry::Geometry, native};
use crate::{
    diychef::session::PreviewContext,
    models::{ChefPreviewBounds, ChefPreviewScene},
};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{Mutex, OnceLock},
};
#[derive(Clone)]
pub(super) struct Part {
    pub name: String,
    pub geometry: Geometry,
    pub texture: Option<Vec<u8>>,
    pub scale: [f32; 2],
    pub offset: [f32; 2],
    pub material_floats: std::collections::BTreeMap<String, f32>,
}
#[derive(Clone)]
pub(super) struct Prepared {
    pub parts: Vec<Part>,
    pub head_bounds: ChefPreviewBounds,
    pub diagnostics: Vec<String>,
}
struct Entry {
    scene: ChefPreviewScene,
    assets: HashMap<String, assets::Asset>,
    sessions: HashSet<String>,
    last: u64,
}
#[derive(Default)]
struct Cache {
    entries: HashMap<String, Entry>,
    tick: u64,
}
fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(Cache::default()))
}

pub(super) fn clear() {
    let retired = {
        let mut c = cache().lock().unwrap_or_else(|error| error.into_inner());
        std::mem::take(&mut *c)
    };
    drop(retired);
}

pub(super) fn retain_sessions(live: &HashSet<String>) -> Result<(), String> {
    let mut c = cache().lock().map_err(|_| "diychef.previewUnavailable")?;
    for entry in c.entries.values_mut() {
        entry.sessions.retain(|id| live.contains(id));
    }
    Ok(())
}

pub(super) fn get(context: &PreviewContext, epoch: u64) -> Result<ChefPreviewScene, String> {
    get_or_prepare(context, epoch, &assets::directory()?, || {
        if context.chef_directory.is_some() {
            custom::scene(context, epoch)
        } else if context.hat_directory.is_some() {
            custom::native_with_custom_hat(context, epoch)
        } else {
            native::native_scene(
                &context.game_root,
                &context.chef_name,
                &context.hat_value,
                epoch,
            )
        }
    })
}

pub(super) fn get_or_prepare(
    context: &PreviewContext,
    epoch: u64,
    directory: &Path,
    prepare: impl FnOnce() -> Result<Prepared, String>,
) -> Result<ChefPreviewScene, String> {
    super::require_epoch(epoch)?;
    let key = super::context_fingerprint(context)?;
    {
        let mut c = cache().lock().map_err(|_| "diychef.previewUnavailable")?;
        super::require_epoch(epoch)?;
        c.tick += 1;
        let tick = c.tick;
        if let Some(entry) = c
            .entries
            .get_mut(&key)
            .filter(|entry| entry.scene.status == "complete")
        {
            entry.sessions.insert(context.session_id.clone());
            entry.last = tick;
            return Ok(entry.scene.clone());
        }
    }
    let prepared = prepare();
    super::require_epoch(epoch)?;
    if super::context_fingerprint(context)? != key {
        return Err("diychef.previewUnavailable".into());
    }
    let (scene, assets) = match prepared.and_then(|p| assets::transport(directory, &key, p, epoch))
    {
        Ok(result) => result,
        Err(error) => (super::unavailable(key.clone(), error), HashMap::new()),
    };
    super::require_epoch(epoch)?;
    // A transient read failure must remain retryable with the same resource metadata.
    if scene.status == "unavailable" {
        return Ok(scene);
    }
    let mut c = cache().lock().map_err(|_| "diychef.previewUnavailable")?;
    super::require_epoch(epoch)?;
    // Raw assets live on disk. This bound counts only small DTO/range indexes;
    // pending frontend scenes cannot lose assets due to raw-RAM byte eviction.
    while c.entries.len() >= 192 && !c.entries.contains_key(&key) {
        let Some(old) = c
            .entries
            .iter()
            .min_by_key(|(_, e)| e.last)
            .map(|(k, _)| k.clone())
        else {
            break;
        };
        c.entries.remove(&old);
    }
    c.tick += 1;
    let last = c.tick;
    // Two live editors can finish the same cold fingerprint concurrently.
    // Replacing its disk ranges must keep both existing authorizations.
    let mut sessions = c
        .entries
        .get(&key)
        .map(|entry| entry.sessions.clone())
        .unwrap_or_default();
    sessions.insert(context.session_id.clone());
    c.entries.insert(
        key,
        Entry {
            scene: scene.clone(),
            assets,
            sessions,
            last,
        },
    );
    Ok(scene)
}
pub(super) fn read_asset(session: &str, id: &str) -> Result<Vec<u8>, String> {
    let epoch = super::cache_epoch();
    let mut c = cache().lock().map_err(|_| "diychef.previewUnavailable")?;
    super::require_epoch(epoch)?;
    c.tick += 1;
    let tick = c.tick;
    let mut selected = None;
    for (key, entry) in c.entries.iter_mut() {
        if entry.sessions.contains(session) {
            if let Some(asset) = entry.assets.get(id) {
                entry.last = tick;
                selected = Some((key.clone(), asset.clone()));
                break;
            }
        }
    }
    drop(c);
    let (key, asset) = selected.ok_or("diychef.previewUnavailable")?;
    let result = assets::read(&asset, epoch);
    if result.is_err() {
        // A pruned or damaged record is retryable via normal scene preparation.
        // Do not remove a newer record installed while this IO was in progress.
        let mut c = cache().lock().map_err(|_| "diychef.previewUnavailable")?;
        if c.entries
            .get(&key)
            .and_then(|entry| entry.assets.get(id))
            .is_some_and(|current| current.shares_record(&asset))
        {
            c.entries.remove(&key);
        }
    }
    result
}

#[cfg(test)]
pub(super) fn memory_profile() -> serde_json::Value {
    let cache = cache().lock().unwrap_or_else(|error| error.into_inner());
    serde_json::json!({
        "scene_entries": cache.entries.len(),
        "asset_ranges": cache.entries.values().map(|entry| entry.assets.len()).sum::<usize>(),
        "authorized_sessions": cache.entries.values().map(|entry| entry.sessions.len()).sum::<usize>(),
        "descriptor_json_bytes": cache.entries.values().map(|entry| serde_json::to_vec(&entry.scene).unwrap().len()).sum::<usize>(),
        "range_inline_bytes": cache.entries.values().map(|entry| entry.assets.capacity() * std::mem::size_of::<(String, assets::Asset)>()).sum::<usize>(),
        "retained_raw_asset_bytes": 0,
    })
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
