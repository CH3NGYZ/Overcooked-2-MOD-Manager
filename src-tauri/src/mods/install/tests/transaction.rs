use super::*;

#[test]
fn online_install_with_no_recyclable_old_files_succeeds_or_preserves_manifest_on_failure() {
    for condition in ["empty", "missing", "shared"] {
        for zip in [false, true] {
            for fail in [false, true] {
                let workspace = InstallWorkspace::new();
                let missing = "BepInEx/plugins/Main.dll";
                let shared = "BepInEx/plugins/Shared.dll";
                let old_files = match condition {
                    "empty" => vec![],
                    "shared" => vec![shared.into()],
                    _ => vec![
                        missing.into(),
                        "BepInEx/plugins/Missing.dll.disabled".into(),
                    ],
                };
                archive::write_manifest(
                    &workspace.manifests,
                    &archive::new_manifest(
                        "package",
                        "v1",
                        archive::single_language("Old package"),
                        Default::default(),
                        old_files,
                        Vec::new(),
                    ),
                )
                .unwrap();
                let original = fs::read(workspace.manifests.join("package.toml")).unwrap();
                let peer_original = if condition == "shared" {
                    workspace.write(shared, b"shared dependency");
                    archive::write_manifest(
                        &workspace.manifests,
                        &archive::new_manifest(
                            "peer",
                            "v1",
                            archive::single_language("Peer"),
                            Default::default(),
                            vec![shared.into()],
                            Vec::new(),
                        ),
                    )
                    .unwrap();
                    Some(fs::read(workspace.manifests.join("peer.toml")).unwrap())
                } else {
                    None
                };
                workspace.write("BepInEx/plugins/Unrelated.dll", b"keep");
                let asset = if zip {
                    if fail {
                        fs::create_dir_all(workspace.game.join("BepInEx/plugins/Blocked.dll"))
                            .unwrap();
                        workspace.zip(&[("Main.dll", b"new plugin"), ("Blocked.dll", b"blocked")])
                    } else {
                        workspace.zip(&[("Main.dll", b"new plugin")])
                    }
                } else {
                    let asset = workspace.root.join("Main.dll");
                    if !fail {
                        fs::write(&asset, b"new plugin").unwrap();
                    }
                    asset
                };
                let repo = repository(
                    "package",
                    if zip {
                        ModDownloadMode::Release
                    } else {
                        ModDownloadMode::RepositoryFile
                    },
                );
                let result = install_downloaded_mod_repository(
                    &asset,
                    if zip { "update.zip" } else { "Main.dll" },
                    &workspace.game,
                    &workspace.manifests,
                    &workspace.recycle,
                    &repo,
                    "v2",
                    false,
                    true,
                );
                if fail {
                    let error = result.unwrap_err();
                    assert!(
                        error.starts_with("mods.installFailedRolledBack"),
                        "{condition}, zip={zip}: {error}"
                    );
                    assert!(!workspace.game.join(missing).exists());
                    assert_eq!(
                        fs::read(workspace.manifests.join("package.toml")).unwrap(),
                        original
                    );
                } else {
                    assert_eq!(
                        result.unwrap_or_else(|error| panic!("{condition}, zip={zip}: {error}")),
                        1
                    );
                    assert_eq!(
                        fs::read(workspace.game.join(missing)).unwrap(),
                        b"new plugin"
                    );
                    let current = archive::read_manifest(&workspace.manifests, "package").unwrap();
                    assert_eq!(current.release_tag, "v2");
                    assert_eq!(current.files, vec![missing]);
                }
                if let Some(bytes) = peer_original {
                    assert_eq!(
                        fs::read(workspace.manifests.join("peer.toml")).unwrap(),
                        bytes
                    );
                    assert_eq!(
                        fs::read(workspace.game.join(shared)).unwrap(),
                        b"shared dependency"
                    );
                }
                assert_eq!(
                    fs::read(workspace.game.join("BepInEx/plugins/Unrelated.dll")).unwrap(),
                    b"keep"
                );
                assert!(recycle_bin::list(&workspace.recycle).unwrap().is_empty());
                assert!(!workspace.recycle.exists());
            }
        }
    }
}

