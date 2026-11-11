//! Admission estimates, not an allocator/driver memory measurement.
//! PNG dimensions are read without decoding a second RGBA image.

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
const FALLBACK_PIXELS: usize = 1024 * 1024;

pub(super) fn part(geometry_bytes: usize, png: Option<&[u8]>) -> usize {
    // Geometry has CPU attributes/indices and a GPU mirror. Keep the encoded
    // PNG, CPU RGBA and GPU RGBA+mipmaps, with headroom for upload/decoder copies.
    let geometry = geometry_bytes.saturating_mul(2);
    let texture = png.map_or(0, |png| {
        png.len()
            .saturating_add(pixels(png).saturating_mul(4).saturating_mul(3))
    });
    geometry.saturating_add(texture)
}

fn pixels(png: &[u8]) -> usize {
    if png.len() < 33
        || &png[..8] != PNG_SIGNATURE
        || png[8..12] != 13u32.to_be_bytes()
        || &png[12..16] != b"IHDR"
    {
        // All producers cap preview PNGs to 1024x1024. Invalid/missing dimensions
        // must not turn a decoded texture into an estimate of zero.
        return FALLBACK_PIXELS;
    }
    let width = u32::from_be_bytes(png[16..20].try_into().unwrap()) as usize;
    let height = u32::from_be_bytes(png[20..24].try_into().unwrap()) as usize;
    if width == 0 || height == 0 {
        FALLBACK_PIXELS
    } else {
        width.saturating_mul(height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = vec![0; 33];
        bytes[..8].copy_from_slice(PNG_SIGNATURE);
        bytes[8..12].copy_from_slice(&13u32.to_be_bytes());
        bytes[12..16].copy_from_slice(b"IHDR");
        bytes[16..20].copy_from_slice(&width.to_be_bytes());
        bytes[20..24].copy_from_slice(&height.to_be_bytes());
        bytes
    }

    #[test]
    fn counts_geometry_cpu_gpu_and_png_decoded_storage_without_a_decode() {
        assert_eq!(part(108, None), 216);
        let bytes = header(16, 8);
        assert_eq!(part(108, Some(&bytes)), 216 + 33 + 16 * 8 * 4 * 3);
        let full = header(1024, 1024);
        assert_eq!(part(0, Some(&full)), 33 + 12 * 1024 * 1024);
        assert_eq!(part(0, None), 0);
    }

    #[test]
    fn malformed_headers_are_conservative_and_arithmetic_never_wraps() {
        for bytes in [vec![], header(0, 8), header(16, 0), vec![0; 64]] {
            assert_eq!(part(0, Some(&bytes)), bytes.len() + FALLBACK_PIXELS * 12);
        }
        assert_eq!(part(usize::MAX, None), usize::MAX);
        assert_eq!(part(1, Some(&header(u32::MAX, u32::MAX))), usize::MAX);
    }

    #[test]
    fn older_scene_json_defaults_missing_estimate_to_zero() {
        let scene = super::super::unavailable("fixture".into(), "unavailable".into());
        let mut json = serde_json::to_value(scene).unwrap();
        json.as_object_mut().unwrap().remove("estimated_bytes");
        let legacy: crate::models::ChefPreviewScene = serde_json::from_value(json).unwrap();
        assert_eq!(legacy.estimated_bytes, 0);
    }
}
