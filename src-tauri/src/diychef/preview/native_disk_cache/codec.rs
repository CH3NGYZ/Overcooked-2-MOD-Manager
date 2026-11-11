//! Streaming cache codec: only the small JSON descriptor has a temporary copy.
use super::{MAX_RECORD, Part, Prepared};
use crate::{diychef::preview::geometry::Geometry, models::ChefPreviewBounds};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{self, BufReader, BufWriter, Read, Write},
};

const MAGIC: &[u8; 8] = b"OC2NAT01";
const MAX_HEADER: usize = 128 * 1024;
const MAX_TEXTURE: usize = 8 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    key: String,
    bounds: ChefPreviewBounds,
    parts: Vec<Descriptor>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Descriptor {
    name: String,
    vertices: usize,
    indices: usize,
    texture: Option<usize>,
    scale: [f32; 2],
    offset: [f32; 2],
    floats: BTreeMap<String, f32>,
}

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid native cache")
}
fn bounds_valid(bounds: &ChefPreviewBounds) -> bool {
    (0..3).all(|i| {
        bounds.min[i].is_finite() && bounds.max[i].is_finite() && bounds.min[i] <= bounds.max[i]
    })
}
fn descriptor_valid(part: &Descriptor) -> bool {
    !part.name.is_empty()
        && part.name.len() <= 512
        && (1..=1_000_000).contains(&part.vertices)
        && (3..=6_000_000).contains(&part.indices)
        && part.indices % 3 == 0
        && part
            .texture
            .is_none_or(|size| (45..=MAX_TEXTURE).contains(&size))
        && part
            .scale
            .iter()
            .chain(&part.offset)
            .all(|value| value.is_finite())
        && part.floats.len() <= 128
        && part
            .floats
            .iter()
            .all(|(name, value)| name.len() <= 128 && value.is_finite())
}
fn payload_size(parts: &[Descriptor]) -> Option<usize> {
    parts.iter().try_fold(0usize, |sum, part| {
        sum.checked_add(part.vertices.checked_mul(32)?)?
            .checked_add(part.indices.checked_mul(4)?)?
            .checked_add(part.texture.unwrap_or(0))
    })
}

pub(super) fn header(
    key: &str,
    parts: &[Part],
    bounds: &ChefPreviewBounds,
) -> Result<Vec<u8>, String> {
    let header = Header {
        key: key.into(),
        bounds: bounds.clone(),
        parts: parts
            .iter()
            .map(|part| Descriptor {
                name: part.name.clone(),
                vertices: part.geometry.vertices.len(),
                indices: part.geometry.indices.len(),
                texture: part.texture.as_ref().map(Vec::len),
                scale: part.scale,
                offset: part.offset,
                floats: part.material_floats.clone(),
            })
            .collect(),
    };
    if !bounds_valid(bounds)
        || parts.is_empty()
        || parts.len() > 128
        || !header.parts.iter().all(descriptor_valid)
        || parts.iter().any(|part| {
            part.geometry
                .vertices
                .iter()
                .flatten()
                .any(|v| !v.is_finite())
                || part
                    .geometry
                    .indices
                    .iter()
                    .any(|i| *i as usize >= part.geometry.vertices.len())
                || part.texture.as_ref().is_some_and(|png| !png_valid(png))
        })
    {
        return Err("diychef.previewLimit".into());
    }
    let bytes = serde_json::to_vec(&header).map_err(|_| "diychef.previewUnavailable")?;
    if bytes.len() > MAX_HEADER
        || payload_size(&header.parts)
            .and_then(|size| size.checked_add(bytes.len() + 44))
            .is_none_or(|size| size as u64 > MAX_RECORD)
    {
        return Err("diychef.previewLimit".into());
    }
    Ok(bytes)
}

pub(super) fn write(output: &mut File, header: &[u8], parts: &[Part]) -> io::Result<()> {
    let mut out = BufWriter::new(output);
    let mut hash = Sha256::new();
    fn put(out: &mut impl Write, hash: &mut Sha256, bytes: &[u8]) -> io::Result<()> {
        out.write_all(bytes)?;
        hash.update(bytes);
        Ok(())
    }
    put(&mut out, &mut hash, MAGIC)?;
    put(&mut out, &mut hash, &(header.len() as u32).to_le_bytes())?;
    put(&mut out, &mut hash, header)?;
    for part in parts {
        for vertex in &part.geometry.vertices {
            let mut bytes = [0; 32];
            for (value, target) in vertex.iter().zip(bytes.chunks_exact_mut(4)) {
                target.copy_from_slice(&value.to_le_bytes());
            }
            put(&mut out, &mut hash, &bytes)?;
        }
        for index in &part.geometry.indices {
            put(&mut out, &mut hash, &index.to_le_bytes())?;
        }
        if let Some(png) = &part.texture {
            put(&mut out, &mut hash, png)?;
        }
    }
    out.write_all(&hash.finalize())?;
    out.flush()
}

