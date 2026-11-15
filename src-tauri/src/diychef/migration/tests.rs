use super::*;
use std::{fs, path::PathBuf};

struct Workspace {
    root: PathBuf,
    game: PathBuf,
    manifests: PathBuf,
    recycle: PathBuf,
}
impl Workspace {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("oc2-chef-migration-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("game/BepInEx/plugins")).unwrap();
        let root = root.canonicalize().unwrap();
        let game = root.join("game");
        fs::create_dir_all(game.join("Overcooked2.app/Contents/MacOS")).unwrap();
        fs::write(game.join("Overcooked2.exe"), b"fixture").unwrap();
        Self {
            manifests: root.join("manager/manifests"),
            recycle: root.join("manager/recycle-bin"),
            game,
            root,
        }
    }
    fn write(&self, relative: &str, bytes: &[u8]) {
        let path = self.game.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    fn plugin(&self, folder: &str, version: &str) -> InstalledMod {
        let path = format!("BepInEx/plugins/{folder}/DIYChef.dll");
        self.write(&path, &mods::plugin_blob(target::GUID, "DIYChef", version));
        serde_json::from_value(serde_json::json!({ "name": "DIYChef", "guid": target::GUID,
            "version": version, "dll_file": path, "enabled": true, "version_source": "BepInPlugin",
            "update_available": false, "duplicate_guid": false, "status": "unknown" }))
        .unwrap()
    }
    fn registered_map_plugin(&self, folder: &str, suffix: &str) -> PathBuf {
        self.write(
            "BepInEx/plugins/MapLoader/levels/OwnedMap/info",
            b"installed map info",
        );
        self.write(
            "BepInEx/plugins/MapLoader/levels/OwnedMap/scene",
            b"installed map scene",
        );
        let dll = format!("BepInEx/plugins/{folder}/MapPlugin.dll");
        let actual = format!("{dll}{suffix}");
        self.write(
            &actual,
            &mods::plugin_blob("com.example.map.owned.plugin", "MapPlugin", "1.0"),
        );
        fs::create_dir_all(&self.manifests).unwrap();
        fs::write(
            self.manifests
                .join(crate::level_sets::LEVEL_SET_PLUGIN_REGISTRY),
            serde_json::to_vec(&serde_json::json!({ "version": 1, "entries": [{
                "dll": dll, "owner": { "name": "OwnedMap", "author": "map-author" }
            }] }))
            .unwrap(),
        )
        .unwrap();
        self.game.join(actual)
    }
    fn execute(&self, installed: &[InstalledMod]) -> Result<(), String> {
        let selected = target::select(&target::candidates(installed, 1), None)?.unwrap();
        migrate_targets(
            &self.game,
            &self.manifests,
            &self.recycle,
            &selected,
            &legacy_targets(installed, 1),
            &[],
            installed,
            &mut recycle_bin::set_diychef_resources_imported,
        )
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        let temporary = std::env::temp_dir().canonicalize().unwrap();
        assert!(self.root.starts_with(&temporary) && self.root != temporary);
        fs::remove_dir_all(&self.root).unwrap();
    }
}

#[test]
fn detection_requires_multiple_plugins_and_only_known_pre_2_versions() {
    let w = Workspace::new();
    let old = w.plugin("old", "1.9");
    assert!(legacy_targets(&[old.clone()], 1).is_empty());
    let modern = w.plugin("new", "2.2");
    let mut unknown = modern.clone();
    unknown.version = None;
    assert_eq!(
        legacy_targets(&[old.clone(), modern, unknown], 8)[0].id,
        old.dll_file
    );
}

#[test]
fn imports_collisions_without_overwriting_and_recycles_all_old_directories_in_one_restorable_record()
 {
    let w = Workspace::new();
    let installed = [
        w.plugin("new", "2.2"),
        w.plugin("old-a", "1.9"),
        w.plugin("old-b", "1.8"),
    ];
    w.write(
        "BepInEx/plugins/new/Resources/Fox/Body.obj",
        b"keep existing",
    );
    w.write(
        "BepInEx/plugins/new/Resources/HATS/Cap/t_Cap.png",
        b"keep hat",
    );
    w.write("BepInEx/plugins/old-a/Resources/Fox/Body.obj", b"old fox");
    w.write(
        "BepInEx/plugins/old-a/Resources/HATS/Cap/t_Cap.png",
        b"old hat",
    );
    w.write(
        "BepInEx/plugins/old-a/Resources/HATS/Cap/m_Cap.obj",
        b"old mesh",
    );
    w.write(
        "BepInEx/plugins/old-b/Resources/Fox/Body.obj",
        b"second fox",
    );
    w.write("BepInEx/plugins/old-b/readme.txt", b"kept in recycle");
    let manifest = crate::archive::InstallManifest {
        repository_id: "old-chef".into(),
        release_tag: "v1.9".into(),
        repository_binding: None,
        package_name_i18n: Default::default(),
        note_i18n: Default::default(),
        group_i18n: Default::default(),
        prerelease: false,
        relations: vec![],
        installed_at: "2026-11-13T00:00:00Z".into(),
        files: vec![
            installed[1].dll_file.clone(),
            "BepInEx/plugins/old-a/Resources/Fox/Body.obj".into(),
        ],
    };
    crate::archive::write_manifest(&w.manifests, &manifest).unwrap();
    w.execute(&installed).unwrap();
    let resources = w.game.join("BepInEx/plugins/new/Resources");
    assert_eq!(
        fs::read(resources.join("Fox/Body.obj")).unwrap(),
        b"keep existing"
    );
    let foxes = fs::read_dir(&resources)
        .unwrap()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("Fox_"))
        .collect::<Vec<_>>();
    assert_eq!(foxes.len(), 2);
    let hat = fs::read_dir(resources.join("HATS"))
        .unwrap()
        .flatten()
        .find(|entry| entry.file_name() != "Cap")
        .unwrap();
    let name = hat.file_name().to_string_lossy().into_owned();
    assert_eq!(
        fs::read(hat.path().join(format!("t_{name}.png"))).unwrap(),
        b"old hat"
    );
    assert!(hat.path().join(format!("m_{name}.obj")).is_file());
    for folder in ["old-a", "old-b"] {
        assert!(!w.game.join(format!("BepInEx/plugins/{folder}")).exists());
    }
    assert!(!w.manifests.join("old-chef.toml").exists());
    let records = recycle_bin::list(&w.recycle).unwrap();
    assert_eq!(records.len(), 1);
    recycle_bin::restore(&w.recycle, &w.manifests, &w.game, &records[0].id).unwrap();
    assert!(w.game.join(&installed[1].dll_file).is_file());
    assert_eq!(
        fs::read(w.game.join("BepInEx/plugins/old-b/readme.txt")).unwrap(),
        b"kept in recycle"
    );
    assert!(w.manifests.join("old-chef.toml").is_file());
}

