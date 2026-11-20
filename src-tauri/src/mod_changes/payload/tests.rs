use super::*;
use crate::mod_changes::inventory::{NodeFiles, NodeSummary};
use std::collections::BTreeMap;
use std::io::Write;

fn candidate(provides: Vec<ProvidedPlugin>) -> Candidate {
    Candidate {
        id: "installed:fixture".into(),
        source_id: None,
        tag: None,
        version: None,
        provides,
        relations: vec![],
        relations_known: true,
        installable: true,
        raw: false,
        verification_pending: false,
    }
}

fn trusted(game: &Path, provides: Vec<ProvidedPlugin>) -> TrustedInventory {
    let config = crate::config::default_config();
    TrustedInventory {
        inventory: Inventory {
            nodes: vec![ChangeNode {
                id: "fixture".into(),
                kind: NodeKind::Mod,
                current: Some(candidate(provides)),
                enabled: true,
                mutable: true,
                candidates: vec![],
                candidates_complete: true,
            }],
        },
        files: BTreeMap::from([(
            "fixture".into(),
            NodeFiles {
                summary: NodeSummary {
                    node_id: "fixture".into(),
                    label: "fixture".into(),
                    kind: NodeKind::Mod,
                    enabled: true,
                    repository_id: None,
                    package_id: Some("original-package".into()),
                    dll_files: vec![],
                    members: vec![],
                    map_dir_name: None,
                    map_id: None,
                    subject_tag: None,
                    candidates: vec![],
                },
                manifests: vec![],
                map_path: None,
                map_metadata: None,
            },
        )]),
        revision: "fixture".into(),
        game_root: game.into(),
        level_roots: crate::level_sets::LevelSetRoots::new(game.into(), None),
        config,
        managed_mods: Default::default(),
        toggle_unbound_nodes: Default::default(),
        installed_mods: Default::default(),
        duplicate_releases: Default::default(),
        duplicate_release_safe_nodes: Default::default(),
    }
}

fn plugin(key: &str, enabled: bool) -> ProvidedPlugin {
    ProvidedPlugin {
        member_key: Some(key.into()),
        guid: "test-primary".into(),
        version: Some("1.0.0".into()),
        prerelease: Some(false),
        enabled,
    }
}

fn write(root: &Path, relative: &str, bytes: &[u8]) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

#[test]
fn real_metadata_and_unique_guid_preserve_disabled_member_after_directory_change() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    let trusted = trusted(
        &root,
        vec![plugin("BepInEx/plugins/old/Primary.dll", false)],
    );
    write(
        &root,
        "BepInEx/plugins/new/Primary.dll",
        &mods::plugin_blob("test-primary", "Primary", "2.0.0"),
    );
    let mut files = vec!["BepInEx/plugins/new/Primary.dll".into()];
    let members = read_members(&trusted, "fixture", &root, &mut files, false, None).unwrap();
    assert!(!members[0].enabled);
    assert_eq!(members[0].version.as_deref(), Some("2.0.0"));
    assert!(files[0].ends_with(".dll.disabled"));
    assert!(root.join(&files[0]).is_file());
    assert!(
        members[0]
            .member_key
            .as_ref()
            .unwrap()
            .ends_with("Primary.dll")
            || cfg!(windows)
    );
}

#[test]
fn new_member_keeps_asset_state_and_duplicate_variants_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    let trusted = trusted(&root, vec![]);
    let bytes = mods::plugin_blob("test-primary", "Primary", "2.0.0");
    write(&root, "Primary.dll.disabled", &bytes);
    let mut files = vec!["Primary.dll.disabled".into()];
    let members = read_members(&trusted, "fixture", &root, &mut files, true, None).unwrap();
    assert!(!members[0].enabled);
    assert_eq!(members[0].prerelease, Some(true));
    write(&root, "Primary.dll", &bytes);
    assert!(
        read_members(
            &trusted,
            "fixture",
            &root,
            &mut ["Primary.dll".into(), "Primary.dll.disabled".into()],
            true,
            None,
        )
        .is_err()
    );
}

#[test]
fn ambiguous_old_guid_does_not_guess_enabled_state() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    let trusted = trusted(
        &root,
        vec![plugin("old/a.dll", true), plugin("old/b.dll", false)],
    );
    write(
        &root,
        "new.dll",
        &mods::plugin_blob("test-primary", "Primary", "2.0.0"),
    );
    assert!(
        read_members(
            &trusted,
            "fixture",
            &root,
            &mut ["new.dll".into()],
            false,
            None
        )
        .is_err()
    );
}

