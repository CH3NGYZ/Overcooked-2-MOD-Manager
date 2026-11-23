use super::*;

struct Fixture {
    root: PathBuf,
    game: PathBuf,
    state: AppState,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("oc2-diy-command-{}", uuid::Uuid::new_v4()));
        let game = root.join("game");
        std::fs::create_dir_all(game.join("BepInEx/plugins")).unwrap();
        std::fs::write(game.join("Overcooked2.exe"), b"fixture").unwrap();
        let config = crate::config::ConfigService::for_test(root.join("data"));
        let mut settings = config.load_or_default();
        settings.game.path = game.to_string_lossy().to_string();
        settings.game.runtime = crate::models::GameRuntime::Wine;
        config.save(&settings).unwrap();
        Self {
            game,
            root,
            state: AppState::with_config(config),
        }
    }
    fn plugin(&self, name: &str, version: &str, enabled: bool) -> InstalledMod {
        let directory = self.game.join("BepInEx/plugins").join(name);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join(if enabled {
                "DIYLevel.dll"
            } else {
                "DIYLevel.dll.disabled"
            }),
            b"metadata comes from the completed scan",
        )
        .unwrap();
        serde_json::from_value(serde_json::json!({
            "repository_id": null, "name": "DIYLevel", "dll_file": format!("BepInEx/plugins/{name}/DIYLevel.dll"),
            "guid": crate::mods::plugin_target::DIYLEVEL_GUID, "enabled": enabled, "version": version,
            "version_source": "plugin", "update_available": false, "duplicate_guid": false, "status": "unknown"
        })).unwrap()
    }
    fn publish(&self, mods: Vec<InstalledMod>) -> u64 {
        let root = std::fs::canonicalize(&self.game).unwrap();
        let mut snapshots = self.state.local_mod_snapshot.lock().unwrap();
        let generation = snapshots.begin(Some(&root));
        snapshots.publish(generation, Some(&root), mods).unwrap();
        generation
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn info_reuses_completed_scan_and_reports_highest_disabled_plugin() {
    let fixture = Fixture::new();
    let lower = fixture.plugin("OC2DIYLevel", "1.2.3", true);
    let highest = fixture.plugin("RenamedFastInit", "1.2.3.4", false);
    let generation = fixture.publish(vec![lower, highest]);
    let info = get_diy_level_info_blocking(&fixture.state).unwrap();
    assert!(info.installed);
    assert!(!info.enabled);
    assert_eq!(info.version.as_deref(), Some("1.2.3.4"));
    assert_eq!(
        info.dll_file.as_deref(),
        Some("BepInEx/plugins/RenamedFastInit/DIYLevel.dll")
    );
    let root = std::fs::canonicalize(&fixture.game).unwrap();
    assert_eq!(
        fixture
            .state
            .local_mod_snapshot
            .lock()
            .unwrap()
            .current(&root)
            .unwrap()
            .generation,
        generation
    );
}

#[test]
fn pending_or_ambiguous_scans_do_not_choose_a_historical_or_first_directory() {
    let fixture = Fixture::new();
    let first = fixture.plugin("First", "2.0", true);
    let second = fixture.plugin("Second", "2.0", false);
    assert!(
        matches!(get_diy_level_info_blocking(&fixture.state), Err(error) if error == "diychef.staleScan")
    );
    fixture.publish(vec![first, second]);
    assert!(
        matches!(get_diy_level_info_blocking(&fixture.state), Err(error) if error == "framework.diyLevelAmbiguous")
    );
    fixture.publish(vec![]);
    let info = get_diy_level_info_blocking(&fixture.state).unwrap();
    assert!(!info.installed);
}

#[test]
fn legacy_framework_toggle_is_blocked_without_changing_disk() {
    let fixture = Fixture::new();
    let lower = fixture.plugin("Lower", "1.0", true);
    let highest = fixture.plugin("Highest", "2.0", false);
    fixture.publish(vec![lower, highest]);
    assert_eq!(
        crate::mod_changes::guard::require_shared_plan::<OperationResult>().unwrap_err(),
        "modChanges.installationBlocked"
    );
    assert!(
        fixture
            .game
            .join("BepInEx/plugins/Highest/DIYLevel.dll.disabled")
            .is_file()
    );
    assert!(
        !fixture
            .game
            .join("BepInEx/plugins/Highest/DIYLevel.dll")
            .exists()
    );
    assert!(
        fixture
            .game
            .join("BepInEx/plugins/Lower/DIYLevel.dll")
            .is_file()
    );
}

#[test]
fn manual_framework_backup_and_uninstall_preserve_other_versions_and_backup_real_root() {
    let fixture = Fixture::new();
    let lower = fixture.plugin("Lower", "1.0", true);
    let highest = fixture.plugin("Highest", "2.0", false);
    fixture.publish(vec![lower, highest]);
    assert!(backup_diy_level_blocking(&fixture.state).unwrap().ok);
    assert!(uninstall_diy_level_blocking(&fixture.state).unwrap().ok);
    assert!(
        !fixture
            .game
            .join("BepInEx/plugins/Highest/DIYLevel.dll.disabled")
            .exists()
    );
    assert!(
        fixture
            .game
            .join("BepInEx/plugins/Lower/DIYLevel.dll")
            .is_file()
    );
    let backups = list_diy_level_backups(&fixture.state.config.backups_dir());
    assert_eq!(backups.len(), 2);
    for backup in backups {
        assert!(
            Path::new(&backup)
                .join("BepInEx/plugins/Highest/DIYLevel.dll.disabled")
                .is_file()
        );
        assert!(!Path::new(&backup).join("BepInEx/plugins/Lower").exists());
    }
}
