use super::*;
use crate::archive::test_support::write_zip;
use uuid::Uuid;

#[test]
fn local_zip_with_only_root_dlls_merges_into_plugins() {
    let root = std::env::temp_dir().join(format!("oc2-two-dll-zip-{}", Uuid::new_v4()));
    let game = root.join("game");
    let plugins = game.join("BepInEx/plugins");
    fs::create_dir_all(plugins.join("Existing MOD")).unwrap();
    fs::write(plugins.join("First.dll"), b"old DLL").unwrap();
    fs::write(plugins.join("Other.dll"), b"other DLL").unwrap();
    fs::write(plugins.join("Existing MOD/config.json"), b"existing config").unwrap();
    let zip_path = root.join("Two MODs.zip");
    write_zip(
        &zip_path,
        &[("First.dll", b"first DLL"), ("Second.DLL", b"second DLL")],
    );

    let mut installed = extract_local_mod_zip(&zip_path, &game, "Two MODs.zip").unwrap();
    installed.sort();
    assert_eq!(
        installed,
        vec![
            "BepInEx/plugins/First.dll".to_string(),
            "BepInEx/plugins/Second.DLL".to_string()
        ]
    );
    assert!(!plugins.join("Two MODs").exists());
    assert_eq!(fs::read(plugins.join("First.dll")).unwrap(), b"first DLL");
    assert_eq!(fs::read(plugins.join("Second.DLL")).unwrap(), b"second DLL");
    assert_eq!(fs::read(plugins.join("Other.dll")).unwrap(), b"other DLL");
    assert_eq!(
        fs::read(plugins.join("Existing MOD/config.json")).unwrap(),
        b"existing config"
    );

    let canonical_root = fs::canonicalize(&root).unwrap();
    let temp = fs::canonicalize(std::env::temp_dir()).unwrap();
    assert!(canonical_root.starts_with(&temp) && canonical_root != temp);
    fs::remove_dir_all(canonical_root).unwrap();
}

