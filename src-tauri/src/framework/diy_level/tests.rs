use super::*;
use crate::framework::{backup_framework, read_backup_manifest, restore_framework_backup};
use std::path::PathBuf;

struct Fixture {
    root: PathBuf,
    game: PathBuf,
    backups: PathBuf,
    manifests: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("oc2-diy-dynamic-{}", uuid::Uuid::new_v4()));
        let game = root.join("game");
        fs::create_dir_all(game.join("BepInEx/plugins")).unwrap();
        Self {
            backups: root.join("backups"),
            manifests: root.join("manifests"),
            game,
            root,
        }
    }
    fn package(&self, name: &str) -> PathBuf {
        let path = self.game.join("BepInEx/plugins").join(name);
        fs::create_dir_all(path.join("levels/Pack")).unwrap();
        fs::write(path.join("DIYLevel.dll.disabled"), b"highest disabled").unwrap();
        fs::write(path.join("levels/Pack/info"), b"map").unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn tree_bytes(root: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .map(Result::unwrap)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| {
            (
                entry.path().strip_prefix(root).unwrap().to_path_buf(),
                fs::read(entry.path()).unwrap(),
            )
        })
        .collect()
}

#[test]
fn dynamic_backup_and_restore_follow_the_recorded_directory_and_preserve_siblings() {
    let fixture = Fixture::new();
    let package = fixture.package("DIYLevel-FastInit-v4");
    let sibling = fixture.package("UnrelatedMod");
    let spec = diy_level_backup_spec(&fixture.game, &package).unwrap();
    let backup = backup_framework(
        &fixture.game,
        &fixture.backups,
        &spec,
        &fixture.manifests,
        "manual",
    )
    .unwrap();
    assert!(
        backup
            .join("BepInEx/plugins/DIYLevel-FastInit-v4/DIYLevel.dll.disabled")
            .is_file()
    );
    assert!(!backup.join("BepInEx/plugins/UnrelatedMod").exists());
    fs::write(package.join("after-backup.txt"), b"later").unwrap();
    let restore_spec = diy_level_restore_spec(&fixture.game, &backup, Some(&package)).unwrap();
    restore_framework_backup(
        &fixture.game,
        &backup,
        &restore_spec,
        &fixture.manifests,
        "manual",
    )
    .unwrap();
    assert!(!package.join("after-backup.txt").exists());
    assert_eq!(
        fs::read(package.join("DIYLevel.dll.disabled")).unwrap(),
        b"highest disabled"
    );
    assert!(sibling.join("DIYLevel.dll.disabled").is_file());
    let manifest = archive::read_manifest(&fixture.manifests, "manual").unwrap();
    assert!(
        manifest
            .files
            .iter()
            .all(|file| file.starts_with("BepInEx/plugins/DIYLevel-FastInit-v4/"))
    );
}

#[test]
fn restoring_a_backup_for_a_different_current_root_is_rejected_before_removal() {
    let fixture = Fixture::new();
    let old = fixture.package("OldVersion");
    let current = fixture.package("NewestVersion");
    let backup = backup_framework(
        &fixture.game,
        &fixture.backups,
        &diy_level_backup_spec(&fixture.game, &old).unwrap(),
        &fixture.manifests,
        "old",
    )
    .unwrap();
    assert!(
        matches!(diy_level_restore_spec(&fixture.game, &backup, Some(&current)), Err(error) if error == "backup.diyLevelTargetMismatch")
    );
    assert_eq!(
        fs::read(current.join("DIYLevel.dll.disabled")).unwrap(),
        b"highest disabled"
    );
    // After uninstall there is no selected current root; restore to the original path.
    fs::remove_dir_all(&old).unwrap();
    let spec = diy_level_restore_spec(&fixture.game, &backup, None).unwrap();
    restore_framework_backup(&fixture.game, &backup, &spec, &fixture.manifests, "old").unwrap();
    assert!(old.join("DIYLevel.dll.disabled").is_file());
    assert!(current.join("DIYLevel.dll.disabled").is_file());
}

