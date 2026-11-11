//! Metadata-keyed, bounded static chef scene preparation.
mod assembly;
mod cache;
mod custom;
mod fingerprint;
mod geometry;
mod native;
mod obj;
mod parts;
#[cfg(test)]
mod tests;

use crate::{
    app_state::AppState,
    models::{ChefPreviewBounds, ChefPreviewScene},
};
use sha2::{Digest, Sha256};

pub(crate) fn fingerprint(
    state: &AppState,
    session: &str,
    chef: &str,
    hat: &str,
) -> Result<String, String> {
    context_fingerprint(&super::resolve_preview_context(state, session, chef, hat)?)
}
fn context_fingerprint(context: &super::session::PreviewContext) -> Result<String, String> {
    let mut metadata = format!(
        "chef-scene-v6-complete-openhands|{}|{}|",
        context.chef_name, context.hat_value
    );
    metadata.push_str(&fingerprint::files(&native::reader::sources(
        &context.game_root,
    ))?);
    if let Some(dir) = &context.chef_directory {
        metadata.push_str(&fingerprint::directory(dir)?);
    }
    if let Some(dir) = &context.hat_directory {
        metadata.push_str(&fingerprint::directory(dir)?);
    }
    // Only this small metadata manifest is digested, never OBJ/PNG/bundle contents.
    Ok(format!("{:x}", Sha256::digest(metadata.as_bytes())))
}

pub(crate) fn get(
    state: &AppState,
    session: &str,
    chef: &str,
    hat: &str,
) -> Result<ChefPreviewScene, String> {
    let context = super::resolve_preview_context(state, session, chef, hat)?;
    cache::get(&context)
}
pub(crate) fn read_asset(state: &AppState, session: &str, id: &str) -> Result<Vec<u8>, String> {
    // Validate the editor session, even when a resource id is already in cache.
    super::session::require_session(state, session)?;
    cache::read_asset(session, id)
}
pub(super) fn unavailable(fingerprint: String, reason: String) -> ChefPreviewScene {
    ChefPreviewScene {
        fingerprint,
        scene_version: 1,
        status: "unavailable".into(),
        meshes: vec![],
        head_bounds: ChefPreviewBounds {
            min: [-0.5; 3],
            max: [0.5; 3],
        },
        diagnostics: vec![reason],
    }
}
