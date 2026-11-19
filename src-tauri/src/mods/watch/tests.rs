use super::*;
use crate::{config::ConfigService, mods::scan_mods_with_manifests};
use std::{
    fs,
    sync::mpsc,
    time::{Duration, Instant},
};

fn write_plugin(path: &Path, version: &str) {
    let mut data = b"MZ".to_vec();
    data.extend(crate::mods::plugin_blob(
        "com.example.live",
        "Live",
        version,
    ));
    fs::write(path, data).unwrap();
}

fn overwrite_preserving_metadata(dll: &Path, version: &str) {
    let before = fs::metadata(dll).unwrap();
    write_plugin(dll, version);
    fs::OpenOptions::new()
        .write(true)
        .open(dll)
        .unwrap()
        .set_modified(before.modified().unwrap())
        .unwrap();
    let after = fs::metadata(dll).unwrap();
    assert_eq!(after.len(), before.len());
    assert_eq!(after.modified().unwrap(), before.modified().unwrap());
}

#[test]
fn os_watch_refreshes_a_same_size_and_timestamp_replacement() {
    let temp = tempfile::tempdir().unwrap();
    let root = game::resolve_existing_ancestor(temp.path()).unwrap();
    let plugins = root.join(PLUGINS_DIR);
    let folder = plugins.join("nested");
    fs::create_dir_all(&folder).unwrap();
    let dll = folder.join("Live.dll");
    let config = super::super::test_support::config(vec![]);
    let scan_version = || {
        scan_mods_with_manifests(&root, &config, None)[0]
            .version
            .clone()
    };
    write_plugin(&dll, "1.0.0");
    assert_eq!(scan_version().as_deref(), Some("1.0.0"));
    let watcher = LocalModWatcher::default();
    let (sent, received) = mpsc::channel();
    watcher.watch(&root, move |_, plugins, _| {
        metadata_read::invalidate_dll_details_under(plugins);
        let _ = sent.send(());
    });

    // An ordinary config write and a scanner read do not refresh MODs.
    fs::write(folder.join("settings.cfg"), "ignored").unwrap();
    assert_eq!(scan_version().as_deref(), Some("1.0.0"));
    assert_eq!(
        received.recv_timeout(Duration::from_millis(900)),
        Err(mpsc::RecvTimeoutError::Timeout)
    );

    let start = Instant::now();
    overwrite_preserving_metadata(&dll, "2.0.0");
    received
        .recv_timeout(Duration::from_secs(5))
        .expect("OS event for DLL overwrite");
    assert_eq!(scan_version().as_deref(), Some("2.0.0"));
    eprintln!(
        "DLL overwrite→OS notification→new scan: {} ms",
        start.elapsed().as_millis()
    );

    fs::rename(&dll, folder.join("Live.dll.disabled")).unwrap();
    received
        .recv_timeout(Duration::from_secs(5))
        .expect("OS event for DLL rename");
    assert!(!scan_mods_with_manifests(&root, &config, None)[0].enabled);
    let moved = root.join("MovedPackage");
    fs::rename(folder, &moved).unwrap();
    received
        .recv_timeout(Duration::from_secs(5))
        .expect("OS event for directory move");
    assert!(scan_mods_with_manifests(&root, &config, None).is_empty());
    drop(watcher);
}

#[test]
fn returning_to_a_game_reloads_dlls_edited_while_its_watch_was_inactive() {
    let temp = tempfile::tempdir().unwrap();
    let root = game::resolve_existing_ancestor(temp.path()).unwrap();
    let previous = root.join("previous-game");
    let other = root.join("other-game");
    for root in [&previous, &other] {
        fs::create_dir_all(root.join(PLUGINS_DIR)).unwrap();
    }
    let dll = previous.join(PLUGINS_DIR).join("Live.dll");
    write_plugin(&dll, "1.0.0");
    let watcher = LocalModWatcher::default();
    let config = super::super::test_support::config(vec![]);
    let scan_version = || {
        scan_mods_with_manifests(&previous, &config, None)[0]
            .version
            .clone()
    };
    watcher.watch(&previous, |_, _, _| {});
    assert_eq!(scan_version().as_deref(), Some("1.0.0"));

    watcher.watch(&other, |_, _, _| {});
    overwrite_preserving_metadata(&dll, "2.0.0");
    watcher.watch(&previous, |_, _, _| {});
    assert_eq!(scan_version().as_deref(), Some("2.0.0"));
}