#[test]
fn restoring_without_a_current_target_preserves_a_reused_nonempty_directory() {
    let fixture = Fixture::new();
    let package = fixture.package("ReusedDirectory");
    let backup = backup_framework(
        &fixture.game,
        &fixture.backups,
        &diy_level_backup_spec(&fixture.game, &package).unwrap(),
        &fixture.manifests,
        "old-diylevel",
    )
    .unwrap();
    let before_backup = tree_bytes(&backup);
    fs::remove_dir_all(&package).unwrap();
    fs::create_dir(&package).unwrap();
    fs::write(package.join("OtherMod.dll"), b"unrelated plugin bytes").unwrap();

    assert!(
        matches!(diy_level_restore_spec(&fixture.game, &backup, None), Err(error) if error == "backup.diyLevelTargetMismatch")
    );
    assert_eq!(
        fs::read(package.join("OtherMod.dll")).unwrap(),
        b"unrelated plugin bytes"
    );
    assert!(!package.join("DIYLevel.dll.disabled").exists());
    assert_eq!(tree_bytes(&backup), before_backup);
}

#[test]
fn restoring_without_a_current_target_accepts_an_empty_directory() {
    let fixture = Fixture::new();
    let package = fixture.package("EmptyDirectory");
    let backup = backup_framework(
        &fixture.game,
        &fixture.backups,
        &diy_level_backup_spec(&fixture.game, &package).unwrap(),
        &fixture.manifests,
        "empty-target",
    )
    .unwrap();
    let before_backup = tree_bytes(&backup);
    fs::remove_dir_all(&package).unwrap();
    fs::create_dir(&package).unwrap();

    let spec = diy_level_restore_spec(&fixture.game, &backup, None).unwrap();
    restore_framework_backup(
        &fixture.game,
        &backup,
        &spec,
        &fixture.manifests,
        "empty-target",
    )
    .unwrap();
    assert_eq!(
        fs::read(package.join("DIYLevel.dll.disabled")).unwrap(),
        b"highest disabled"
    );
    assert_eq!(fs::read(package.join("levels/Pack/info")).unwrap(), b"map");
    assert_eq!(tree_bytes(&backup), before_backup);
}

#[test]
fn restoring_without_a_current_target_does_not_treat_a_directory_read_error_as_empty() {
    let fixture = Fixture::new();
    let package = fixture.package("UnreadableDirectory");
    let backup = backup_framework(
        &fixture.game,
        &fixture.backups,
        &diy_level_backup_spec(&fixture.game, &package).unwrap(),
        &fixture.manifests,
        "file-target",
    )
    .unwrap();
    let before_backup = tree_bytes(&backup);
    fs::remove_dir_all(&package).unwrap();
    fs::write(&package, b"existing file").unwrap();
    assert!(fs::read_dir(&package).is_err());

    assert!(diy_level_restore_spec(&fixture.game, &backup, None).is_err());
    assert_eq!(fs::read(&package).unwrap(), b"existing file");
    assert_eq!(tree_bytes(&backup), before_backup);
}

#[test]
fn legacy_backups_restore_only_the_directory_they_contain() {
    let fixture = Fixture::new();
    let legacy = fixture.package("OC2DIYLevel");
    let backup = backup_framework(
        &fixture.game,
        &fixture.backups,
        &diy_level_backup_spec(&fixture.game, &legacy).unwrap(),
        &fixture.manifests,
        "legacy",
    )
    .unwrap();
    fs::remove_file(backup.join(LAYOUT_FILE)).unwrap();
    let spec = diy_level_restore_spec(&fixture.game, &backup, Some(&legacy)).unwrap();
    restore_framework_backup(&fixture.game, &backup, &spec, &fixture.manifests, "legacy").unwrap();
    assert!(legacy.join("DIYLevel.dll.disabled").is_file());
    assert!(
        diy_level_restore_spec(&fixture.game, &backup, Some(&fixture.package("Elsewhere")))
            .is_err()
    );
}

