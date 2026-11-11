use super::*;
use crate::mods::plugin_target::{DIYCHEF_GUID, DIYLEVEL_GUID};
use crate::{app_state::AppState, config::ConfigService, models::InstalledMod};
use std::fs;

#[test]
fn older_scan_cannot_publish() {
    let mut s = LocalModSnapshots::default();
    let p = Path::new("game");
    let a = s.begin(Some(p));
    let b = s.begin(Some(p));
    assert!(s.publish(a, Some(p), vec![]).is_err());
    assert!(s.publish(b, Some(p), vec![]).is_ok());
    s.invalidate();
    assert!(s.get(b, p).is_err());
}

#[test]
fn trusted_generation_is_bound_to_root_and_completed_scan() {
    let mut s = LocalModSnapshots::default();
    let a = Path::new("game-a");
    let b = Path::new("game-b");
    let first = s.begin(Some(a));
    assert!(
        s.get(first, a).is_err(),
        "pending scan cannot authorize an editor"
    );
    assert!(s.publish(first, Some(b), vec![]).is_err());
    s.publish(first, Some(a), vec![]).unwrap();
    assert!(s.get(first, b).is_err());
    assert_eq!(s.get(first, a).unwrap().generation, first);
    let next = s.begin(Some(b));
    assert!(next > first);
    assert!(s.get(first, a).is_err());
    assert!(s.publish(first, Some(a), vec![]).is_err());
    s.publish(next, Some(b), vec![]).unwrap();
    assert_eq!(s.get(next, b).unwrap().generation, next);
}

struct Workspace {
    root: PathBuf,
    game: PathBuf,
    state: AppState,
}