#[test]
fn manifest_keeps_original_package_identity_and_real_disabled_paths() {
    let trusted = trusted(Path::new("C:/fixture"), vec![]);
    let files = vec!["BepInEx/plugins/Primary.dll.disabled".into()];
    let manifest = create_manifest(
        &trusted,
        "fixture",
        &candidate(vec![]),
        None,
        &SourceOrigin::Manual("preview"),
        "Primary.dll",
        files.clone(),
        vec![],
        false,
    );
    assert_eq!(manifest.repository_id, "original-package");
    assert_eq!(manifest.files, files);
}

fn repository_payload(primary_guid: &str, members: &[(&str, &str)]) -> Result<Payload, String> {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    let game = root.join("game");
    fs::create_dir_all(&game).unwrap();
    let state = AppState::with_config(crate::config::ConfigService::for_test(root.join("data")));
    let mut trusted = trusted(&game, vec![plugin("BepInEx/plugins/Primary.dll", true)]);
    let mut repo = crate::community::repository_from_coordinates(
        &Provider::Github,
        "Fixture",
        "fixture-repository",
        "",
        "",
    )
    .unwrap();
    repo.plugin_guid = primary_guid.into();
    repo.install_target = "BepInEx/plugins".into();
    repo.asset_rules = vec![AssetRule {
        rule_type: AssetRuleType::Exact,
        value: "fixture.zip".into(),
    }];
    repo.cached_releases = vec![ReleaseInfo {
        release_id: None,
        tag_name: "v2.0.0".into(),
        version: Some("2.0.0".into()),
        name: "Fixture".into(),
        body: String::new(),
        published_at: None,
        api_prerelease: Some(false),
        prerelease: false,
        draft: false,
        assets: vec![ReleaseAsset {
            name: "fixture.zip".into(),
            size: 0,
            download_count: None,
            download_url: "https://example.invalid/fixture.zip".into(),
            digest: None,
        }],
    }];
    let mut selected = candidate(vec![]);
    selected.id = "release:fixture:v2.0.0".into();
    selected.source_id = Some(repo.id.clone());
    selected.tag = Some("v2.0.0".into());
    selected.verification_pending = true;
    let preview_id = uuid::Uuid::new_v4().to_string();
    let preview = staging::preview_root(&state).join(&preview_id);
    fs::create_dir_all(&preview).unwrap();
    let mut zip = zip::ZipWriter::new(fs::File::create(preview.join("fixture.zip")).unwrap());
    for (file, guid) in members {
        zip.start_file(*file, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&mods::plugin_blob(guid, "Primary", "2.0.0"))
            .unwrap();
    }
    zip.finish().unwrap();
    fs::write(
        preview.join(staging::ORIGIN_FILE),
        serde_json::to_vec(&staging::PreviewOriginRecord {
            repository_id: repo.id.clone(),
            release_tag: "v2.0.0".into(),
            prerelease: false,
        })
        .unwrap(),
    )
    .unwrap();
    trusted.config.repositories = vec![repo];
    let stage = root.join("data/stage/actual");
    let result = from_preview(
        &state,
        &trusted,
        &preview_id,
        "fixture",
        &selected,
        None,
        None,
        &stage,
    );
    if result.is_err() {
        assert!(!stage.exists());
    }
    assert!(preview.join("fixture.zip").is_file());
    assert!(fs::read_dir(&game).unwrap().next().is_none());
    result
}