#[test]
fn malformed_or_escaping_layouts_and_shared_plugins_root_are_rejected() {
    let fixture = Fixture::new();
    let package = fixture.package("Selected");
    assert!(diy_level_backup_spec(&fixture.game, &fixture.game.join("BepInEx/plugins")).is_err());
    assert!(diy_level_backup_spec(&fixture.game, &fixture.game.join("BepInEx/core")).is_err());
    let backup = backup_framework(
        &fixture.game,
        &fixture.backups,
        &diy_level_backup_spec(&fixture.game, &package).unwrap(),
        &fixture.manifests,
        "test",
    )
    .unwrap();
    for directory in [
        "../outside",
        "BepInEx/plugins",
        "BepInEx/core",
        "C:/outside",
    ] {
        fs::write(
            backup.join(LAYOUT_FILE),
            toml::to_string(&DiyLevelLayout {
                version: 1,
                directory: directory.into(),
            })
            .unwrap(),
        )
        .unwrap();
        assert!(
            diy_level_restore_spec(&fixture.game, &backup, None).is_err(),
            "{directory}"
        );
    }
    fs::write(backup.join(LAYOUT_FILE), b"not a layout").unwrap();
    assert!(diy_level_restore_spec(&fixture.game, &backup, None).is_err());
    assert!(package.join("DIYLevel.dll.disabled").is_file());
}

#[test]
fn dynamic_backup_manifest_cannot_claim_sibling_plugin_files() {
    let fixture = Fixture::new();
    let package = fixture.package("Selected");
    let mut manifest = archive::new_manifest(
        "wide-package",
        "v4",
        archive::single_language("DIYLevel"),
        Default::default(),
        vec![
            "BepInEx/plugins/Selected/DIYLevel.dll".into(),
            "BepInEx/plugins/Other/plugin.dll".into(),
            "BepInEx/plugins/Selected/../../core/BepInEx.dll".into(),
        ],
        vec![],
    );
    archive::write_manifest(&fixture.manifests, &manifest).unwrap();
    let backup = backup_framework(
        &fixture.game,
        &fixture.backups,
        &diy_level_backup_spec(&fixture.game, &package).unwrap(),
        &fixture.manifests,
        "wide-package",
    )
    .unwrap();
    manifest = read_backup_manifest(&backup).unwrap();
    assert_eq!(manifest.files, ["BepInEx/plugins/Selected/DIYLevel.dll"]);
}

#[test]
fn different_manifest_ownership_blocks_restore_before_any_write() {
    let mut registered_paths = vec![
        "BepInEx/plugins/Selected/DIYLevel.dll.disabled".to_string(),
        "BepInEx\\plugins\\Selected\\DIYLevel.dll.disabled".to_string(),
    ];
    if cfg!(windows) {
        registered_paths.push("BEPINEX\\PLUGINS\\SELECTED\\DIYLEVEL.DLL.DISABLED".into());
    }
    for registered in registered_paths {
        let fixture = Fixture::new();
        let package = fixture.package("Selected");
        let spec = diy_level_backup_spec(&fixture.game, &package).unwrap();
        let backup = backup_framework(
            &fixture.game,
            &fixture.backups,
            &spec,
            &fixture.manifests,
            "backup-owner",
        )
        .unwrap();
        fs::write(package.join("DIYLevel.dll.disabled"), b"current DLL bytes").unwrap();
        fs::write(package.join("OtherMod.dll"), b"other plugin bytes").unwrap();
        let other = archive::new_manifest(
            "other-owner",
            "v5",
            archive::single_language("Other package"),
            Default::default(),
            vec![registered.clone()],
            vec![],
        );
        archive::write_manifest(&fixture.manifests, &other).unwrap();
        let before_game = tree_bytes(&fixture.game);
        let before_manifests = tree_bytes(&fixture.manifests);
        let before_backup = tree_bytes(&backup);
        let restore_spec = diy_level_restore_spec(&fixture.game, &backup, Some(&package)).unwrap();

        assert!(
            matches!(restore_framework_backup(&fixture.game, &backup, &restore_spec, &fixture.manifests, "backup-owner"), Err(error) if error == "backup.diyLevelTargetMismatch"),
            "another manifest owns {registered}"
        );
        assert_eq!(tree_bytes(&fixture.game), before_game);
        assert_eq!(tree_bytes(&fixture.manifests), before_manifests);
        assert_eq!(tree_bytes(&backup), before_backup);
    }
}

