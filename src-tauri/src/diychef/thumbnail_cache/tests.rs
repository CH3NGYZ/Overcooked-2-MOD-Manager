use super::*;

#[test]
fn fixed_cache_children_reject_files_and_atomic_failure_leaves_no_record() {
    let root = std::env::temp_dir().join(format!("chef-fixed-cache-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    assert_eq!(
        directory_under(&root, CacheDirectory::Native).unwrap(),
        root.join("cache/diychef-native")
    );
    fs::write(root.join("cache"), b"preserve").unwrap();
    assert_eq!(
        directory_under(&root, CacheDirectory::Native),
        Err("diychef.previewUnsafeResource".into())
    );
    fs::remove_file(root.join("cache")).unwrap();
    let directory = root.join("cache/diychef-native");
    let file = directory.join(format!("{}.bin", "a".repeat(64)));
    assert!(
        write_atomic(&directory, &file, |output| {
            output.write_all(b"partial")?;
            Err(std::io::Error::other("fixture interruption"))
        })
        .is_err()
    );
    assert!(!file.exists());
    assert_eq!(fs::read_dir(&directory).unwrap().count(), 0);
    assert!(write_atomic(&directory, &root.join("prefer.txt"), |_| Ok(())).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn persistent_hit_miss_revision_and_input_boundary() {
    let directory = std::env::temp_dir().join(format!("chef-png-{}", uuid::Uuid::new_v4()));
    let mut png = Cursor::new(vec![]);
    image::DynamicImage::new_rgba8(512, 640)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    assert!(
        access_in(&directory, "fox-head-v1:abc", None, false)
            .unwrap()
            .is_empty()
    );
    access_in(&directory, "fox-head-v1:abc", Some(png.get_ref()), true).unwrap();
    let cached = access_in(&directory, "fox-head-v1:abc", None, false).unwrap();
    assert_eq!(
        cached[0], 1,
        "partial previews must retain their status across restarts"
    );
    assert_eq!(cached[1..], *png.get_ref());
    assert!(
        access_in(&directory, "fox-head-v1:changed", None, false)
            .unwrap()
            .is_empty()
    );
    for invalid in ["../outside", "C:\\data", "", "bad/name"] {
        assert!(access_in(&directory, invalid, None, false).is_err());
    }
    assert!(access_in(&directory, "bad-image", Some(b"invalid"), false).is_err());
    let mut small = Cursor::new(vec![]);
    image::DynamicImage::new_rgba8(1, 1)
        .write_to(&mut small, image::ImageFormat::Png)
        .unwrap();
    assert!(access_in(&directory, "small-image", Some(small.get_ref()), false).is_err());
    // A damaged cache entry becomes a miss instead of a permanent broken tile.
    let file = directory.join(format!("{:x}.bin", Sha256::digest(b"fox-head-v1:abc")));
    fs::write(&file, b"broken").unwrap();
    assert!(
        access_in(&directory, "fox-head-v1:abc", None, false)
            .unwrap()
            .is_empty()
    );
    // A valid IHDR is insufficient: a truncated IDAT must also be rejected.
    let truncated = &png.get_ref()[..png.get_ref().len() / 2];
    assert!(access_in(&directory, "truncated-image", Some(truncated), false).is_err());
    let mut damaged = png.get_ref().clone();
    let idat = damaged
        .windows(4)
        .position(|bytes| bytes == b"IDAT")
        .unwrap();
    damaged[idat + 9] ^= 0xff;
    assert!(access_in(&directory, "damaged-image", Some(&damaged), false).is_err());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn eviction_preserves_current_png_and_unrelated_files() {
    let directory = std::env::temp_dir().join(format!("chef-lru-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&directory).unwrap();
    let current = directory.join(format!("{}.bin", "a".repeat(64)));
    let old = directory.join(format!("{}.bin", "b".repeat(64)));
    fs::write(&old, [0; 32]).unwrap();
    fs::write(&current, [0; 32]).unwrap();
    fs::write(directory.join("other.txt"), "retain").unwrap();
    fs::write(directory.join("unrelated-file.bin"), "retain").unwrap();
    prune(&directory, &current, 32).unwrap();
    assert!(current.is_file());
    assert!(!old.exists());
    assert!(directory.join("other.txt").exists());
    assert_eq!(
        fs::read(directory.join("unrelated-file.bin")).unwrap(),
        b"retain"
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn clear_removes_generated_records_and_interrupted_writes_without_following_subdirectories() {
    let root = std::env::temp_dir().join(format!("chef-cache-clear-{}", uuid::Uuid::new_v4()));
    let directory = root.join("thumbnails");
    fs::create_dir_all(&directory).unwrap();
    let mut png = Cursor::new(vec![]);
    image::DynamicImage::new_rgba8(512, 640)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    for key in ["chef-body-v1:first", "chef-body-v1:second"] {
        access_in(&directory, key, Some(png.get_ref()), false).unwrap();
    }
    let temporary = directory.join(format!("{}.tmp", uuid::Uuid::new_v4()));
    fs::write(&temporary, b"interrupted").unwrap();
    fs::write(directory.join("unrelated-cache.bin"), b"preserve").unwrap();
    fs::write(root.join("outside-cache.bin"), b"preserve").unwrap();
    let nested = directory.join(format!("{}.bin", "f".repeat(64)));
    fs::create_dir(&nested).unwrap();
    fs::write(nested.join("inside-cache.bin"), b"preserve").unwrap();
    assert_eq!(clear_in(&directory).unwrap(), 2);
    assert!(!temporary.exists());
    assert_eq!(
        fs::read(directory.join("unrelated-cache.bin")).unwrap(),
        b"preserve"
    );
    assert_eq!(
        fs::read(nested.join("inside-cache.bin")).unwrap(),
        b"preserve"
    );
    assert_eq!(
        fs::read(root.join("outside-cache.bin")).unwrap(),
        b"preserve"
    );
    assert_eq!(clear_in(&directory).unwrap(), 0);
    assert_eq!(clear_in(&root.join("missing")).unwrap(), 0);
    assert!(
        access_at(
            &directory,
            "chef-body-v1:odd-epoch",
            Some(png.get_ref()),
            false,
            1
        )
        .is_err()
    );
    fs::remove_dir_all(root).unwrap();
}
