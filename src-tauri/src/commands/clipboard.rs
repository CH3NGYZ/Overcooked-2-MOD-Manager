use crate::models::OperationResult;

fn decode_clipboard_png(png: &[u8]) -> Result<image::RgbaImage, String> {
    if png.len() > 32 * 1024 * 1024 {
        return Err("PNG exceeds clipboard limit".into());
    }
    let mut reader =
        image::ImageReader::with_format(std::io::Cursor::new(png), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    reader
        .decode()
        .map(|i| i.to_rgba8())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn copy_image(png: Vec<u8>) -> Result<OperationResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let rgba = decode_clipboard_png(&png)?;
        let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        clipboard
            .set_image(arboard::ImageData {
                width: rgba.width() as usize,
                height: rgba.height() as usize,
                bytes: std::borrow::Cow::Owned(rgba.into_raw()),
            })
            .map_err(|e| e.to_string())?;
        Ok(OperationResult {
            ok: true,
            message: "common.action.copied".into(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clipboard_png_preserves_pixels_and_rejects_invalid_input() {
        let image = image::RgbaImage::from_pixel(2, 3, image::Rgba([12, 34, 56, 255]));
        let mut png = std::io::Cursor::new(Vec::new());
        image.write_to(&mut png, image::ImageFormat::Png).unwrap();
        assert_eq!(decode_clipboard_png(png.get_ref()).unwrap(), image);
        assert!(decode_clipboard_png(b"not png").is_err());
    }
}

#[tauri::command]
pub fn read_clipboard() -> Result<String, String> {
    log::debug!("读取剪贴板文本");
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.get_text().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn copy_text(text: String) -> Result<OperationResult, String> {
    log::debug!("复制文本到剪贴板：长度={}", text.len());
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_text(text).map_err(|e| e.to_string())?;
    Ok(OperationResult {
        ok: true,
        message: "common.action.copied".into(),
    })
}
