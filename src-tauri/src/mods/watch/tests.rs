use super::*;
use crate::{config::ConfigService, mods::scan_mods_with_manifests};
use notify::{
    Event, EventKind,
    event::{CreateKind, DataChange, ModifyKind, RemoveKind, RenameMode},
};
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

fn wait_for_mod_change(
    received: &mpsc::Receiver<Vec<PathBuf>>,
    expected: &[&Path],
    ignored: &[PathBuf],
    operation: &str,
) {
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
        assert!(
            !paths
                .iter()
                .any(|path| ignored.iter().any(|root| path.starts_with(root))),
            "{operation}: an ignored path was retained in {paths:?}"
        );
        if paths.iter().any(|path| expected.contains(&path.as_path())) {
            return;
        }
        observed.extend(paths);
    }
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

    // FSEvents can deliver fixture creation late. Construct the DLL outside
    // the watched tree, then observe its arrival before priming the cache.
    fs::rename(seed, &dll).unwrap();
    wait_for_mod_change(&received, &[&dll], &[], "OS event for initial DLL arrival");
    assert_eq!(scan_version().as_deref(), Some("1.0.0"));

    let start = Instant::now();
    overwrite_preserving_metadata(&dll, "2.0.0");
    wait_for_mod_change(&received, &[&dll], &[], "OS event for DLL overwrite");
    assert_eq!(scan_version().as_deref(), Some("2.0.0"));
    eprintln!(
        "DLL overwrite→OS notification→new scan: {} ms",
        start.elapsed().as_millis()
    );

    let disabled = folder.join("Live.dll.disabled");
    fs::rename(&dll, &disabled).unwrap();
    // Native backends can report the departure or arrival separately. Either
    // is enough to refresh, but the rescan must see the resulting disabled DLL.
    wait_for_mod_change(
        &received,
        &[&dll, &disabled],
        &[],
        "OS event for DLL rename",
    );
    assert!(!scan()[0].enabled);
    let moved = root.join("MovedPackage");
    fs::rename(&folder, &moved).unwrap();
    wait_for_mod_change(&received, &[&folder], &[], "OS event for directory move");
    assert!(scan_mods_with_manifests(&root, &config, None).is_empty());
    drop(watcher);
}

