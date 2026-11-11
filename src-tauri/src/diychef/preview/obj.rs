//! The plugin's small OBJ dialect, including relative indices and empty parts.
use super::geometry::Geometry;
use std::collections::HashMap;

pub(crate) fn parse(text: &str) -> Result<Geometry, String> {
    let mut positions = vec![];
    let mut normals = vec![];
    let mut uvs = vec![];
    let mut geometry = Geometry::default();
    let mut lookup = HashMap::<(usize, Option<usize>, Option<usize>), u32>::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or_default();
        let mut words = line.split_whitespace();
        match words.next() {
            Some("v") => positions.push(vector::<3>(&mut words)?),
            Some("vn") => normals.push(vector::<3>(&mut words)?),
            Some("vt") => uvs.push(vector::<2>(&mut words)?),
            Some("f") => {
                let mut face = vec![];
                for token in words {
                    let parts: Vec<_> = token.split('/').collect();
                    let p = index(parts.first().copied().unwrap_or_default(), positions.len())?;
                    let uv = optional(parts.get(1).copied(), uvs.len())?;
                    let n = optional(parts.get(2).copied(), normals.len())?;
                    let key = (p, uv, n);
                    let next = geometry.vertices.len() as u32;
                    let id = *lookup.entry(key).or_insert_with(|| {
                        let p = positions[p];
                        let n = n.map(|i| normals[i]).unwrap_or([0.0, 0.0, 1.0]);
                        let uv = uv.map(|i| uvs[i]).unwrap_or([0.0; 2]);
                        geometry
                            .vertices
                            .push([p[0], p[1], p[2], n[0], n[1], n[2], uv[0], uv[1]]);
                        next
                    });
                    face.push(id);
                }
                if face.len() < 3 {
                    return Err("diychef.previewInvalidObj".into());
                }
                for i in 1..face.len() - 1 {
                    geometry.indices.extend([face[0], face[i], face[i + 1]]);
                }
            }
            _ => {}
        }
        if geometry.vertices.len() > 1_000_000 || geometry.indices.len() > 6_000_000 {
            return Err("diychef.previewLimit".into());
        }
    }
    geometry.reflect_x();
    Ok(geometry)
}
fn vector<const N: usize>(words: &mut std::str::SplitWhitespace<'_>) -> Result<[f32; N], String> {
    let mut values = [0.0_f32; N];
    for value in &mut values {
        *value = words
            .next()
            .ok_or("diychef.previewInvalidObj")?
            .parse()
            .map_err(|_| "diychef.previewInvalidObj")?;
        if !value.is_finite() {
            return Err("diychef.previewInvalidObj".into());
        }
    }
    Ok(values)
}
fn index(text: &str, count: usize) -> Result<usize, String> {
    let number: i64 = text.parse().map_err(|_| "diychef.previewInvalidObj")?;
    let absolute = if number > 0 {
        number - 1
    } else {
        count as i64 + number
    };
    if number == 0 || absolute < 0 || absolute >= count as i64 {
        return Err("diychef.previewInvalidObj".into());
    }
    Ok(absolute as usize)
}
fn optional(text: Option<&str>, count: usize) -> Result<Option<usize>, String> {
    match text {
        None | Some("") => Ok(None),
        Some(v) => index(v, count).map(Some),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn triangulates_relative_faces_and_reflects_once() {
        let mesh = parse("v 1 0 0\nv 0 1 0\nv 0 0 1\nf -3 -2 -1").unwrap();
        assert_eq!(mesh.vertices[0][0], -1.0);
        assert_eq!(mesh.indices, [0, 2, 1]);
        assert!(parse("# optional hidden mesh").unwrap().vertices.is_empty());
        assert!(parse("v NaN 0 0").is_err());
        assert!(parse("v 0 0 0\nf 1 2 3").is_err());
    }
}