#[tokio::test]
async fn split_repository_payload_binds_verified_volumes_to_the_exact_staged_zip() {
    use archive::release_download::{self, tests::*};
    use sha2::{Digest, Sha256};

    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    let game = root.join("game");
    fs::create_dir_all(&game).unwrap();
    let state = AppState::with_config(crate::config::ConfigService::for_test(root.join("data")));
    let mut trusted = trusted(&game, vec![]);
    let plugin = mods::plugin_blob("test-primary", "Primary", "2.0.0");
    let volumes = split_zip_parts(&[
        ("padding.txt", &vec![1u8; 150_000]),
        ("Primary.dll", &plugin),
    ]);
    let names = [
        "FixturePayload.z01",
        "FixturePayload.z02",
        "FixturePayload.zip",
    ];
    let digests: Vec<_> = volumes
        .iter()
        .map(|bytes| format!("sha256:{:x}", Sha256::digest(bytes)))
        .collect();
    let server = Server::new(volumes.clone(), false);
    let mut repo = crate::community::repository_from_coordinates(
        &Provider::Github,
        "Fixture",
        "fixture-repository",
        "",
        "",
    )
    .unwrap();
    repo.plugin_guid = "test-primary".into();
    repo.asset_rules = vec![AssetRule {
        rule_type: AssetRuleType::Exact,
        value: names[2].into(),
    }];
    let tag = "v2.0.0";
    let assets: Vec<_> = volumes
        .iter()
        .enumerate()
        .map(|(i, bytes)| ReleaseAsset {
            name: names[i].into(),
            size: bytes.len() as u64,
            download_count: None,
            download_url: format!("{}/{i}", server.url),
            digest: Some(digests[i].clone()),
        })
        .collect();
    repo.cached_releases.push(ReleaseInfo {
        release_id: None,
        tag_name: tag.into(),
        version: Some("2.0.0".into()),
        name: "Fixture".into(),
        body: String::new(),
        published_at: None,
        api_prerelease: Some(false),
        prerelease: false,
        draft: false,
        assets: assets.clone(),
    });
    let mut selected = candidate(vec![]);
    selected.source_id = Some(repo.id.clone());
    selected.tag = Some(tag.into());
    selected.verification_pending = true;
    trusted.config.repositories = vec![repo.clone()];

    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let cache = state.config.cache_dir();
    let scope = release_download::ReleaseCacheScope::for_mod(
        Provider::Github,
        &repo.username,
        &repo.repository,
        tag,
    );
    for hit in [false, true] {
        let download = release_download::download(
            &client,
            release_download::asset_set(&assets[2], &assets).unwrap(),
            &cache,
            Some(&scope),
            "split MOD",
            None,
            &NetworkConfig::default(),
            |url| {
                assert!(!hit, "verified split ZIP should reuse every local volume");
                reuse_tests::direct(url)
            },
        )
        .await
        .unwrap();
        let split_proof = download.split_proof().unwrap().unwrap();
        assert!(fs::metadata(download.path()).unwrap().len() > assets[2].size);
        let temporary_path = download.path().to_owned();
        let preview = staging::stage_downloaded_file(
            &state, download, names[2], &repo.id, "Fixture", tag, "", false,
        )
        .unwrap();
        assert!(!temporary_path.exists());
        let id = &preview.preview_id;
        let stage = root.join(format!("data/stage/{id}"));
        let prepare = |trusted: &TrustedInventory, proof| {
            from_preview(
                &state, trusted, id, "fixture", &selected, None, proof, &stage,
            )
        };

        assert_eq!(
            prepare(&trusted, None).err().unwrap(),
            "modChanges.targetChanged"
        );
        let mut stale = trusted.clone();
        stale.config.repositories[0].cached_releases[0].assets[0].digest = None;
        assert_eq!(
            prepare(&stale, Some(&split_proof)).err().unwrap(),
            "modChanges.targetChanged"
        );
        assert!(!stage.exists());

        let payload = prepare(&trusted, Some(&split_proof)).unwrap();
        assert!(payload.release_files.is_some());
        assert_eq!(payload.candidate.version.as_deref(), Some("2.0.0"));
        assert!(
            payload
                .candidate
                .provides
                .iter()
                .any(|plugin| plugin.guid == "test-primary")
        );

        // A copied stage must still contain the exact bytes that were formed
        // by the verified download, even if changes leave the length intact.
        let source = staging::preview_root(&state).join(id).join(names[2]);
        let mut replacement = fs::read(&source).unwrap();
        replacement[100] ^= 1;
        fs::write(source, replacement).unwrap();
        assert!(
            prepare(&trusted, Some(&split_proof))
                .err()
                .unwrap()
                .starts_with("download.checksumMismatch")
        );
    }
    assert!(fs::read_dir(&game).unwrap().next().is_none());
}

#[test]
fn repository_primary_guid_cannot_be_replaced_by_matching_filename_or_old_binding() {
    assert_eq!(
        repository_payload("test-primary", &[("Primary.dll", "test-unrelated")])
            .err()
            .unwrap(),
        "modChanges.targetChanged"
    );
}

#[test]
fn repository_primary_guid_must_be_unique_even_when_another_copy_is_disabled() {
    assert_eq!(
        repository_payload(
            "test-primary",
            &[
                ("Primary.dll", "test-primary"),
                ("Other.dll.disabled", "TEST-PRIMARY"),
            ],
        )
        .err()
        .unwrap(),
        "modChanges.ambiguousOwnership"
    );
}

#[test]
fn repository_primary_guid_uses_actual_metadata_case_insensitively() {
    let payload = repository_payload(
        "test-primary",
        &[
            ("Primary.dll", "test-unrelated"),
            ("Renamed.dll.disabled", "TEST-PRIMARY"),
        ],
    )
    .unwrap();
    assert!(!payload.candidate.verification_pending);
    assert_eq!(payload.candidate.provides.len(), 2);
    assert_eq!(payload.candidate.version.as_deref(), Some("2.0.0"));
}

#[test]
fn repository_without_a_declared_primary_keeps_actual_member_metadata() {
    let payload = repository_payload("", &[("Primary.dll", "test-unrelated")]).unwrap();
    assert_eq!(payload.candidate.provides[0].guid, "test-unrelated");
    assert!(!payload.candidate.verification_pending);
}
