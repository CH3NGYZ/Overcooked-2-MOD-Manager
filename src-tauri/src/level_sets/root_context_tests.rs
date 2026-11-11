//! 直接调用地图领域验证选中的 DLL 根参与扫描、移动与 ZIP 落盘。
use super::paths::{LevelSetRoots, disabled_root, levels_root, migrate_legacy_disabled_root};
use super::plugin_registry::read_registered_plugins;
use super::zip_inspect::ImportedLevelSet;
use crate::models::{LevelSetLayout, ZipLevelSetPlugin};
use std::io::Write;
use std::path::{Path, PathBuf};

struct Workspace(PathBuf);

impl Workspace {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("oc2-map-roots-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("game")).unwrap();
        Self(root)
    }

    fn roots(&self) -> LevelSetRoots {
        let game = self.0.join("game");
        LevelSetRoots::new(
            game.clone(),
            Some(game.join("BepInEx/plugins/LevelFast-v3")),
        )
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn zip(path: &Path, entries: &[(&str, &[u8])]) {
    let mut writer = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (name, bytes) in entries {
        writer
            .start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap();
}

fn set(name: &str, layout: LevelSetLayout) -> ImportedLevelSet {
    ImportedLevelSet {
        dir_name: name.into(),
        layout,
        metadata: crate::asset_bundle::LevelSetMetadata {
            name: name.into(),
            author: "map-root-test".into(),
            ..Default::default()
        },
        files: Vec::new(),
        plugins: Vec::new(),
    }
}

#[test]
fn scan_and_toggle_use_selected_package_and_preserve_other_versions() {
    let ws = Workspace::new();
    let roots = ws.roots();
    let selected = roots.package_root().unwrap().join("levels/Selected/info");
    let old = roots
        .game_root()
        .join("BepInEx/plugins/OC2DIYLevel/levels/Old/info");
    write(&selected, b"selected");
    write(&old, b"old");
    write(
        &disabled_root(roots.game_root()).join("Off/info"),
        b"disabled",
    );
    let scanned = super::scan::scan_level_sets(&roots);
    assert_eq!(
        scanned
            .iter()
            .map(|s| (s.dir_name.as_str(), s.enabled))
            .collect::<Vec<_>>(),
        [("Selected", true), ("Off", false)]
    );
    let moved = super::set_level_set_enabled(&roots, "Selected", false).unwrap();
    assert_eq!(std::fs::read(moved.join("info")).unwrap(), b"selected");
    assert!(!selected.exists());
    assert!(old.is_file());
    let enabled = super::set_level_set_enabled(&roots, "Selected", true).unwrap();
    assert_eq!(enabled, selected.parent().unwrap().canonicalize().unwrap());
    assert_eq!(
        super::locate_level_set_directory(&roots, "Selected", true).unwrap(),
        enabled
    );
}

#[test]
fn missing_plugin_only_scans_disabled_and_rejects_writes_without_creating_default_root() {
    let ws = Workspace::new();
    let roots = LevelSetRoots::new(ws.0.join("game"), None);
    let old = roots.game_root().join("BepInEx/plugins/OC2DIYLevel");
    write(
        &disabled_root(roots.game_root()).join("Off/info"),
        b"disabled",
    );
    assert_eq!(super::scan::scan_level_sets(&roots).len(), 1);
    assert_eq!(
        super::set_level_set_enabled(&roots, "Off", true).unwrap_err(),
        "framework.diyLevelMissing"
    );
    let report =
        super::flatten_level_sets(&roots, &ws.0.join("recycle"), &ws.0.join("manifests"), true)
            .unwrap();
    assert!(report.candidates.is_empty());
    assert_eq!(
        super::flatten_level_sets(
            &roots,
            &ws.0.join("recycle"),
            &ws.0.join("manifests"),
            false
        )
        .unwrap_err(),
        "framework.diyLevelMissing"
    );
    let map = set("One", LevelSetLayout::Flattened);
    assert_eq!(
        super::install_parsed_sets(
            &[map],
            &ws.0.join("unused.zip"),
            &roots,
            &ws.0.join("manifests"),
            &["One".into()],
            None
        )
        .unwrap_err(),
        "framework.diyLevelMissing"
    );
    assert!(!old.exists());
    assert!(disabled_root(roots.game_root()).join("Off/info").is_file());
}

#[test]
fn migration_moves_selected_package_disabled_without_using_it_as_enabled_root() {
    let ws = Workspace::new();
    let roots = ws.roots();
    let legacy = roots.package_root().unwrap().join("disabled/Previous/info");
    write(&legacy, b"previous");
    migrate_legacy_disabled_root(&roots);
    assert!(!legacy.exists());
    assert_eq!(
        std::fs::read(disabled_root(roots.game_root()).join("Previous/info")).unwrap(),
        b"previous"
    );
    assert!(!levels_root(&roots).unwrap().exists());
}

#[test]
fn flattened_zip_installs_only_into_selected_package() {
    let ws = Workspace::new();
    let roots = ws.roots();
    let archive = ws.0.join("flat.zip");
    zip(
        &archive,
        &[("source/info_One", b"info"), ("source/scene", b"scene")],
    );
    let mut map = set("One", LevelSetLayout::Flattened);
    map.files = vec![
        ("source/info_One".into(), "info_One".into()),
        ("source/scene".into(), "scene".into()),
    ];
    let installed = super::install_parsed_sets(
        &[map],
        &archive,
        &roots,
        &ws.0.join("manifests"),
        &["One".into()],
        None,
    )
    .unwrap();
    assert_eq!(installed, ["One"]);
    let target = levels_root(&roots).unwrap().join("One");
    assert_eq!(std::fs::read(target.join("info")).unwrap(), b"info");
    assert_eq!(std::fs::read(target.join("scene")).unwrap(), b"scene");
    assert!(
        !roots
            .game_root()
            .join("BepInEx/plugins/OC2DIYLevel")
            .exists()
    );
}

#[test]
fn layer_zip_redirects_maps_preserves_plugin_paths_and_filters_unselected_sets() {
    let ws = Workspace::new();
    let roots = ws.roots();
    let archive = ws.0.join("layer.zip");
    zip(
        &archive,
        &[
            ("BepInEx/plugins/OC2DIYLevel/levels/One/info", b"one"),
            ("BepInEx/plugins/OC2DIYLevel/levels/Two/info", b"two"),
            ("BepInEx/plugins/Fix/Fix.dll", b"fix"),
            ("BepInEx/plugins/Other/Other.dll", b"other"),
        ],
    );
    let mut one = set("One", LevelSetLayout::Bepinex);
    let mut two = set("Two", LevelSetLayout::Bepinex);
    for (map, name, path) in [
        (&mut one, "Fix.dll", "BepInEx/plugins/Fix/Fix.dll"),
        (&mut two, "Other.dll", "BepInEx/plugins/Other/Other.dll"),
    ] {
        map.plugins.push(ZipLevelSetPlugin {
            path: path.into(),
            install_path: Some(path.into()),
            file_name: name.into(),
            info: super::zip_plugins::fallback_plugin_info(name),
        });
    }
    let manifests = ws.0.join("manifests");
    super::install_parsed_sets(
        &[one, two],
        &archive,
        &roots,
        &manifests,
        &["One".into()],
        None,
    )
    .unwrap();
    assert_eq!(
        std::fs::read(levels_root(&roots).unwrap().join("One/info")).unwrap(),
        b"one"
    );
    assert!(!levels_root(&roots).unwrap().join("Two").exists());
    assert!(
        !roots
            .game_root()
            .join("BepInEx/plugins/OC2DIYLevel")
            .exists()
    );
    assert!(
        roots
            .game_root()
            .join("BepInEx/plugins/Fix/Fix.dll")
            .is_file()
    );
    assert!(!roots.game_root().join("BepInEx/plugins/Other").exists());
    let registered = read_registered_plugins(&manifests);
    assert_eq!(registered.len(), 1);
    assert_eq!(registered[0].dll, "BepInEx/plugins/Fix/Fix.dll");
}

#[test]
fn layer_zip_remapping_collision_rolls_back_created_maps_and_overwritten_files() {
    let ws = Workspace::new();
    let roots = ws.roots();
    let config = roots.game_root().join("BepInEx/config/existing.cfg");
    write(&config, b"original");
    let archive = ws.0.join("collision.zip");
    zip(
        &archive,
        &[
            ("BepInEx/config/existing.cfg", b"replacement"),
            ("BepInEx/plugins/OC2DIYLevel/levels/One/info", b"mapped"),
            ("BepInEx/plugins/LevelFast-v3/levels/One/info", b"collision"),
        ],
    );
    let result = super::install_parsed_sets(
        &[set("One", LevelSetLayout::Bepinex)],
        &archive,
        &roots,
        &ws.0.join("manifests"),
        &["One".into()],
        None,
    );
    assert_eq!(result.unwrap_err(), "archive.pathEscape");
    assert_eq!(std::fs::read(config).unwrap(), b"original");
    assert!(!levels_root(&roots).unwrap().join("One").exists());
    assert!(
        !roots
            .game_root()
            .join("BepInEx/plugins/OC2DIYLevel")
            .exists()
    );
    assert!(read_registered_plugins(&ws.0.join("manifests")).is_empty());
}
