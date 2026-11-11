use super::{custom, geometry::Geometry, native};
use crate::{
    diychef::session::PreviewContext,
    models::{ChefPreviewBounds, ChefPreviewMesh, ChefPreviewScene},
};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, OnceLock},
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
    assets: HashMap<String, Arc<Vec<u8>>>,
    sessions: HashSet<String>,
    bytes: usize,
    last: u64,
}
#[derive(Default)]
struct Cache {
    entries: HashMap<String, Entry>,
    bytes: usize,
    tick: u64,
}
fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(Cache::default()))
}

pub(super) fn get(context: &PreviewContext) -> Result<ChefPreviewScene, String> {
    let key = super::context_fingerprint(context)?;
    {
        let mut c = cache().lock().map_err(|_| "diychef.previewUnavailable")?;
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
    let prepared = if context.chef_directory.is_some() {
        custom::scene(context)
    } else if context.hat_directory.is_some() {
        custom::native_with_custom_hat(context)
    } else {
        native::native_scene(&context.game_root, &context.chef_name, &context.hat_value)
    };
    if super::context_fingerprint(context)? != key {
        return Err("diychef.previewUnavailable".into());
    }
    let (scene, assets) = match prepared {
        Ok(p) => transport(&key, p),
        Err(error) => (super::unavailable(key.clone(), error), HashMap::new()),
    };
    // A transient read failure must remain retryable with the same resource metadata.
    if scene.status == "unavailable" {
        return Ok(scene);
    }
    let used = assets.values().map(|b| b.len()).sum::<usize>();
    if used > 64 * 1024 * 1024 {
        return Ok(super::unavailable(key, "diychef.previewLimit".into()));
    }
    let mut c = cache().lock().map_err(|_| "diychef.previewUnavailable")?;
    while c.bytes + used > 128 * 1024 * 1024 || c.entries.len() >= 192 {
        let Some(old) = c
            .entries
            .iter()
            .min_by_key(|(_, e)| e.last)
            .map(|(k, _)| k.clone())
        else {
            break;
        };
        if let Some(e) = c.entries.remove(&old) {
            c.bytes = c.bytes.saturating_sub(e.bytes);
        }
    }
    c.tick += 1;
    let last = c.tick;
    if let Some(old) = c.entries.remove(&key) {
        c.bytes = c.bytes.saturating_sub(old.bytes);
    }
    c.bytes += used;
    c.entries.insert(
        key,
        Entry {
            scene: scene.clone(),
            assets,
            sessions: HashSet::from([context.session_id.clone()]),
            bytes: used,
            last,
        },
    );
    Ok(scene)
}
pub(super) fn read_asset(session: &str, id: &str) -> Result<Vec<u8>, String> {
    let mut c = cache().lock().map_err(|_| "diychef.previewUnavailable")?;
    c.tick += 1;
    let tick = c.tick;
    for entry in c.entries.values_mut() {
        if entry.sessions.contains(session) {
            if let Some(bytes) = entry.assets.get(id) {
                entry.last = tick;
                return Ok(bytes.as_ref().clone());
            }
        }
    }
    Err("diychef.previewUnavailable".into())
}
fn transport(key: &str, prepared: Prepared) -> (ChefPreviewScene, HashMap<String, Arc<Vec<u8>>>) {
    let mut assets = HashMap::new();
    let mut meshes = vec![];
    for (index, part) in prepared.parts.into_iter().enumerate() {
        if part.geometry.vertices.is_empty() || part.geometry.indices.is_empty() {
            continue;
        }
        let geometry_asset = format!("{key}:g{index}");
        assets.insert(geometry_asset.clone(), Arc::new(part.geometry.bytes()));
        let texture_asset = part.texture.map(|bytes| {
            let id = format!("{key}:t{index}");
            assets.insert(id.clone(), Arc::new(bytes));
            id
        });
        meshes.push(ChefPreviewMesh {
            name: part.name,
            geometry_asset,
            vertex_count: part.geometry.vertices.len(),
            index_count: part.geometry.indices.len(),
            texture_asset,
            texture_scale: part.scale,
            texture_offset: part.offset,
            material_floats: part.material_floats,
        });
    }
    let status = if meshes.is_empty() {
        "unavailable"
    } else if prepared.diagnostics.is_empty() {
        "complete"
    } else {
        "partial"
    };
    (
        ChefPreviewScene {
            fingerprint: key.into(),
            scene_version: 1,
            status: status.into(),
            meshes,
            head_bounds: prepared.head_bounds,
            diagnostics: prepared.diagnostics,
        },
        assets,
    )
}
