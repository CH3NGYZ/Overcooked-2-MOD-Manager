use super::*;
use crate::diychef::{preview, thumbnail_cache};
use std::{io::Cursor, sync::mpsc, time::Duration};

fn prepared(value: f32) -> Prepared {
    Prepared {
        parts: vec![Part {
            name: "Head".into(),
            geometry: Geometry {
                vertices: vec![[value; 8]; 3],
                indices: vec![0, 1, 2],
            },
            texture: None,
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

struct Temporary(std::path::PathBuf);
impl Temporary {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("chef-range-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }
    fn context(&self, session: &str) -> PreviewContext {
        PreviewContext {
            session_id: session.into(),
            game_root: self.0.clone(),
            chef_name: "immutable-range-fixture".into(),
            chef_directory: None,
            hat_value: "None".into(),
            hat_directory: None,
        }
    }
}
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn missing_disk_assets_invalidate_only_the_authorized_entry_and_normal_preparation_rebuilds() {
    let _guard = preview::TEST_CACHE_ACCESS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    clear();
    let root = Temporary::new();
    let directory = root.0.join("diychef-assets");
    let epoch = preview::cache_epoch();
    let context = root.context("authorized-editor");
    let scene = get_or_prepare(&context, epoch, &directory, || Ok(prepared(1.))).unwrap();
    let id = &scene.meshes[0].geometry_asset;
    assert_eq!(
        read_asset(&context.session_id, id).unwrap(),
        prepared(1.).parts[0].geometry.bytes()
    );
    assert!(read_asset(&context.session_id, &format!("{id}:suffix")).is_err());
    let file = std::fs::read_dir(&directory)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    std::fs::remove_file(file).unwrap();
    assert!(read_asset("unrelated-editor", id).is_err());
    assert!(
        cache()
            .lock()
            .unwrap()
            .entries
            .contains_key(&scene.fingerprint)
    );
    assert!(read_asset(&context.session_id, id).is_err());
    assert!(
        !cache()
            .lock()
            .unwrap()
            .entries
            .contains_key(&scene.fingerprint)
    );
    let rebuilt = get_or_prepare(&context, epoch, &directory, || Ok(prepared(2.))).unwrap();
    assert_eq!(rebuilt.fingerprint, scene.fingerprint);
    assert_eq!(
        read_asset(&context.session_id, id).unwrap(),
        prepared(2.).parts[0].geometry.bytes()
    );
    get_or_prepare(&context, epoch, &directory, || {
        panic!("rebuilt index must be reusable")
    })
    .unwrap();
    clear();
}

#[test]
fn concurrent_preparations_keep_exact_asset_authorization_for_both_live_editors() {
    let _guard = preview::TEST_CACHE_ACCESS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    clear();
    let root = Temporary::new();
    let directory = root.0.join("diychef-assets");
    let first = root.context("first-editor");
    let second = root.context("second-editor");
    let epoch = preview::cache_epoch();
    let (entered, receive_entry) = mpsc::channel();
    let (release, receive_release) = mpsc::channel();
    std::thread::scope(|scope| {
        let directory = &directory;
        let first = &first;
        let delayed = scope.spawn(move || {
            get_or_prepare(first, epoch, directory, || {
                entered.send(()).unwrap();
                receive_release
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap();
                Ok(prepared(1.))
            })
        });
        receive_entry.recv_timeout(Duration::from_secs(3)).unwrap();
        let second_scene = get_or_prepare(&second, epoch, directory, || Ok(prepared(1.))).unwrap();
        release.send(()).unwrap();
        let first_scene = delayed.join().unwrap().unwrap();
        assert_eq!(first_scene.fingerprint, second_scene.fingerprint);
        let id = &first_scene.meshes[0].geometry_asset;
        assert!(read_asset(&first.session_id, id).is_ok());
        assert!(read_asset(&second.session_id, id).is_ok());
        assert!(read_asset("third-editor", id).is_err());
        assert_eq!(cache().lock().unwrap().entries.len(), 1);
    });
    clear();
}

#[test]
fn clearing_invalidates_scene_assets_and_queued_png_writes_without_old_completion_repopulating() {
    let _guard = preview::TEST_CACHE_ACCESS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let root = std::env::temp_dir().join(format!("chef-clear-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let directory = root.join("thumbnails");
    let asset_directory = root.join("diychef-assets");
    let context = PreviewContext {
        session_id: "cache-clear-fixture".into(),
        game_root: root.clone(),
        chef_name: "initial".into(),
        chef_directory: None,
        hat_value: "None".into(),
        hat_directory: None,
    };
    let old_epoch = preview::cache_epoch();
    let first = get_or_prepare(&context, old_epoch, &asset_directory, || Ok(prepared(1.))).unwrap();
    let asset_id = &first.meshes[0].geometry_asset;
    assert_eq!(
        read_asset(&context.session_id, asset_id).unwrap(),
        prepared(1.).parts[0].geometry.bytes()
    );
    let mut png = Cursor::new(vec![]);
    image::DynamicImage::new_rgba8(512, 640)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    thumbnail_cache::access_at(
        &directory,
        "chef-body-v1:fixture",
        Some(png.get_ref()),
        false,
        old_epoch,
    )
    .unwrap();
    let native_directory = root.join("diychef-native");
    std::fs::create_dir(&native_directory).unwrap();
    std::fs::write(
        native_directory.join(format!("{}.bin", "a".repeat(64))),
        b"baked",
    )
    .unwrap();
    std::fs::write(
        native_directory.join(format!("{}.tmp", uuid::Uuid::new_v4())),
        b"unfinished",
    )
    .unwrap();

    let mut slow_context = context.clone();
    slow_context.chef_name = "in-flight".into();
    let (entered, receive_entry) = mpsc::channel();
    let (release, receive_release) = mpsc::channel();
    std::thread::scope(|scope| {
        let asset_directory = &asset_directory;
        let old = scope.spawn(move || {
            get_or_prepare(&slow_context, old_epoch, asset_directory, || {
                entered.send(()).unwrap();
                receive_release
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap();
                Ok(prepared(9.))
            })
        });
        receive_entry.recv_timeout(Duration::from_secs(3)).unwrap();
        assert_eq!(preview::clear_cache_in(&directory).unwrap(), 3);
        assert_eq!(std::fs::read_dir(&native_directory).unwrap().count(), 0);
        assert_eq!(std::fs::read_dir(asset_directory).unwrap().count(), 0);
        assert!(read_asset(&context.session_id, asset_id).is_err());
        assert!(
            get_or_prepare(&context, old_epoch, asset_directory, || panic!(
                "old task must be rejected before conversion"
            ))
            .is_err()
        );
        assert!(
            thumbnail_cache::access_at(
                &directory,
                "chef-body-v1:queued",
                Some(png.get_ref()),
                false,
                old_epoch
            )
            .is_err()
        );
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 0);

        let current_epoch = preview::cache_epoch();
        let fresh = get_or_prepare(&context, current_epoch, asset_directory, || {
            Ok(prepared(2.))
        })
        .unwrap();
        assert_eq!(
            first.fingerprint, fresh.fingerprint,
            "manual clear does not change resource identity"
        );
        assert_eq!(
            read_asset(&context.session_id, asset_id).unwrap(),
            prepared(2.).parts[0].geometry.bytes()
        );
        let warm = get_or_prepare(&context, current_epoch, asset_directory, || {
            panic!("new generation should be reusable")
        })
        .unwrap();
        assert_eq!(warm.fingerprint, fresh.fingerprint);
        {
            let mut c = cache().lock().unwrap();
            c.entries
                .get_mut(&fresh.fingerprint)
                .unwrap()
                .sessions
                .extend((0..1024).map(|id| format!("expired-editor-{id}")));
        }
        retain_sessions(&HashSet::from([context.session_id.clone()])).unwrap();
        assert_eq!(
            cache().lock().unwrap().entries[&fresh.fingerprint]
                .sessions
                .len(),
            1
        );
        assert!(read_asset(&context.session_id, asset_id).is_ok());
        assert!(read_asset("expired-editor-0", asset_id).is_err());
        assert!(read_asset("another-live-but-not-authorized-editor", asset_id).is_err());
        release.send(()).unwrap();
        assert!(old.join().unwrap().is_err());
        let c = cache().lock().unwrap();
        assert_eq!(
            c.entries.len(),
            1,
            "old conversion must not resurrect an entry"
        );
    });

    let bad_directory = root.join("regular-file");
    std::fs::write(&bad_directory, b"preserve").unwrap();
    assert_eq!(
        preview::clear_cache_in(&bad_directory),
        Err("diychef.previewUnsafeResource".into())
    );
    assert!(
        preview::require_epoch(preview::cache_epoch()).is_ok(),
        "failed disk cleanup must reopen admission"
    );
    get_or_prepare(&context, preview::cache_epoch(), &asset_directory, || {
        Ok(prepared(3.))
    })
    .unwrap();
    assert_eq!(std::fs::read(&bad_directory).unwrap(), b"preserve");
    preview::clear_cache_in(&directory).unwrap();
    std::fs::remove_dir_all(root).unwrap();
}
