use super::*;
use std::{fs, path::PathBuf, sync::mpsc, time::Duration};

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
    let watcher = ChefWatcher::default();
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
    let watcher = ChefWatcher::default();
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
