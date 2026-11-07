use super::*;
use crate::archive::test_support::write_zip;
use std::io::Write;
use uuid::Uuid;

#[test]
fn copying_an_installed_dll_onto_itself_keeps_its_contents() {
    let root = std::env::temp_dir().join(format!("oc2-same-asset-{}", Uuid::new_v4()));
    let plugins = root.join("BepInEx/plugins");
    fs::create_dir_all(&plugins).unwrap();
    let source = plugins.join("Existing.dll");
    fs::write(&source, b"original plugin").unwrap();
    assert!(copy_asset_pending(&source, &root, "BepInEx/plugins", "Existing.dll").is_err());
    assert_eq!(fs::read(&source).unwrap(), b"original plugin");
    fs::remove_file(source).unwrap();
    fs::remove_dir(plugins).unwrap();
    fs::remove_dir(root.join("BepInEx")).unwrap();
    fs::remove_dir(root).unwrap();
}

#[test]
fn copying_a_hard_link_alias_never_changes_the_source() {
    let root = std::env::temp_dir().join(format!("oc2-linked-asset-{}", Uuid::new_v4()));
    let plugins = root.join("BepInEx/plugins");
    fs::create_dir_all(&plugins).unwrap();
    let source = root.join("alias.dll");
    let destination = plugins.join("Existing.dll");
    fs::write(&source, b"original plugin").unwrap();
    fs::hard_link(&source, &destination).unwrap();
    let permissions = fs::metadata(&source).unwrap().permissions();
    let mut readonly = permissions.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&source, readonly).unwrap();

    assert_eq!(
        copy_asset_pending(&source, &root, "BepInEx/plugins", "Existing.dll")
            .err()
            .unwrap(),
        "archive.sourceEqualsTarget"
    );
    assert!(fs::metadata(&source).unwrap().permissions().readonly());
    assert_eq!(fs::read(&source).unwrap(), b"original plugin");
    assert_eq!(fs::read(&destination).unwrap(), b"original plugin");

    fs::set_permissions(&source, permissions.clone()).unwrap();
    fs::set_permissions(&destination, permissions).unwrap();
    fs::remove_file(source).unwrap();
    fs::remove_file(destination).unwrap();
    fs::remove_dir(plugins).unwrap();
    fs::remove_dir(root.join("BepInEx")).unwrap();
    fs::remove_dir(root).unwrap();
}

#[test]
fn zip_slip_is_rejected() {
    let zip_path = std::env::temp_dir().join("oc2-slip-test.zip");
    let root = std::env::temp_dir().join("oc2-slip-root");
    fs::create_dir_all(&root).unwrap();
    let file = File::create(&zip_path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    writer
        .start_file("../outside.txt", zip::write::SimpleFileOptions::default())
        .unwrap();
    writer.write_all(b"bad").unwrap();
    writer.finish().unwrap();
    assert!(extract_framework_zip_pending(&zip_path, &root).is_err());
}

#[test]
fn online_zip_rejects_unsafe_paths_before_writing_preserved_directories() {
    let root = std::env::temp_dir().join(format!("oc2-online-unsafe-zip-{}", Uuid::new_v4()));
    let game = root.join("game");
    fs::create_dir_all(&game).unwrap();
    let zip = root.join("unsafe.zip");
    write_zip(
        &zip,
        &[
            ("OC2DIYLevel/Plugin.dll", b"plugin"),
            ("../outside.dll", b"unsafe"),
        ],
    );

    assert!(extract_mod_zip(&zip, &game, "BepInEx/plugins", "unsafe.zip").is_err());
    assert!(!game.join("BepInEx/plugins/OC2DIYLevel/Plugin.dll").exists());
    assert!(!root.join("outside.dll").exists());
    fs::remove_dir_all(root).unwrap();
}
