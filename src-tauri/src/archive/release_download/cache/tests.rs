use super::*;
use std::cell::Cell;
use std::sync::Barrier;

struct Fixture {
    dir: tempfile::TempDir,
    cache: ReleaseDownloadCache,
    staging: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let cache = ReleaseDownloadCache::new(dir.path().join("cache/release-downloads"));
        let staging = dir.path().join("stage");
        fs::create_dir(&staging).unwrap();
        Self {
            dir,
            cache,
            staging,
        }
    }

    fn remember(&self, key: &ReleaseCacheKey, bytes: &[u8]) {
        let mut source = tempfile::NamedTempFile::new_in(self.dir.path()).unwrap();
        source.write_all(bytes).unwrap();
        self.cache
            .remember_set(&[(key, source.path(), Some(bytes.len() as u64))], || false);
        assert_eq!(fs::read(source.path()).unwrap(), bytes);
    }

    fn restore(&self, key: &ReleaseCacheKey) -> Option<TempPath> {
        self.cache.restore(key, &self.staging, None, || false)
    }
}

fn key(scope: &str, version: &str, asset: &str, bytes: &[u8]) -> ReleaseCacheKey {
    let api_digest = format!("sha256:{:x}", Sha256::digest(bytes));
    ReleaseCacheKey::new(scope, version, asset, Some(&api_digest)).unwrap()
}

const MOD_REPO: &str = "github/example/example-mod";

#[test]
fn provider_hash_is_required_and_is_not_a_url_id_or_weak_checksum() {
    for digest in [
        None,
        Some(""),
        Some("12345"),
        Some("md5:d41d8cd98f00b204e9800998ecf8427e"),
        Some("sha256:aaaaaaaa"),
        Some("W/\"etag\""),
        Some("https://example.com/file.zip"),
    ] {
        assert!(ReleaseCacheKey::new(MOD_REPO, "v1.0", "mod.zip", digest).is_none());
    }
    let mut raw = format!("{:x}", Sha256::digest(b"archive"));
    raw.make_ascii_uppercase();
    let tagged = format!("SHA256:{raw}");
    let bare = ReleaseCacheKey::new(MOD_REPO, "v1.0", "mod.zip", Some(&raw)).unwrap();
    let prefixed = ReleaseCacheKey::new(MOD_REPO, "v1.0", "mod.zip", Some(&tagged)).unwrap();
    assert_eq!(bare.digest, prefixed.digest);
}

#[test]
fn restores_after_reconstruction_and_stage_is_disposable() {
    let f = Fixture::new();
    let key = key(MOD_REPO, "v1.0", "mod.zip", b"download one");
    f.remember(&key, b"download one");
    let reopened = ReleaseDownloadCache::new(f.cache.root.clone());
    let copy = reopened
        .restore(&key, &f.staging, Some(12), || false)
        .unwrap();
    assert_eq!(copy.parent(), Some(f.staging.as_path()));
    assert_eq!(copy.extension().unwrap(), "download");
    assert!(Uuid::parse_str(copy.file_stem().unwrap().to_str().unwrap()).is_ok());
    fs::write(&copy, b"the installer can overwrite its own copy").unwrap();
    let copy_path = copy.to_path_buf();
    drop(copy);
    assert!(!copy_path.exists());
    assert_eq!(fs::read(f.restore(&key).unwrap()).unwrap(), b"download one");
}

#[test]
fn retains_two_most_recently_used_versions_with_all_their_assets() {
    let f = Fixture::new();
    let one = key(MOD_REPO, "v1", "mod.zip", b"archive 1");
    let companion = key(MOD_REPO, "v1", "Mod.z01", b"chunk 1");
    let two = key(MOD_REPO, "v2", "mod.zip", b"archive 2");
    let three = key(MOD_REPO, "v3", "mod.zip", b"archive 3");
    f.remember(&one, b"archive 1");
    f.remember(&companion, b"chunk 1");
    f.remember(&two, b"archive 2");
    assert!(f.restore(&one).is_some());
    f.cache.touch(&one); // A successful downgrade promotes v1, not a new version slot.
    f.remember(&three, b"archive 3");
    assert!(f.restore(&two).is_none());
    assert!(f.restore(&one).is_some());
    assert!(f.restore(&companion).is_some());
    assert!(f.restore(&three).is_some());
    let scope = f.cache.scope(&two, false).unwrap();
    assert!(!scope.join(&two.version).exists());
    assert_eq!(fs::read_dir(scope).unwrap().count(), 3); // Two directories + index.
}