#[test]
fn online_reinstall_rejects_invalid_old_manifests_and_nonfile_entries() {
    for condition in ["invalid_toml", "unsafe_path", "directory"] {
        let workspace = InstallWorkspace::new();
        let files = if condition == "unsafe_path" {
            vec!["../outside.dll".into()]
        } else {
            vec!["BepInEx/plugins/Old.dll".into()]
        };
        archive::write_manifest(
            &workspace.manifests,
            &archive::new_manifest(
                "package",
                "v1",
                Default::default(),
                Default::default(),
                files,
                Vec::new(),
            ),
        )
        .unwrap();
        if condition == "invalid_toml" {
            fs::write(workspace.manifests.join("package.toml"), "invalid = [").unwrap();
        } else if condition == "directory" {
            fs::create_dir_all(workspace.game.join("BepInEx/plugins/Old.dll")).unwrap();
            workspace.write("BepInEx/plugins/Old.dll/keep.txt", b"keep");
        }
        let original = fs::read(workspace.manifests.join("package.toml")).unwrap();
        let asset = workspace.root.join("Main.dll");
        fs::write(&asset, b"new plugin").unwrap();
        let repo = repository("package", ModDownloadMode::Release);
        assert!(
            install_downloaded_mod_repository(
                &asset,
                "Main.dll",
                &workspace.game,
                &workspace.manifests,
                &workspace.recycle,
                &repo,
                "v2",
                false,
                true,
            )
            .is_err()
        );
        assert_eq!(
            fs::read(workspace.manifests.join("package.toml")).unwrap(),
            original
        );
        assert!(!workspace.game.join("BepInEx/plugins/Main.dll").exists());
        assert!(!workspace.recycle.exists());
        if condition == "directory" {
            assert_eq!(
                fs::read(workspace.game.join("BepInEx/plugins/Old.dll/keep.txt")).unwrap(),
                b"keep"
            );
        }
    }
}

#[cfg(windows)]
#[test]
fn online_install_without_old_files_rolls_back_when_manifest_commit_fails() {
    for zip in [false, true] {
        let workspace = InstallWorkspace::new();
        archive::write_manifest(
            &workspace.manifests,
            &archive::new_manifest(
                "package",
                "v1",
                Default::default(),
                Default::default(),
                vec!["BepInEx/plugins/Main.dll".into()],
                Vec::new(),
            ),
        )
        .unwrap();
        let path = workspace.manifests.join("package.toml");
        let original = fs::read(&path).unwrap();
        let permissions = fs::metadata(&path).unwrap().permissions();
        let mut readonly = permissions.clone();
        readonly.set_readonly(true);
        fs::set_permissions(&path, readonly).unwrap();
        let asset = if zip {
            workspace.zip(&[("Main.dll", b"new plugin")])
        } else {
            let asset = workspace.root.join("Main.dll");
            fs::write(&asset, b"new plugin").unwrap();
            asset
        };
        let result = install_downloaded_mod_repository(
            &asset,
            if zip { "update.zip" } else { "Main.dll" },
            &workspace.game,
            &workspace.manifests,
            &workspace.recycle,
            &repository("package", ModDownloadMode::Release),
            "v2",
            false,
            true,
        );
        fs::set_permissions(&path, permissions).unwrap();
        let error = result.unwrap_err();
        assert!(error.starts_with("mods.installFailedRolledBack"), "{error}");
        assert_eq!(fs::read(&path).unwrap(), original);
        assert!(!workspace.game.join("BepInEx/plugins/Main.dll").exists());
        assert!(!workspace.recycle.exists());
    }
}

