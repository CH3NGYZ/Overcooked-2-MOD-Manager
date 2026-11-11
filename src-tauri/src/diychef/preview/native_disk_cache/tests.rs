use super::*;
use crate::diychef::preview::geometry::Geometry;
use crate::models::ChefPreviewBounds;
use std::{
    collections::BTreeMap,
    io::Cursor,
    sync::atomic::{AtomicUsize, Ordering},
};

struct Temporary(PathBuf);
fn cache_guard() -> std::sync::MutexGuard<'static, ()> {
    super::super::TEST_CACHE_ACCESS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}
impl Temporary {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("native-disk-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn sample() -> Prepared {
    let mut png = Cursor::new(vec![]);
    image::DynamicImage::new_rgba8(2, 2)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    Prepared {
        parts: vec![Part {
            name: "Body".into(),
            geometry: Geometry {
                vertices: vec![
                    [-0.2, 0., 0., 0., 0., 1., 0., 0.],
                    [0.2, 0., 0., 0., 0., 1., 1., 0.],
                    [0., 0.4, 0., 0., 0., 1., 0.5, 1.],
                ],
                indices: vec![0, 1, 2],
            },
            texture: Some(png.into_inner()),
            scale: [0.8, 1.2],
            offset: [0.1, 0.2],
            material_floats: BTreeMap::from([("_Mode".into(), 0.), ("_MaskColorG".into(), 0.1)]),
        }],
        head_bounds: ChefPreviewBounds {
            min: [-0.2, 0., 0.],
            max: [0.2, 0.4, 0.],
        },
        diagnostics: vec![],
    }
}
fn assert_same(first: &Prepared, second: &Prepared) {
    assert_eq!(first.parts.len(), second.parts.len());
    assert_eq!(first.head_bounds.min, second.head_bounds.min);
    assert_eq!(first.head_bounds.max, second.head_bounds.max);
    for (a, b) in first.parts.iter().zip(&second.parts) {
        assert_eq!(a.name, b.name);
        assert_eq!(a.geometry.bytes(), b.geometry.bytes());
        assert_eq!(a.texture, b.texture);
        assert_eq!(a.scale, b.scale);
        assert_eq!(a.offset, b.offset);
        assert_eq!(a.material_floats, b.material_floats);
    }
}

#[test]
fn complete_scene_survives_new_owners_without_reopening_unity() {
    let _guard = cache_guard();
    let temp = Temporary::new();
    let expected = sample();
    let key = key(&[b"scene", b"resource stamp", b"Chef_Black_cat", b"None"]);
    let epoch = super::super::cache_epoch();
    let prepared = AtomicUsize::new(0);
    let first = scene_in(&temp.0, &key, epoch, || {
        prepared.fetch_add(1, Ordering::Relaxed);
        Ok(sample())
    })
    .unwrap();
    assert_same(&first, &expected);
    drop(first);
    // No module cache or live Store survives: the only reusable state is a file.
    let second = scene_in(&temp.0, &key, epoch, || {
        panic!("disk hit must precede Store creation")
    })
    .unwrap();
    assert_same(&expected, &second);
    assert_eq!(prepared.load(Ordering::Relaxed), 1);
}

#[test]
fn metadata_stamp_and_hat_identity_invalidate_without_hashing_resource_contents() {
    let _guard = cache_guard();
    let temp = Temporary::new();
    let source = temp.0.join("bundle");
    fs::write(&source, b"old").unwrap();
    let stamp = || super::super::fingerprint::files(std::slice::from_ref(&source)).unwrap();
    let before = key(&[b"scene", stamp().as_bytes(), b"native", b"None"]);
    let epoch = super::super::cache_epoch();
    scene_in(&temp.0, &before, epoch, || Ok(sample())).unwrap();
    fs::write(&source, b"changed-size").unwrap();
    let after = key(&[b"scene", stamp().as_bytes(), b"native", b"None"]);
    assert_ne!(before, after);
    assert!(load_in(&temp.0, &after, epoch).unwrap().is_none());
    assert_ne!(
        after,
        key(&[b"scene", stamp().as_bytes(), b"native", b"Santa"])
    );
}

#[test]
fn damaged_truncated_oversized_or_foreign_records_are_disposable_misses() {
    let _guard = cache_guard();
    let temp = Temporary::new();
    let epoch = super::super::cache_epoch();
    let key = key(&[b"complete"]);
    let path = file(&temp.0, &key).unwrap();
    for damage in 0..4 {
        let scene = sample();
        save_in(&temp.0, &key, &scene.parts, &scene.head_bounds, epoch).unwrap();
        let original = fs::read(&path).unwrap();
        match damage {
            0 => fs::write(&path, &original[..original.len() - 1]).unwrap(),
            1 => {
                let mut bytes = original;
                let last = bytes.len() - 1;
                bytes[last] ^= 1;
                fs::write(&path, bytes).unwrap();
            }
            2 => {
                let mut bytes = original;
                bytes[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
                fs::write(&path, bytes).unwrap();
            }
            _ => fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .unwrap()
                .set_len(MAX_RECORD + 1)
                .unwrap(),
        }
        assert!(load_in(&temp.0, &key, epoch).unwrap().is_none());
        assert!(!path.exists());
    }
    let scene = sample();
    save_in(&temp.0, &key, &scene.parts, &scene.head_bounds, epoch).unwrap();
    let foreign = super::key(&[b"different"]);
    fs::copy(&path, file(&temp.0, &foreign).unwrap()).unwrap();
    assert!(load_in(&temp.0, &foreign, epoch).unwrap().is_none());
    assert!(file(&temp.0, "../outside").is_err());
    assert!(file(&temp.0, &"x".repeat(64)).is_err());
}

#[test]
fn partial_invalid_geometry_and_stale_epochs_never_become_persistent_hits() {
    let _guard = cache_guard();
    let temp = Temporary::new();
    let epoch = super::super::cache_epoch();
    let key = key(&[b"partial"]);
    let mut partial = sample();
    partial
        .diagnostics
        .push("diychef.previewUnavailable".into());
    scene_in(&temp.0, &key, epoch, || Ok(partial)).unwrap();
    assert!(!file(&temp.0, &key).unwrap().exists());
    let mut invalid = sample();
    invalid.parts[0].geometry.indices[2] = 999;
    assert!(save_in(&temp.0, &key, &invalid.parts, &invalid.head_bounds, epoch).is_err());
    invalid = sample();
    invalid.parts[0].geometry.vertices[0][0] = f32::NAN;
    assert!(save_in(&temp.0, &key, &invalid.parts, &invalid.head_bounds, epoch).is_err());
    let good = sample();
    assert!(
        save_in(
            &temp.0,
            &key,
            &good.parts,
            &good.head_bounds,
            epoch.wrapping_add(1)
        )
        .is_err()
    );
    assert!(!file(&temp.0, &key).unwrap().exists());
}

#[test]
fn shared_cleaner_clears_both_native_records_and_atomic_leftovers_but_preserves_other_files() {
    let _guard = cache_guard();
    let temp = Temporary::new();
    let native = temp.0.join("diychef-native");
    let key = key(&[b"scene"]);
    let epoch = super::super::cache_epoch();
    let scene = sample();
    save_in(&native, &key, &scene.parts, &scene.head_bounds, epoch).unwrap();
    fs::write(
        native.join(format!("{}.tmp", uuid::Uuid::new_v4())),
        b"interrupted",
    )
    .unwrap();
    fs::write(native.join("prefer.txt"), b"never modify user data").unwrap();
    let nested = native.join("nested");
    fs::create_dir(&nested).unwrap();
    fs::write(nested.join("keep-fixture.bin"), b"keep").unwrap();
    assert_eq!(disk::clear_in(&native).unwrap(), 1);
    assert_eq!(
        fs::read(native.join("prefer.txt")).unwrap(),
        b"never modify user data"
    );
    assert_eq!(fs::read(nested.join("keep-fixture.bin")).unwrap(), b"keep");
    assert!(load_in(&native, &key, epoch).unwrap().is_none());
}

#[test]
fn concurrent_atomic_writers_leave_one_complete_record_without_temporaries() {
    let _guard = cache_guard();
    let temp = Temporary::new();
    let key = key(&[b"concurrent"]);
    let epoch = super::super::cache_epoch();
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                let sample = sample();
                save_in(&temp.0, &key, &sample.parts, &sample.head_bounds, epoch).unwrap();
            });
        }
    });
    assert_same(&sample(), &load_in(&temp.0, &key, epoch).unwrap().unwrap());
    assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 1);
}