#[test]
fn config_create_beside_an_indexed_dll_does_not_trigger_a_mod_refresh() {
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

const DIY_LEVEL_PACKAGES: [(&str, &str); 2] =
    [("FrameworkOld", "0.10.0"), ("FrameworkNew", "0.11.1")];

fn map_watch_fixture(root: &Path) -> (PathBuf, ScannedPluginPaths) {
    let plugins = root.join(PLUGINS_DIR);
    for (package, version) in DIY_LEVEL_PACKAGES {
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
    let mut paths = ScannedPluginPaths::default();
    let config = super::super::test_support::config(vec![]);
    assert_eq!(
        crate::mods::scan_mods(root, &config, None, Some(&mut paths)).len(),
        3
    );
    (plugins, paths)
}

fn mutate_ignored_paths(plugins: &Path, mut check: impl FnMut(EventKind, &Path)) -> Vec<PathBuf> {
    let regular = plugins.join("Other/levels");
    let non_dll = regular.join("settings.cfg");
    let new = regular.join("settings-new.cfg");
    let renamed = regular.join("settings-renamed.cfg");
    fs::write(&new, "ignored").unwrap();
    check(EventKind::Create(CreateKind::File), &new);
    fs::rename(&non_dll, &renamed).unwrap();
    for path in [&non_dll, &renamed] {
        check(EventKind::Modify(ModifyKind::Name(RenameMode::Any)), path);
    }
    fs::remove_file(&renamed).unwrap();
    check(EventKind::Remove(RemoveKind::File), &renamed);
    let non_plugin = plugins.join("folder.dll");
    fs::create_dir(&non_plugin).unwrap();
    check(EventKind::Create(CreateKind::Folder), &non_plugin);
    fs::remove_dir(&non_plugin).unwrap();
    check(EventKind::Remove(RemoveKind::Folder), &non_plugin);
    let mut ignored = vec![non_dll, new, renamed, non_plugin];
    for (package, _) in DIY_LEVEL_PACKAGES {
        let package = plugins.join(package);
        let active = package.join("levels");
        let disabled = package.join("disabled");
        let map = active.join("Map");
        let dll = map.join("map.dll");
        write_plugin(&dll, "1.1.0");
        check(
            EventKind::Modify(ModifyKind::Data(DataChange::Content)),
            &dll,
        );
        let disabled_dll = disabled.join("Map/added.dll.disabled");
        write_plugin(&disabled_dll, "1.0.0");
        check(EventKind::Create(CreateKind::File), &disabled_dll);
        fs::remove_file(&disabled_dll).unwrap();
        check(EventKind::Remove(RemoveKind::File), &disabled_dll);
        let moved = active.join("MapMoved");
        fs::rename(&map, &moved).unwrap();
        for path in [&map, &moved] {
            check(EventKind::Modify(ModifyKind::Name(RenameMode::Any)), path);
        }
        ignored.extend([active, disabled]);
    }
    ignored
}

#[test]
fn map_watch_distinguishes_fixture_directory_arrivals_from_ignored_changes() {
    let temp = tempfile::tempdir().unwrap();
    let root = game::resolve_existing_ancestor(temp.path()).unwrap();
    let (plugins, paths) = map_watch_fixture(&root);
    let scope = RootScope::new(&root, plugins.clone(), plugins.clone());
    let mut classifier = PluginPaths::default();
    classifier.reconcile(&plugins, Arc::new(paths));
    // A package directory can itself carry an arrival event. That remains
    // relevant even if a scan has already indexed the framework DLL.
    for (package, _) in DIY_LEVEL_PACKAGES {
        let mut arrival =
            Event::new(EventKind::Create(CreateKind::Folder)).add_path(plugins.join(package));
        assert!(classifier.accept(&scope, &mut arrival), "{arrival:?}");
    }
    mutate_ignored_paths(&plugins, |kind, path| {
        let mut event = Event::new(kind).add_path(path.to_path_buf());
        assert!(!classifier.accept(&scope, &mut event), "{kind:?} {path:?}");
    });
}

#[test]
fn ordinary_package_departure_is_detected_without_an_intervening_scan() {
    let temp = tempfile::tempdir().unwrap();
    let root = game::resolve_existing_ancestor(temp.path()).unwrap();
    let plugins = root.join(PLUGINS_DIR);
    fs::create_dir_all(&plugins).unwrap();
    let outside = root.join("IncomingSource");
    fs::create_dir(&outside).unwrap();
    write_plugin(&outside.join("Incoming.dll.disabled"), "1.0.0");
    let moved = plugins.join("Incoming");
    let scope = RootScope::new(&root, plugins.clone(), plugins);
    let mut classifier = PluginPaths::default();
    let renamed =
        || Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Any))).add_path(moved.clone());

    fs::rename(&outside, &moved).unwrap();
    assert!(classifier.accept(&scope, &mut renamed()));
    fs::rename(&moved, &outside).unwrap();
    // Native batches can coalesce identical path strings. Exercise the raw
    // one-endpoint rename twice, checking that the first removes the index.
    let mut departure = renamed();
    assert!(classifier.accept(&scope, &mut departure));
    assert_eq!(departure.paths, vec![moved.clone()]);
    assert!(!classifier.accept(&scope, &mut renamed()));
}

#[test]
fn os_watch_ignores_non_dll_and_both_diy_level_map_trees() {
    let temp = tempfile::tempdir().unwrap();
    let root = game::resolve_existing_ancestor(temp.path()).unwrap();
    let (plugins, paths) = map_watch_fixture(&root);
    let watcher = LocalModWatcher::default();
    watcher.publish_paths(&root, paths);
    let (sent, received) = mpsc::channel();
    watcher.watch(&root, move |_, _, _, paths| {
        let _ = sent.send(paths);
    });

    let ignored = mutate_ignored_paths(&plugins, |_, _| {});
    // FSEvents can replay setup directories. Only inspect paths belonging to
    // the ignored changes, and require a fresh DLL that could not be a replay.
    let marker = plugins.join("Other/levels/Added.dll");
    write_plugin(&marker, "1.1.0");
    wait_for_mod_change(&received, &[&marker], &ignored, "ordinary DLL below levels");

    // Copying a plain folder above the watcher first and moving it into the
    // watcher gives only the directory arrival on some native backends.
    let outside = root.join("Incoming");
    fs::create_dir(&outside).unwrap();
    write_plugin(&outside.join("Incoming.dll.disabled"), "1.0.0");
    let moved = plugins.join("Incoming");
    let moved_dll = moved.join("Incoming.dll.disabled");
    fs::rename(&outside, &moved).unwrap();
    wait_for_mod_change(
        &received,
        &[&moved, &moved_dll],
        &ignored,
        "move in an ordinary DLL folder",
    );
    // No intervening scan: the index must retain arrivals immediately.
    fs::rename(&moved, &outside).unwrap();
    wait_for_mod_change(
        &received,
        &[&moved, &moved_dll],
        &ignored,
        "move out an incrementally indexed DLL folder",
    );
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