#[test]
fn online_reinstall_with_duplicate_manifests_succeeds_or_rolls_back() {
    for zip in [false, true] {
        for fail in [false, true] {
            let workspace = InstallWorkspace::new();
            let dll = "BepInEx/plugins/Main.dll";
            workspace.write(&format!("{dll}.disabled"), b"old plugin");
            workspace.write("BepInEx/plugins/Unrelated.dll", b"keep");
            for id in ["package", "manual"] {
                archive::write_manifest(
                    &workspace.manifests,
                    &archive::new_manifest(
                        id,
                        "v1",
                        archive::single_language(id),
                        Default::default(),
                        vec![dll.into()],
                        Vec::new(),
                    ),
                )
                .unwrap();
            }
            let originals = ["package", "manual"].map(|id| {
                (
                    id,
                    fs::read(workspace.manifests.join(format!("{id}.toml"))).unwrap(),
                )
            });
            let asset = if zip {
                if fail {
                    fs::create_dir_all(workspace.game.join("BepInEx/plugins/Blocked.dll")).unwrap();
                    workspace.zip(&[("Main.dll", b"new plugin"), ("Blocked.dll", b"blocked")])
                } else {
                    workspace.zip(&[("Main.dll", b"new plugin")])
                }
            } else {
                let asset = workspace.root.join("Main.dll");
                if !fail {
                    fs::write(&asset, b"new plugin").unwrap();
                }
                asset
            };
            let repo = repository("package", ModDownloadMode::Release);
            let result = install_downloaded_mod_repository(
                &asset,
                if zip { "update.zip" } else { "Main.dll" },
                &workspace.game,
                &workspace.manifests,
                &workspace.recycle,
                &repo,
                "v2",
                false,
                true,
            );
            if fail {
                let error = result.unwrap_err();
                assert!(error.starts_with("mods.installFailedRolledBack"), "{error}");
                assert_eq!(
                    fs::read(workspace.game.join(format!("{dll}.disabled"))).unwrap(),
                    b"old plugin"
                );
                assert!(!workspace.game.join(dll).exists());
                for (id, bytes) in originals {
                    assert_eq!(
                        fs::read(workspace.manifests.join(format!("{id}.toml"))).unwrap(),
                        bytes
                    );
                }
                assert!(recycle_bin::list(&workspace.recycle).unwrap().is_empty());
            } else {
                assert_eq!(result.unwrap(), 1);
                assert_eq!(fs::read(workspace.game.join(dll)).unwrap(), b"new plugin");
                assert!(!workspace.game.join(format!("{dll}.disabled")).exists());
                assert!(archive::read_manifest(&workspace.manifests, "manual").is_none());
                assert_eq!(
                    archive::read_manifest(&workspace.manifests, "package")
                        .unwrap()
                        .release_tag,
                    "v2"
                );
                let entries = recycle_bin::list(&workspace.recycle).unwrap();
                assert_eq!(entries.len(), 1);
                assert_eq!(
                    fs::read(
                        workspace
                            .recycle
                            .join(&entries[0].id)
                            .join("files")
                            .join(format!("{dll}.disabled"))
                    )
                    .unwrap(),
                    b"old plugin"
                );
            }
            assert_eq!(
                fs::read(workspace.game.join("BepInEx/plugins/Unrelated.dll")).unwrap(),
                b"keep"
            );
        }
    }
}

#[test]
fn online_reinstall_recycles_complete_old_package_before_zip_or_dll_install() {
    for zip in [false, true] {
        let workspace = InstallWorkspace::new();
        let original = workspace.original();
        let repo = repository(
            "package",
            if zip {
                ModDownloadMode::Release
            } else {
                ModDownloadMode::RepositoryFile
            },
        );
        let asset = if zip {
            workspace.zip(&[("Main.dll", b"new main"), ("New.dll", b"new member")])
        } else {
            let path = workspace.root.join("Main.dll");
            fs::write(&path, b"new main").unwrap();
            path
        };
        let count = install_downloaded_mod_repository(
            &asset,
            if zip { "update.zip" } else { "Main.dll" },
            &workspace.game,
            &workspace.manifests,
            &workspace.recycle,
            &repo,
            "v2",
            false,
            true,
        )
        .unwrap();
        assert_eq!(count, if zip { 2 } else { 1 });
        assert_eq!(
            fs::read(workspace.game.join("BepInEx/plugins/Main.dll")).unwrap(),
            b"new main"
        );
        assert!(
            !workspace
                .game
                .join("BepInEx/plugins/Hidden.dll.disabled")
                .exists()
        );
        assert!(
            !workspace
                .game
                .join("BepInEx/plugins/data/settings.json")
                .exists()
        );
        assert_eq!(
            fs::read(workspace.game.join("BepInEx/plugins/Shared.dll")).unwrap(),
            b"shared"
        );
        assert_eq!(
            fs::read(workspace.game.join("BepInEx/plugins/Unrelated.dll")).unwrap(),
            b"unrelated"
        );
        let entries = recycle_bin::list(&workspace.recycle).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].file_count, 3);
        assert_eq!(entries[0].release_tag.as_deref(), Some("v1"));
        let staged = workspace.recycle.join(&entries[0].id);
        assert_eq!(fs::read(staged.join("manifest.toml")).unwrap(), original);
        assert_eq!(
            fs::read(staged.join("files/BepInEx/plugins/Main.dll")).unwrap(),
            b"old main"
        );
        assert_eq!(
            fs::read(staged.join("files/BepInEx/plugins/Hidden.dll.disabled")).unwrap(),
            b"old hidden"
        );
        assert_eq!(
            fs::read(staged.join("files/BepInEx/plugins/data/settings.json")).unwrap(),
            b"old settings"
        );
        let manifest = archive::read_manifest(&workspace.manifests, "package").unwrap();
        assert_eq!(manifest.release_tag, "v2");
        assert_eq!(manifest.files.len(), count);
    }
}