pub(super) fn read(input: File, size: u64, key: &str) -> io::Result<Prepared> {
    if !(44..=MAX_RECORD).contains(&size) {
        return Err(invalid());
    }
    let mut input = BufReader::new(input);
    let mut remaining = size - 32;
    let mut hash = Sha256::new();
    fn take(
        input: &mut impl Read,
        hash: &mut Sha256,
        left: &mut u64,
        bytes: &mut [u8],
    ) -> io::Result<()> {
        *left = left.checked_sub(bytes.len() as u64).ok_or_else(invalid)?;
        input.read_exact(bytes)?;
        hash.update(bytes);
        Ok(())
    }
    let mut magic = [0; 8];
    take(&mut input, &mut hash, &mut remaining, &mut magic)?;
    if &magic != MAGIC {
        return Err(invalid());
    }
    let mut length = [0; 4];
    take(&mut input, &mut hash, &mut remaining, &mut length)?;
    let length = u32::from_le_bytes(length) as usize;
    if length == 0 || length > MAX_HEADER {
        return Err(invalid());
    }
    let mut bytes = vec![0; length];
    take(&mut input, &mut hash, &mut remaining, &mut bytes)?;
    let header: Header = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if header.key != key
        || !bounds_valid(&header.bounds)
        || header.parts.is_empty()
        || header.parts.len() > 128
        || !header.parts.iter().all(descriptor_valid)
        || payload_size(&header.parts).is_none_or(|len| len as u64 != remaining)
    {
        return Err(invalid());
    }
    drop(bytes);
    let mut parts = Vec::with_capacity(header.parts.len());
    for descriptor in header.parts {
        let mut vertices = Vec::with_capacity(descriptor.vertices);
        for _ in 0..descriptor.vertices {
            let mut bytes = [0; 32];
            take(&mut input, &mut hash, &mut remaining, &mut bytes)?;
            let vertex = std::array::from_fn(|i| {
                f32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap())
            });
            if vertex.iter().any(|value| !value.is_finite()) {
                return Err(invalid());
            }
            vertices.push(vertex);
        }
        let mut indices = Vec::with_capacity(descriptor.indices);
        for _ in 0..descriptor.indices {
            let mut bytes = [0; 4];
            take(&mut input, &mut hash, &mut remaining, &mut bytes)?;
            let index = u32::from_le_bytes(bytes);
            if index as usize >= descriptor.vertices {
                return Err(invalid());
            }
            indices.push(index);
        }
        let texture = if let Some(length) = descriptor.texture {
            let mut png = vec![0; length];
            take(&mut input, &mut hash, &mut remaining, &mut png)?;
            if !png_valid(&png) {
                return Err(invalid());
            }
            Some(png)
        } else {
            None
        };
        parts.push(Part {
            name: descriptor.name,
            geometry: Geometry { vertices, indices },
            texture,
            scale: descriptor.scale,
            offset: descriptor.offset,
            material_floats: descriptor.floats,
        });
    }
    let mut expected = [0; 32];
    input.read_exact(&mut expected)?;
    if remaining != 0 || hash.finalize().as_slice() != expected {
        return Err(invalid());
    }
    Ok(Prepared {
        parts,
        head_bounds: header.bounds,
        diagnostics: vec![],
    })
}

fn png_valid(bytes: &[u8]) -> bool {
    bytes.len() >= 45
        && bytes.len() <= MAX_TEXTURE
        && bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        && bytes.get(12..16) == Some(b"IHDR")
        && (1..=1024).contains(&u32::from_be_bytes(bytes[16..20].try_into().unwrap()))
        && (1..=1024).contains(&u32::from_be_bytes(bytes[20..24].try_into().unwrap()))
        && bytes.ends_with(b"\0\0\0\0IEND\xaeB`\x82")
}
