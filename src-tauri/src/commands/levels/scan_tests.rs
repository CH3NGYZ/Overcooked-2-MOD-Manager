use super::*;
use crate::{config::ConfigService, models::InstalledMod, mods::plugin_target::DIYLEVEL_GUID};
use std::fs;

struct Workspace {
    root: PathBuf,
    game: PathBuf,
    state: AppState,
}
impl Workspace {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("oc2-level-scan-command-{}", uuid::Uuid::new_v4()));
        let game = root.join("game");
        fs::create_dir_all(game.join("BepInEx/plugins/Selected/disabled/LegacyMap")).unwrap();
        fs::write(game.join("Overcooked2.exe"), b"fixture").unwrap();
        fs::write(
            game.join("BepInEx/plugins/Selected/Selected.dll"),
            b"fixture",
        )
        .unwrap();
        let config = ConfigService::for_test(root.join("manager"));
        let mut settings = config.load_or_default();
        settings.game.path = game.to_string_lossy().into_owned();
        config.save(&settings).unwrap();
        Self {
            root,
            game,
            state: AppState::with_config(config),
        }
    }
    fn publish(&self, selected: bool) {
        let game = self.game.canonicalize().unwrap();
        let installed: InstalledMod = serde_json::from_value(serde_json::json!({
            "name":"DIYLevel", "guid":DIYLEVEL_GUID, "version":"3.0",
            "dll_file":"BepInEx/plugins/Selected/Selected.dll", "enabled":true,
            "version_source":"plugin", "update_available":false, "duplicate_guid":false,"status":"unknown"
        })).unwrap();
        let mut snapshots = self.state.local_mod_snapshot.lock().unwrap();
        let generation = snapshots.begin(Some(&game));
        snapshots
            .publish(
                generation,
                Some(&game),
                if selected { vec![installed] } else { vec![] },
            )
            .unwrap();
    }
    fn legacy(&self) -> PathBuf {
        self.game
            .join("BepInEx/plugins/Selected/disabled/LegacyMap")
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
fn migration_requires_current_completed_plugin_scan_and_pure_scanning_never_moves_files() {
    let ws = Workspace::new();
    assert_eq!(
        prepare_level_set_scan(&ws.state).unwrap_err(),
        "diychef.staleScan"
    );
    assert!(ws.legacy().is_dir());
    ws.publish(true);
    let (_, roots) = crate::mods::snapshot::level_roots(&ws.state, &ws.game).unwrap();
    crate::level_sets::scan_level_sets_with_progress(&roots, None, 1, &|_, _, _| {});
    assert!(
        ws.legacy().is_dir(),
        "metadata scanning must stay read-only"
    );
    assert!(!crate::level_sets::disabled_root(&ws.game).exists());
    ws.state.invalidate_local_mod_snapshot().unwrap();
    assert_eq!(
        prepare_level_set_scan(&ws.state).unwrap_err(),
        "diychef.staleScan"
    );
    assert!(
        ws.legacy().is_dir(),
        "invalidated roots cannot authorize migration"
    );
    ws.publish(true);
    let (_, _, selected) = prepare_level_set_scan(&ws.state).unwrap().unwrap();
    assert_eq!(
        selected.package_root().unwrap(),
        ws.game
            .join("BepInEx/plugins/Selected")
            .canonicalize()
            .unwrap()
    );
    assert!(!ws.legacy().exists());
    assert!(
        crate::level_sets::disabled_root(&ws.game)
            .join("LegacyMap")
            .is_dir()
    );
}

#[test]
fn a_completed_scan_without_plugin_leaves_legacy_directories_untouched() {
    let ws = Workspace::new();
    ws.publish(false);
    let (_, _, roots) = prepare_level_set_scan(&ws.state).unwrap().unwrap();
    assert!(roots.package_root().is_none());
    assert!(ws.legacy().is_dir());
    assert!(!crate::level_sets::disabled_root(&ws.game).exists());
}
