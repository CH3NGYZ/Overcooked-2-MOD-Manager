use super::*;

#[test]
fn real_manifest_scan_keeps_dll_metadata_but_selects_owner_repository_tag_first() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/verification/community-20261013")
        .join(format!("binding-scan-{}", uuid::Uuid::new_v4()));
    let relative = "BepInEx/plugins/Example.dll";
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut bytes = b"BepInPlugin".to_vec();
    bytes.extend_from_slice(&[1, 0]);
    for value in ["com.example.guid", "Example", "98.5.0"] {
        bytes.push(value.len() as u8);
        bytes.extend_from_slice(value.as_bytes());
    }
    std::fs::write(&path, &bytes).unwrap();
    let mut bound = binding_repository();
    bound.id = "refreshed-source-id".into();
    bound.plugin_guid.clear();
    bound.local_product_name.clear();
    bound.local_original_filename.clear();
    bound.cached_releases = vec![high_cached_release()];
    let mut misleading = binding_repository();
    misleading.id = "dll-matching-but-wrong-repository".into();
    misleading.repository = "unrelated".into();
    let config: crate::models::AppConfig = serde_json::from_value(serde_json::json!({
        "schema_version":1, "ui_zoom":1.5,
        "game":{"path":""},
        "network":{"timeout_seconds":5,"download_threads":1,"max_retry_count":3},
        "launch":{"ask_user_to_start_from_steam":false,"auto_restore_winhttp":false},
        "auth":{"github_token":"","gitee_token":"","official_api_only":true},
        "repositories":[misleading, bound]
    }))
    .unwrap();
    let manifests = root.join("manifests");
    let mut manifest = crate::archive::new_manifest(
        "old-source-id",
        "v98.0.0",
        crate::archive::single_language("Package"),
        Default::default(),
        vec![relative.into()],
        Vec::new(),
    );
    manifest.repository_binding = crate::models::RepositoryBinding::from_release(&bound, "v98.0.0");
    crate::archive::write_manifest(&manifests, &manifest).unwrap();
    let scan = super::scan_mods_with_manifests(&root, &config, Some(&manifests));
    assert_eq!(scan.len(), 1);
    let installed = &scan[0];
    assert_eq!(installed.guid.as_deref(), Some("com.example.guid"));
    assert_eq!(installed.version.as_deref(), Some("98.5.0"));
    assert_eq!(
        installed.repository_id.as_deref(),
        Some("refreshed-source-id")
    );
    assert_eq!(installed.repository_ids, ["refreshed-source-id"]);
    assert_eq!(installed.package_id.as_deref(), Some("old-source-id"));
    assert_eq!(installed.repository_binding, manifest.repository_binding);
    assert!(installed.update_available);
    assert_eq!(
        super::local_update_version(installed).as_deref(),
        Some("v98.0.0")
    );

    std::fs::rename(&path, root.join(format!("{relative}.disabled"))).unwrap();
    let disabled = super::scan_mods_with_manifests(&root, &config, Some(&manifests));
    assert!(!disabled[0].enabled);
    assert_eq!(disabled[0].repository_binding, installed.repository_binding);
    assert_eq!(disabled[0].package_id, installed.package_id);

    manifest.repository_binding = None;
    crate::archive::write_manifest(&manifests, &manifest).unwrap();
    let legacy = super::scan_mods_with_manifests(&root, &config, Some(&manifests));
    assert_eq!(
        legacy[0].repository_id.as_deref(),
        Some("dll-matching-but-wrong-repository")
    );
    assert!(legacy[0].repository_binding.is_none());
    let verified = std::fs::canonicalize(&root).unwrap();
    assert!(verified.starts_with(
        std::fs::canonicalize(std::path::Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap()
    ));
    std::fs::remove_dir_all(verified).unwrap();
}

