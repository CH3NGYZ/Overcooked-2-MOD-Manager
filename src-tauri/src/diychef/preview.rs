//! Metadata-keyed, bounded static chef scene preparation.
mod admission;
mod assembly;
mod assets;
mod cache;
mod custom;
mod fingerprint;
mod geometry;
mod memory;
mod metadata;
mod native;
mod native_disk_cache;
mod obj;
mod parts;
#[cfg(test)]
mod tests;

use crate::{
    app_state::AppState,
    models::{ChefPreviewBounds, ChefPreviewScene},
};
pub(crate) use admission::{fingerprint_permit, scene_permit};
use sha2::{Digest, Sha256};
use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};

static CACHE_EPOCH: AtomicU64 = AtomicU64::new(0);
static CLEAR: Mutex<()> = Mutex::new(());
#[cfg(test)]
pub(super) static TEST_CACHE_ACCESS: Mutex<()> = Mutex::new(());

pub(crate) fn cache_epoch() -> u64 {
    CACHE_EPOCH.load(Ordering::Acquire)
}

pub(crate) fn require_epoch(epoch: u64) -> Result<(), String> {
    if epoch % 2 != 0 || cache_epoch() != epoch {
        return Err("diychef.previewUnavailable".into());
    }
    Ok(())
}

struct Clearing;
impl Drop for Clearing {
    fn drop(&mut self) {
        CACHE_EPOCH.fetch_add(1, Ordering::AcqRel);
    }
}

pub(crate) fn clear_cache() -> Result<usize, String> {
    let directory = super::thumbnail_cache::directory()?;
    // Validate fixed user-data children before entering the clearing epoch.
    native_disk_cache::directory()?;
    assets::directory()?;
    clear_cache_in(&directory)
}

fn clear_cache_in(directory: &std::path::Path) -> Result<usize, String> {
    let _clear = CLEAR.lock().map_err(|_| "diychef.previewUnavailable")?;
    // Odd epochs reject new work while all owners are being invalidated. The
    // guard reopens admission even if deleting disposable disk data fails.
    let ready_epoch = CACHE_EPOCH.fetch_add(1, Ordering::AcqRel).wrapping_add(2);
    let _clearing = Clearing;
    cache::clear();
    metadata::clear();
    native::clear(ready_epoch);
    let native_directory = directory
        .parent()
        .ok_or("diychef.previewUnsafeResource")?
        .join("diychef-native");
    let asset_directory = directory
        .parent()
        .ok_or("diychef.previewUnsafeResource")?
        .join("diychef-assets");
    // All calls run even after one cleanup fails. Old work remains invalidated.
    let thumbnails = super::thumbnail_cache::clear_in(directory);
    let native = super::thumbnail_cache::clear_in(&native_directory);
    let assets = super::thumbnail_cache::clear_in(&asset_directory);
    Ok(thumbnails? + native? + assets?)
}

pub(crate) fn configure_concurrency(stored: u32) {
    admission::configure_concurrency(stored);
    native::configure_concurrency(stored);
}

pub(crate) fn fingerprint(
    state: &AppState,
    session: &str,
    chef: &str,
    hat: &str,
) -> Result<String, String> {
    context_fingerprint(&super::resolve_preview_context(state, session, chef, hat)?)
}
fn context_fingerprint(context: &super::session::PreviewContext) -> Result<String, String> {
    let prefix = format!(
        "chef-scene-v6-complete-openhands|{}|{}|",
        context.chef_name, context.hat_value
    );
    let mut digest = Sha256::new();
    digest.update(prefix.as_bytes());
    digest.update(metadata::game(&context.game_root)?.manifest.as_bytes());
    if let Some(dir) = &context.chef_directory {
        digest.update(fingerprint::directory(dir)?.as_bytes());
    }
    if let Some(dir) = &context.hat_directory {
        digest.update(fingerprint::directory(dir)?.as_bytes());
    }
    // Stream the original manifest bytes to preserve existing PNG identities.
    // Resource contents are never read, and no combined manifest is allocated.
    Ok(format!("{:x}", digest.finalize()))
}

pub(crate) fn get(
    state: &AppState,
    session: &str,
    chef: &str,
    hat: &str,
    epoch: u64,
) -> Result<ChefPreviewScene, String> {
    require_epoch(epoch)?;
    let context = super::resolve_preview_context(state, session, chef, hat)?;
    retain_live_sessions(state)?;
    cache::get(&context, epoch)
}
pub(crate) fn read_asset(state: &AppState, session: &str, id: &str) -> Result<Vec<u8>, String> {
    // Validate the editor session, even when a resource id is already in cache.
    super::session::require_session(state, session)?;
    retain_live_sessions(state)?;
    cache::read_asset(session, id)
}

fn retain_live_sessions(state: &AppState) -> Result<(), String> {
    let ids = state
        .diychef_sessions
        .lock()
        .map_err(|_| "diychef.previewUnavailable")?
        .values
        .keys()
        .cloned()
        .collect();
    cache::retain_sessions(&ids)
}
pub(super) fn unavailable(fingerprint: String, reason: String) -> ChefPreviewScene {
    ChefPreviewScene {
        fingerprint,
        scene_version: 1,
        estimated_bytes: 0,
        status: "unavailable".into(),
        meshes: vec![],
        head_bounds: ChefPreviewBounds {
            min: [-0.5; 3],
            max: [0.5; 3],
        },
        diagnostics: vec![reason],
    }
}