#[test]
fn part_identity_survives_different_worker_indices_and_tracks_pose_and_stamp() {
    let mut store = Store::test_seed();
    store.disk_stamp = "same exact file metadata".into();
    fn document(source: Source) -> std::sync::Arc<super::super::native::reader::Document> {
        std::sync::Arc::new(super::super::native::reader::Document {
            name: "CAB-fixture".into(),
            source,
            file: unityfs::SerializedFile {
                data: unityfs::reader::ByteSource::Heap(std::sync::Arc::from(Vec::<u8>::new())),
                version: 17,
                endian: unityfs::reader::Endian::Little,
                unity_version: Default::default(),
                target_platform: 0,
                enable_type_tree: false,
                types: vec![],
                objects: vec![],
                externals: vec![],
                object_map: Default::default(),
                ref_types: vec![],
            },
        })
    }
    store.docs = vec![
        document(Source::Bundle(
            "fixture/bundleA".into(),
            "CAB-renderer".into(),
        )),
        document(Source::Bundle(
            "fixture/bundleB".into(),
            "CAB-material".into(),
        )),
    ];
    let renderer = super::super::native::reader::Ref { file: 0, id: 42 };
    let material = super::super::native::reader::Ref { file: 1, id: 21 };
    let original = part_key(
        &store,
        renderer,
        material,
        None,
        &super::super::assembly::IDENTITY,
        false,
    )
    .unwrap();
    let mut worker = Store::test_seed();
    worker.disk_stamp = store.disk_stamp.clone();
    worker.docs = store.docs.clone();
    worker.docs.swap(0, 1);
    let shifted = part_key(
        &worker,
        super::super::native::reader::Ref {
            file: 1,
            ..renderer
        },
        super::super::native::reader::Ref {
            file: 0,
            ..material
        },
        None,
        &super::super::assembly::IDENTITY,
        false,
    )
    .unwrap();
    assert_eq!(
        original, shifted,
        "worker-local Ref.file must never become disk identity"
    );
    assert_ne!(
        Some(original.clone()),
        part_key(
            &store,
            renderer,
            material,
            Some(renderer),
            &super::super::assembly::IDENTITY,
            false
        )
    );
    let mut changed_pose = super::super::assembly::IDENTITY;
    changed_pose[3] = 0.25;
    assert_ne!(
        Some(original.clone()),
        part_key(&store, renderer, material, None, &changed_pose, false)
    );
    store.disk_stamp.push_str("changed");
    assert_ne!(
        Some(original),
        part_key(
            &store,
            renderer,
            material,
            None,
            &super::super::assembly::IDENTITY,
            false
        )
    );
}