#[test]
fn mod_zip_layouts_use_one_shared_install_policy() {
    let root = std::env::temp_dir().join(format!("oc2-zip-layout-{}", Uuid::new_v4()));
    let game = root.join("game");
    fs::create_dir_all(&game).unwrap();

    let folder_zip = root.join("folder.zip");
    write_zip(&folder_zip, &[("Package/Plugin.dll", b"plugin")]);
    fs::create_dir_all(game.join("BepInEx/plugins/Package")).unwrap();
    fs::write(game.join("BepInEx/plugins/Package/Plugin.dll"), b"old").unwrap();
    fs::write(game.join("BepInEx/plugins/Package/other.txt"), b"other").unwrap();
    extract_local_mod_zip(&folder_zip, &game, "folder.zip").unwrap();
    assert!(game.join("BepInEx/plugins/Package/Plugin.dll").is_file());
    assert!(!game.join("BepInEx/plugins/folder").exists());
    assert_eq!(
        fs::read(game.join("BepInEx/plugins/Package/Plugin.dll")).unwrap(),
        b"plugin"
    );
    assert_eq!(
        fs::read(game.join("BepInEx/plugins/Package/other.txt")).unwrap(),
        b"other"
    );

    let dll_zip = root.join("single.dll.zip");
    write_zip(&dll_zip, &[("Single.dll", b"plugin")]);
    fs::create_dir_all(game.join("BepInEx/plugins")).unwrap();
    fs::write(game.join("BepInEx/plugins/Single.dll"), b"old").unwrap();
    let files = extract_local_mod_zip(&dll_zip, &game, "single.dll.zip").unwrap();
    assert_eq!(files, vec!["BepInEx/plugins/Single.dll"]);
    assert!(game.join("BepInEx/plugins/Single.dll").is_file());
    assert!(!game.join("BepInEx/plugins/single.dll/Single.dll").exists());
    assert_eq!(
        fs::read(game.join("BepInEx/plugins/Single.dll")).unwrap(),
        b"plugin"
    );
    assert!(game.join("BepInEx/plugins/Package/Plugin.dll").is_file());

    let documented_zip = root.join("documented.zip");
    write_zip(
        &documented_zip,
        &[
            ("Documented.dll", b"plugin"),
            ("README.txt", b"instructions"),
        ],
    );
    let files = extract_local_mod_zip(&documented_zip, &game, "documented.zip").unwrap();
    assert_eq!(
        files,
        vec![
            "BepInEx/plugins/documented/Documented.dll",
            "BepInEx/plugins/documented/README.txt"
        ]
    );
    assert_eq!(
        fs::read(game.join("BepInEx/plugins/documented/README.txt")).unwrap(),
        b"instructions"
    );
    assert!(!game.join("BepInEx/plugins/Documented.dll").exists());

    let mixed_zip = root.join("mixed.zip");
    write_zip(
        &mixed_zip,
        &[("Package/Plugin.dll", b"plugin"), ("Loose.dll", b"loose")],
    );
    fs::create_dir_all(game.join("BepInEx/plugins/mixed")).unwrap();
    fs::write(game.join("BepInEx/plugins/mixed/old.txt"), b"old").unwrap();
    extract_local_mod_zip(&mixed_zip, &game, "mixed.zip").unwrap();
    assert!(
        game.join("BepInEx/plugins/mixed/Package/Plugin.dll")
            .is_file()
    );
    assert!(game.join("BepInEx/plugins/mixed/Loose.dll").is_file());
    assert_eq!(
        fs::read(game.join("BepInEx/plugins/mixed/old.txt")).unwrap(),
        b"old"
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn mod_install_scenarios_respect_local_and_repository_targets() {
    struct Case {
        entries: &'static [(&'static str, &'static [u8])],
        local: &'static [&'static str],
        target: &'static str,
        online: &'static [&'static str],
    }
    let cases = [
        Case {
            entries: &[("Only.dll", b"DLL")],
            local: &["BepInEx/plugins/Only.dll"],
            target: "BepInEx/plugins/Configured",
            online: &["BepInEx/plugins/Configured/Only.dll"],
        },
        Case {
            entries: &[
                ("OC2HostUtilities/", b""),
                ("OC2HostUtilities/Main.dll", b"DLL"),
                ("OC2HostUtilities/data/config.json", b"data"),
            ],
            local: &[
                "BepInEx/plugins/OC2HostUtilities/Main.dll",
                "BepInEx/plugins/OC2HostUtilities/data/config.json",
            ],
            target: "BepInEx/plugins/Configured",
            online: &[
                "BepInEx/plugins/Configured/OC2HostUtilities/Main.dll",
                "BepInEx/plugins/Configured/OC2HostUtilities/data/config.json",
            ],
        },
        Case {
            entries: &[
                ("BepInEx/", b""),
                ("BepInEx/plugins/Main.dll", b"DLL"),
                ("BepInEx/config/mod.cfg", b"config"),
            ],
            local: &["BepInEx/plugins/Main.dll", "BepInEx/config/mod.cfg"],
            target: "BepInEx",
            online: &["BepInEx/plugins/Main.dll", "BepInEx/config/mod.cfg"],
        },
        Case {
            entries: &[("plugins/", b""), ("plugins/Main.dll", b"DLL")],
            local: &["BepInEx/plugins/Main.dll"],
            target: "BepInEx",
            online: &["BepInEx/plugins/Main.dll"],
        },
        Case {
            entries: &[
                ("OC2HostUtilities/Main.dll", b"DLL"),
                ("HostUtilities.dll", b"loose"),
                ("README.txt", b"readme"),
            ],
            local: &[
                "BepInEx/plugins/Scenario/OC2HostUtilities/Main.dll",
                "BepInEx/plugins/Scenario/HostUtilities.dll",
                "BepInEx/plugins/Scenario/README.txt",
            ],
            target: "BepInEx/plugins/Configured",
            online: &[
                "BepInEx/plugins/Configured/Scenario/OC2HostUtilities/Main.dll",
                "BepInEx/plugins/Configured/Scenario/HostUtilities.dll",
                "BepInEx/plugins/Configured/Scenario/README.txt",
            ],
        },
    ];
    let root = std::env::temp_dir().join(format!("oc2-install-scenarios-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let zip = root.join("Scenario.zip");
    for (index, case) in cases.iter().enumerate() {
        write_zip(&zip, case.entries);
        for online in [false, true] {
            let game = root.join(format!("game-{index}-{online}"));
            let expected = if online { case.online } else { case.local };
            // Exercise merging into existing folders and creating missing subfolders.
            let replaced = game.join(expected[0]);
            fs::create_dir_all(replaced.parent().unwrap()).unwrap();
            fs::write(&replaced, b"old").unwrap();
            fs::write(game.join("unrelated.txt"), b"other").unwrap();
            let files = if online {
                extract_mod_zip(&zip, &game, case.target, "Scenario.zip").unwrap()
            } else {
                extract_local_mod_zip(&zip, &game, "Scenario.zip").unwrap()
            };
            assert_eq!(files, expected, "scenario {index}, online={online}");
            for (relative, (_, contents)) in files
                .iter()
                .zip(case.entries.iter().filter(|(name, _)| !name.ends_with('/')))
            {
                assert_eq!(&fs::read(game.join(relative)).unwrap(), contents);
            }
            assert_eq!(fs::read(game.join("unrelated.txt")).unwrap(), b"other");
        }
    }
    let source = root.join("Only.dll");
    fs::write(&source, b"direct DLL").unwrap();
    for target in ["BepInEx/plugins", "BepInEx/plugins/Configured"] {
        let game = root.join(format!("direct-{}", target.len()));
        fs::create_dir_all(&game).unwrap();
        let files = copy_asset(&source, &game, target, "Only.dll").unwrap();
        assert_eq!(files, vec![format!("{target}/Only.dll")]);
        assert_eq!(fs::read(game.join(&files[0])).unwrap(), b"direct DLL");
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn special_root_merging_obeys_empty_or_explicit_online_target_and_creates_missing_folders() {
    let root = std::env::temp_dir().join(format!("oc2-special-targets-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let zip = root.join("Scenario.zip");
    for (prefix, target, expected) in [
        ("BepInEx/plugins/", "", "BepInEx/plugins/Main.dll"),
        ("BepInEx/plugins/", "BepInEx", "BepInEx/plugins/Main.dll"),
        ("plugins/", "", "plugins/Main.dll"),
        ("plugins/", "BepInEx", "BepInEx/plugins/Main.dll"),
        ("plugins/", "BepInEx/plugins", "BepInEx/plugins/Main.dll"),
    ] {
        for existing in [false, true] {
            let game = root.join(Uuid::new_v4().to_string());
            fs::create_dir_all(&game).unwrap();
            if existing {
                let path = game.join(expected);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(&path, b"old").unwrap();
                fs::write(path.parent().unwrap().join("Other.dll"), b"other").unwrap();
            }
            let name = format!("{prefix}Main.dll");
            write_zip(&zip, &[(name.as_str(), b"new")]);
            let files = extract_mod_zip(&zip, &game, target, "Scenario.zip").unwrap();
            assert_eq!(files, vec![expected]);
            assert_eq!(fs::read(game.join(expected)).unwrap(), b"new");
            if existing {
                assert_eq!(
                    fs::read(game.join(expected).parent().unwrap().join("Other.dll")).unwrap(),
                    b"other"
                );
            }
        }
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn multiple_folder_fallback_preserves_structure_and_duplicate_targets_are_rejected() {
    let root = std::env::temp_dir().join(format!("oc2-zip-fallback-{}", Uuid::new_v4()));
    let game = root.join("game");
    fs::create_dir_all(&game).unwrap();
    let zip = root.join("Bundle.zip");
    write_zip(
        &zip,
        &[("First/A.dll", b"first"), ("Second/B.dll", b"second")],
    );
    let local = extract_local_mod_zip(&zip, &game, "Bundle.zip").unwrap();
    assert_eq!(
        local,
        vec![
            "BepInEx/plugins/Bundle/First/A.dll",
            "BepInEx/plugins/Bundle/Second/B.dll"
        ]
    );
    let online = extract_mod_zip(&zip, &game, "Configured", "Bundle.zip").unwrap();
    assert_eq!(
        online,
        vec![
            "Configured/Bundle/First/A.dll",
            "Configured/Bundle/Second/B.dll"
        ]
    );
    write_zip(
        &zip,
        &[("Duplicated.dll", b"first"), ("DUPLICATED.DLL", b"second")],
    );
    assert!(extract_local_mod_zip(&zip, &game, "Bundle.zip").is_err());
    assert!(!game.join("BepInEx/plugins/Duplicated.dll").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn online_mixed_zip_uses_archive_named_directory_and_reuses_explicit_package_target() {
    let root = std::env::temp_dir().join(format!("oc2-mixed-targets-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    // Cache names do not identify the downloaded ZIP's package directory.
    let zip = root.join("random-cache-file.tmp");
    write_zip(
        &zip,
        &[
            ("Folder/Main.dll", b"plugin"),
            ("Loose.dll", b"loose"),
            ("README.txt", b"readme"),
        ],
    );
    for (target, name, package) in [
        ("BepInEx/plugins", "Bundle.zip", "BepInEx/plugins/Bundle"),
        (
            "BepInEx/plugins/Bundle",
            "Bundle.zip",
            "BepInEx/plugins/Bundle",
        ),
        (
            "BepInEx/plugins/bundle",
            "Bundle.zip",
            "BepInEx/plugins/bundle",
        ),
        ("", "Bundle.zip", "Bundle"),
        ("BepInEx/plugins", "plugins.zip", "BepInEx/plugins/plugins"),
        ("BepInEx", "BepInEx.zip", "BepInEx/BepInEx"),
    ] {
        let game = root.join(Uuid::new_v4().to_string());
        let package_path = game.join(package);
        fs::create_dir_all(package_path.join("Folder")).unwrap();
        fs::write(package_path.join("Folder/Main.dll"), b"old").unwrap();
        fs::write(game.join("Unrelated.dll"), b"other").unwrap();
        let files = extract_mod_zip(&zip, &game, target, name).unwrap();
        assert_eq!(
            files,
            vec![
                format!("{package}/Folder/Main.dll"),
                format!("{package}/Loose.dll"),
                format!("{package}/README.txt")
            ]
        );
        assert_eq!(
            fs::read(package_path.join("Folder/Main.dll")).unwrap(),
            b"plugin"
        );
        assert_eq!(fs::read(package_path.join("Loose.dll")).unwrap(), b"loose");
        assert_eq!(
            fs::read(package_path.join("README.txt")).unwrap(),
            b"readme"
        );
        assert_eq!(fs::read(game.join("Unrelated.dll")).unwrap(), b"other");
        if target != package {
            assert!(!game.join(target).join("Loose.dll").exists());
            assert!(!game.join(target).join("README.txt").exists());
        }
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn online_zip_merges_target_wrappers_and_preserves_ordinary_directories() {
    let root = std::env::temp_dir().join(format!("oc2-online-zip-{}", Uuid::new_v4()));
    let game = root.join("game");
    fs::create_dir_all(&game).unwrap();

    let bepinex_zip = root.join("bepinex.zip");
    write_zip(&bepinex_zip, &[("BepInEx/plugins/Online.dll", b"plugin")]);
    extract_mod_zip(&bepinex_zip, &game, "BepInEx", "bepinex.zip").unwrap();
    assert!(game.join("BepInEx/plugins/Online.dll").is_file());
    assert!(!game.join("BepInEx/BepInEx/plugins/Online.dll").exists());

    let plugins_zip = root.join("plugins.zip");
    write_zip(&plugins_zip, &[("plugins/Online2.dll", b"plugin")]);
    extract_mod_zip(&plugins_zip, &game, "BepInEx/plugins", "plugins.zip").unwrap();
    assert!(game.join("BepInEx/plugins/Online2.dll").is_file());

    let arbitrary_zip = root.join("arbitrary.zip");
    write_zip(&arbitrary_zip, &[("Package/Online3.dll", b"plugin")]);
    extract_mod_zip(&arbitrary_zip, &game, "BepInEx/plugins", "arbitrary.zip").unwrap();
    assert!(game.join("BepInEx/plugins/Package/Online3.dll").is_file());
    assert!(!game.join("BepInEx/plugins/Online3.dll").exists());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn online_zip_keeps_diy_level_root_and_nested_resource_paths() {
    let root = std::env::temp_dir().join(format!("oc2-diy-level-zip-{}", Uuid::new_v4()));
    let game = root.join("game");
    fs::create_dir_all(&game).unwrap();
    let zip = root.join("DIYLevel.zip");
    write_zip(
        &zip,
        &[
            ("OC2DIYLevel/OC2DIYLevel.dll", b"plugin"),
            ("OC2DIYLevel/levels/story/example.json", b"level"),
            ("OC2DIYLevel/assets/icons/example.png", b"icon"),
        ],
    );

    let files = extract_mod_zip(&zip, &game, "BepInEx/plugins", "DIYLevel.zip").unwrap();
    assert_eq!(
        files,
        vec![
            "BepInEx/plugins/OC2DIYLevel/OC2DIYLevel.dll",
            "BepInEx/plugins/OC2DIYLevel/levels/story/example.json",
            "BepInEx/plugins/OC2DIYLevel/assets/icons/example.png",
        ]
    );
    for relative in files {
        assert!(game.join(relative).is_file());
    }
    assert!(!game.join("BepInEx/plugins/OC2DIYLevel.dll").exists());
    assert!(!game.join("BepInEx/plugins/levels").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn online_zip_explicit_target_deduplicates_only_special_directory_prefixes() {
    let root = std::env::temp_dir().join(format!("oc2-explicit-target-zip-{}", Uuid::new_v4()));
    let game = root.join("game");
    fs::create_dir_all(&game).unwrap();
    for (index, prefix) in [
        "BepInEx/plugins/OC2DIYLevel",
        "plugins/OC2DIYLevel",
        "OC2DIYLevel",
    ]
    .iter()
    .enumerate()
    {
        let zip = root.join(format!("target-{index}.zip"));
        let entry = format!("{prefix}/resources/Level{index}.json");
        write_zip(&zip, &[(entry.as_str(), b"level")]);
        let files =
            extract_mod_zip(&zip, &game, "BepInEx/plugins/OC2DIYLevel", "DIYLevel.zip").unwrap();
        assert_eq!(
            files,
            vec![if index < 2 {
                format!("BepInEx/plugins/OC2DIYLevel/resources/Level{index}.json")
            } else {
                format!("BepInEx/plugins/OC2DIYLevel/OC2DIYLevel/resources/Level{index}.json")
            }]
        );
        assert!(game.join(&files[0]).is_file());
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn local_non_special_plugin_directory_is_preserved() {
    let root = std::env::temp_dir().join(format!("oc2-local-plugin-{}", Uuid::new_v4()));
    let game = root.join("game");
    fs::create_dir_all(&game).unwrap();
    let plugin_zip = root.join("plugin.zip");
    write_zip(&plugin_zip, &[("plugin/Plugin.dll", b"plugin")]);
    extract_local_mod_zip(&plugin_zip, &game, "plugin.zip").unwrap();
    assert!(game.join("BepInEx/plugins/plugin/Plugin.dll").is_file());
    assert!(!game.join("BepInEx/plugin/Plugin.dll").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn local_special_roots_with_other_top_level_entries_preserve_the_whole_archive() {
    let root = std::env::temp_dir().join(format!("oc2-local-special-root-{}", Uuid::new_v4()));
    let game = root.join("game");
    fs::create_dir_all(&game).unwrap();

    let bepinex_zip = root.join("bepinex-mixed.zip");
    write_zip(
        &bepinex_zip,
        &[
            ("BepInEx/plugins/Mixed.dll", b"plugin"),
            ("README.txt", b"documentation"),
        ],
    );
    extract_local_mod_zip(&bepinex_zip, &game, "bepinex-mixed.zip").unwrap();
    assert!(
        game.join("BepInEx/plugins/bepinex-mixed/BepInEx/plugins/Mixed.dll")
            .is_file()
    );
    assert_eq!(
        fs::read(game.join("BepInEx/plugins/bepinex-mixed/README.txt")).unwrap(),
        b"documentation"
    );
    assert!(!game.join("README.txt").exists());

    let plugins_zip = root.join("plugins-mixed.zip");
    write_zip(
        &plugins_zip,
        &[
            ("plugins/PluginsMixed.dll", b"plugin"),
            ("Loose.dll", b"documentation"),
        ],
    );
    extract_local_mod_zip(&plugins_zip, &game, "plugins-mixed.zip").unwrap();
    assert!(
        game.join("BepInEx/plugins/plugins-mixed/plugins/PluginsMixed.dll")
            .is_file()
    );
    assert_eq!(
        fs::read(game.join("BepInEx/plugins/plugins-mixed/Loose.dll")).unwrap(),
        b"documentation"
    );
    assert!(!game.join("BepInEx/plugins/Loose.dll").exists());

    fs::remove_dir_all(root).unwrap();
}
