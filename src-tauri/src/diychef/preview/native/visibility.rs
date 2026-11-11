use crate::diychef::preview::geometry::Geometry;

pub(super) fn is_blink(name: &str) -> bool {
    name.starts_with("Eyes2_Blinks")
}

pub(super) fn remove_blinks(geometry: &mut Geometry, hidden: &[bool]) {
    if !hidden.iter().any(|hidden| *hidden) {
        return;
    }
    // Native chef meshes merge the open eyes and the alternate closed-eye layer.
    // Their Eyes2_Blinks bone identifies the alternate layer even with one renderer.
    geometry.indices = geometry
        .indices
        .chunks_exact(3)
        .filter(|triangle| !triangle.iter().any(|index| hidden[*index as usize]))
        .flatten()
        .copied()
        .collect();
    let mut remap = vec![u32::MAX; geometry.vertices.len()];
    let mut vertices = Vec::with_capacity(geometry.vertices.len());
    for index in &mut geometry.indices {
        let old = *index as usize;
        if remap[old] == u32::MAX {
            remap[old] = vertices.len() as u32;
            vertices.push(geometry.vertices[old]);
        }
        *index = remap[old];
    }
    // Discard the closed-eye vertices too, so bounds only describe rendered parts.
    geometry.vertices = vertices;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merged_closed_eyes_are_removed_without_hiding_open_eyes_or_eyebrows() {
        let mut geometry = Geometry {
            vertices: (0..9)
                .map(|index| [index as f32, 0., 0., 0., 0., 1., 0., 0.])
                .collect(),
            // Open eyes, eyebrows, and the independent closed-eye layer.
            indices: vec![0, 1, 2, 3, 4, 5, 6, 7, 8],
        };
        remove_blinks(
            &mut geometry,
            &[false, false, false, false, false, false, true, true, true],
        );
        assert_eq!(geometry.indices, vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(geometry.vertices.len(), 6);
        assert_eq!(geometry.bounds().max[0], 5.);
        assert!(is_blink("Eyes2_Blinks"));
        assert!(is_blink("Eyes2_Blinks 1"));
        assert!(!is_blink("Eyes"));
        assert!(!is_blink("Eyebrows"));
    }

    #[test]
    fn ordinary_geometry_retains_its_original_index_and_vertex_order() {
        let mut geometry = Geometry {
            vertices: vec![[0.; 8], [1.; 8], [2.; 8]],
            indices: vec![2, 1, 0],
        };
        let before = geometry.bytes();
        remove_blinks(&mut geometry, &[false; 3]);
        assert_eq!(geometry.bytes(), before);
    }

    #[test]
    #[ignore = "requires installed Windows game assets; only reads game files"]
    fn real_native_merged_meshes_keep_only_open_eye_triangles() {
        use crate::diychef::preview::native;
        let game = std::path::Path::new("E:/SteamLibrary/steamapps/common/Overcooked! 2");
        native::with_store(game, |store| {
            // Independently inspected native m_Skin: closed-eye vertices have
            // weight 1 on Eyes2_Blinks and form disjoint triangle islands.
            for (name, visible_triangles) in [
                ("Chef_Black_cat", 4144),
                ("Chef_Male_Asian", 3823),
                ("Chef_Ghost", 3948),
            ] {
                let tree = native::tree_type(store, name, 1)?;
                let index = native::head_index(&tree, name)?;
                let node = &tree.nodes[index];
                let part = native::part(store, &tree, node, name, None, None)?;
                assert_eq!(part.geometry.indices.len() / 3, visible_triangles, "{name}");
                assert!(
                    part.geometry
                        .vertices
                        .iter()
                        .flatten()
                        .all(|v| v.is_finite())
                );
            }
            Ok(())
        })
        .unwrap();
    }
}
