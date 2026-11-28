use super::*;
use crate::models::{
    AppConfig, AssetRule, AssetRuleType, Channel, ModRepositoryBinding, Provider, ReleaseAsset,
    ReleaseInfo, ReplacedModRepository,
};
use crate::mods::{
    binding::rebind_installed_mods,
    test_support::{config, requires},
};
use std::fs;
use std::path::Path;

const PATH: &str = "BepInEx/plugins/OC2SkipLoading.dll";
const GUID: &str = "com.ch3ngyz.plugin.skiploading";
const REPOSITORY: &str = "Overcooked-2-SkipLoading";
const OLD_ID: &str = "257c4fa1-3ee1-4836-8267-f8b86b6eaf55";
const NEXT_ID: &str = "8870c366-2319-416d-9c00-0786e68f8962";

fn candidate(tag: &str, asset_name: &str) -> ReleaseInfo {
    ReleaseInfo {
        tag_name: tag.into(),
        version: None,
        assets: vec![ReleaseAsset {
            name: asset_name.into(),
            size: 1,
            download_count: None,
            download_url: "https://example.invalid/fixture".into(),
            digest: None,
        }],
        ..high_cached_release()
    }
}

fn setup(root: &Path) -> (AppConfig, crate::archive::InstallManifest) {
    let dll = root.join(PATH);
    fs::create_dir_all(dll.parent().unwrap()).unwrap();
    fs::write(&dll, plugin_blob(GUID, "Skip Loading", "1.0.0")).unwrap();
    let old = ModRepositoryBinding {
        provider: Provider::Gitee,
        owner: "ch3ngyz".into(),
        repository: REPOSITORY.into(),
        tag: "v1.0.0".into(),
        channel: Channel::All,
    };
    let mut repo = binding_repository();
    repo.id = NEXT_ID.into();
    repo.provider = Provider::Github;
    repo.username = "CH3NGY2".into();
    repo.repository = REPOSITORY.into();
    repo.channel = Channel::All;
    repo.plugin_guid = GUID.into();
    repo.local_product_name.clear();
    repo.local_original_filename.clear();
    repo.version_regex = r"^1\.0\.\d+$".into();
    repo.asset_rules = vec![AssetRule {
        rule_type: AssetRuleType::Regex,
        value: r"^Overcooked-2-SkipLoading-v\d+\.\d+\.\d+\.zip$".into(),
    }];
    repo.cached_releases = vec![
        candidate("v1.3.0-beta.1", "Overcooked-2-SkipLoading-v1.3.0.zip"),
        candidate("v1.2.0", "Overcooked-2-SkipLoading-v1.2.0.zip"),
    ];
    let mut config = config(vec![repo]);
    let successor = &mut config.repositories[0];
    successor.replaces = vec![ReplacedModRepository {
        provider: old.provider.clone(),
        username: old.owner.clone(),
        repository: old.repository.clone(),
        channel: old.channel,
    }];
    successor.replaces_revision = Some(1);
    let mut manifest = crate::archive::new_manifest(
        OLD_ID,
        &old.tag,
        Default::default(),
        Default::default(),
        vec![PATH.into()],
        vec![requires("com.example.old-dependency", ">=1.0.0")],
    );
    manifest.repository_binding = Some(old);
    crate::archive::write_manifest(&root.join("manifests"), &manifest).unwrap();
    (config, manifest)
}

fn scan(root: &Path, config: &AppConfig) -> Vec<crate::models::InstalledMod> {
    scan_mods_with_manifests(root, config, Some(&root.join("manifests")))
}

#[test]
fn matching_guid_and_version_offer_replacement_without_rewriting_the_old_record() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let (mut config, manifest) = setup(root);
    let manifest_path = root.join(format!("manifests/{OLD_ID}.toml"));
    let original = (
        fs::read(&manifest_path).unwrap(),
        fs::read(root.join(PATH)).unwrap(),
    );
    let mut installed = scan(root, &config);
    for clear_declaration in [false, true] {
        if clear_declaration {
            config.repositories[0].clear_replacements();
            rebind_installed_mods(&config, &mut installed);
            assert_eq!(
                scan(root, &config)[0].replacement_repository_id.as_deref(),
                Some(NEXT_ID)
            );
        }
        let [item] = installed.as_slice() else {
            panic!("expected one DLL")
        };
        assert_eq!(item.replacement_repository_id.as_deref(), Some(NEXT_ID));
        assert_eq!(item.package_id.as_deref(), Some(OLD_ID));
        assert_eq!(item.repository_binding, manifest.repository_binding);
        assert_eq!(item.relations, manifest.relations);
        assert!(item.repository_id.is_none() && item.repository_ids.is_empty());
        assert!(item.update_available);
        let status = crate::mods::status::collect_update_status(&config, &installed);
        assert_eq!(status[0].state, "update_available");
        assert_eq!(status[0].remote_version.as_deref(), Some("v1.2.0"));
    }

    assert_eq!(
        original,
        (
            fs::read(manifest_path).unwrap(),
            fs::read(root.join(PATH)).unwrap()
        )
    );
}