#[test]
fn complete_hit_prunes_a_version_left_on_disk_after_index_publication() {
    let f = Fixture::new();
    let one = key(MOD_REPO, "v1", "mod.zip", b"archive 1");
    let two = key(MOD_REPO, "v2", "mod.zip", b"archive 2");
    let three = key(MOD_REPO, "v3", "mod.zip", b"archive 3");
    f.remember(&one, b"archive 1");
    f.remember(&two, b"archive 2");

    // Reproduce disk state if the process stopped after publishing the new
    // index but before deleting the old version's files.
    let scope = f.cache.scope(&three, false).unwrap();
    let third = scope.join(&three.version);
    fs::create_dir(&third).unwrap();
    fs::write(three.asset_path(&third), b"archive 3").unwrap();
    write_index(&scope, &[three.version.clone(), two.version.clone()]).unwrap();

    let old = scope.join(&one.version);
    assert!(old.is_dir());
    let copy = f.restore(&three).unwrap();
    assert!(old.is_dir()); // The caller has not completed ZIP validation yet.
    f.cache.touch(&three); // v3 was already most recent.

    assert!(!old.exists());
    assert_eq!(fs::read(copy).unwrap(), b"archive 3");
    assert_eq!(fs::read(f.restore(&two).unwrap()).unwrap(), b"archive 2");
}

#[test]
fn partial_set_does_not_publish_or_evict_the_preceding_versions() {
    let f = Fixture::new();
    let one = key(MOD_REPO, "v1", "mod.zip", b"archive 1");
    let two = key(MOD_REPO, "v2", "mod.zip", b"archive 2");
    let three = key(MOD_REPO, "v3", "mod.zip", b"archive 3");
    let companion = key(MOD_REPO, "v3", "Mod.z01", b"chunk 3");
    f.remember(&one, b"archive 1");
    f.remember(&two, b"archive 2");
    let main = f.dir.path().join("release.zip");
    let chunk = f.dir.path().join("Release.z01");
    fs::write(&main, b"archive 3").unwrap();
    fs::write(&chunk, b"corrupt").unwrap();
    let set = [
        (&three, main.as_path(), None),
        (&companion, chunk.as_path(), None),
    ];
    f.cache.remember_set(&set, || false);
    assert!(f.restore(&three).is_none());
    assert!(f.restore(&companion).is_none());
    let scope = f.cache.scope(&three, false).unwrap();
    assert_eq!(
        read_index(&scope).unwrap(),
        [two.version.clone(), one.version.clone()]
    );
    assert_eq!(fs::read_dir(&scope).unwrap().count(), 3);

    fs::write(&chunk, b"chunk 3").unwrap();
    f.cache.remember_set(&set, || false);
    assert!(f.restore(&one).is_none());
    assert!(f.restore(&two).is_some());
    assert!(f.restore(&three).is_some());
    assert!(f.restore(&companion).is_some());
}

#[test]
fn map_identity_scopes_are_independent_even_when_versions_and_assets_match() {
    let f = Fixture::new();
    let first = "gitcode/author/map-repo/map-one";
    let second = "gitcode/author/map-repo/map-two";
    let other = key(second, "1", "map.zip", b"same bytes");
    let old = key(first, "1", "map.zip", b"same bytes");
    let newer = key(first, "2", "map.zip", b"same bytes");
    let newest = key(first, "3", "map.zip", b"same bytes");
    for asset in [&other, &old, &newer, &newest] {
        f.remember(asset, b"same bytes");
    }
    assert!(f.restore(&old).is_none());
    assert!(f.restore(&other).is_some());
    assert!(f.restore(&newer).is_some());
    assert!(f.restore(&newest).is_some());
}

#[test]
fn api_replacement_invalidates_old_asset_and_bad_bytes_never_hit() {
    let f = Fixture::new();
    let old = key(MOD_REPO, "v1", "mod.zip", b"old");
    let new = key(MOD_REPO, "v1", "mod.zip", b"new");
    f.remember(&old, b"old");
    assert!(f.restore(&new).is_none());
    assert!(
        f.cache
            .restore(&old, &f.staging, Some(4), || false)
            .is_none()
    );
    f.remember(&new, b"bad"); // Wrong bytes with correct size cannot replace valid entry.
    assert!(f.restore(&old).is_some());
    f.remember(&new, b"new");
    assert!(f.restore(&old).is_none());
    let version = f.cache.scope(&new, false).unwrap().join(&new.version);
    let cached = new.asset_path(&version);
    assert_eq!(fs::read_dir(&version).unwrap().count(), 1);
    fs::write(&cached, b"bad").unwrap();
    assert!(f.restore(&new).is_none());
    assert_eq!(fs::read_dir(&f.staging).unwrap().count(), 0);
    f.remember(&new, b"new");
    assert_eq!(fs::read(f.restore(&new).unwrap()).unwrap(), b"new");
}

