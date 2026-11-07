use super::*;
use crate::archive::test_support::write_zip;
use crate::archive::{new_manifest, write_manifest};
use uuid::Uuid;

#[test]
fn rollback_removes_only_directories_created_by_the_installation() {
    let verification =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("target/verification/archive-rollback");
    for existing_framework in [false, true] {
        let root = verification.join(Uuid::new_v4().to_string());
        let game = root.join("game");
        fs::create_dir_all(game.join("Existing")).unwrap();
        if existing_framework {
            fs::create_dir_all(game.join("BepInEx/plugins")).unwrap();
        }
        let archive = root.join("framework.zip");
        write_zip(
            &archive,
            &[
                ("BepInEx/plugins/Plugin.dll", b"new plugin"),
                ("doorstop_libs/libdoorstop.so", b"new loader"),
                ("Existing/nested/file.dll", b"new file"),
            ],
        );
        let mut pending = extract_framework_zip_pending(&archive, &game).unwrap();
        pending.rollback().unwrap();
        assert_eq!(game.join("BepInEx").exists(), existing_framework);
        assert_eq!(game.join("BepInEx/plugins").exists(), existing_framework);
        assert!(game.join("Existing").is_dir());
        assert!(!game.join("Existing/nested").exists());
        assert!(!game.join("doorstop_libs").exists());
        assert!(
            fs::canonicalize(&root)
                .unwrap()
                .starts_with(fs::canonicalize(&verification).unwrap())
        );
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn rollback_keeps_unrelated_files_added_to_new_directories() {
    let verification =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("target/verification/archive-rollback");
    let root = verification.join(Uuid::new_v4().to_string());
    let game = root.join("game");
    fs::create_dir_all(&game).unwrap();
    let archive = root.join("framework.zip");
    write_zip(&archive, &[("BepInEx/plugins/Plugin.dll", b"new plugin")]);
    let mut pending = extract_framework_zip_pending(&archive, &game).unwrap();
    fs::write(game.join("BepInEx/plugins/user.cfg"), b"keep user file").unwrap();
    pending.rollback().unwrap();
    assert!(!game.join("BepInEx/plugins/Plugin.dll").exists());
    assert_eq!(
        fs::read(game.join("BepInEx/plugins/user.cfg")).unwrap(),
        b"keep user file"
    );
    assert!(
        fs::canonicalize(&root)
            .unwrap()
            .starts_with(fs::canonicalize(&verification).unwrap())
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn failed_manifest_commit_restores_overwritten_dll_and_zip_files() {
    for zip in [false, true] {
        let root = std::env::temp_dir().join(format!("oc2-manifest-rollback-{}", Uuid::new_v4()));
        let game = root.join("game");
        let plugins = game.join("BepInEx/plugins");
        fs::create_dir_all(&plugins).unwrap();
        fs::write(plugins.join("Existing.dll"), b"old DLL").unwrap();
        let asset = root.join(if zip { "update.zip" } else { "Existing.dll" });
        let pending = if zip {
            write_zip(
                &asset,
                &[("Existing.dll", b"new DLL"), ("New.dll", b"new member")],
            );
            extract_local_mod_zip_pending(&asset, &game, "update.zip").unwrap()
        } else {
            fs::write(&asset, b"new DLL").unwrap();
            copy_asset_pending(&asset, &game, "BepInEx/plugins", "Existing.dll").unwrap()
        };
        assert_eq!(fs::read(plugins.join("Existing.dll")).unwrap(), b"new DLL");
        let manifest = new_manifest(
            "package",
            "v2",
            Default::default(),
            Default::default(),
            pending.files().to_vec(),
            Vec::new(),
        );
        let blocked = root.join("blocked-manifests");
        fs::write(&blocked, b"not a directory").unwrap();
        assert!(write_manifest(&blocked, &manifest).is_err());
        drop(pending);
        assert_eq!(fs::read(plugins.join("Existing.dll")).unwrap(), b"old DLL");
        assert!(!plugins.join("New.dll").exists());
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn online_zip_rolls_back_written_files_when_preserved_directory_install_fails() {
    let root = std::env::temp_dir().join(format!("oc2-online-rollback-zip-{}", Uuid::new_v4()));
    let game = root.join("game");
    let blocked = game.join("BepInEx/plugins/OC2DIYLevel/Blocked.dll");
    fs::create_dir_all(&blocked).unwrap();
    fs::write(blocked.join("existing.txt"), b"existing").unwrap();
    let replaced = game.join("BepInEx/plugins/OC2DIYLevel/Plugin.dll");
    fs::write(&replaced, b"old plugin").unwrap();
    let zip = root.join("rollback.zip");
    write_zip(
        &zip,
        &[
            ("OC2DIYLevel/Plugin.dll", b"plugin"),
            ("OC2DIYLevel/Blocked.dll", b"blocked"),
        ],
    );

    assert!(extract_mod_zip(&zip, &game, "BepInEx/plugins", "rollback.zip").is_err());
    assert_eq!(fs::read(replaced).unwrap(), b"old plugin");
    assert_eq!(fs::read(blocked.join("existing.txt")).unwrap(), b"existing");
    fs::remove_dir_all(root).unwrap();
}