#[test]
fn replacement_requires_one_available_repository_with_matching_guid_and_version() {
    let dir = tempfile::tempdir().unwrap();
    let (config, _) = setup(dir.path());
    let mut old = binding_repository();
    old.repository = REPOSITORY.into();
    old.username = "CH3NGYZ".into();
    old.last_error = Some("temporary upstream timeout".into());
    old.enabled = false;
    let mut original_still_exists = config.clone();
    original_still_exists.repositories.push(old);

    let mut second = config.repositories[0].clone();
    second.id = "ambiguous-successor".into();
    second.username = "another-owner".into();
    second.version_regex = r"^1\.\d+\.0$".into();
    let mut ambiguous = config.clone();
    ambiguous.repositories.push(second);

    let mut disabled_competitor = ambiguous.clone();
    disabled_competitor.repositories[1].enabled = false;
    let mut other_version = ambiguous.clone();
    other_version.repositories[1].version_regex = r"^2\.".into();

    let mut wrong_guid = config.clone();
    wrong_guid.repositories[0].plugin_guid = "com.example.wrong".into();
    let mut wrong_version = config.clone();
    wrong_version.repositories[0].version_regex = r"^2\.".into();
    let mut disabled = config;
    disabled.repositories[0].enabled = false;

    for (name, variant, successor) in [
        ("old source still configured", original_still_exists, None),
        ("overlapping version patterns", ambiguous, None),
        ("disabled competitor", disabled_competitor, Some(NEXT_ID)),
        (
            "competitor for another version",
            other_version,
            Some(NEXT_ID),
        ),
        ("incorrect GUID", wrong_guid, None),
        ("incorrect version", wrong_version, None),
        ("disabled replacement", disabled, None),
    ] {
        let installed = scan(dir.path(), &variant);
        assert_eq!(
            installed[0].replacement_repository_id.as_deref(),
            successor,
            "{name}"
        );
        assert_eq!(installed[0].update_available, successor.is_some(), "{name}");
    }
}

#[test]
fn replacement_prefers_the_installed_line_when_the_destination_offers_it() {
    let dir = tempfile::tempdir().unwrap();
    let (mut config, _) = setup(dir.path());
    for (channel, expected_tag) in [(Channel::All, "v1.2.0"), (Channel::Beta, "v1.3.0-beta.1")] {
        config.repositories[0].channel = channel;
        let installed = scan(dir.path(), &config);
        assert_eq!(
            installed[0].replacement_repository_id.as_deref(),
            Some(NEXT_ID)
        );
        assert_eq!(installed[0].prerelease, false);
        let status = crate::mods::status::collect_update_status(&config, &installed);
        assert_eq!(status[0].state, "update_available");
        assert_eq!(status[0].remote_version.as_deref(), Some(expected_tag));
    }
}

#[test]
fn replacement_requires_a_single_installed_dll_and_online_record() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let (config, manifest) = setup(root);
    for duplicate in [
        format!("{PATH}.disabled"),
        "BepInEx/plugins/another-copy.dll.disabled".into(),
    ] {
        fs::copy(root.join(PATH), root.join(&duplicate)).unwrap();
        assert!(
            scan(root, &config)
                .iter()
                .all(|item| item.replacement_repository_id.is_none())
        );
        fs::remove_file(root.join(duplicate)).unwrap();
    }
    let mut multi_file = manifest.clone();
    multi_file.files.push("BepInEx/plugins/helper.dll".into());
    crate::archive::write_manifest(&root.join("manifests"), &multi_file).unwrap();
    assert!(scan(root, &config)[0].replacement_repository_id.is_none());
    let mut bad_tag = manifest.clone();
    bad_tag.release_tag = "v0.1.0".into();
    crate::archive::write_manifest(&root.join("manifests"), &bad_tag).unwrap();
    assert!(scan(root, &config)[0].replacement_repository_id.is_none());
    let mut manual = manifest;
    manual.manual_install = Some(true);
    crate::archive::write_manifest(&root.join("manifests"), &manual).unwrap();
    assert!(scan(root, &config)[0].replacement_repository_id.is_none());
}

#[test]
fn replacement_only_offers_a_strictly_newer_default_zip_on_the_same_line() {
    let dir = tempfile::tempdir().unwrap();
    let (mut config, _) = setup(dir.path());
    for (tag, asset, pin, update) in [
        ("v1.0.0", "Overcooked-2-SkipLoading-v1.0.0.zip", None, false),
        ("v0.9.0", "Overcooked-2-SkipLoading-v0.9.0.zip", None, false),
        (
            "nightly",
            "Overcooked-2-SkipLoading-v1.2.0.zip",
            None,
            false,
        ),
        ("v2.0.0", "OC2SkipLoading.dll", None, false),
        (
            "v2.0.0",
            "Overcooked-2-SkipLoading-v2.0.0.zip",
            Some("missing"),
            false,
        ),
        (
            "v2.0.0",
            "Overcooked-2-SkipLoading-v2.0.0.zip",
            Some("v2.0.0"),
            true,
        ),
    ] {
        let repo = &mut config.repositories[0];
        repo.cached_releases = vec![
            candidate(tag, asset),
            candidate("v1.2.0", "Overcooked-2-SkipLoading-v1.2.0.zip"),
        ];
        repo.pinned_version = pin.map(str::to_owned);
        let installed = scan(dir.path(), &config);
        assert_eq!(
            installed[0].replacement_repository_id.as_deref(),
            Some(NEXT_ID)
        );
        assert_eq!(
            installed[0].update_available, update,
            "{tag} / {asset} / {pin:?}"
        );
        let status = crate::mods::status::collect_update_status(&config, &installed);
        assert_eq!(status[0].state == "update_available", update);
    }
}
