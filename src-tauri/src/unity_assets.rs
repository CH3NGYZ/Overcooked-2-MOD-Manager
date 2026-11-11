//! Shared bounded Unity object/texture reading for map and chef previews.
mod sources;
mod texture;
pub(crate) use sources::{game_resource_roots, game_sources};
pub(crate) use texture::{decode_texture, texture_dimensions};

use crate::asset_bundle::{BundleBlockCache, try_read_serialized};
use std::{
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};
use unityfs::{Reader, SerializedFile, UnityValue};

#[derive(Clone, Debug)]
pub(crate) enum Source {
    File(PathBuf),
    Bundle(PathBuf, String),
}

pub(crate) fn failure() -> String {
    "assetBundle.previewUnavailable".into()
}

pub(crate) fn read_range(
    source: &Source,
    offset: usize,
    size: usize,
    cache: &mut BundleBlockCache,
) -> Result<Vec<u8>, String> {
    if size > 128 * 1024 * 1024 {
        return Err(failure());
    }
    match source {
        Source::Bundle(path, node) => {
            crate::asset_bundle::read_bundle_file_range(path, node, offset, Some(size), Some(cache))
                .map_err(|_| failure())
        }
        Source::File(path) => file_range(path, offset, size),
    }
}

pub(crate) fn file_range(path: &Path, offset: usize, size: usize) -> Result<Vec<u8>, String> {
    let mut file = std::fs::File::open(path).map_err(|_| failure())?;
    let end = offset.checked_add(size).ok_or_else(failure)? as u64;
    if end > file.metadata().map_err(|_| failure())?.len() || size > 128 * 1024 * 1024 {
        return Err(failure());
    }
    file.seek(SeekFrom::Start(offset as u64))
        .map_err(|_| failure())?;
    let mut bytes = vec![0; size];
    file.read_exact(&mut bytes).map_err(|_| failure())?;
    Ok(bytes)
}

/// Only the type/object directory is retained, never an entire large bundle.
pub(crate) fn metadata(
    source: &Source,
    cache: &mut BundleBlockCache,
) -> Result<SerializedFile, String> {
    let header = read_range(source, 0, 48, cache)?;
    let version = u32::from_be_bytes(header[8..12].try_into().unwrap());
    if !(9..=30).contains(&version) {
        return Err(failure());
    }
    let offset = if version >= 22 {
        usize::try_from(u64::from_be_bytes(header[32..40].try_into().unwrap()))
            .map_err(|_| failure())?
    } else {
        u32::from_be_bytes(header[12..16].try_into().unwrap()) as usize
    };
    if !(48..=16 * 1024 * 1024).contains(&offset) {
        return Err(failure());
    }
    let mut reader = Reader::new(
        read_range(source, 0, offset, cache)?,
        unityfs::UnityVersion::default(),
    );
    try_read_serialized(&mut reader).ok_or_else(failure)
}

pub(crate) fn object(
    file: &SerializedFile,
    object: &unityfs::objectreader::ObjectReader,
    backing: Option<&Source>,
    cache: &mut BundleBlockCache,
) -> Result<UnityValue, String> {
    if object.byte_size > 128 * 1024 * 1024 {
        return Err(failure());
    }
    let local_object;
    let mut reader = if let Some(backing) = backing {
        let bytes = read_range(backing, object.byte_start, object.byte_size, cache)?;
        // Typetree alignment is relative to the original file offset.
        let padding = object.byte_start % 16;
        let mut data = vec![0; padding];
        data.extend(bytes);
        local_object = unityfs::objectreader::ObjectReader::new(
            object.path_id,
            object.class_id,
            padding,
            object.byte_size,
            object.serialized_type.clone(),
            object.version,
        );
        let mut reader = Reader::new(data, file.unity_version.clone());
        reader.endian = file.endian;
        reader
    } else {
        local_object = unityfs::objectreader::ObjectReader::new(
            object.path_id,
            object.class_id,
            object.byte_start,
            object.byte_size,
            object.serialized_type.clone(),
            object.version,
        );
        Reader {
            data: file.data.clone(),
            pos: 0,
            unity_version: file.unity_version.clone(),
            endian: file.endian,
        }
    };
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        local_object.read_typetree(&mut reader, &file.ref_types)
    }))
    .map_err(|_| failure())?
    .map_err(|_| failure())
}