impl Workspace {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("oc2-snapshot-roots-{}", uuid::Uuid::new_v4()));
        let game = root.join("game");
        fs::create_dir_all(game.join("BepInEx/plugins")).unwrap();
        fs::write(game.join("Overcooked2.exe"), b"fixture").unwrap();
        let config = ConfigService::for_test(root.join("manager"));
        let mut settings = config.load_or_default();
        settings.game.path = game.to_string_lossy().into_owned();
        settings.game.runtime = crate::models::GameRuntime::Wine;
        config.save(&settings).unwrap();
        Self {
            root,
            game,
            state: AppState::with_config(config),
        }
    }

    fn plugin(&self, relative: &str, guid: &str, version: &str, enabled: bool) -> PathBuf {
        let path = self.game.join(if enabled {
            relative.to_string()
        } else {
            crate::mods::disabled_dll_file(relative)
        });
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            crate::mods::test_support::plugin_blob(guid, "FastInit", version),
        )
        .unwrap();
        path
    }

    fn publish(&self, game: &Path, installed: Vec<InstalledMod>) -> LocalModSnapshot {
        let canonical = game.canonicalize().unwrap();
        let mut snapshots = self.state.local_mod_snapshot.lock().unwrap();
        let generation = snapshots.begin(Some(&canonical));
        snapshots
            .publish(generation, Some(&canonical), installed)
            .unwrap()
    }

    fn scan_and_publish(&self) -> LocalModSnapshot {
        let installed = crate::mods::scan_mods_with_manifests(
            &self.game,
            &self.state.config.load_or_default(),
            None,
        );
        self.publish(&self.game, installed)
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let root = self.root.canonicalize().unwrap();
        let temp = std::env::temp_dir().canonicalize().unwrap();
        assert!(root.starts_with(&temp) && root != temp);
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn completed_real_scan_chooses_highest_disabled_dll_and_its_actual_directory() {
    let ws = Workspace::new();
    let lower = ws.plugin(
        "BepInEx/plugins/OC2DIYLevel/DIYLevel.dll",
        DIYLEVEL_GUID,
        "1.2.3",
        true,
    );
    let highest = ws.plugin(
        "BepInEx/plugins/RenamedFastInit/Actual.dll",
        DIYLEVEL_GUID,
        "1.2.3.4",
        false,
    );
    ws.plugin(
        "BepInEx/plugins/DIYChef/Chef.dll",
        DIYCHEF_GUID,
        "99.0.0",
        true,
    );
    let published = ws.scan_and_publish();
    assert_eq!(
        published.diy_level_target.as_deref(),
        Some("BepInEx/plugins/RenamedFastInit/Actual.dll")
    );
    let (snapshot, roots) = level_roots(&ws.state, &ws.game).unwrap();
    assert_eq!(snapshot.generation, published.generation);
    assert_eq!(roots.game_root(), ws.game.canonicalize().unwrap());
    assert_eq!(
        roots.package_root(),
        Some(highest.parent().unwrap().canonicalize().unwrap().as_path())
    );
    assert!(lower.is_file() && highest.is_file());
    assert!(
        snapshot
            .mods
            .iter()
            .any(|item| !item.enabled && item.version.as_deref() == Some("1.2.3.4"))
    );
}

#[test]
fn equal_highest_versions_remain_ambiguous_even_when_one_dll_is_disabled() {
    let ws = Workspace::new();
    ws.plugin(
        "BepInEx/plugins/Lower/DIYLevel.dll",
        DIYLEVEL_GUID,
        "1.0",
        true,
    );
    ws.plugin(
        "BepInEx/plugins/LatestA/DIYLevel.dll",
        DIYLEVEL_GUID,
        "2.0",
        true,
    );
    ws.plugin(
        "BepInEx/plugins/LatestB/DIYLevel.dll",
        DIYLEVEL_GUID,
        "2.0.0",
        false,
    );
    let published = ws.scan_and_publish();
    assert!(published.diy_level_target.is_none());
    assert_eq!(
        level_roots(&ws.state, &ws.game).unwrap_err(),
        "framework.diyLevelAmbiguous"
    );
    assert_eq!(
        optional_level_roots(&ws.state).unwrap_err(),
        "framework.diyLevelAmbiguous"
    );
}

#[test]
fn no_plugin_keeps_package_absent_even_with_a_historical_levels_directory() {
    let ws = Workspace::new();
    let historical = ws.game.join("BepInEx/plugins/OC2DIYLevel/levels/OldMap");
    fs::create_dir_all(&historical).unwrap();
    fs::write(historical.join("info"), b"old map").unwrap();
    ws.plugin(
        "BepInEx/plugins/ChefOnly/DIYChef.dll",
        DIYCHEF_GUID,
        "2.2.0",
        true,
    );
    let published = ws.scan_and_publish();
    assert!(published.diy_level_target.is_none());
    let (_, roots) = level_roots(&ws.state, &ws.game).unwrap();
    assert!(roots.package_root().is_none());
    assert_eq!(
        crate::level_sets::levels_root(&roots).unwrap_err(),
        "framework.diyLevelMissing"
    );
    assert!(
        optional_level_roots(&ws.state)
            .unwrap()
            .unwrap()
            .package_root()
            .is_none()
    );
    assert!(historical.join("info").is_file());
}

#[test]
fn unpublished_files_cannot_replace_the_completed_backend_scan() {
    let ws = Workspace::new();
    let canonical = ws.game.canonicalize().unwrap();
    let generation = ws
        .state
        .local_mod_snapshot
        .lock()
        .unwrap()
        .begin(Some(&canonical));
    ws.plugin(
        "BepInEx/plugins/OC2DIYLevel/DIYLevel.dll",
        DIYLEVEL_GUID,
        "9.0",
        true,
    );
    assert_eq!(
        level_roots(&ws.state, &ws.game).unwrap_err(),
        "diychef.staleScan"
    );
    assert_eq!(
        optional_level_roots(&ws.state).unwrap_err(),
        "diychef.staleScan"
    );
    ws.state
        .local_mod_snapshot
        .lock()
        .unwrap()
        .publish(generation, Some(&canonical), vec![])
        .unwrap();
    assert!(
        level_roots(&ws.state, &ws.game)
            .unwrap()
            .1
            .package_root()
            .is_none()
    );
}

#[test]
fn vanished_selected_dll_errors_instead_of_falling_back_to_a_lower_version() {
    let ws = Workspace::new();
    let lower = ws.plugin(
        "BepInEx/plugins/OC2DIYLevel/DIYLevel.dll",
        DIYLEVEL_GUID,
        "1.0",
        true,
    );
    let highest = ws.plugin(
        "BepInEx/plugins/Latest/DIYLevel.dll",
        DIYLEVEL_GUID,
        "2.0",
        false,
    );
    ws.scan_and_publish();
    fs::remove_file(highest).unwrap();
    assert_eq!(
        level_roots(&ws.state, &ws.game).unwrap_err(),
        "mods.fileMissing"
    );
    assert!(lower.is_file());
}

#[test]
fn beginning_a_new_scan_revokes_old_leases_before_new_results_exist() {
    let ws = Workspace::new();
    ws.plugin(
        "BepInEx/plugins/Current/DIYLevel.dll",
        DIYLEVEL_GUID,
        "2.0",
        true,
    );
    let first = ws.scan_and_publish();
    let canonical = ws.game.canonicalize().unwrap();
    let first_lease = ws
        .state
        .local_mod_snapshot
        .lock()
        .unwrap()
        .lease(first.generation, &canonical)
        .unwrap();
    let worker_lease = first_lease.clone();
    assert!(worker_lease.load(Ordering::SeqCst));
    let generation = ws
        .state
        .local_mod_snapshot
        .lock()
        .unwrap()
        .begin(Some(&canonical));
    assert!(!worker_lease.load(Ordering::SeqCst));
    assert_eq!(
        level_roots(&ws.state, &ws.game).unwrap_err(),
        "diychef.staleScan"
    );
    let mut snapshots = ws.state.local_mod_snapshot.lock().unwrap();
    assert!(snapshots.lease(first.generation, &canonical).is_err());
    assert!(snapshots.lease(generation, &canonical).is_err());
    assert!(
        snapshots
            .publish(first.generation, Some(&canonical), first.mods.clone())
            .is_err()
    );
    snapshots
        .publish(generation, Some(&canonical), first.mods)
        .unwrap();
    let current = snapshots.lease(generation, &canonical).unwrap();
    assert!(current.load(Ordering::SeqCst));
    assert!(!Arc::ptr_eq(&first_lease, &current));
    assert!(!first_lease.load(Ordering::SeqCst));
}

#[test]
fn invalidation_revokes_current_lease_and_rejects_old_publication() {
    let ws = Workspace::new();
    ws.plugin(
        "BepInEx/plugins/Current/DIYLevel.dll",
        DIYLEVEL_GUID,
        "2.0",
        true,
    );
    let first = ws.scan_and_publish();
    let canonical = ws.game.canonicalize().unwrap();
    let lease = ws
        .state
        .local_mod_snapshot
        .lock()
        .unwrap()
        .lease(first.generation, &canonical)
        .unwrap();
    ws.state.invalidate_local_mod_snapshot().unwrap();
    assert!(!lease.load(Ordering::SeqCst));
    assert_eq!(
        level_roots(&ws.state, &ws.game).unwrap_err(),
        "diychef.staleScan"
    );
    let mut snapshots = ws.state.local_mod_snapshot.lock().unwrap();
    assert!(snapshots.get(first.generation, &canonical).is_err());
    assert!(snapshots.lease(first.generation, &canonical).is_err());
    assert!(
        snapshots
            .publish(first.generation, Some(&canonical), first.mods)
            .is_err()
    );
}

#[test]
fn switching_game_root_requires_a_new_completed_scan_and_keeps_old_lease_revoked() {
    let ws = Workspace::new();
    ws.plugin(
        "BepInEx/plugins/Current/DIYLevel.dll",
        DIYLEVEL_GUID,
        "2.0",
        true,
    );
    let first = ws.scan_and_publish();
    let original = ws.game.canonicalize().unwrap();
    let lease = ws
        .state
        .local_mod_snapshot
        .lock()
        .unwrap()
        .lease(first.generation, &original)
        .unwrap();
    let other = ws.root.join("game-b");
    fs::create_dir_all(other.join("BepInEx/plugins")).unwrap();
    fs::write(other.join("Overcooked2.exe"), b"fixture").unwrap();
    let mut settings = ws.state.config.load_or_default();
    settings.game.path = other.to_string_lossy().into_owned();
    ws.state.config.save(&settings).unwrap();
    assert_eq!(
        optional_level_roots(&ws.state).unwrap_err(),
        "diychef.staleScan"
    );
    ws.publish(&other, vec![]);
    assert!(!lease.load(Ordering::SeqCst));
    assert_eq!(
        level_roots(&ws.state, &ws.game).unwrap_err(),
        "diychef.staleScan"
    );
    let roots = optional_level_roots(&ws.state).unwrap().unwrap();
    assert_eq!(roots.game_root(), other.canonicalize().unwrap());
    assert!(roots.package_root().is_none());
    assert!(
        ws.state
            .local_mod_snapshot
            .lock()
            .unwrap()
            .get(first.generation, &original)
            .is_err()
    );
}