#[test]
fn failed_recycling_rolls_back_only_imported_files_and_keeps_old_directories() {
    let w = Workspace::new();
    let installed = [w.plugin("new", "2.2"), w.plugin("old", "1.0")];
    w.write("BepInEx/plugins/new/Resources/Fox/Body.obj", b"existing");
    w.write("BepInEx/plugins/old/Resources/Fox/Body.obj", b"old");
    fs::create_dir_all(w.recycle.parent().unwrap()).unwrap();
    fs::write(&w.recycle, b"blocks recycle directory").unwrap();
    assert!(w.execute(&installed).is_err());
    assert!(w.game.join(&installed[1].dll_file).is_file());
    assert_eq!(
        fs::read_dir(w.game.join("BepInEx/plugins/new/Resources"))
            .unwrap()
            .count(),
        1
    );
    assert_eq!(
        fs::read(w.game.join("BepInEx/plugins/new/Resources/Fox/Body.obj")).unwrap(),
        b"existing"
    );
}

#[test]
fn shared_plugin_directory_and_old_destination_are_rejected_before_copying() {
    let w = Workspace::new();
    let mut installed = vec![w.plugin("new", "2.2"), w.plugin("old", "1.0")];
    let mut unrelated = installed[1].clone();
    unrelated.guid = Some("unrelated".into());
    unrelated.dll_file = "BepInEx/plugins/old/Other.dll".into();
    w.write(&unrelated.dll_file, b"other");
    installed.push(unrelated);
    assert_eq!(
        w.execute(&installed).unwrap_err(),
        "diychef.migrationSharedDirectory"
    );
    installed[0].version = Some("1.9".into());
    assert_eq!(
        w.execute(&installed).unwrap_err(),
        "diychef.migrationUnavailable"
    );
    assert!(w.game.join(&installed[1].dll_file).is_file());
}

#[test]
fn active_peer_manifest_using_legacy_resources_rejects_migration_before_copy() {
    let w = Workspace::new();
    let modern = w.plugin("new", "2.2");
    let old = w.plugin("old", "1.9");
    let model = "BepInEx/plugins/old/Resources/Fox/Body.obj";
    w.write(model, b"old model");
    let peer = crate::archive::InstallManifest {
        repository_id: "unrelated-package".into(),
        release_tag: "v1".into(),
        repository_binding: None,
        package_name_i18n: Default::default(),
        note_i18n: Default::default(),
        group_i18n: Default::default(),
        prerelease: false,
        relations: vec![],
        installed_at: "2026-11-16T00:00:00Z".into(),
        files: vec![model.into()],
    };
    crate::archive::write_manifest(&w.manifests, &peer).unwrap();

    assert_eq!(
        w.execute(&[modern, old.clone()]).unwrap_err(),
        "recycle.fileSharedByOtherPackage"
    );
    assert!(w.game.join(&old.dll_file).is_file());
    assert!(w.game.join(model).is_file());
    assert!(!w.game.join("BepInEx/plugins/new/Resources").exists());
    assert!(!w.recycle.exists());
}