#[test]
fn os_root_guard_catches_creation_and_same_name_replacement() {
    let temp = tempfile::tempdir().unwrap();
    let root = game::resolve_existing_ancestor(temp.path()).unwrap();
    let plugins = root.join(PLUGINS_DIR);
    // Start without even the BepInEx parent directory.
    let watcher = LocalModWatcher::default();
    let register = || {
        let (sent, received) = mpsc::channel();
        watcher.watch(&root, move |_, _, topology_changed| {
            let _ = sent.send(topology_changed);
        });
        received
    };
    let received = register();
    fs::create_dir_all(&plugins).unwrap();
    assert!(
        received
            .recv_timeout(Duration::from_secs(5))
            .expect("OS event for new root")
    );
    let received = register();
    fs::rename(&plugins, root.join("retired-plugins")).unwrap();
    fs::create_dir(&plugins).unwrap();
    assert!(
        received
            .recv_timeout(Duration::from_secs(5))
            .expect("parent event for replacement")
    );
    watcher.directory.stop();
    let received = register();
    write_plugin(&plugins.join("new.dll"), "1.0.0");
    assert!(
        !received
            .recv_timeout(Duration::from_secs(5))
            .expect("new recursive watch")
    );
    drop(watcher);
}

#[test]
fn notification_revokes_inflight_scan_for_the_selected_root_only() {
    let temp = tempfile::tempdir().unwrap();
    let root = game::resolve_existing_ancestor(temp.path()).unwrap();
    let config = ConfigService::for_test(root.join("data"));
    let mut value = config.load_or_default();
    value.game.path = root.to_string_lossy().into_owned();
    config.save(&value).unwrap();
    let state = AppState::with_config(config);
    let _operation = state.operation.lock().unwrap();
    let in_flight = state.local_mod_snapshot.lock().unwrap().begin(Some(&root));
    let old_root = root.join("previous-game");
    assert!(!invalidate_if_selected(
        &state,
        &old_root,
        &old_root.join(PLUGINS_DIR)
    ));
    assert!(invalidate_if_selected(
        &state,
        &root,
        &root.join(PLUGINS_DIR)
    ));
    let mut snapshots = state.local_mod_snapshot.lock().unwrap();
    assert!(
        snapshots
            .publish(in_flight, Some(&root), Vec::new())
            .is_err()
    );
    let fresh = snapshots.begin(Some(&root));
    snapshots.publish(fresh, Some(&root), Vec::new()).unwrap();
    assert!(snapshots.current(&root).is_ok());
}

#[cfg(any(unix, windows))]
#[test]
fn escaping_plugins_root_does_not_register_a_watcher() {
    let temp = tempfile::tempdir().unwrap();
    let root = game::resolve_existing_ancestor(temp.path()).unwrap();
    let external = tempfile::tempdir().unwrap();
    let plugins = root.join(PLUGINS_DIR);
    fs::create_dir_all(plugins.parent().unwrap()).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(external.path(), &plugins).unwrap();
    #[cfg(windows)]
    {
        let link = temp.path().join("BepInEx").join("plugins");
        let result = std::process::Command::new("cmd.exe")
            .args(["/D", "/C", "mklink", "/J"])
            .arg(&link)
            .arg(external.path())
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "failed to create test junction: {} {}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let watcher = LocalModWatcher::default();
    let (sent, received) = mpsc::channel();
    watcher.watch(&root, move |_, _, _| {
        let _ = sent.send(());
    });
    #[cfg(windows)]
    fs::remove_dir(plugins).unwrap();
    assert_eq!(received.try_recv(), Err(mpsc::TryRecvError::Disconnected));
}
