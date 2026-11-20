use super::*;
use notify::{
    EventKind,
    event::{CreateKind, ModifyKind, RemoveKind, RenameMode},
};
use std::{fs, path::Path, sync::mpsc, time::Duration};

#[test]
fn changed_paths_select_only_the_affected_set() {
    let root = PathBuf::from("game/levels");
    assert_eq!(
        changed_sets(
            std::slice::from_ref(&root),
            &[root.join("pear/info"), root.join("pear/scene")]
        ),
        Some(vec!["pear".into()])
    );
    assert_eq!(
        changed_sets(std::slice::from_ref(&root), &[root.clone()]),
        None
    );
}

#[test]
fn filter_projects_only_current_sets_and_real_root_topology() {
    let temp = tempfile::tempdir().unwrap();
    let game = crate::game::resolve_existing_ancestor(temp.path()).unwrap();
    let map = game.join("BepInEx/plugins/New/levels");
    let disabled = game.join("BepInEx/cache/modmanager_disabled_maps");
    fs::create_dir_all(&map).unwrap();
    let scopes = [
        RootScope::new(&game, map.clone(), map.clone()),
        RootScope::new(&game, disabled.clone(), disabled.clone()),
    ];
    let event = |kind, path: &Path| Event::new(kind).add_path(path.to_path_buf());
    let remove = EventKind::Remove(RemoveKind::Any);
    for path in [
        game.join("save.json"),
        game.join("BepInEx/plugins/Old/levels/pear/info"),
        game.join("BepInEx/plugins/New/settings.cfg"),
        game.join("BepInEx/cache/Other"),
        map.clone(),
    ] {
        assert!(!accept_event(
            &scopes,
            &mut event(EventKind::Modify(ModifyKind::Any), &path)
        ));
        if path != map {
            assert!(!accept_event(&scopes, &mut event(remove, &path)));
        }
    }
    let mut multi = event(
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
        &map.join("pear/info"),
    )
    .add_path(disabled.join("pear/info"))
    .add_path(game.join("outside"));
    assert!(accept_event(&scopes, &mut multi));
    assert_eq!(multi.paths, [map.join("pear"), disabled.join("pear")]);
    let cache = game.join("BepInEx/cache");
    fs::create_dir(&cache).unwrap();
    let mut created = event(EventKind::Create(CreateKind::Any), &cache);
    assert!(accept_event(&scopes, &mut created));
    assert_eq!(created.paths, [PathBuf::new()]);
    let mut removed = event(remove, map.parent().unwrap());
    assert!(accept_event(&scopes, &mut removed));
    assert_eq!(removed.paths, [PathBuf::new()]);
}

#[test]
fn os_watch_ignores_siblings_and_recovers_missing_and_replaced_roots() {
    let temp = tempfile::tempdir().unwrap();
    let game = crate::game::resolve_existing_ancestor(temp.path()).unwrap();
    let map = game.join("BepInEx/plugins/New/levels");
    let old = game.join("BepInEx/plugins/Old/levels");
    let disabled = game.join("BepInEx/cache/modmanager_disabled_maps");
    fs::create_dir_all(&map).unwrap();
    fs::create_dir_all(&old).unwrap();
    let watcher = LevelWatcher::default();
    let register = |reattach| {
        let (sent, received) = mpsc::channel();
        let scopes = [map.clone(), disabled.clone()]
            .into_iter()
            .map(|path| RootScope::new(&game, path.clone(), path))
            .collect();
        watcher.watch_paths(scopes, reattach, move |batch| {
            let _ = sent.send(batch);
        });
        received
    };
    let received = register(false);
    fs::write(old.join("not-a-current-map"), "ignored").unwrap();
    fs::write(map.parent().unwrap().join("not-in-levels"), "ignored").unwrap();
    fs::write(game.join("BepInEx/neighbor"), "ignored").unwrap();
    assert_eq!(
        received.recv_timeout(Duration::from_millis(1200)),
        Err(mpsc::RecvTimeoutError::Timeout)
    );
    let current_set = map.join("current");
    fs::create_dir(&current_set).unwrap();
    fs::write(current_set.join("info"), "first").unwrap();
    assert_eq!(
        received.recv_timeout(Duration::from_secs(5)).unwrap(),
        Some(vec!["current".into()])
    );

    // Missing intermediate cache directory then the disabled root both appear.
    fs::create_dir_all(&disabled).unwrap();
    assert_eq!(received.recv_timeout(Duration::from_secs(5)).unwrap(), None);
    let received = register(true);
    fs::create_dir(disabled.join("paused")).unwrap();
    assert_eq!(
        received.recv_timeout(Duration::from_secs(5)).unwrap(),
        Some(vec!["paused".into()])
    );

    // Directory replacement at the same path is not the same native handle.
    fs::rename(&map, game.join("retired-levels")).unwrap();
    fs::create_dir(&map).unwrap();
    assert_eq!(received.recv_timeout(Duration::from_secs(5)).unwrap(), None);
    let received = register(true);
    fs::create_dir(map.join("fresh")).unwrap();
    assert_eq!(
        received.recv_timeout(Duration::from_secs(5)).unwrap(),
        Some(vec!["fresh".into()])
    );
}