#[test]
fn registered_map_dll_hidden_from_mod_scan_blocks_migration_before_copy_or_recycle() {
    for suffix in ["", ".disabled"] {
        let w = Workspace::new();
        let modern = w.plugin("new", "2.2");
        let old = w.plugin("old", "1.0");
        w.write("BepInEx/plugins/old/Resources/Fox/Body.obj", b"old model");
        let map_dll = w.registered_map_plugin("old", suffix);
        let registry = w
            .manifests
            .join(crate::level_sets::LEVEL_SET_PLUGIN_REGISTRY);
        let registry_bytes = fs::read(&registry).unwrap();
        let scanned = mods::scan_mods_with_manifests(
            &w.game,
            &crate::config::default_config(),
            Some(&w.manifests),
        );
        assert_eq!(
            scanned.len(),
            2,
            "the map-owned DLL is hidden from the MOD scan"
        );
        assert!(scanned.iter().any(|item| item.dll_file == modern.dll_file));
        assert!(scanned.iter().any(|item| item.dll_file == old.dll_file));
        assert_eq!(
            w.execute(&scanned).unwrap_err(),
            "diychef.migrationSharedDirectory"
        );
        assert!(!w.game.join("BepInEx/plugins/new/Resources").exists());
        assert!(
            !w.recycle.exists(),
            "must reject before opening a recycle transaction"
        );
        assert!(w.game.join(&old.dll_file).is_file());
        assert!(map_dll.is_file());
        assert_eq!(fs::read(&registry).unwrap(), registry_bytes);
        assert_eq!(
            fs::read(w.game.join("BepInEx/plugins/old/Resources/Fox/Body.obj")).unwrap(),
            b"old model"
        );
        assert_eq!(
            fs::read(
                w.game
                    .join("BepInEx/plugins/MapLoader/levels/OwnedMap/info")
            )
            .unwrap(),
            b"installed map info"
        );
    }
}

#[test]
fn unrelated_registered_map_plugin_keeps_migration_and_recycle_roundtrip_working() {
    let w = Workspace::new();
    let installed = [w.plugin("new", "2.2"), w.plugin("old", "1.0")];
    w.write("BepInEx/plugins/old/Resources/Fox/Body.obj", b"old model");
    let map_dll = w.registered_map_plugin("old-sibling", "");
    let registry = w
        .manifests
        .join(crate::level_sets::LEVEL_SET_PLUGIN_REGISTRY);
    let registry_bytes = fs::read(&registry).unwrap();
    w.execute(&installed).unwrap();
    assert_eq!(
        fs::read(w.game.join("BepInEx/plugins/new/Resources/Fox/Body.obj")).unwrap(),
        b"old model"
    );
    assert!(!w.game.join("BepInEx/plugins/old").exists());
    assert!(map_dll.is_file());
    assert_eq!(fs::read(&registry).unwrap(), registry_bytes);
    let entries = recycle_bin::list(&w.recycle).unwrap();
    assert_eq!(entries.len(), 1);
    recycle_bin::restore(&w.recycle, &w.manifests, &w.game, &entries[0].id).unwrap();
    assert!(w.game.join(&installed[1].dll_file).is_file());
    assert!(map_dll.is_file());
    assert_eq!(fs::read(&registry).unwrap(), registry_bytes);
}

#[test]
fn command_rejects_stale_scan_without_copying() {
    let w = Workspace::new();
    let installed = [w.plugin("new", "2.2"), w.plugin("old", "1.0")];
    let state = AppState::with_config(crate::config::ConfigService::for_test(
        w.root.join("manager"),
    ));
    let mut config = state.config.load().unwrap();
    config.game.path = w.game.to_string_lossy().into();
    state.config.save(&config).unwrap();
    assert_eq!(
        migrate(&state, 42, &installed[0].dll_file).unwrap_err(),
        "diychef.staleScan"
    );
    assert!(w.game.join(&installed[1].dll_file).is_file());
}

#[path = "orphan_repro_tests.rs"]
mod orphan_repro_tests;

#[path = "recovery_tests.rs"]
mod recovery_tests;
