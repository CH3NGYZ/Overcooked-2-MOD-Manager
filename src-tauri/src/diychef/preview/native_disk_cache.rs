//! Disposable baked native parts/scenes. Nothing here retains model bytes in RAM.
mod codec;
use super::{
    cache::{Part, Prepared},
    native::reader::{Ref, Store},
};
use crate::{diychef::thumbnail_cache as disk, unity_assets::Source};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

const VERSION: &[u8] = b"native-baked-v1-openhands-blue";
const MAX_RECORD: u64 = 64 * 1024 * 1024;
const MAX_CACHE: u64 = 512 * 1024 * 1024;

pub(super) fn directory() -> Result<PathBuf, String> {
    disk::fixed_directory(disk::CacheDirectory::Native)
}

fn key(fields: &[&[u8]]) -> String {
    let mut hash = Sha256::new();
    hash.update(VERSION);
    for field in fields {
        hash.update((field.len() as u64).to_le_bytes());
        hash.update(field);
    }
    format!("{:x}", hash.finalize())
}

pub(super) fn scene(
    game: &Path,
    name: &str,
    hat: &str,
    epoch: u64,
    prepare: impl FnOnce() -> Result<Prepared, String>,
) -> Result<Prepared, String> {
    scene_at(game, name, hat, epoch, directory(), prepare)
}

pub(super) fn scene_at(
    game: &Path,
    name: &str,
    hat: &str,
    epoch: u64,
    directory: Result<PathBuf, String>,
    prepare: impl FnOnce() -> Result<Prepared, String>,
) -> Result<Prepared, String> {
    super::require_epoch(epoch)?;
    let snapshot = super::metadata::game(game)?;
    let key = key(&[
        b"scene",
        snapshot.stamp.as_bytes(),
        name.as_bytes(),
        hat.as_bytes(),
    ]);
    let prepare = || {
        let prepared = prepare()?;
        super::require_epoch(epoch)?;
        if super::metadata::game(game)?.stamp != snapshot.stamp {
            return Err("diychef.previewUnavailable".into());
        }
        Ok(prepared)
    };
    // A cache permission/path failure must not make otherwise readable game
    // resources unavailable. The real resource metadata remains mandatory.
    let result = match directory {
        Ok(directory) => scene_in(&directory, &key, epoch, prepare),
        Err(_) => prepare(),
    }?;
    super::require_epoch(epoch)?;
    if super::metadata::game(game)?.stamp != snapshot.stamp {
        return Err("diychef.previewUnavailable".into());
    }
    Ok(result)
}

fn scene_in(
    directory: &Path,
    key: &str,
    epoch: u64,
    prepare: impl FnOnce() -> Result<Prepared, String>,
) -> Result<Prepared, String> {
    if let Ok(Some(scene)) = load_in(directory, key, epoch) {
        return Ok(scene);
    }
    super::require_epoch(epoch)?;
    let prepared = prepare()?;
    super::require_epoch(epoch)?;
    if prepared.diagnostics.is_empty() {
        let _ = save_in(
            directory,
            key,
            &prepared.parts,
            &prepared.head_bounds,
            epoch,
        );
    }
    Ok(prepared)
}

/// Ref.file is worker-local. Persist only its physical source/node + pathID.
pub(super) fn part_key(
    store: &Store,
    renderer: Ref,
    material: Ref,
    pose: Option<Ref>,
    world: &[f32; 16],
    player_body: bool,
) -> Option<String> {
    if store.disk_stamp.is_empty() {
        return None;
    }
    let mut identity = Vec::new();
    for reference in [Some(renderer), Some(material), pose] {
        let Some(reference) = reference else {
            identity.push(0);
            continue;
        };
        identity.push(1);
        let document = store.docs.get(reference.file)?;
        let (path, node) = match &document.source {
            Source::File(path) => (path, ""),
            Source::Bundle(path, node) => (path, node.as_str()),
        };
        for bytes in [path.as_os_str().as_encoded_bytes(), node.as_bytes()] {
            identity.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
            identity.extend_from_slice(bytes);
        }
        identity.extend_from_slice(&reference.id.to_le_bytes());
    }
    for value in world {
        identity.extend_from_slice(&value.to_bits().to_le_bytes());
    }
    identity.push(u8::from(player_body));
    if player_body {
        for value in store.player_colour.as_ref()?.mask {
            identity.extend_from_slice(&value.to_bits().to_le_bytes());
        }
    }
    Some(key(&[b"part", store.disk_stamp.as_bytes(), &identity]))
}

pub(super) fn load_part(key: &str, name: &str, epoch: u64) -> Option<Part> {
    let mut scene = load_in(&directory().ok()?, key, epoch).ok()??;
    if scene.parts.len() != 1 {
        return None;
    }
    let mut part = scene.parts.pop()?;
    part.name = name.into();
    Some(part)
}

pub(super) fn save_part(key: &str, part: &Part, epoch: u64) {
    if let Ok(directory) = directory() {
        let _ = save_in(
            &directory,
            key,
            std::slice::from_ref(part),
            &part.geometry.bounds(),
            epoch,
        );
    }
}

fn file(directory: &Path, key: &str) -> Result<PathBuf, String> {
    if key.len() != 64 || !key.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("diychef.previewUnsafeResource".into());
    }
    Ok(directory.join(format!("{key}.bin")))
}

fn load_in(directory: &Path, key: &str, epoch: u64) -> Result<Option<Prepared>, String> {
    let _access = disk::lock_access()?;
    super::require_epoch(epoch)?;
    let path = file(directory, key)?;
    let Some(info) = disk::regular_file(&path)? else {
        return Ok(None);
    };
    let scene = if info.len() <= MAX_RECORD {
        fs::File::open(&path)
            .ok()
            .and_then(|input| codec::read(input, info.len(), key).ok())
    } else {
        None
    };
    if scene.is_none() {
        let _ = fs::remove_file(&path);
    } else {
        let _ = fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .and_then(|file| file.set_modified(SystemTime::now()));
    }
    super::require_epoch(epoch)?;
    Ok(scene)
}

fn save_in(
    directory: &Path,
    key: &str,
    parts: &[Part],
    bounds: &crate::models::ChefPreviewBounds,
    epoch: u64,
) -> Result<(), String> {
    let _access = disk::lock_access()?;
    super::require_epoch(epoch)?;
    let path = file(directory, key)?;
    if disk::regular_file(&path)?.is_some() {
        return Ok(());
    }
    let header = codec::header(key, parts, bounds)?;
    disk::write_atomic(directory, &path, |output| {
        codec::write(output, &header, parts)
    })?;
    disk::prune(directory, &path, MAX_CACHE)
}

#[cfg(test)]
mod tests;