#[test]
fn same_manifest_with_external_siblings_still_blocks_restore() {
    let fixture = Fixture::new();
    let package = fixture.package("Selected");
    fixture.package("Sibling");
    let manifest = archive::new_manifest(
        "wide-package",
        "v4",
        archive::single_language("DIYLevel"),
        Default::default(),
        vec![
            "BepInEx/plugins/Selected/DIYLevel.dll.disabled".into(),
            "BepInEx/plugins/Sibling/DIYLevel.dll.disabled".into(),
        ],
        vec![],
    );
    archive::write_manifest(&fixture.manifests, &manifest).unwrap();
    let spec = diy_level_backup_spec(&fixture.game, &package).unwrap();
    let backup = backup_framework(
        &fixture.game,
        &fixture.backups,
        &spec,
        &fixture.manifests,
        "wide-package",
    )
    .unwrap();
    let before_game = tree_bytes(&fixture.game);
    let before_manifests = tree_bytes(&fixture.manifests);
    let before_backup = tree_bytes(&backup);
    assert!(
        matches!(restore_framework_backup(&fixture.game, &backup, &spec, &fixture.manifests, "wide-package"), Err(error) if error == "backup.diyLevelTargetMismatch")
    );
    assert_eq!(tree_bytes(&fixture.game), before_game);
    assert_eq!(tree_bytes(&fixture.manifests), before_manifests);
    assert_eq!(tree_bytes(&backup), before_backup);
}

#[test]
fn another_manifest_with_a_similarly_named_sibling_does_not_block_restore() {
    let fixture = Fixture::new();
    let package = fixture.package("Selected");
    let sibling = fixture.package("SelectedSibling");
    let other = archive::new_manifest(
        "other-owner",
        "v5",
        archive::single_language("Other package"),
        Default::default(),
        vec!["BepInEx/plugins/SelectedSibling/DIYLevel.dll.disabled".into()],
        vec![],
    );
    archive::write_manifest(&fixture.manifests, &other).unwrap();
    let spec = diy_level_backup_spec(&fixture.game, &package).unwrap();
    let backup = backup_framework(
        &fixture.game,
        &fixture.backups,
        &spec,
        &fixture.manifests,
        "backup-owner",
    )
    .unwrap();
    let other_path = fixture.manifests.join("other-owner.toml");
    let before_manifest = fs::read(&other_path).unwrap();
    let before_sibling = tree_bytes(&sibling);
    let before_backup = tree_bytes(&backup);
    fs::write(package.join("DIYLevel.dll.disabled"), b"current DLL bytes").unwrap();

    restore_framework_backup(
        &fixture.game,
        &backup,
        &spec,
        &fixture.manifests,
        "backup-owner",
    )
    .unwrap();
    assert_eq!(
        fs::read(package.join("DIYLevel.dll.disabled")).unwrap(),
        b"highest disabled"
    );
    assert_eq!(fs::read(&other_path).unwrap(), before_manifest);
    assert_eq!(tree_bytes(&sibling), before_sibling);
    assert_eq!(tree_bytes(&backup), before_backup);
}
