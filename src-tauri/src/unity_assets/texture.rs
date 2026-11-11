use super::failure;
use crate::asset_bundle::int_of;
use std::collections::BTreeMap;
use unityfs::UnityValue as V;

pub(crate) fn texture_dimensions(map: &BTreeMap<String, V>) -> Result<(u32, u32), String> {
    let width = int_of(map, "m_Width").max(0) as u32;
    let height = int_of(map, "m_Height").max(0) as u32;
    if width == 0
        || height == 0
        || width > 8192
        || height > 8192
        || u64::from(width) * u64::from(height) > 16_777_216
    {
        return Err(failure());
    }
    Ok((width, height))
}

/// Decoder output already uses top-left pixels. Callers must not flip it again.
pub(crate) fn decode_texture(
    map: &BTreeMap<String, V>,
    data: &[u8],
) -> Result<image::RgbaImage, String> {
    let (width, height) = texture_dimensions(map)?;
    if data.is_empty() || data.len() > 128 * 1024 * 1024 {
        return Err(failure());
    }
    let rgba = std::panic::catch_unwind(|| {
        unityfs::decompress_texture(
            width as usize,
            height as usize,
            int_of(map, "m_TextureFormat"),
            data,
        )
    })
    .map_err(|_| failure())?
    .ok_or_else(failure)?;
    image::RgbaImage::from_raw(width, height, rgba).ok_or_else(failure)
}
