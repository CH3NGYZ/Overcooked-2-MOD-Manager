mod avatar;
mod mesh;
mod palette;
pub(super) mod reader;
mod visibility;
use super::{
    assembly::{Matrix, Node, Tree},
    cache::{Part, Prepared},
};
use crate::models::ChefPreviewBounds;
use reader::{Ref, Store};
use std::{
    path::Path,
    sync::{Arc, Mutex, OnceLock},
};

struct CachedStore {
    stamp: String,
    store: Store,
}
pub(super) fn with_store<T>(
    game: &Path,
    operation: impl FnOnce(&mut Store) -> Result<T, String>,
) -> Result<T, String> {
    static CACHE: OnceLock<Mutex<Option<CachedStore>>> = OnceLock::new();
    let paths = reader::sources(game);
    let stamp = super::fingerprint::files(&paths)?;
    let mut cache = CACHE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "diychef.previewUnavailable")?;
    if cache.as_ref().is_none_or(|c| c.stamp != stamp) {
        *cache = Some(CachedStore {
            stamp,
            store: Store::new(paths)?,
        });
    }
    let result = operation(&mut cache.as_mut().unwrap().store);
    if result.is_err() {
        *cache = None;
    }
    result
}
pub(super) fn tree(store: &mut Store, name: &str) -> Result<Arc<Tree>, String> {
    // UI prefabs provide both relaxed/open hands and retain chef-specific features.
    tree_type(store, name, 2)
}
pub(super) fn tree_type(store: &mut Store, name: &str, kind: usize) -> Result<Arc<Tree>, String> {
    let avatar = store
        .avatars
        .get(name)
        .cloned()
        .ok_or("diychef.previewUnavailable")?;
    let root = store.resolve(
        Ref {
            file: avatar.file,
            id: 0,
        },
        &avatar.prefabs[kind],
    )?;
    if let Some(tree) = store.trees.get(&root) {
        return Ok(tree.clone());
    }
    let tree = Arc::new(Tree::load(store, root)?);
    if store.trees.len() < 32 {
        store.trees.insert(root, tree.clone());
    }
    Ok(tree)
}
pub(super) fn part(
    store: &mut Store,
    tree: &Tree,
    node: &Node,
    name: &str,
    body_template: Option<&str>,
    skeleton: Option<&Tree>,
) -> Result<Part, String> {
    let renderer = node.renderer.ok_or("diychef.previewUnavailable")?;
    let player_body = name.starts_with("Body");
    let material = if name == "Body"
        && body_template
            .and_then(|name| store.avatars.get(name))
            .is_some_and(|avatar| avatar.colourisation_mode == 0)
    {
        store
            .player_colour
            .as_ref()
            .ok_or("diychef.previewUnavailable")?
            .material
    } else {
        mesh::material(store, renderer)?
    };
    let pose = skeleton
        .map(|tree| {
            tree.by_ref
                .iter()
                .find_map(|(reference, index)| (*index == 0).then_some(*reference))
                .ok_or("diychef.previewUnavailable")
        })
        .transpose()?;
    let key = (renderer, material, pose);
    if let Some(part) = store.parts.get(&key) {
        let mut p = part.clone();
        p.name = name.into();
        return Ok(p);
    }
    let geometry = mesh::read(store, tree, renderer, skeleton).inspect_err(|_| {
        #[cfg(test)]
        eprintln!("native geometry failed: {name}");
    })?;
    let (texture, scale, offset) = mesh::texture(store, material).inspect_err(|_| {
        #[cfg(test)]
        eprintln!("native texture failed: {name}");
    })?;
    let mut material_floats = mesh::floats(store, material)?;
    if player_body {
        player_body_floats(store, &mut material_floats)?;
    }
    let part = Part {
        name: name.into(),
        geometry,
        texture: Some(texture),
        scale,
        offset,
        material_floats,
    };
    let used = part.geometry.vertices.len() * 32
        + part.geometry.indices.len() * 4
        + part.texture.as_ref().map_or(0, Vec::len);
    if store.part_bytes + used <= 64 * 1024 * 1024 {
        store.part_bytes += used;
        store.parts.insert(key, part.clone());
    }
    Ok(part)
}
pub(super) fn head_index(tree: &Tree, name: &str) -> Result<usize, String> {
    tree.nodes
        .iter()
        .position(|n| n.name == name && n.renderer.is_some())
        .ok_or_else(|| "diychef.previewUnavailable".into())
}
pub(super) fn model_parts(store: &mut Store, tree: &Tree, head: &str) -> Result<Prepared, String> {
    let index = head_index(tree, head)?;
    let mut parts = vec![];
    let mut head_bounds = vec![];
    let mut diagnostics = vec![];
    for (i, node) in tree.nodes.iter().enumerate() {
        if !model_node(tree, i, index) {
            continue;
        }
        match part(store, tree, node, &node.name, Some(head), None) {
            Ok(p) => {
                if tree.below(i, index) && head_part(node) {
                    head_bounds.push(p.geometry.bounds());
                }
                parts.push(p);
            }
            Err(e) => diagnostics.push(e),
        }
    }
    if head_bounds.is_empty() {
        return Err("diychef.previewUnavailable".into());
    }
    let head_bounds = bounds(head_bounds.into_iter());
    Ok(Prepared {
        parts,
        head_bounds,
        diagnostics,
    })
}
/// Shared prefabs contain every base chef and animation props. Keep the chosen
/// chef's full appearance plus its shared body/equipment, with hats selected separately.
pub(super) fn model_node(tree: &Tree, index: usize, head: usize) -> bool {
    let node = &tree.nodes[index];
    if node.renderer.is_none()
        || visibility::is_blink(&node.name)
        || node.name.starts_with("Props")
        || node.name.starts_with("Knife")
    {
        return false;
    }
    let Some(mesh) = tree.nodes[head].parent else {
        return false;
    };
    let mut branch = index;
    loop {
        // SetHeadVisibility activates the chosen chef even if its prefab flag
        // was false; other descendants retain their serialized active state.
        if branch != head && !tree.nodes[branch].active {
            return false;
        }
        match tree.nodes[branch].parent {
            Some(parent) if parent == mesh => break,
            Some(parent) => branch = parent,
            None => return false,
        }
    }
    let name = tree.nodes[branch].name.as_str();
    branch == head
        || ["Body", "Wheelchair"]
            .iter()
            .any(|prefix| name.starts_with(prefix))
}
pub(super) fn hat(store: &mut Store, tree: &Tree, value: &str) -> Result<Vec<Part>, String> {
    // Automatic is intentionally shown without a fixed hat in static head thumbnails.
    if value == "None" || value.is_empty() {
        return Ok(vec![]);
    }
    let name = format!("Hat_{value}");
    let source;
    let (tree, skeleton) = if tree.nodes.iter().any(|n| n.name.starts_with(&name)) {
        (tree, None)
    } else if matches!(value, "Santa" | "Baseballcap") {
        // FastInit clones these two hats from defaultTemplate.ModelPrefab and
        // rebinds each renderer bone by name to the target UI skeleton.
        let default = store.default_template.clone();
        source = tree_type(store, &default, 0)?;
        (&*source, Some(tree))
    } else {
        return Err("diychef.previewUnavailable".into());
    };
    let index = tree
        .nodes
        .iter()
        .position(|n| n.name.starts_with(&name))
        .ok_or("diychef.previewUnavailable")?;
    let mut parts = vec![];
    for (i, node) in tree.nodes.iter().enumerate() {
        if tree.below(i, index) && node.renderer.is_some() {
            parts.push(part(store, tree, node, &node.name, None, skeleton)?);
        }
    }
    Ok(parts)
}
pub(super) fn native_scene(game: &Path, name: &str, value: &str) -> Result<Prepared, String> {
    with_store(game, |store| {
        let tree = tree(store, name)?;
        let mut scene = model_parts(store, &tree, name)?;
        match hat(store, &tree, value) {
            Ok(h) => scene.parts.extend(h),
            Err(e) => scene.diagnostics.push(e),
        }
        Ok(scene)
    })
}
pub(super) fn bone(tree: &Tree, name: &str) -> Result<Matrix, String> {
    tree.name(name)
        .map(|n| n.world)
        .ok_or_else(|| "diychef.previewUnavailable".into())
}
pub(super) fn material_floats(
    store: &mut Store,
    node: &Node,
) -> Result<std::collections::BTreeMap<String, f32>, String> {
    let material = mesh::material(store, node.renderer.ok_or("diychef.previewUnavailable")?)?;
    mesh::floats(store, material)
}
pub(super) fn material_uv(store: &mut Store, node: &Node) -> Result<([f32; 2], [f32; 2]), String> {
    let material = mesh::material(store, node.renderer.ok_or("diychef.previewUnavailable")?)?;
    mesh::uv(store, material)
}
pub(super) fn player_body_floats(
    store: &Store,
    floats: &mut std::collections::BTreeMap<String, f32>,
) -> Result<(), String> {
    let colour = store
        .player_colour
        .as_ref()
        .ok_or("diychef.previewUnavailable")?;
    for (name, value) in ["_MaskColorR", "_MaskColorG", "_MaskColorB", "_MaskColorA"]
        .into_iter()
        .zip(colour.mask)
    {
        floats.insert(name.into(), value);
    }
    Ok(())
}
pub(super) fn head_part(node: &Node) -> bool {
    node.renderer.is_some()
        && !visibility::is_blink(&node.name)
        && !["Hand_", "Body", "Tail", "Knife", "Wheelchair", "HeldItem"]
            .iter()
            .any(|prefix| node.name.starts_with(prefix))
}
pub(super) fn bounds(values: impl Iterator<Item = ChefPreviewBounds>) -> ChefPreviewBounds {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for value in values {
        for a in 0..3 {
            min[a] = min[a].min(value.min[a]);
            max[a] = max[a].max(value.max[a]);
        }
    }
    ChefPreviewBounds { min, max }
}
