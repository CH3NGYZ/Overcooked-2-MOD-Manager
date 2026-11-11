use super::*;

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
    let current = directory.join("current.bin");
    fs::write(directory.join("old.bin"), [0; 32]).unwrap();
    fs::write(&current, [0; 32]).unwrap();
    fs::write(directory.join("other.txt"), "retain").unwrap();
    prune(&directory, &current, 32).unwrap();
    assert!(current.is_file());
    assert!(!directory.join("old.bin").exists());
    assert!(directory.join("other.txt").exists());
    fs::remove_dir_all(directory).unwrap();
}