#[test]
fn online_reinstall_mixed_zip_records_package_directory_and_recycles_previous_package() {
    let workspace = InstallWorkspace::new();
    let original = workspace.original();
    let asset = workspace.zip(&[
        ("Main.dll", b"new main"),
        ("Data/settings.json", b"new settings"),
        ("README.txt", b"readme"),
    ]);
    let repo = repository("package", ModDownloadMode::Release);
    assert_eq!(
        install_downloaded_mod_repository(
            &asset,
            "MOD Bundle.zip",
            &workspace.game,
            &workspace.manifests,
            &workspace.recycle,
            &repo,
            "v2",
            false,
            true,
        )
        .unwrap(),
        3
    );
    let manifest = archive::read_manifest(&workspace.manifests, "package").unwrap();
    assert_eq!(
        manifest.files,
        vec![
            "BepInEx/plugins/MOD Bundle/Main.dll",
            "BepInEx/plugins/MOD Bundle/Data/settings.json",
            "BepInEx/plugins/MOD Bundle/README.txt"
        ]
    );
    assert_eq!(
        fs::read(workspace.game.join(&manifest.files[0])).unwrap(),
        b"new main"
    );
    assert_eq!(
        fs::read(workspace.game.join(&manifest.files[1])).unwrap(),
        b"new settings"
    );
    assert_eq!(
        fs::read(workspace.game.join(&manifest.files[2])).unwrap(),
        b"readme"
    );
    assert!(!workspace.game.join("BepInEx/plugins/Main.dll").exists());
    assert!(!workspace.game.join("BepInEx/plugins/README.txt").exists());
    let entries = recycle_bin::list(&workspace.recycle).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].file_count, 3);
    assert_eq!(
        fs::read(workspace.recycle.join(&entries[0].id).join("manifest.toml")).unwrap(),
        original
    );
}

#[test]
fn online_reinstall_failure_restores_disabled_files_manifest_and_shared_overwrites() {
    let workspace = InstallWorkspace::new();
    let original = workspace.original();
    fs::create_dir_all(workspace.game.join("BepInEx/plugins/Blocked.dll")).unwrap();
    workspace.write(
        "BepInEx/plugins/Blocked.dll/unrelated.txt",
        b"blocked contents",
    );
    let asset = workspace.zip(&[
        ("Main.dll", b"new main"),
        ("Shared.dll", b"changed shared"),
        ("New.dll", b"new member"),
        ("Blocked.dll", b"cannot write"),
    ]);
    let repo = repository("package", ModDownloadMode::Release);
    assert!(
        install_downloaded_mod_repository(
            &asset,
            "update.zip",
            &workspace.game,
            &workspace.manifests,
            &workspace.recycle,
            &repo,
            "v2",
            false,
            true,
        )
        .is_err()
    );
    assert_eq!(
        fs::read(workspace.game.join("BepInEx/plugins/Main.dll")).unwrap(),
        b"old main"
    );
    assert_eq!(
        fs::read(workspace.game.join("BepInEx/plugins/Hidden.dll.disabled")).unwrap(),
        b"old hidden"
    );
    assert_eq!(
        fs::read(workspace.game.join("BepInEx/plugins/data/settings.json")).unwrap(),
        b"old settings"
    );
    assert_eq!(
        fs::read(workspace.game.join("BepInEx/plugins/Shared.dll")).unwrap(),
        b"shared"
    );
    assert_eq!(
        fs::read(
            workspace
                .game
                .join("BepInEx/plugins/Blocked.dll/unrelated.txt")
        )
        .unwrap(),
        b"blocked contents"
    );
    assert_eq!(
        fs::read(workspace.manifests.join("package.toml")).unwrap(),
        original
    );
    assert!(!workspace.game.join("BepInEx/plugins/New.dll").exists());
    assert!(!workspace.game.join("BepInEx/plugins/Hidden.dll").exists());
    assert!(recycle_bin::list(&workspace.recycle).unwrap().is_empty());
}

#[test]
fn online_reinstall_rejects_unsafe_or_empty_archives_before_recycling() {
    for entries in [&[("../outside.dll", b"bad".as_slice())][..], &[][..]] {
        let workspace = InstallWorkspace::new();
        let original = workspace.original();
        let asset = workspace.zip(entries);
        let repo = repository("package", ModDownloadMode::Release);
        assert!(
            install_downloaded_mod_repository(
                &asset,
                "update.zip",
                &workspace.game,
                &workspace.manifests,
                &workspace.recycle,
                &repo,
                "v2",
                false,
                true,
            )
            .is_err()
        );
        assert_eq!(
            fs::read(workspace.game.join("BepInEx/plugins/Main.dll")).unwrap(),
            b"old main"
        );
        assert_eq!(
            fs::read(workspace.manifests.join("package.toml")).unwrap(),
            original
        );
        assert!(recycle_bin::list(&workspace.recycle).unwrap().is_empty());
    }
}
