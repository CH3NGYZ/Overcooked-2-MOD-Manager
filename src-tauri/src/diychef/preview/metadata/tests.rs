use super::*;
use std::{
    sync::{
        Barrier,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, UNIX_EPOCH},
};

fn snapshot(stamp: &str) -> ResultSnapshot {
    Ok(Arc::new(Snapshot {
        paths: vec![],
        manifest: String::new(),
        stamp: stamp.into(),
    }))
}

#[test]
fn overlapping_requests_share_one_scan_but_the_next_call_always_scans_again() {
    let flights = Flights::default();
    let started = Barrier::new(2);
    let release = Barrier::new(2);
    let reads = AtomicUsize::new(0);
    let key = (0, PathBuf::from("same-game"));
    std::thread::scope(|scope| {
        let owner = scope.spawn(|| {
            flights
                .run(key.clone(), || {
                    reads.fetch_add(1, Ordering::SeqCst);
                    started.wait();
                    release.wait();
                    snapshot("first")
                })
                .unwrap()
        });
        started.wait();
        let others: Vec<_> = (0..31)
            .map(|_| {
                scope.spawn(|| {
                    flights
                        .run(key.clone(), || {
                            reads.fetch_add(1, Ordering::SeqCst);
                            snapshot("duplicate")
                        })
                        .unwrap()
                })
            })
            .collect();
        // Each caller has cloned the same flight once its strong count rises.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let joined = loop {
            let count = flights
                .active
                .lock()
                .unwrap()
                .get(&key)
                .map(Arc::strong_count)
                .unwrap();
            if count == 33 {
                break true;
            }
            if std::time::Instant::now() >= deadline {
                break false;
            }
            std::thread::yield_now();
        };
        release.wait();
        let first = owner.join().unwrap();
        for other in others {
            let other = other.join().unwrap();
            assert!(Arc::ptr_eq(&first, &other));
        }
        assert!(joined, "callers did not join the scan");
    });
    assert_eq!(reads.load(Ordering::SeqCst), 1);
    assert!(flights.active.lock().unwrap().is_empty());
    assert_eq!(
        flights
            .run(key, || {
                reads.fetch_add(1, Ordering::SeqCst);
                snapshot("changed")
            })
            .unwrap()
            .stamp,
        "changed"
    );
    assert_eq!(
        reads.load(Ordering::SeqCst),
        2,
        "completed metadata is never a TTL cache"
    );
}

#[test]
fn clearing_releases_old_waiters_and_does_not_admit_old_results_into_a_new_epoch() {
    let flights = Flights::default();
    let started = Barrier::new(2);
    let release = Barrier::new(2);
    std::thread::scope(|scope| {
        let owner = scope.spawn(|| {
            flights.run((0, "game".into()), || {
                started.wait();
                release.wait();
                snapshot("old")
            })
        });
        started.wait();
        flights.clear();
        assert_eq!(
            flights
                .run((2, "game".into()), || snapshot("new"))
                .unwrap()
                .stamp,
            "new"
        );
        release.wait();
        assert!(owner.join().unwrap().is_err());
    });
    assert!(flights.active.lock().unwrap().is_empty());
}

#[test]
fn errors_and_panics_leave_the_scan_retryable() {
    let flights = Flights::default();
    assert!(
        flights
            .run((0, "game".into()), || Err("fixture".into()))
            .is_err()
    );
    assert!(
        std::panic::catch_unwind(|| flights.run((0, "game".into()), || panic!("fixture"))).is_err()
    );
    assert_eq!(
        flights
            .run((0, "game".into()), || snapshot("retry"))
            .unwrap()
            .stamp,
        "retry"
    );
    assert!(flights.active.lock().unwrap().is_empty());
}

fn stamp(game: &Path) -> String {
    super::super::fingerprint::metadata_files(&source_metadata(game).unwrap()).unwrap()
}

#[test]
fn every_new_scan_detects_source_add_rename_delete_size_and_mtime_changes() {
    let root = std::env::temp_dir().join(format!("chef-source-metadata-{}", uuid::Uuid::new_v4()));
    let base = root.join("Overcooked2_Data/StreamingAssets/DLC");
    std::fs::create_dir_all(&base).unwrap();
    let path = base.join("bundle1");
    std::fs::write(&path, b"abc").unwrap();
    let first = stamp(&root);
    assert_eq!(first, stamp(&root));
    let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    let time = file.metadata().unwrap().modified().unwrap();
    file.set_len(4).unwrap();
    file.set_times(std::fs::FileTimes::new().set_modified(time))
        .unwrap();
    assert_ne!(first, stamp(&root));
    file.set_len(3).unwrap();
    file.set_times(std::fs::FileTimes::new().set_modified(time))
        .unwrap();
    assert_eq!(first, stamp(&root));
    file.set_times(
        std::fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(2_000_000_000)),
    )
    .unwrap();
    let retimed = stamp(&root);
    assert_ne!(first, retimed);
    drop(file);
    std::fs::rename(&path, base.join("renamed")).unwrap();
    let renamed = stamp(&root);
    assert_ne!(retimed, renamed);
    std::fs::write(base.join("extra"), b"x").unwrap();
    assert_ne!(renamed, stamp(&root));
    std::fs::remove_file(base.join("extra")).unwrap();
    assert_eq!(renamed, stamp(&root));
    let canonical = root.canonicalize().unwrap();
    let temp = std::env::temp_dir().canonicalize().unwrap();
    assert!(canonical.starts_with(&temp) && canonical != temp);
    std::fs::remove_dir_all(canonical).unwrap();
}

