use super::reader::{Map, Ref, Store};
use crate::diychef::preview::{
    assembly::{Tree, array, multiply, number},
    geometry::{Geometry, transform},
};
use unityfs::{
    UnityValue as V,
    classes::{TryFromUnityValue, mesh::Mesh},
};

pub(super) fn read(
    store: &mut Store,
    tree: &Tree,
    renderer: Ref,
    skeleton: Option<&Tree>,
) -> Result<Geometry, String> {
    let r = store.object(renderer)?;
    let mesh_ref = store.resolve(
        renderer,
        r.get("m_Mesh").ok_or("diychef.previewUnavailable")?,
    )?;
    let map = store.object(mesh_ref)?;
    let mut mesh = Mesh::try_from_unity_value(&V::Map((*map).clone()))
        .map_err(|_| "diychef.previewUnavailable")?;
    if mesh.get_vertex_count() > 1_000_000 || mesh.m_IndexBuffer.len() > 24_000_000 {
        return Err("diychef.previewLimit".into());
    }
    if let Some(stream) = &mesh.m_StreamData {
        if stream.size > 0 {
            let bytes = store.stream(
                mesh_ref.file,
                &stream.path,
                stream.offset as usize,
                stream.size as usize,
            )?;
            mesh.m_VertexData.m_DataSize = Some(unityfs::classes::TypelessData(bytes));
        }
    }
    // Unity 2017 UV0 lives at channel 3. Swap channels only in the decoder copy.
    if mesh.m_VertexData.m_Channels.len() > 4
        && mesh.m_VertexData.m_Channels[3].dimension & 15 == 2
        && mesh.m_VertexData.m_Channels[4].dimension & 15 == 0
    {
        mesh.m_VertexData.m_Channels.swap(3, 4);
    }
    let positions = mesh
        .get_vertices()
        .map_err(|_| "diychef.previewUnavailable")?;
    let normals = mesh
        .get_normals()
        .map_err(|_| "diychef.previewUnavailable")?;
    let uvs = mesh
        .extract_uvs()
        .map_err(|_| "diychef.previewUnavailable")?;
    let all = mesh
        .get_indices()
        .map_err(|_| "diychef.previewUnavailable")?;
    let mut indices = vec![];
    let step = if mesh.m_IndexFormat == Some(1) { 4 } else { 2 };
    for sm in &mesh.m_SubMeshes {
        if sm.topology != 0 {
            continue;
        }
        let start = sm.firstByte as usize / step;
        let end = start
            .checked_add(sm.indexCount as usize)
            .ok_or("diychef.previewLimit")?;
        let ids = all.get(start..end).ok_or("diychef.previewUnavailable")?;
        for id in ids {
            indices.push(
                id.checked_add(sm.baseVertex)
                    .ok_or("diychef.previewLimit")?,
            );
        }
    }
    if indices.iter().any(|i| *i as usize >= positions.len())
        || normals.len() != positions.len()
        || uvs.len() != positions.len()
    {
        return Err("diychef.previewUnavailable".into());
    }
    let bones: Vec<_> = array(&r, "m_Bones")
        .iter()
        .map(|v| store.resolve(renderer, v))
        .collect::<Result<_, _>>()?;
    let blink_bones: Vec<_> = bones
        .iter()
        .map(|bone| {
            tree.by_ref
                .get(bone)
                .is_some_and(|index| super::visibility::is_blink(&tree.nodes[*index].name))
        })
        .collect();
    let bindposes = mesh.m_BindPose.as_deref().unwrap_or_default();
    let mut matrices = vec![];
    for (bone, bind) in bones.iter().zip(bindposes) {
        let world = if let Some(skeleton) = skeleton {
            let index = tree.by_ref.get(bone).ok_or("diychef.previewUnavailable")?;
            skeleton
                .name(&tree.nodes[*index].name)
                .map(|node| node.world)
        } else {
            tree.world(*bone)
        };
        matrices.push(multiply(
            &world.ok_or("diychef.previewUnavailable")?,
            &bind.e,
        ));
    }
    let old_skin = array(&map, "m_Skin");
    let mut vertices = Vec::with_capacity(positions.len());
    let mut blink_vertices = vec![false; positions.len()];
    for (i, p) in positions.iter().enumerate() {
        let n = normals[i];
        let uv = uvs[i];
        let (position, normal) = if !matrices.is_empty() {
            let weights = old_skin.get(i).ok_or("diychef.previewUnavailable")?;
            let V::Map(weights) = weights else {
                return Err("diychef.previewUnavailable".into());
            };
            let mut pos = [0.; 3];
            let mut normal = [0.; 3];
            let mut total = 0.;
            for j in 0..4 {
                let w = weights
                    .get(&format!("weight[{j}]"))
                    .map(number)
                    .unwrap_or_default();
                if w <= 0. {
                    continue;
                }
                let b = weights
                    .get(&format!("boneIndex[{j}]"))
                    .and_then(V::as_i64)
                    .ok_or("diychef.previewUnavailable")?;
                let matrix = matrices
                    .get(b as usize)
                    .ok_or("diychef.previewUnavailable")?;
                if blink_bones.get(b as usize).copied().unwrap_or(false) {
                    blink_vertices[i] = true;
                }
                let pp = transform(matrix, [p.x, p.y, p.z], 1.);
                let nn = transform(matrix, [n.x, n.y, n.z], 0.);
                for a in 0..3 {
                    pos[a] += pp[a] * w;
                    normal[a] += nn[a] * w;
                }
                total += w;
            }
            if total < 0.001 {
                return Err("diychef.previewUnavailable".into());
            }
            (pos, normal)
        } else {
            ([p.x, p.y, p.z], [n.x, n.y, n.z])
        };
        if position.iter().chain(&normal).any(|v| !v.is_finite()) {
            return Err("diychef.previewUnavailable".into());
        }
        vertices.push([
            position[0],
            position[1],
            position[2],
            normal[0],
            normal[1],
            normal[2],
            uv.0,
            uv.1,
        ]);
    }
    let mut geometry = Geometry { vertices, indices };
    super::visibility::remove_blinks(&mut geometry, &blink_vertices);
    Ok(geometry)
}

