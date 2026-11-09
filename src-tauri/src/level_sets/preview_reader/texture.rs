use super::{Context, Map, Ref, V, failure, int_of};
use base64::Engine;
use image::{DynamicImage, ImageFormat, RgbaImage, imageops};
fn number(m: &Map, k: &str) -> u32 {
    match m.get(k) {
        Some(V::Float(v)) if v.is_finite() && *v >= 0.0 => *v as u32,
        Some(V::UInt64(v)) => u32::try_from(*v).unwrap_or(u32::MAX),
        _ => int_of(m, k).max(0) as u32,
    }
}
impl Context {
    pub(super) fn image(&mut self, r: Ref, max_side: u32) -> Result<(String, u32, u32), String> {
        if let Some(image) = self.images.get(&(r.file, r.id, max_side)) {
            return Ok(image.clone());
        }
        let m = self.object(r)?;
        let rd = match m.get("m_RD") {
            Some(V::Map(rd)) => Some(rd),
            _ => None,
        };
        let (texture_ref, texture, rect, settings) = if let Some(rd) = rd {
            let t = self.resolve(r, rd.get("texture").ok_or_else(failure)?)?;
            (
                t,
                self.object(t)?,
                rd.get("textureRect").cloned(),
                number(rd, "settingsRaw"),
            )
        } else {
            (r, m, None, 0)
        };
        let width = number(&texture, "m_Width");
        let height = number(&texture, "m_Height");
        if width == 0
            || height == 0
            || width > 8192
            || height > 8192
            || u64::from(width) * u64::from(height) > 16_777_216
        {
            return Err(failure());
        }
        let cached = self
            .textures
            .get(&(texture_ref.file, texture_ref.id))
            .cloned();
        let decoded = if let Some(image) = cached {
            image
        } else {
            let inline = texture
                .get("image data")
                .or_else(|| texture.get("image_data"));
            let mut data = match inline {
                Some(V::Bytes(b)) => b.clone(),
                _ => vec![],
            };
            if let Some(V::Map(stream)) = texture.get("m_StreamData") {
                let size = number(stream, "size") as usize;
                if size > 0 {
                    let path = super::string_of(stream, "path");
                    let name = path.rsplit('/').next().ok_or_else(failure)?;
                    let offset = number(stream, "offset") as usize;
                    let end = offset.checked_add(size).ok_or_else(failure)?;
                    data = if let Some(bytes) = self.raw.get(name) {
                        bytes.get(offset..end).ok_or_else(failure)?.to_vec()
                    } else if let Some((path, node)) = self.streams.get(name) {
                        crate::asset_bundle::read_bundle_file_range(
                            path,
                            node,
                            offset,
                            Some(size),
                            Some(&mut self.blocks),
                        )
                        .map_err(|_| failure())?
                    } else {
                        return Err(failure());
                    };
                }
            }
            if data.is_empty() || data.len() > 128 * 1024 * 1024 {
                return Err(failure());
            }
            let rgba = std::panic::catch_unwind(|| {
                unityfs::decompress_texture(
                    width as usize,
                    height as usize,
                    int_of(&texture, "m_TextureFormat"),
                    &data,
                )
            })
            .map_err(|_| failure())?
            .ok_or_else(failure)?;
            let decoded =
                std::sync::Arc::new(RgbaImage::from_raw(width, height, rgba).ok_or_else(failure)?);
            let used: usize = self
                .textures
                .values()
                .map(|image| image.as_raw().len())
                .sum();
            if used + decoded.as_raw().len() <= 64 * 1024 * 1024 {
                self.textures
                    .insert((texture_ref.file, texture_ref.id), decoded.clone());
            }
            decoded
        };
        let mut image;
        if let Some(V::Map(rect)) = rect {
            let x = number(&rect, "x");
            let y = number(&rect, "y");
            let w = number(&rect, "width");
            let h = number(&rect, "height");
            if w == 0
                || h == 0
                || x.checked_add(w).is_none_or(|n| n > width)
                || y.checked_add(h).is_none_or(|n| n > height)
            {
                return Err(failure());
            }
            image = imageops::crop_imm(decoded.as_ref(), x, height - y - h, w, h).to_image();
            // Unity Sprite packing rotation (bits 2..5), with the crop already converted to top-left coordinates.
            image = match (settings >> 2) & 15 {
                0 => image,
                1 => imageops::flip_horizontal(&image),
                2 => imageops::flip_vertical(&image),
                3 => imageops::rotate180(&image),
                4 => imageops::rotate270(&image),
                _ => return Err(failure()),
            };
        } else {
            image = decoded.as_ref().clone();
        }
        // unityfs::decompress_texture already returns top-left RGBA.
        let (w, h) = image.dimensions();
        let image = DynamicImage::ImageRgba8(image);
        let image = if w > max_side || h > max_side {
            image.thumbnail(max_side, max_side)
        } else {
            image
        };
        let mut out = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut out, ImageFormat::Png)
            .map_err(|_| failure())?;
        if out.get_ref().len() > 8 * 1024 * 1024 {
            return Err(failure());
        }
        let result = (
            format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(out.into_inner())
            ),
            w,
            h,
        );
        if self.images.len() < 1024 {
            self.images.insert((r.file, r.id, max_side), result.clone());
        }
        Ok(result)
    }
}
