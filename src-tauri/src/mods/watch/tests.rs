use super::*;
use crate::{config::ConfigService, mods::scan_mods_with_manifests};
use std::{
    fs,
    sync::mpsc,
    time::{Duration, Instant},
};

fn write_plugin(path: &Path, version: &str) {
    write_plugin_guid(path, "com.example.live", version);
}

fn write_plugin_guid(path: &Path, guid: &str, version: &str) {
    let mut data = b"MZ".to_vec();
    data.extend(crate::mods::plugin_blob(guid, "Live", version));
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
    let seed = root.join("Initial.dll");
    write_plugin(&seed, "1.0.0");
    let config = super::super::test_support::config(vec![]);
    let watcher = LocalModWatcher::default();
    let scan = || {
        let mut paths = ScannedPluginPaths::default();
        let mods = crate::mods::scan_mods(&root, &config, None, Some(&mut paths));
        watcher.publish_paths(&root, paths);
        mods
    };
    let scan_version = || scan()[0].version.clone();
    let (sent, received) = mpsc::channel();
    watcher.watch(&root, move |_, _, plugins, paths| {
        metadata_read::invalidate_dll_details_under(plugins);
        let _ = sent.send(paths);
    });
    let wait_for = |expected: &[&Path], operation: &str| {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut observed = Vec::new();
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                !remaining.is_zero(),
                "{operation}: expected one of {expected:?}, saw {observed:?}"
            );
            let paths = received.recv_timeout(remaining).unwrap_or_else(|error| {
                panic!("{operation}: expected one of {expected:?}, saw {observed:?}: {error}")
            });
            if paths
                .iter()
                .any(|change| expected.contains(&change.as_path()))
            {
                return;
            }
            observed.extend(paths);
        }
    };

    // FSEvents can deliver fixture creation late. Construct the DLL outside
    // the watched tree, then observe its arrival before priming the cache.
    fs::rename(seed, &dll).unwrap();
    wait_for(&[&dll], "OS event for initial DLL arrival");
    assert_eq!(scan_version().as_deref(), Some("1.0.0"));

    let start = Instant::now();
    overwrite_preserving_metadata(&dll, "2.0.0");
    wait_for(&[&dll], "OS event for DLL overwrite");
    assert_eq!(scan_version().as_deref(), Some("2.0.0"));
    eprintln!(
        "DLL overwrite→OS notification→new scan: {} ms",
        start.elapsed().as_millis()
    );

    let disabled = folder.join("Live.dll.disabled");
    fs::rename(&dll, &disabled).unwrap();
    // Native backends can report the departure or arrival separately. Either
    // is enough to refresh, but the rescan must see the resulting disabled DLL.
    wait_for(&[&dll, &disabled], "OS event for DLL rename");
    assert!(!scan()[0].enabled);
    let moved = root.join("MovedPackage");
    fs::rename(&folder, &moved).unwrap();
    wait_for(&[&folder], "OS event for directory move");
    assert!(scan_mods_with_manifests(&root, &config, None).is_empty());
    drop(watcher);
}

#[test]
fn config_create_beside_an_indexed_dll_does_not_trigger_a_mod_refresh() {
    use notify::{Event, EventKind, event::CreateKind};

    let temp = tempfile::tempdir().unwrap();
    let root = game::resolve_existing_ancestor(temp.path()).unwrap();
    let plugins = root.join(PLUGINS_DIR);
    let folder = plugins.join("nested");
    fs::create_dir_all(&folder).unwrap();
    let dll = folder.join("Live.dll");
    write_plugin(&dll, "1.0.0");
    let settings = folder.join("settings.cfg");
    fs::write(&settings, "ignored").unwrap();

    let scope = RootScope::new(&root, plugins.clone(), plugins.clone());
    let mut paths = PluginPaths::default();
    paths.reconcile(
        &plugins,
        Arc::new(ScannedPluginPaths {
            ordinary: vec![dll.strip_prefix(&plugins).unwrap().to_path_buf()],
            ..ScannedPluginPaths::default()
        }),
    );
    let mut config_only =
        Event::new(EventKind::Create(CreateKind::File)).add_path(settings.clone());
    assert!(!paths.accept(&scope, &mut config_only));
    let mut mixed = Event::new(EventKind::Create(CreateKind::File))
        .add_path(settings)
        .add_path(dll.clone());
    assert!(paths.accept(&scope, &mut mixed));
    assert_eq!(mixed.paths, vec![dll]);
}

