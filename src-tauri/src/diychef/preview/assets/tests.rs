use super::*;
use crate::diychef::preview::{cache::Part, geometry::Geometry};
use crate::models::ChefPreviewBounds;

struct Temporary(PathBuf);
impl Temporary {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("chef-assets-{}", uuid::Uuid::new_v4())))
    }
}
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn prepared(value: f32, texture: Option<Vec<u8>>) -> Prepared {
    Prepared {
        parts: vec![Part {
            name: "Head".into(),
            geometry: Geometry {
                vertices: vec![[value; 8]; 3],
                indices: vec![0, 1, 2],
            },
            texture,
            scale: [1.; 2],
            offset: [0.; 2],
            material_floats: Default::default(),
        }],
        head_bounds: ChefPreviewBounds {
            min: [0.; 3],
            max: [1.; 3],
        },
        diagnostics: vec![],
    }
}

#[test]
fn immutable_ranges_roundtrip_geometry_and_original_png_without_retaining_payloads() {
    let _guard = super::super::TEST_CACHE_ACCESS
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let root = Temporary::new();
    let epoch = super::super::cache_epoch();
    let png = vec![7; 4096];
    let input = prepared(2., Some(png.clone()));
    let expected_geometry = input.parts[0].geometry.bytes();
    let (scene, assets) = transport(&root.0, &"a".repeat(64), input, epoch).unwrap();
    assert_eq!(scene.status, "complete");
    assert_eq!(scene.meshes.len(), 1);
    let geometry = &assets[&scene.meshes[0].geometry_asset];
    let texture = &assets[scene.meshes[0].texture_asset.as_ref().unwrap()];
    assert!(geometry.shares_record(texture));
    assert_eq!(read(geometry, epoch).unwrap(), expected_geometry);
    assert_eq!(read(texture, epoch).unwrap(), png);
    assert_eq!(geometry.offset, HEADER_BYTES as u64);
    assert_eq!(texture.offset, HEADER_BYTES as u64 + 108);
    assert_eq!(geometry.backing.length, HEADER_BYTES as u64 + 108 + 4096);
    assert_eq!(scene.estimated_bytes, memory::part(108, Some(&png)));
    assert!(std::mem::size_of_val(geometry) < 128);
    assert_eq!(fs::read_dir(&root.0).unwrap().count(), 1);
    assert_eq!(disk::clear_in(&root.0).unwrap(), 1);
    assert!(read(geometry, epoch).is_err());
}

#[test]
fn damaged_missing_or_stale_records_are_rejected_before_returning_raw_assets() {
    let _guard = super::super::TEST_CACHE_ACCESS
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let root = Temporary::new();
    let epoch = super::super::cache_epoch();
    let key = "b".repeat(64);
    for damage in ["header", "payload", "truncated"] {
        let (scene, assets) = transport(&root.0, &key, prepared(1., None), epoch).unwrap();
        let asset = &assets[&scene.meshes[0].geometry_asset];
        let mut bytes = fs::read(&asset.backing.path).unwrap();
        match damage {
            "header" => bytes[0] ^= 1,
            "payload" => bytes[HEADER_BYTES] ^= 1,
            _ => {
                bytes.pop();
            }
        }
        fs::write(&asset.backing.path, bytes).unwrap();
        assert!(read(asset, epoch).is_err());
        assert!(!asset.backing.path.exists());
    }
    let (scene, assets) = transport(&root.0, &key, prepared(1., None), epoch).unwrap();
    let asset = &assets[&scene.meshes[0].geometry_asset];
    assert!(read(asset, epoch.wrapping_add(2)).is_err());
    assert!(asset.backing.path.exists());
    let mut overflowing = asset.clone();
    overflowing.offset = u64::MAX;
    assert!(read(&overflowing, epoch).is_err());
    assert!(transport(&root.0, "../outside", prepared(1., None), epoch).is_err());
    assert!(transport(&root.0, &key, prepared(1., None), epoch.wrapping_add(2)).is_err());
}

#[test]
fn later_records_and_disk_pruning_never_require_raw_asset_ram_or_remove_current_file() {
    let _guard = super::super::TEST_CACHE_ACCESS
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let root = Temporary::new();
    let epoch = super::super::cache_epoch();
    let (first, first_assets) =
        transport(&root.0, &"c".repeat(64), prepared(1., None), epoch).unwrap();
    let old = &first_assets[&first.meshes[0].geometry_asset];
    let expected = read(old, epoch).unwrap();
    let (next, next_assets) =
        transport(&root.0, &"d".repeat(64), prepared(2., None), epoch).unwrap();
    let current = &next_assets[&next.meshes[0].geometry_asset];
    assert_eq!(read(old, epoch).unwrap(), expected);
    fs::write(root.0.join("unrelated-cache.bin"), b"preserve").unwrap();
    {
        let _access = disk::lock_access().unwrap();
        disk::prune(&root.0, &current.backing.path, current.backing.length).unwrap();
    }
    assert!(read(old, epoch).is_err());
    assert_eq!(
        read(current, epoch).unwrap(),
        prepared(2., None).parts[0].geometry.bytes()
    );
    assert_eq!(
        fs::read(root.0.join("unrelated-cache.bin")).unwrap(),
        b"preserve"
    );
}