#[test]
fn cancelled_copies_do_not_publish_or_leave_staging_files() {
    let f = Fixture::new();
    let bytes = vec![0x47; 2 * 64 * 1024];
    let key = key(MOD_REPO, "v1", "mod.zip", &bytes);
    let source = f.dir.path().join("download.zip");
    fs::write(&source, &bytes).unwrap();
    let checks = Cell::new(0u8);
    let mid_copy = || {
        checks.set(checks.get() + 1);
        checks.get() == 3
    };
    f.cache.remember_set(&[(&key, &source, None)], mid_copy);
    assert_eq!(
        fs::read_dir(f.cache.scope(&key, false).unwrap())
            .unwrap()
            .count(),
        0
    );
    assert_eq!(fs::read(&source).unwrap(), bytes);
    f.remember(&key, &bytes);
    checks.set(1);
    assert!(f.cache.restore(&key, &f.staging, None, mid_copy).is_none());
    assert_eq!(fs::read_dir(&f.staging).unwrap().count(), 0);
    assert_eq!(fs::read(f.restore(&key).unwrap()).unwrap(), bytes);
}

#[test]
fn fresh_write_recovers_broken_index_and_stale_temp_without_pruning_unknown_files() {
    let f = Fixture::new();
    let one = key(MOD_REPO, "v1", "mod.zip", b"archive 1");
    let two = key(MOD_REPO, "v2", "mod.zip", b"archive 2");
    f.remember(&one, b"archive 1");
    let scope = f.cache.scope(&one, false).unwrap();
    fs::write(scope.join(INDEX), b"[\"../../external-directory\"]").unwrap();
    let stale = scope.join(format!("incoming-{}-AbCdEf.tmp", Uuid::new_v4()));
    let unknown = scope.join("user-file.tmp");
    fs::write(&stale, b"stale incomplete download").unwrap();
    fs::write(&unknown, b"unknown data").unwrap();
    assert!(f.restore(&one).is_none());
    f.remember(&two, b"archive 2");
    assert!(f.restore(&two).is_some());
    assert!(!scope.join(&one.version).exists());
    assert!(!stale.exists());
    assert_eq!(fs::read(&unknown).unwrap(), b"unknown data");
}

#[test]
fn concurrent_open_asset_and_version_eviction_leave_verified_copy_intact() {
    let f = Fixture::new();
    let one = key(MOD_REPO, "v1", "mod.zip", b"archive 1");
    let two = key(MOD_REPO, "v2", "mod.zip", b"archive 2");
    let three = key(MOD_REPO, "v3", "mod.zip", b"archive 3");
    f.remember(&one, b"archive 1");
    f.remember(&two, b"archive 2");
    let opened = Barrier::new(2);
    let evicted = Barrier::new(2);
    let staging = std::thread::scope(|threads| {
        let reader = threads.spawn(|| {
            let first = Cell::new(true);
            f.cache.restore(&one, &f.staging, None, || {
                if first.replace(false) {
                    opened.wait();
                    evicted.wait();
                }
                false
            })
        });
        opened.wait();
        f.remember(&three, b"archive 3");
        evicted.wait();
        reader.join().unwrap().unwrap()
    });
    assert!(f.restore(&one).is_none());
    assert_eq!(fs::read(staging).unwrap(), b"archive 1");
}

#[test]
fn rejects_linked_cache_directory_including_windows_junctions() {
    let f = Fixture::new();
    let outside = f.dir.path().join("outside");
    fs::create_dir(&outside).unwrap();
    let cache_parent = f.cache.root.parent().unwrap();
    #[cfg(windows)]
    assert!(
        std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(cache_parent)
            .arg(&outside)
            .output()
            .unwrap()
            .status
            .success()
    );
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, cache_parent).unwrap();
    let key = key(MOD_REPO, "v1", "mod.zip", b"archive 1");
    f.remember(&key, b"archive 1");
    assert!(f.restore(&key).is_none());
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
}