pub(super) fn material(store: &mut Store, renderer: Ref) -> Result<Ref, String> {
    let r = store.object(renderer)?;
    store.resolve(
        renderer,
        array(&r, "m_Materials")
            .first()
            .ok_or("diychef.previewUnavailable")?,
    )
}
pub(super) fn texture(
    store: &mut Store,
    material: Ref,
) -> Result<(Vec<u8>, [f32; 2], [f32; 2]), String> {
    let m = store.object(material)?;
    let Some(V::Map(properties)) = m.get("m_SavedProperties") else {
        return Err("diychef.previewUnavailable".into());
    };
    let env = diffuse(properties).ok_or("diychef.previewUnavailable")?;
    let t = store.resolve(
        material,
        env.get("m_Texture").ok_or("diychef.previewUnavailable")?,
    )?;
    let map = store.object(t)?;
    let mut bytes = match map.get("image data").or_else(|| map.get("image_data")) {
        Some(V::Bytes(b)) => b.clone(),
        _ => vec![],
    };
    if let Some(V::Map(s)) = map.get("m_StreamData") {
        let size = s.get("size").and_then(V::as_i64).unwrap_or_default();
        if size > 0 {
            let offset = s.get("offset").and_then(V::as_i64).unwrap_or_default();
            let name = s
                .get("path")
                .and_then(V::as_str)
                .ok_or("diychef.previewUnavailable")?;
            if offset < 0 || size > 128 * 1024 * 1024 {
                return Err("diychef.previewLimit".into());
            }
            bytes = store.stream(t.file, name, offset as usize, size as usize)?;
        }
    }
    let rgba = crate::unity_assets::decode_texture(&map, &bytes)?;
    let image = image::DynamicImage::ImageRgba8(rgba).thumbnail(1024, 1024);
    let mut out = std::io::Cursor::new(vec![]);
    image
        .write_to(&mut out, image::ImageFormat::Png)
        .map_err(|_| "diychef.previewUnavailable")?;
    Ok((
        out.into_inner(),
        xy(env, "m_Scale", [1., 1.]),
        xy(env, "m_Offset", [0., 0.]),
    ))
}
fn xy(map: &Map, key: &str, default: [f32; 2]) -> [f32; 2] {
    let Some(V::Map(v)) = map.get(key) else {
        return default;
    };
    [
        v.get("x").map(number).unwrap_or(default[0]),
        v.get("y").map(number).unwrap_or(default[1]),
    ]
}
pub(super) fn floats(
    store: &mut Store,
    material: Ref,
) -> Result<std::collections::BTreeMap<String, f32>, String> {
    let m = store.object(material)?;
    let Some(V::Map(properties)) = m.get("m_SavedProperties") else {
        return Err("diychef.previewUnavailable".into());
    };
    let mut values = std::collections::BTreeMap::new();
    for pair in array(properties, "m_Floats") {
        let V::Map(pair) = pair else { continue };
        let key = pair
            .get("first")
            .and_then(V::as_str)
            .ok_or("diychef.previewUnavailable")?;
        let value = pair
            .get("second")
            .map(number)
            .ok_or("diychef.previewUnavailable")?;
        if value.is_finite() {
            values.insert(key.to_owned(), value);
        }
    }
    Ok(values)
}
pub(super) fn uv(store: &mut Store, material: Ref) -> Result<([f32; 2], [f32; 2]), String> {
    let m = store.object(material)?;
    let Some(V::Map(properties)) = m.get("m_SavedProperties") else {
        return Err("diychef.previewUnavailable".into());
    };
    if let Some(env) = diffuse(properties) {
        return Ok((xy(env, "m_Scale", [1., 1.]), xy(env, "m_Offset", [0., 0.])));
    }
    Ok(([1., 1.], [0., 0.]))
}

fn diffuse(properties: &Map) -> Option<&Map> {
    for name in ["_DiffuseMap", "_Diffuse", "_MainTex"] {
        for pair in array(properties, "m_TexEnvs") {
            let V::Map(pair) = pair else { continue };
            if pair.get("first").and_then(V::as_str) == Some(name) {
                if let Some(V::Map(env)) = pair.get("second") {
                    return Some(env);
                }
            }
        }
    }
    None
}