#[tokio::test]
async fn metadata_admission_is_async_and_limited_before_blocking_work() {
    let peak = Arc::new(AtomicUsize::new(0));
    let active = Arc::new(AtomicUsize::new(0));
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..64 {
        let (active, peak) = (active.clone(), peak.clone());
        tasks.spawn(async move {
            let _permit = super::super::fingerprint_permit().await.unwrap();
            let now = active.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(now, Ordering::SeqCst);
            tokio::task::yield_now().await;
            active.fetch_sub(1, Ordering::SeqCst);
        });
    }
    while let Some(result) = tasks.join_next().await {
        result.unwrap();
    }
    assert_eq!(peak.load(Ordering::SeqCst), 4);
    assert_eq!(active.load(Ordering::SeqCst), 0);
}

#[test]
#[ignore = "requires installed game resources; reads metadata only, no game process"]
fn real_metadata_benchmark() {
    let game = Path::new("E:/SteamLibrary/steamapps/common/Overcooked! 2");
    let start = std::time::Instant::now();
    let rows = source_metadata(game).unwrap();
    let bytes: usize = rows
        .iter()
        .map(|(path, _)| path.as_os_str().as_encoded_bytes().len())
        .sum();
    let manifest = super::super::fingerprint::metadata_files(&rows).unwrap();
    let snapshot_bytes = bytes + rows.len() * std::mem::size_of::<PathBuf>() + manifest.len() + 64;
    eprintln!(
        "native metadata files={} path_bytes={bytes} snapshot_payload_bytes={snapshot_bytes} elapsed={:?}",
        rows.len(),
        start.elapsed()
    );
    let start = std::time::Instant::now();
    for _ in 0..61 {
        assert_eq!(manifest, stamp(game));
    }
    eprintln!("61 exact native metadata checks: {:?}", start.elapsed());
}

#[test]
#[ignore = "requires installed real DIYChef; reads game only, writes UUID temp manager config"]
fn real_preview_authorization_and_fingerprint_benchmark() {
    use crate::{app_state::AppState, config::ConfigService, models::InstalledMod};
    let game = Path::new("E:/SteamLibrary/steamapps/common/Overcooked! 2")
        .canonicalize()
        .unwrap();
    let temp =
        std::env::temp_dir().join(format!("chef-preview-auth-bench-{}", uuid::Uuid::new_v4()));
    let state = AppState::with_config(ConfigService::for_test(temp.clone()));
    let mut config = state.config.load().unwrap();
    config.game.path = game.to_string_lossy().into_owned();
    state.config.save(&config).unwrap();
    let installed: InstalledMod = serde_json::from_value(serde_json::json!({
        "name":"DIYChefFastInit", "guid":crate::diychef::target::GUID, "version":"2.2.0",
        "dll_file":"BepInEx/plugins/OC2DIYChef-FastInit/DIYChefFastInit.dll", "enabled":true,
        "version_source":"plugin", "update_available":false, "duplicate_guid":false, "status":"unknown"
    })).unwrap();
    let generation = state.local_mod_snapshot.lock().unwrap().begin(Some(&game));
    state
        .local_mod_snapshot
        .lock()
        .unwrap()
        .publish(generation, Some(&game), vec![installed])
        .unwrap();
    let opened = crate::diychef::session::open(&state, generation, None)
        .unwrap()
        .snapshot
        .unwrap();
    let hat = opened
        .hats
        .iter()
        .find(|hat| hat.config_value == "None")
        .unwrap();
    let natives: Vec<_> = opened
        .chefs
        .iter()
        .filter(|chef| chef.source_kind == "native")
        .collect();
    let start = std::time::Instant::now();
    for chef in &natives {
        let context =
            crate::diychef::resolve_preview_context(&state, &opened.session_id, &chef.id, &hat.id)
                .unwrap();
        assert_eq!(context.game_root, game);
    }
    eprintln!(
        "{} native read-only context authorizations: {:?}",
        natives.len(),
        start.elapsed()
    );
    let start = std::time::Instant::now();
    for chef in &natives {
        assert_eq!(
            crate::diychef::preview::fingerprint(&state, &opened.session_id, &chef.id, &hat.id)
                .unwrap()
                .len(),
            64
        );
    }
    eprintln!(
        "{} native full authorization + exact fingerprints: {:?}",
        natives.len(),
        start.elapsed()
    );
    let root = temp.canonicalize().unwrap();
    let system_temp = std::env::temp_dir().canonicalize().unwrap();
    assert!(root.starts_with(&system_temp) && root != system_temp);
    std::fs::remove_dir_all(root).unwrap();
}