#[test]
fn os_watch_ignores_non_dll_and_both_diy_level_map_trees() {
    let temp = tempfile::tempdir().unwrap();
    let root = game::resolve_existing_ancestor(temp.path()).unwrap();
    let plugins = root.join(PLUGINS_DIR);
    for (package, version) in [("FrameworkOld", "0.10.0"), ("FrameworkNew", "0.11.1")] {
        let package = plugins.join(package);
        fs::create_dir_all(package.join("levels/Map")).unwrap();
        fs::create_dir_all(package.join("disabled/Map")).unwrap();
        write_plugin_guid(
            &package.join("framework.dll"),
            super::super::plugin_target::DIYLEVEL_GUID,
            version,
        );
        // No info fixture: exclusion must come from the DLL's actual GUID.
        write_plugin(&package.join("levels/Map/map.dll"), "1.0.0");
    }
    let regular = plugins.join("Other/levels");
    fs::create_dir_all(&regular).unwrap();
    write_plugin(&regular.join("Tool.dll"), "1.0.0");
    fs::write(regular.join("settings.cfg"), "ignored").unwrap();
    let watcher = LocalModWatcher::default();
    let (sent, received) = mpsc::channel();
    watcher.watch(&root, move |_, _, _, _| {
        let _ = sent.send(());
    });
    let mut paths = ScannedPluginPaths::default();
    let config = super::super::test_support::config(vec![]);
    assert_eq!(
        crate::mods::scan_mods(&root, &config, None, Some(&mut paths)).len(),
        3
    );
    watcher.publish_paths(&root, paths);

    let non_dll = regular.join("settings.cfg");
    fs::write(regular.join("settings-new.cfg"), "ignored").unwrap();
    fs::rename(&non_dll, regular.join("settings-renamed.cfg")).unwrap();
    fs::remove_file(regular.join("settings-renamed.cfg")).unwrap();
    let non_plugin = plugins.join("folder.dll");
    fs::create_dir(&non_plugin).unwrap();
    fs::remove_dir(&non_plugin).unwrap();
    for package in [plugins.join("FrameworkOld"), plugins.join("FrameworkNew")] {
        let map = package.join("levels/Map");
        write_plugin(&map.join("map.dll"), "1.1.0");
        let disabled = package.join("disabled/Map/added.dll.disabled");
        write_plugin(&disabled, "1.0.0");
        fs::remove_file(disabled).unwrap();
        fs::rename(&map, package.join("levels/MapMoved")).unwrap();
    }
    assert_eq!(
        received.recv_timeout(Duration::from_millis(1200)),
        Err(mpsc::RecvTimeoutError::Timeout)
    );

    // A real non-map MOD is still watched when its parent happens to be `levels`.
    write_plugin(&regular.join("Tool.dll"), "1.1.0");
    received
        .recv_timeout(Duration::from_secs(5))
        .expect("ordinary DLL below levels");

    // Copying a plain folder above the watcher first and moving it into the
    // watcher gives only the directory arrival on some native backends.
    let outside = root.join("Incoming");
    fs::create_dir(&outside).unwrap();
    write_plugin(&outside.join("Incoming.dll.disabled"), "1.0.0");
    let moved = plugins.join("Incoming");
    fs::rename(&outside, &moved).unwrap();
    received
        .recv_timeout(Duration::from_secs(5))
        .expect("move in an ordinary DLL folder");
    // No intervening scan: the index must retain arrivals immediately.
    fs::rename(&moved, &outside).unwrap();
    received
        .recv_timeout(Duration::from_secs(5))
        .expect("move out an incrementally indexed DLL folder");
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
    watcher.watch(&previous, |_, _, _, _| {});
    assert_eq!(scan_version().as_deref(), Some("1.0.0"));

    watcher.watch(&other, |_, _, _, _| {});
    overwrite_preserving_metadata(&dll, "2.0.0");
    watcher.watch(&previous, |_, _, _, _| {});
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
        watcher.watch(&root, move |_, _, plugins, paths| {
            let _ = sent.send(topology_changed(&[plugins.to_path_buf()], &paths));
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
    assert!(!is_selected(&state, &old_root));
    assert!(is_selected(&state, &root));
    assert!(invalidate_content(&state, &root.join(PLUGINS_DIR)));
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
    watcher.watch(&root, move |_, _, _, _| {
        let _ = sent.send(());
    });
    #[cfg(windows)]
    fs::remove_dir(plugins).unwrap();
    assert_eq!(received.try_recv(), Err(mpsc::TryRecvError::Disconnected));
}
