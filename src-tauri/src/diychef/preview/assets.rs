//! Scene transport keeps immutable disk ranges in RAM, never raw model buffers.
use super::{cache::Prepared, memory};
use crate::{
    diychef::thumbnail_cache as disk,
    models::{ChefPreviewMesh, ChefPreviewScene},
};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

const MAGIC: &[u8; 8] = b"OC2A0001";
const HEADER_BYTES: usize = 72;
const MAX_SCENE: usize = 64 * 1024 * 1024;
const MAX_CACHE: u64 = 512 * 1024 * 1024;

struct Backing {
    path: PathBuf,
    header: [u8; HEADER_BYTES],
    length: u64,
}

#[derive(Clone)]
pub(super) struct Asset {
    backing: Arc<Backing>,
    offset: u64,
    length: usize,
    digest: [u8; 32],
}

impl Asset {
    pub(super) fn shares_record(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.backing, &other.backing)
    }
}

pub(super) fn directory() -> Result<PathBuf, String> {
    disk::fixed_directory(disk::CacheDirectory::Assets)
}

fn unavailable() -> String {
    "diychef.previewUnavailable".into()
}

pub(super) fn transport(
    directory: &Path,
    key: &str,
    prepared: Prepared,
    epoch: u64,
) -> Result<(ChefPreviewScene, HashMap<String, Asset>), String> {
    if key.len() != 64 || !key.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("diychef.previewUnsafeResource".into());
    }
    let used = prepared
        .parts
        .iter()
        .filter(|p| !p.geometry.vertices.is_empty() && !p.geometry.indices.is_empty())
        .try_fold(0usize, |used, p| {
            used.checked_add(
                p.geometry
                    .vertices
                    .len()
                    .checked_mul(32)?
                    .checked_add(p.geometry.indices.len().checked_mul(4)?)?
                    .checked_add(p.texture.as_ref().map_or(0, Vec::len))?,
            )
        })
        .filter(|used| *used <= MAX_SCENE)
        .ok_or("diychef.previewLimit")?;
    let Prepared {
        parts,
        head_bounds,
        diagnostics,
    } = prepared;
    let mut scene = ChefPreviewScene {
        fingerprint: key.into(),
        scene_version: 1,
        estimated_bytes: 0,
        status: "unavailable".into(),
        meshes: vec![],
        head_bounds,
        diagnostics,
    };
    let mut assets = HashMap::new();
    if used == 0 {
        return Ok((scene, assets));
    }
    let path = directory.join(format!(
        "{:x}.bin",
        Sha256::digest(format!("chef-assets-v1:{key}").as_bytes())
    ));
    let mut header = [0; HEADER_BYTES];
    header[..8].copy_from_slice(MAGIC);
    header[8..].copy_from_slice(key.as_bytes());
    let backing = Arc::new(Backing {
        path: path.clone(),
        header,
        length: (HEADER_BYTES + used) as u64,
    });
    let _access = disk::lock_access()?;
    super::require_epoch(epoch)?;
    disk::regular_file(&path)?;
    disk::write_atomic(directory, &path, |output| {
        output.write_all(&header)?;
        let mut offset = HEADER_BYTES as u64;
        for (index, part) in parts.into_iter().enumerate() {
            if part.geometry.vertices.is_empty() || part.geometry.indices.is_empty() {
                continue;
            }
            let geometry_asset = format!("{key}:g{index}");
            let geometry = part.geometry.bytes();
            scene.estimated_bytes = scene
                .estimated_bytes
                .saturating_add(memory::part(geometry.len(), part.texture.as_deref()));
            output.write_all(&geometry)?;
            assets.insert(
                geometry_asset.clone(),
                Asset {
                    backing: backing.clone(),
                    offset,
                    length: geometry.len(),
                    digest: Sha256::digest(&geometry).into(),
                },
            );
            offset += geometry.len() as u64;
            // Only this part's serialization is transient; no combined scene
            // Vec and no second long-lived geometry/PNG cache is created.
            drop(geometry);
            let texture_asset = if let Some(texture) = part.texture {
                let id = format!("{key}:t{index}");
                output.write_all(&texture)?;
                assets.insert(
                    id.clone(),
                    Asset {
                        backing: backing.clone(),
                        offset,
                        length: texture.len(),
                        digest: Sha256::digest(&texture).into(),
                    },
                );
                offset += texture.len() as u64;
                Some(id)
            } else {
                None
            };
            scene.meshes.push(ChefPreviewMesh {
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
        Ok(())
    })?;
    super::require_epoch(epoch)?;
    disk::prune(directory, &path, MAX_CACHE)?;
    scene.status = if scene.diagnostics.is_empty() {
        "complete"
    } else {
        "partial"
    }
    .into();
    Ok((scene, assets))
}

pub(super) fn read(asset: &Asset, epoch: u64) -> Result<Vec<u8>, String> {
    if asset.length > MAX_SCENE
        || asset.offset < HEADER_BYTES as u64
        || asset
            .offset
            .checked_add(asset.length as u64)
            .is_none_or(|end| end > asset.backing.length)
    {
        return Err("diychef.previewLimit".into());
    }
    let _access = disk::lock_access()?;
    super::require_epoch(epoch)?;
    let path = &asset.backing.path;
    let Some(info) = disk::regular_file(path)? else {
        return Err(unavailable());
    };
    let result = (|| {
        if info.len() != asset.backing.length {
            return Err(unavailable());
        }
        let mut input = fs::File::open(path).map_err(|_| unavailable())?;
        let mut header = [0; HEADER_BYTES];
        input.read_exact(&mut header).map_err(|_| unavailable())?;
        if header != asset.backing.header {
            return Err(unavailable());
        }
        input
            .seek(SeekFrom::Start(asset.offset))
            .map_err(|_| unavailable())?;
        let mut bytes = vec![0; asset.length];
        input.read_exact(&mut bytes).map_err(|_| unavailable())?;
        let digest: [u8; 32] = Sha256::digest(&bytes).into();
        if digest != asset.digest {
            return Err(unavailable());
        }
        Ok(bytes)
    })();
    if result.is_err() {
        // A damaged disposable record must not become a permanent warm hit.
        let _ = fs::remove_file(path);
    } else {
        let _ = fs::OpenOptions::new()
            .write(true)
            .open(path)
            .and_then(|file| file.set_modified(std::time::SystemTime::now()));
    }
    super::require_epoch(epoch)?;
    result
}

#[cfg(test)]
mod tests;
