use super::*;
use sha2::Digest;
use std::{
    fs,
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

// Keep raw receipts in memory until a fixture fails: per-event stderr changes
// callback timing, and paths from concurrent fixtures must stay distinguishable.
type WatchTrace = Vec<(PathBuf, Instant, String)>;
static WATCH_TRACE: std::sync::Mutex<WatchTrace> = std::sync::Mutex::new(Vec::new());

pub(super) fn trace(root: &Path, message: String) {
    let mut events = WATCH_TRACE
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    events.push((root.to_path_buf(), Instant::now(), message));
}

fn finish_trace(root: &Path) {
    let canonical_root = root.canonicalize().ok();
    let mut events = WATCH_TRACE
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let mut start = None;
    events.retain(|(event_root, received, message)| {
        if !event_root.starts_with(root)
            && !canonical_root
                .as_ref()
                .is_some_and(|root| event_root.starts_with(root))
        {
            return true;
        }
        if std::thread::panicking() {
            let elapsed = received.duration_since(*start.get_or_insert(*received));
            eprintln!("[chef-watch-trace +{elapsed:?}] root={event_root:?} {message}");
        }
        false
    });
}

struct Workspace(PathBuf);
impl Workspace {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("oc2-chef-watch-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        finish_trace(&self.0);
        let root = self.0.canonicalize().unwrap();
        let temporary = std::env::temp_dir().canonicalize().unwrap();
        assert!(root.starts_with(&temporary) && root != temporary);
        fs::remove_dir_all(root).unwrap();
    }
}
fn target(generation: u64) -> ChefTarget {
    ChefTarget {
        id: "chef".into(),
        guid: super::super::target::GUID.into(),
        version: Some("2.2.0".into()),
        display_path: "not-emitted".into(),
        enabled: true,
        generation,
        compatibility: "editable".into(),
        supports_custom_default: true,
    }
}

fn wait_for_pending(watcher: &ChefWatcher, root: &Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !watcher.resources_revision.has_pending(root) {
        assert!(
            Instant::now() < deadline,
            "resource event never reached raw accept"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn replace_resource_preserving_metadata(resource: &Path) {
    let before = fs::metadata(resource).unwrap();
    let modified = before.modified().unwrap();
    fs::write(resource, b"new bytes").unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(resource)
        .unwrap()
        .set_modified(modified)
        .unwrap();
    let after = fs::metadata(resource).unwrap();
    assert_eq!(after.len(), before.len());
    assert_eq!(after.modified().unwrap(), modified);
}

#[test]
fn filter_includes_both_prefer_names_resources_and_target_removal() {
    let root = Path::new("game/DIYChef");
    for name in ["prefer.txt", "prefer.txt.txt", "PREFER.TXT"] {
        assert_eq!(classify(root, &root.join(name)), Some(false));
    }
    for name in [
        "Resources",
        "resources/Fox/INFO",
        "Resources/Fox/m_Head.txt",
        "Resources/HATS/Hat/t_Hat.png",
    ] {
        assert_eq!(classify(root, &root.join(name)), Some(true));
    }
    assert_eq!(classify(root, root), Some(true));
    for name in [
        "README.md",
        "DIYChef.dll",
        "prefer.txt.tmp",
        "Unrelated/prefer.txt",
        "Resources2/model",
    ] {
        assert_eq!(classify(root, &root.join(name)), None);
    }
    assert_eq!(classify(root, Path::new("game/Another/Resources")), None);
}

#[test]
fn real_notify_atomic_prefer_and_resource_creation_rename_removal_are_scoped() {
    let fixture = Workspace::new();
    let watcher = ChefWatcher::new(fixture.0.join("cache"));
    let (tx, rx) = mpsc::channel();
    watcher.watch(&fixture.0, &target(7), move |event| {
        tx.send(event).unwrap();
    });
    fs::write(fixture.0.join("README.md"), "unrelated").unwrap();
    assert!(rx.recv_timeout(Duration::from_millis(850)).is_err());
    fs::write(fixture.0.join("temporary-prefer"), "Chef_Black_cat\n").unwrap();
    fs::rename(
        fixture.0.join("temporary-prefer"),
        fixture.0.join("prefer.txt"),
    )
    .unwrap();
    let changed = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(
        changed,
        FilesChanged {
            target_id: "chef".into(),
            generation: 7,
            resources_changed: false
        }
    );
    assert!(
        rx.recv_timeout(Duration::from_millis(850)).is_err(),
        "one settled replace produces one batch"
    );
    let resources = fixture.0.join("Resources");
    fs::create_dir(&resources).unwrap();
    assert!(
        rx.recv_timeout(Duration::from_secs(5))
            .unwrap()
            .resources_changed
    );
    fs::write(resources.join("INFO"), "ID=7").unwrap();
    assert!(
        rx.recv_timeout(Duration::from_secs(5))
            .unwrap()
            .resources_changed
    );
    fs::rename(&resources, fixture.0.join("moved-resources")).unwrap();
    assert!(
        rx.recv_timeout(Duration::from_secs(5))
            .unwrap()
            .resources_changed
    );
    fs::rename(fixture.0.join("moved-resources"), &resources).unwrap();
    assert!(
        rx.recv_timeout(Duration::from_secs(5))
            .unwrap()
            .resources_changed
    );
    let checked = resources.canonicalize().unwrap();
    assert!(checked.starts_with(fixture.0.canonicalize().unwrap()));
    fs::remove_dir_all(&checked).unwrap();
    assert!(
        rx.recv_timeout(Duration::from_secs(5))
            .unwrap()
            .resources_changed
    );
    watcher.stop();
    fs::write(fixture.0.join("prefer.txt.txt"), "Chef_Beard").unwrap();
    assert!(rx.recv_timeout(Duration::from_millis(850)).is_err());
}

#[test]
fn same_path_replaces_callback_when_target_or_generation_changes_and_drop_stops() {
    let fixture = Workspace::new();
    let watcher = ChefWatcher::new(fixture.0.join("cache"));
    let (tx, rx) = mpsc::channel();
    let old_tx = tx.clone();
    watcher.watch(&fixture.0, &target(1), move |event| {
        old_tx.send(event).unwrap();
    });
    // Queue old work, then change identity before the quiet window can publish.
    fs::write(fixture.0.join("prefer.txt"), "Chef_Black_cat").unwrap();
    let mut next = target(2);
    next.id = "new-chef".into();
    watcher.watch(&fixture.0, &next, move |event| {
        tx.send(event).unwrap();
    });
    fs::write(fixture.0.join("prefer.txt.txt"), "Chef_Beard").unwrap();
    let changed = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(changed.target_id, "new-chef");
    assert_eq!(changed.generation, 2);
    drop(watcher);
    fs::write(fixture.0.join("prefer.txt"), "Chef_Beard").unwrap();
    assert!(rx.recv_timeout(Duration::from_millis(850)).is_err());
}

#[test]
fn real_resource_change_survives_restart_when_size_and_mtime_stay_equal() {
    let fixture = Workspace::new();
    let cache = fixture.0.join("cache");
    let root = fixture.0.join("mod");
    fs::create_dir(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let (tx, rx) = mpsc::channel();
    let first_tx = tx.clone();
    let watcher = ChefWatcher::new(cache.clone());
    let initial = watcher.watch(&root, &target(1), move |event| {
        first_tx.send(event).unwrap()
    });
    assert_ne!(
        initial, 0,
        "missing records must never revive legacy revision zero"
    );

    // Windows can report directory metadata well after fixture creation. Keep
    // Resources absent during TXT-only checks so those phases cannot overlap.
    fs::write(root.join("README.md"), "unrelated").unwrap();
    assert!(rx.recv_timeout(Duration::from_millis(850)).is_err());
    fs::write(root.join("prefer.txt"), "Chef_Black_cat").unwrap();
    assert!(
        !rx.recv_timeout(Duration::from_secs(5))
            .unwrap()
            .resources_changed
    );
    assert_eq!(watcher.watch(&root, &target(1), |_| {}), initial);

    fs::create_dir_all(root.join("Resources/Fox")).unwrap();
    let resource = root.join("Resources/Fox/t_Head.png");
    fs::write(&resource, b"old bytes").unwrap();
    let before = fs::metadata(&resource).unwrap();
    let modified = before.modified().unwrap();
    assert!(
        rx.recv_timeout(Duration::from_secs(5))
            .unwrap()
            .resources_changed
    );
    let resource_baseline = watcher.watch(&root, &target(1), |_| {});
    assert_ne!(resource_baseline, 0);
    assert_ne!(resource_baseline, initial);

    fs::write(&resource, b"new bytes").unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&resource)
        .unwrap()
        .set_modified(modified)
        .unwrap();
    assert!(
        rx.recv_timeout(Duration::from_secs(5))
            .unwrap()
            .resources_changed
    );
    // Stop joins the worker and flushes every accepted resource notification;
    // compare the persisted final stamp, including a last coalesced batch.
    watcher.stop();
    let changed = watcher.resources_revision.current(&root);
    assert_ne!(changed, 0);
    assert_ne!(changed, initial);
    assert_ne!(changed, resource_baseline);
    let after = fs::metadata(&resource).unwrap();
    assert_eq!(after.len(), before.len());
    assert_eq!(after.modified().unwrap(), modified);
    drop(watcher);

    let restarted = ChefWatcher::new(cache);
    let mut next = target(99);
    next.id = "new-scan-identity".into();
    let restored = restarted.watch(&root, &next, move |event| tx.send(event).unwrap());
    assert_eq!(
        restored, changed,
        "the observed resource stamp must survive restart"
    );
    assert_ne!(
        restored, initial,
        "the pre-change PNG key must remain invalid"
    );
    let other_root = fixture.0.join("other-mod");
    fs::create_dir(&other_root).unwrap();
    let other_root = other_root.canonicalize().unwrap();
    let other = restarted.watch(&other_root, &target(100), |_| {});
    assert_ne!(other, 0);
    assert_ne!(
        other, restored,
        "switching targets must not reuse the previous stamp"
    );
    assert_eq!(restarted.watch(&root, &target(101), |_| {}), restored);
}

#[test]
fn revisions_are_persistent_per_root_and_repair_corrupt_records() {
    let fixture = Workspace::new();
    let cache = fixture.0.join("cache");
    let first_root = fixture.0.join("first-mod");
    let second_root = fixture.0.join("second-mod");
    fs::create_dir(&first_root).unwrap();
    fs::create_dir(&second_root).unwrap();
    let first_root = first_root.canonicalize().unwrap();
    let second_root = second_root.canonicalize().unwrap();
    let revisions = ResourceRevisions::new(cache.clone());
    let first = revisions.current(&first_root);
    let second = revisions.current(&second_root);
    assert_ne!(first, 0);
    assert_ne!(second, 0);
    assert_ne!(first, second);
    revisions
        .pending_flag(&first_root)
        .store(true, Ordering::Release);
    let changed = revisions.current(&first_root);
    assert_ne!(changed, first);
    assert_eq!(revisions.current(&first_root), changed);
    assert_eq!(revisions.current(&second_root), second);
    drop(revisions);

    let revisions = ResourceRevisions::new(cache.clone());
    assert_eq!(revisions.current(&first_root), changed);
    assert_eq!(revisions.current(&second_root), second);
    drop(revisions);
    let record = cache.join("diychef-resource-revisions").join(format!(
        "{:x}.bin",
        sha2::Sha256::digest(first_root.as_os_str().as_encoded_bytes())
    ));
    for corrupt in [&b"broken"[..], &[0; 8][..]] {
        fs::write(&record, corrupt).unwrap();
        let revisions = ResourceRevisions::new(cache.clone());
        let recovered = revisions.current(&first_root);
        assert_ne!(recovered, 0);
        assert_ne!(recovered, changed);
        assert_eq!(fs::read(&record).unwrap(), recovered.to_le_bytes());
        assert_eq!(revisions.current(&first_root), recovered);
        drop(revisions);
        assert_eq!(
            ResourceRevisions::new(cache.clone()).current(&first_root),
            recovered
        );
    }
}

#[test]
fn unavailable_revision_storage_keeps_a_stable_nonzero_fallback_without_writing_outside() {
    let fixture = Workspace::new();
    let cache = fixture.0.join("cache");
    fs::write(&cache, b"preserve").unwrap();
    let revisions = ResourceRevisions::new(cache.clone());
    let initial = revisions.current(&fixture.0);
    assert_ne!(initial, 0);
    assert_eq!(revisions.current(&fixture.0), initial);
    revisions
        .pending_flag(&fixture.0)
        .store(true, Ordering::Release);
    let changed = revisions.current(&fixture.0);
    assert_ne!(changed, 0);
    assert_ne!(changed, initial);
    assert_eq!(revisions.current(&fixture.0), changed);
    assert_eq!(fs::read(cache).unwrap(), b"preserve");
}

#[test]
fn pending_resource_change_is_persisted_when_switching_targets_before_debounce() {
    let fixture = Workspace::new();
    let cache = fixture.0.join("cache");
    let root = fixture.0.join("mod");
    let other = fixture.0.join("other-mod");
    fs::create_dir_all(root.join("Resources")).unwrap();
    fs::create_dir(&other).unwrap();
    let root = root.canonicalize().unwrap();
    let other = other.canonicalize().unwrap();
    let resource = root.join("Resources/model");
    fs::write(&resource, b"old bytes").unwrap();
    let watcher = ChefWatcher::new(cache.clone());
    let (tx, rx) = mpsc::channel();
    let old_tx = tx.clone();
    let initial = watcher.watch(&root, &target(1), move |event| old_tx.send(event).unwrap());
    replace_resource_preserving_metadata(&resource);
    wait_for_pending(&watcher, &root);
    assert!(
        rx.try_recv().is_err(),
        "switch must precede the debounced UI event"
    );

    let mut next = target(2);
    next.id = "other-chef".into();
    watcher.watch(&other, &next, |_| {});
    let changed = watcher.watch(&root, &target(3), |_| {});
    assert_ne!(changed, 0);
    assert_ne!(changed, initial);
    assert!(!watcher.resources_revision.has_pending(&root));
    assert!(
        rx.recv_timeout(Duration::from_millis(850)).is_err(),
        "old UI work must be cancelled"
    );
    drop(watcher);
    let restarted = ChefWatcher::new(cache);
    assert_eq!(restarted.watch(&root, &target(4), |_| {}), changed);
}

#[test]
fn pending_resource_change_survives_stop_or_drop_before_debounce_and_restart() {
    for stop_first in [false, true] {
        let fixture = Workspace::new();
        let cache = fixture.0.join("cache");
        let root = fixture.0.join("mod");
        fs::create_dir_all(root.join("Resources")).unwrap();
        let root = root.canonicalize().unwrap();
        let resource = root.join("Resources/model");
        fs::write(&resource, b"old bytes").unwrap();
        let watcher = ChefWatcher::new(cache.clone());
        let (tx, rx) = mpsc::channel();
        let old_tx = tx.clone();
        let initial = watcher.watch(&root, &target(1), move |event| old_tx.send(event).unwrap());
        replace_resource_preserving_metadata(&resource);
        wait_for_pending(&watcher, &root);
        assert!(
            rx.try_recv().is_err(),
            "shutdown must precede the debounced UI event"
        );
        if stop_first {
            watcher.stop();
            assert!(!watcher.resources_revision.has_pending(&root));
        }
        drop(watcher);
        let restarted = ChefWatcher::new(cache.clone());
        let restored = restarted.watch(&root, &target(2), |_| {});
        assert_ne!(restored, 0);
        assert_ne!(
            restored, initial,
            "pending change must survive stop_first={stop_first}"
        );
        assert!(
            rx.recv_timeout(Duration::from_millis(850)).is_err(),
            "shutdown must not emit old UI work"
        );
        drop(restarted);
        assert_eq!(
            ChefWatcher::new(cache).watch(&root, &target(3), |_| {}),
            restored
        );
    }
}