#[test]
fn channel_bound_install_only_checks_its_own_release_track() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/verification/channel-binding")
        .join(format!("scan-{}", uuid::Uuid::new_v4()));
    let relative = "BepInEx/plugins/Example.dll";
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, plugin_blob("com.example.guid", "Example", "98.5.0")).unwrap();
    let mut stable = binding_repository();
    stable.id = "stable-track".into();
    stable.channel = crate::models::Channel::Stable;
    stable.cached_releases = vec![high_cached_release()];
    let mut beta = binding_repository();
    beta.id = "beta-track".into();
    beta.channel = crate::models::Channel::Beta;
    let mut beta_release = high_cached_release();
    beta_release.tag_name = "v99.0.0-beta.1".into();
    beta_release.version = Some("99.0.0-beta.1".into());
    beta_release.prerelease = true;
    beta.cached_releases = vec![beta_release];
    let config: crate::models::AppConfig = serde_json::from_value(serde_json::json!({
        "schema_version":1, "ui_zoom":1.5,
        "game":{"path":""},
        "network":{"timeout_seconds":5,"download_threads":1,"max_retry_count":3},
        "launch":{"ask_user_to_start_from_steam":false,"auto_restore_winhttp":false},
        "auth":{"github_token":"","gitee_token":"","official_api_only":true},
        "repositories":[stable, beta]
    }))
    .unwrap();
    let manifests = root.join("manifests");
    let mut manifest = crate::archive::new_manifest(
        "package",
        "v98.0.0-beta.1",
        crate::archive::single_language("Package"),
        Default::default(),
        vec![relative.into()],
        Vec::new(),
    );
    manifest.repository_binding =
        crate::models::RepositoryBinding::from_release(&beta, "v98.0.0-beta.1");
    crate::archive::write_manifest(&manifests, &manifest).unwrap();
    let scan = super::scan_mods_with_manifests(&root, &config, Some(&manifests));
    assert_eq!(scan.len(), 1);
    assert_eq!(scan[0].repository_id.as_deref(), Some("beta-track"));
    assert_eq!(scan[0].repository_ids, ["beta-track"]);
    assert!(scan[0].update_available);

    manifest.repository_binding =
        crate::models::RepositoryBinding::from_release(&stable, "v98.0.0");
    crate::archive::write_manifest(&manifests, &manifest).unwrap();
    let stable_scan = super::scan_mods_with_manifests(&root, &config, Some(&manifests));
    assert_eq!(
        stable_scan[0].repository_id.as_deref(),
        Some("stable-track")
    );
    assert_eq!(stable_scan[0].repository_ids, ["stable-track"]);
    assert!(stable_scan[0].update_available);

    let verified = std::fs::canonicalize(&root).unwrap();
    assert!(verified.starts_with(
        std::fs::canonicalize(std::path::Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap()
    ));
    std::fs::remove_dir_all(verified).unwrap();
}

#[test]
fn scanning_real_raw_manifest_prevents_same_guid_release_update() {
    for installed_version in ["1.2.3", "1.2.3.4"] {
        let root = std::env::temp_dir().join(format!("oc2-raw-binding-{}", uuid::Uuid::new_v4()));
        let relative = "BepInEx/plugins/Example.dll";
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut bytes = b"BepInPlugin".to_vec();
        bytes.extend_from_slice(&[1, 0]);
        for value in ["com.example.guid", "Example", installed_version] {
            bytes.push(value.len() as u8);
            bytes.extend_from_slice(value.as_bytes());
        }
        std::fs::write(&path, bytes).unwrap();
        let mut raw = binding_repository();
        raw.id = "raw".into();
        raw.download_mode = crate::models::RepositoryDownloadMode::RepositoryFile;
        raw.cached_releases = vec![high_cached_release()];
        let mut release = raw.clone();
        release.id = "release".into();
        release.download_mode = crate::models::RepositoryDownloadMode::Release;
        let config: crate::models::AppConfig = serde_json::from_value(serde_json::json!({
            "schema_version":1,"ui_zoom":1.5,
            "game":{"path":""},
            "network":{"timeout_seconds":5,"download_threads":1,"max_retry_count":3},
            "launch":{"ask_user_to_start_from_steam":false,"auto_restore_winhttp":false},
            "auth":{"github_token":"","gitee_token":"","official_api_only":true},
            "repositories":[release,raw]
        }))
        .unwrap();
        let manifest = crate::archive::InstallManifest {
            repository_id: "raw".into(),
            release_tag: String::new(),
            repository_binding: None,
            package_name_i18n: crate::archive::single_language("Raw MOD"),
            note_i18n: Default::default(),
            group_i18n: Default::default(),
            prerelease: false,
            relations: Vec::new(),
            installed_at: "2026-10-07T00:00:00Z".into(),
            files: vec![relative.into()],
        };
        let installed =
            super::inspect_dll_at(&path, &root, &config, std::slice::from_ref(&manifest));
        assert_eq!(installed.version.as_deref(), Some(installed_version));
        assert_eq!(installed.repository_id.as_deref(), Some("raw"));
        assert_eq!(installed.repository_ids, vec!["raw"]);
        assert_eq!(installed.status, "not_release_asset");
        assert!(!installed.update_available);
        assert!(!installed.has_config);
        std::fs::create_dir_all(root.join("BepInEx/config")).unwrap();
        std::fs::write(
            root.join("BepInEx/config/com.example.guid.cfg"),
            "[General]\nEnabled = true\n",
        )
        .unwrap();
        assert!(super::inspect_dll_at(&path, &root, &config, &[]).has_config);
        let release_install = super::inspect_dll_at(&path, &root, &config, &[]);
        assert_eq!(release_install.repository_id.as_deref(), Some("release"));
        assert!(release_install.update_available);
        let mut restricted = config;
        restricted.repositories[1].version_regex = r"^2\.".into();
        let invalid_raw = super::inspect_dll_at(&path, &root, &restricted, &[manifest]);
        assert_eq!(invalid_raw.status, "unbound");
        assert!(!invalid_raw.update_available);
        assert!(invalid_raw.repository_ids.is_empty());
        let target = std::fs::canonicalize(&root).unwrap();
        let temp = std::fs::canonicalize(std::env::temp_dir()).unwrap();
        assert!(target.starts_with(&temp) && target != temp);
        std::fs::remove_dir_all(root).unwrap();
    }
}
