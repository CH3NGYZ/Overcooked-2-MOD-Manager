use super::*;

#[test]
fn all_release_install_sources_persist_repository_tags_and_recycle_the_original_binding() {
    for source in [Some("official"), Some(crate::community::SOURCE_ID), None] {
        for provider in [Provider::Github, Provider::Gitee] {
            let workspace = InstallWorkspace::new();
            let mut repo = repository("package", ModDownloadMode::Release);
            repo.provider = provider;
            repo.source_id = source.map(str::to_string);
            repo.builtin = source == Some("official");
            let asset = workspace.zip(&[("Main.dll", b"first"), ("Child.dll", b"child")]);
            install_downloaded_mod_repository(
                &asset,
                "package.zip",
                &workspace.game,
                &workspace.manifests,
                &workspace.recycle,
                &repo,
                "v1.0.0",
                false,
                true,
            )
            .unwrap();
            let original = archive::read_manifest(&workspace.manifests, "package").unwrap();
            let binding = original.repository_binding.as_ref().unwrap();
            assert_eq!(binding.owner, repo.username);
            assert_eq!(binding.repository, repo.repository);
            assert_eq!(binding.provider, repo.provider);
            assert_eq!(binding.tag, "v1.0.0");
            assert_eq!(binding.channel, repo.channel);
            let scanned = mods::scan_mods_with_manifests(
                &workspace.game,
                &config(vec![repo.clone()]),
                Some(&workspace.manifests),
            );
            assert_eq!(scanned.len(), 2);
            assert!(
                scanned
                    .iter()
                    .all(|item| item.repository_binding.as_ref() == Some(binding)
                        && item.package_mod_count == 2)
            );
            fs::rename(
                workspace.game.join("BepInEx/plugins/Child.dll"),
                workspace.game.join("BepInEx/plugins/Child.dll.disabled"),
            )
            .unwrap();
            let asset = workspace.zip(&[("Main.dll", b"second")]);
            install_downloaded_mod_repository(
                &asset,
                "package.zip",
                &workspace.game,
                &workspace.manifests,
                &workspace.recycle,
                &repo,
                "v2.0.0",
                false,
                true,
            )
            .unwrap();
            let updated = archive::read_manifest(&workspace.manifests, "package").unwrap();
            assert_eq!(updated.repository_binding.as_ref().unwrap().tag, "v2.0.0");
            let entries = recycle_bin::list(&workspace.recycle).unwrap();
            assert_eq!(entries.len(), 1);
            let snapshot: archive::InstallManifest = toml::from_str(
                &fs::read_to_string(workspace.recycle.join(&entries[0].id).join("manifest.toml"))
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(snapshot.repository_binding, original.repository_binding);
            assert_eq!(entries[0].file_count, 2);
        }
    }
    let workspace = InstallWorkspace::new();
    let raw = repository("package", ModDownloadMode::RepositoryFile);
    let asset = workspace.zip(&[("Main.dll", b"raw")]);
    install_downloaded_mod_repository(
        &asset,
        "package.zip",
        &workspace.game,
        &workspace.manifests,
        &workspace.recycle,
        &raw,
        "",
        false,
        true,
    )
    .unwrap();
    assert!(
        archive::read_manifest(&workspace.manifests, "package")
            .unwrap()
            .repository_binding
            .is_none()
    );
}

#[test]
fn vanished_source_ids_reuse_the_bound_package_without_merging_live_packages() {
    let workspace = InstallWorkspace::new();
    let mut old_repo = repository("old-source-id", ModDownloadMode::Release);
    let asset = workspace.zip(&[("Main.dll", b"old")]);
    install_downloaded_mod_repository(
        &asset,
        "package.zip",
        &workspace.game,
        &workspace.manifests,
        &workspace.recycle,
        &old_repo,
        "v1.0.0",
        false,
        true,
    )
    .unwrap();
    let mut new_repo = old_repo.clone();
    new_repo.id = "discovered-source-id".into();
    let identity = mod_repository_install_identity(
        &new_repo,
        &config(vec![new_repo.clone()]),
        &workspace.manifests,
    )
    .unwrap();
    assert_eq!(identity.id, "old-source-id");
    assert_eq!(
        mod_repository_install_identity(
            &new_repo,
            &config(vec![old_repo.clone(), new_repo.clone()]),
            &workspace.manifests
        )
        .unwrap()
        .id,
        new_repo.id
    );
    let asset = workspace.zip(&[("Main.dll", b"new")]);
    install_downloaded_mod_repository(
        &asset,
        "package.zip",
        &workspace.game,
        &workspace.manifests,
        &workspace.recycle,
        &identity,
        "v2.0.0",
        false,
        true,
    )
    .unwrap();
    assert!(
        !workspace
            .manifests
            .join("discovered-source-id.toml")
            .exists()
    );
    assert_eq!(
        archive::read_manifest(&workspace.manifests, "old-source-id")
            .unwrap()
            .repository_binding
            .unwrap()
            .tag,
        "v2.0.0"
    );
    old_repo.id = "another-orphan".into();
    let mut other = archive::new_manifest(
        &old_repo.id,
        "v1.0.0",
        Default::default(),
        Default::default(),
        vec![],
        Vec::new(),
    );
    other.repository_binding = ModRepositoryBinding::from_release(&old_repo, "v1.0.0");
    archive::write_manifest(&workspace.manifests, &other).unwrap();
    assert!(
        mod_repository_install_identity(
            &new_repo,
            &config(vec![new_repo.clone()]),
            &workspace.manifests
        )
        .is_err()
    );
}
