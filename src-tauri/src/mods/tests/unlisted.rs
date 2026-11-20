use crate::models::{InstalledModStatusLabel as Label, ModRepositoryBinding};
use crate::mods::binding::rebind_installed_mods;
use crate::mods::test_support::{binding_repository, config, plain_mod, plugin_blob};

#[test]
fn a_scanned_mod_absent_from_the_catalog_is_unlisted_until_rebound() {
    let game = tempfile::tempdir().unwrap();
    let dll = game.path().join("BepInEx/plugins/Example.dll");
    std::fs::create_dir_all(dll.parent().unwrap()).unwrap();
    std::fs::write(&dll, plugin_blob("com.example.guid", "Example", "1.0.0")).unwrap();
    let empty = config(vec![]);
    let mut mods = super::scan_mods_with_manifests(game.path(), &empty, None);
    assert_eq!(
        mods[0].status, "unbound",
        "bindings and labels are distinct"
    );
    assert_eq!(mods[0].labels, [Label::Unlisted]);
    assert_eq!(
        serde_json::to_value(&mods[0]).unwrap()["labels"][0],
        "unlisted"
    );

    let repo = binding_repository();
    let id = repo.id.clone();
    let with_source = config(vec![repo]);
    let expected = super::scan_mods_with_manifests(game.path(), &with_source, None);
    std::fs::remove_file(dll).unwrap();
    rebind_installed_mods(&with_source, &mut mods);
    assert_eq!(mods[0].repository_id.as_deref(), Some(id.as_str()));
    assert_eq!(mods[0].labels, expected[0].labels);
    assert_eq!(mods[0].labels, [Label::Unknown]);
    rebind_installed_mods(&empty, &mut mods);
    assert_eq!(mods[0].labels, [Label::Unlisted]);
}

#[test]
fn listed_identity_evidence_survives_catalog_filters_and_companion_matching() {
    let base = binding_repository();
    let platform = if cfg!(target_os = "macos") {
        "windows"
    } else {
        "macos"
    };
    let mut cases = vec![];
    let mut disabled = base.clone();
    disabled.enabled = false;
    cases.push((disabled, "disabled"));
    let mut region = base.clone();
    region.region = Some("cn".into());
    cases.push((region, "region"));
    let mut platform_only = base.clone();
    platform_only.platform = Some(platform.into());
    cases.push((platform_only, "platform"));
    let mut other_version = base.clone();
    other_version.version_regex = r"^2\.".into();
    cases.push((other_version, "version regex"));
    let mut other_channel = base.clone();
    other_channel.channel = crate::models::Channel::Beta;
    cases.push((other_channel, "channel"));
    let mut pe = base.clone();
    pe.plugin_guid.clear();
    pe.version_regex = r"^2\.".into();
    cases.push((pe, "PE evidence"));
    let mut companion = base;
    companion.plugin_guid = "com.example.different-core".into();
    companion.bundled_plugin_guids = vec!["COM.EXAMPLE.GUID ".into()];
    cases.push((companion, "declared companion"));

    for (repo, case) in cases {
        let mut config = config(vec![repo]);
        config.region = "global".into();
        let mut mods = [plain_mod(
            "BepInEx/plugins/Example.dll",
            serde_json::json!({"guid":"com.example.guid", "product_name":"Example Product", "version":"1.0.0"}),
        )];
        rebind_installed_mods(&config, &mut mods);
        assert!(mods[0].repository_id.is_none(), "{case}");
        assert_eq!(mods[0].labels, [Label::Unbound], "{case}");
    }
}

#[test]
fn an_absent_entry_is_not_inferred_from_missing_guid_or_a_historical_online_install() {
    let empty = config(vec![]);
    let repo = binding_repository();
    let binding = ModRepositoryBinding::from_release(&repo, "v1.0.0").unwrap();
    let mut mods = [
        plain_mod("guidless.dll", serde_json::json!({"version":"1.0.0"})),
        plain_mod(
            "old-raw.dll",
            serde_json::json!({"guid":"com.example.old-raw", "package_id":"old-repo"}),
        ),
        plain_mod(
            "old-release.dll",
            serde_json::json!({"guid":"com.example.old-release", "repository_binding":binding}),
        ),
        plain_mod(
            "manual.dll",
            serde_json::json!({"guid":"com.example.manual", "package_id":"manual-x", "manual_install":true}),
        ),
        plain_mod(
            "dependency.dll",
            serde_json::json!({"is_dependency":true, "enabled":false}),
        ),
    ];
    rebind_installed_mods(&empty, &mut mods);
    assert_eq!(mods[0].labels, [Label::Unbound]);
    assert_eq!(mods[1].labels, [Label::Unbound]);
    assert_eq!(mods[2].labels, [Label::BoundRepositoryMissing]);
    assert_eq!(mods[3].labels, [Label::Unlisted]);
    assert_eq!(mods[4].labels, [Label::Dependency, Label::Disabled]);
}
