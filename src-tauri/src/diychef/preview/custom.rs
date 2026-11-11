use super::{
    assembly::{self, Tree, multiply},
    cache::{Part, Prepared},
    native::{self, reader::Store},
    obj, parts as chef_parts,
};
use crate::diychef::session::PreviewContext;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

pub(super) fn scene(context: &PreviewContext) -> Result<Prepared, String> {
    let dir = context
        .chef_directory
        .as_deref()
        .ok_or("diychef.previewUnavailable")?;
    native::with_store(&context.game_root, |store| {
        let stamp = super::fingerprint::directory(dir)?;
        let (template, body_warning) = body(dir, store)?;
        let tree = native::tree(store, &template)?;
        if let Some(cached) = store.custom_models.get(&stamp) {
            let mut prepared = cached.clone();
            add_hat(context, store, &tree, &mut prepared);
            return Ok(prepared);
        }
        let root = native::head_index(&tree, &template)?;
        let mut parts = vec![];
        let mut head_bounds = vec![];
        let mut diagnostics = vec![];
        let mut replaced = HashSet::new();
        if body_warning {
            diagnostics.push("diychef.previewUnknownBody".into());
        }
        for (name, _) in chef_parts::CUSTOM {
            if *name == "Knife" {
                continue;
            }
            let filename = format!("{name}.obj");
            let Some(path) = file(dir, &filename)? else {
                continue;
            };
            replaced.insert(*name);
            if *name == "Eyes2_Blinks" {
                continue;
            }
            // FastInit replaces only hand variants already present in this
            // UI prefab. It never creates all four poses simultaneously.
            if name.starts_with("Hand_") && original(&tree, root, name).is_none() {
                continue;
            }
            match custom_part(store, &tree, dir, &path, name, &template) {
                Ok(p) => {
                    if !p.geometry.vertices.is_empty() {
                        if chef_parts::is_head(name) {
                            head_bounds.push(p.geometry.bounds());
                        }
                        parts.push(p);
                    }
                }
                Err(e) => diagnostics.push(e),
            }
        }
        // The mod retains template parts unless an OBJ explicitly replaces them.
        for (i, node) in tree.nodes.iter().enumerate() {
            if !native::model_node(&tree, i, root) {
                continue;
            }
            let key = if i == root {
                "Head"
            } else if node.name == "Body" && replaced.contains("Body_Body") {
                "Body_Body"
            } else {
                chef_parts::CUSTOM
                    .iter()
                    .filter_map(|(part, _)| node.name.starts_with(*part).then_some(*part))
                    .max_by_key(|part| part.len())
                    .unwrap_or(&node.name)
            };
            if replaced.contains(key) {
                continue;
            }
            match native::part(store, &tree, node, &node.name, Some(&template), None) {
                Ok(p) => {
                    if tree.below(i, root) && native::head_part(node) {
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
        let head_bounds = native::bounds(head_bounds.into_iter());
        let mut prepared = Prepared {
            parts,
            head_bounds,
            diagnostics,
        };
        let used = prepared
            .parts
            .iter()
            .map(|p| {
                p.geometry.vertices.len() * 32
                    + p.geometry.indices.len() * 4
                    + p.texture.as_ref().map_or(0, Vec::len)
            })
            .sum::<usize>();
        if prepared.diagnostics.is_empty() && used <= 32 * 1024 * 1024 {
            if store.custom_bytes + used > 32 * 1024 * 1024 {
                store.custom_models.clear();
                store.custom_bytes = 0;
            }
            store.custom_bytes += used;
            store.custom_models.insert(stamp, prepared.clone());
        }
        add_hat(context, store, &tree, &mut prepared);
        Ok(prepared)
    })
}
pub(super) fn native_with_custom_hat(context: &PreviewContext) -> Result<Prepared, String> {
    native::with_store(&context.game_root, |store| {
        let tree = native::tree(store, &context.chef_name)?;
        let mut prepared = native::model_parts(store, &tree, &context.chef_name)?;
        add_hat(context, store, &tree, &mut prepared);
        Ok(prepared)
    })
}
fn add_hat(context: &PreviewContext, store: &mut Store, tree: &Tree, prepared: &mut Prepared) {
    let result = if let Some(dir) = &context.hat_directory {
        let value = &context.hat_value;
        (|| {
            let path = file(dir, &format!("{value}.obj"))?.ok_or("diychef.previewUnavailable")?;
            let mut geometry = obj::parse(&text(&path)?)?;
            let matrix = multiply(
                &native::bone(tree, "HatBase")?,
                &assembly::bind("HatBase").unwrap(),
            );
            geometry.apply(&matrix);
            let texture = png(dir, &format!("t_{value}.png"))?;
            // Custom hats inherit the default in-game Baseballcap material, then remap HatBase.
            let default = store.default_template.clone();
            let source = native::tree_type(store, &default, 0)?;
            let original = source
                .name("Hat_Baseballcap")
                .ok_or("diychef.previewUnavailable")?;
            let mut material_floats = native::material_floats(store, original)?;
            material_floats.extend(material(dir, &format!("m_{value}.txt"))?);
            let (scale, offset) = native::material_uv(store, original)?;
            Ok(vec![Part {
                name: format!("Hat_{value}"),
                geometry,
                texture: Some(texture),
                scale,
                offset,
                material_floats,
            }])
        })()
    } else {
        native::hat(store, tree, &context.hat_value)
    };
    match result {
        Ok(parts) => prepared.parts.extend(parts),
        Err(e) => prepared.diagnostics.push(e),
    }
}
fn custom_part(
    store: &mut Store,
    tree: &Tree,
    dir: &Path,
    path: &Path,
    name: &str,
    template: &str,
) -> Result<Part, String> {
    let mut geometry = obj::parse(&text(path)?)?;
    chef_parts::apply(&mut geometry, tree, name)?;
    let texture = png(dir, &part_resource(dir, name, "t", "png")?)?;
    let root = native::head_index(tree, template)?;
    let original = original(tree, root, name)
        .or_else(|| {
            if name.starts_with("Body_") {
                tree.name("Body")
            } else {
                Some(&tree.nodes[root])
            }
        })
        .ok_or("diychef.previewUnavailable")?;
    let mut material_floats = native::material_floats(store, original)?;
    material_floats.extend(material(dir, &part_resource(dir, name, "m", "txt")?)?);
    if name.starts_with("Body_") {
        native::player_body_floats(store, &mut material_floats)?;
    }
    let (scale, offset) = native::material_uv(store, original)?;
    Ok(Part {
        name: name.into(),
        geometry,
        texture: Some(texture),
        scale,
        offset,
        material_floats,
    })
}
fn part_resource(dir: &Path, part: &str, prefix: &str, extension: &str) -> Result<String, String> {
    let dedicated = format!("{prefix}_{part}.{extension}");
    if file(dir, &dedicated)?.is_some() {
        return Ok(dedicated);
    }
    let body = format!("{prefix}_Body.{extension}");
    if part.starts_with("Body_") && file(dir, &body)?.is_some() {
        return Ok(body);
    }
    Ok(format!("{prefix}_Head.{extension}"))
}
fn original<'a>(tree: &'a Tree, head: usize, name: &str) -> Option<&'a assembly::Node> {
    match name {
        "Head" => Some(&tree.nodes[head]),
        "Knife" => tree.name("Knife"),
        _ => tree
            .nodes
            .iter()
            .enumerate()
            .find(|(i, node)| {
                tree.below(*i, head) && node.renderer.is_some() && node.name.starts_with(name)
            })
            .map(|(_, node)| node),
    }
}
fn body(dir: &Path, store: &Store) -> Result<(String, bool), String> {
    let mut template = store.default_template.clone();
    let mut warning = false;
    let Some(path) = file(dir, "INFO")? else {
        return Ok((template, false));
    };
    for name in text(&path)?.lines().filter_map(|l| l.strip_prefix("BODY=")) {
        if store.avatars.contains_key(name) {
            template = name.to_owned();
        } else {
            warning = true;
        }
    }
    Ok((template, warning))
}
pub(super) fn file(root: &Path, name: &str) -> Result<Option<PathBuf>, String> {
    let path = root.join(name);
    if !path.exists() {
        return Ok(None);
    }
    let root = root
        .canonicalize()
        .map_err(|_| "diychef.previewUnavailable")?;
    let p = path
        .canonicalize()
        .map_err(|_| "diychef.previewUnavailable")?;
    if !p.starts_with(root) || !p.is_file() {
        return Err("diychef.previewUnsafeResource".into());
    }
    Ok(Some(p))
}
fn bytes(path: &Path, max: usize) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|_| "diychef.previewUnavailable")?;
    if file
        .metadata()
        .map_err(|_| "diychef.previewUnavailable")?
        .len()
        > max as u64
    {
        return Err("diychef.previewLimit".into());
    }
    let mut bytes = vec![];
    file.take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "diychef.previewUnavailable")?;
    if bytes.len() > max {
        return Err("diychef.previewLimit".into());
    }
    Ok(bytes)
}
fn text(path: &Path) -> Result<String, String> {
    crate::diychef::prefer::decode(&bytes(path, 32 * 1024 * 1024)?)
        .map(|(text, _)| text)
        .map_err(|_| "diychef.previewInvalidObj".into())
}
fn png(dir: &Path, name: &str) -> Result<Vec<u8>, String> {
    let path = file(dir, name)?.ok_or("diychef.previewUnavailable")?;
    let raw = bytes(&path, 32 * 1024 * 1024)?;
    let mut reader = image::ImageReader::new(std::io::Cursor::new(&raw));
    reader.set_format(image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(96 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().map_err(|_| "diychef.previewUnavailable")?;
    // Keep normal-sized source PNGs exactly; re-encoding dominates warm hat latency.
    if image.width() <= 1024 && image.height() <= 1024 {
        return Ok(raw);
    }
    let image = image.thumbnail(1024, 1024);
    let mut out = std::io::Cursor::new(vec![]);
    image
        .write_to(&mut out, image::ImageFormat::Png)
        .map_err(|_| "diychef.previewUnavailable")?;
    Ok(out.into_inner())
}
fn material(dir: &Path, name: &str) -> Result<std::collections::BTreeMap<String, f32>, String> {
    let path = file(dir, name)?.ok_or("diychef.previewUnavailable")?;
    let mut values = std::collections::BTreeMap::new();
    for line in text(&path)?.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let Ok(value) = value.trim().parse::<f32>() else {
            continue;
        };
        if !value.is_finite() {
            continue;
        }
        values.insert(key.to_owned(), value);
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_texture_and_material_fall_back_independently_without_affecting_tail_or_hands() {
        let dir = std::env::temp_dir().join(format!("chef-part-fallback-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        for name in ["t_Body.png", "m_Body.txt", "m_Body_Top.txt"] {
            std::fs::write(dir.join(name), b"fixture").unwrap();
        }
        assert_eq!(
            part_resource(&dir, "Body_Top", "t", "png").unwrap(),
            "t_Body.png"
        );
        assert_eq!(
            part_resource(&dir, "Body_Top", "m", "txt").unwrap(),
            "m_Body_Top.txt"
        );
        assert_eq!(
            part_resource(&dir, "Body_Body", "m", "txt").unwrap(),
            "m_Body.txt"
        );
        for part in ["Tail", "Hand_Open_L", "Head1", "NotBody_Top"] {
            assert_eq!(part_resource(&dir, part, "t", "png").unwrap(), "t_Head.png");
            assert_eq!(part_resource(&dir, part, "m", "txt").unwrap(), "m_Head.txt");
        }
        for name in ["t_Body.png", "m_Body.txt", "m_Body_Top.txt"] {
            std::fs::remove_file(dir.join(name)).unwrap();
        }
        assert_eq!(
            part_resource(&dir, "Body_Top", "t", "png").unwrap(),
            "t_Head.png"
        );
        std::fs::remove_dir(dir).unwrap();
    }
}