#[test]
#[ignore = "requires installed Windows game; writes only UUID temporary cache"]
fn real_native_scene_roundtrips_without_store_after_metadata_match() {
    let _guard = cache_guard();
    let game = PathBuf::from("E:/SteamLibrary/steamapps/common/Overcooked! 2");
    assert!(game.is_dir(), "real resource fixture missing");
    let temp = Temporary::new();
    let epoch = super::super::cache_epoch();
    let snapshot = super::super::metadata::game(&game).unwrap();
    let key = key(&[
        b"scene",
        snapshot.stamp.as_bytes(),
        b"Chef_Racoon",
        b"Santa",
    ]);
    let first_start = std::time::Instant::now();
    let first = scene_at(
        &game,
        "Chef_Racoon",
        "Santa",
        epoch,
        Ok(temp.0.clone()),
        || {
            super::super::native::with_store(&game, epoch, |store| {
                // The real test writes all cache artifacts to this UUID directory.
                store.disk_stamp.clear();
                let tree = super::super::native::tree(store, "Chef_Racoon")?;
                let mut scene = super::super::native::model_parts(store, &tree, "Chef_Racoon")?;
                scene
                    .parts
                    .extend(super::super::native::hat(store, &tree, "Santa")?);
                Ok(scene)
            })
        },
    )
    .unwrap();
    let first_elapsed = first_start.elapsed();
    assert!(first.diagnostics.is_empty());
    assert!(first.parts.iter().any(|p| p.name.starts_with("Wheelchair")));
    let started = std::time::Instant::now();
    let second = scene_at(
        &game,
        "Chef_Racoon",
        "Santa",
        epoch,
        Ok(temp.0.clone()),
        || panic!("warm resource disk scene must not read bundles"),
    )
    .unwrap();
    let disk_elapsed = started.elapsed();
    assert_same(&first, &second);
    eprintln!(
        "real native persisted parts={} first={first_elapsed:?} disk={disk_elapsed:?} record_bytes={}",
        first.parts.len(),
        fs::metadata(file(&temp.0, &key).unwrap()).unwrap().len()
    );
}
