use crate::config::ConfigService;
use crate::config::availability::REGION_CHINESE_MAINLAND;
use crate::config::availability::REGION_INTERNATIONAL;
use crate::config::availability::current_user_region;
use crate::config::defaults::default_config;
use crate::config::identity::mod_repository_from_export;
use crate::models::*;

use super::REGION_REPOSITORY;

#[test]
fn bundle_round_trip_is_toml() {
    let service = ConfigService {
        root: std::env::temp_dir(),
        cache: std::sync::Arc::new(std::sync::Mutex::new(None)),
        proxies: Default::default(),
    };
    let config = default_config();
    let text = service.export_bundle(&config, None).unwrap();
    assert!(text.contains("format = \"oc2-mod-repository-bundle\""));
    let bundle = service.validate_bundle(&text).unwrap();
    assert_eq!(bundle.repositories.len(), 0);
    assert_eq!(bundle.repository_sources, config.repository_sources);
}

#[test]
fn export_includes_builtin_disabled_and_other_region_repositories_with_their_sources() {
    let service = ConfigService {
        root: std::env::temp_dir(),
        cache: std::sync::Arc::new(std::sync::Mutex::new(None)),
        proxies: Default::default(),
    };
    let location = crate::repository_sources::SourceLocation::parse("index.toml").unwrap();
    let template = crate::repository_sources::parse_document(
        REGION_REPOSITORY,
        &location,
        &Default::default(),
    )
    .unwrap()
    .repositories
    .remove(0);
    let mut config = default_config();
    let builtin_source = config.repository_sources[0].id.clone();
    let other_region = if current_user_region() == REGION_CHINESE_MAINLAND {
        REGION_INTERNATIONAL
    } else {
        REGION_CHINESE_MAINLAND
    };
    config.repository_sources.push(RepositorySource {
        note_i18n: Default::default(),
        network_plan: Default::default(),
        id: "other-region-child".into(),
        nickname: "Child source".into(),
        nickname_i18n: Default::default(),
        url: "https://example.com/child.toml".into(),
        region: Some(other_region.into()),
        builtin: true,
        parent_id: Some(builtin_source.clone()),
    });
    let mut builtin = mod_repository_from_export(&template, "builtin".into(), true);
    builtin.name = "Built-in MOD".into();
    builtin
        .name_i18n
        .insert("zh-CN".into(), builtin.name.clone());
    builtin.source_id = Some(builtin_source);
    let mut disabled = mod_repository_from_export(&template, "disabled".into(), true);
    disabled.name = "Disabled other-region MOD".into();
    disabled
        .name_i18n
        .insert("zh-CN".into(), disabled.name.clone());
    disabled.region = Some(other_region.into());
    disabled.source_id = Some("other-region-child".into());
    disabled.enabled = false;
    let mut detached = mod_repository_from_export(&template, "detached".into(), false);
    detached.name = "Detached MOD".into();
    detached
        .name_i18n
        .insert("zh-CN".into(), detached.name.clone());
    detached.repository = "detached".into();
    detached.source_id = Some("unavailable-source".into());
    config.repositories = vec![builtin, disabled, detached];

    let text = service.export_bundle(&config, None).unwrap();
    let bundle = service.validate_bundle(&text).unwrap();
    assert_eq!(
        bundle
            .repositories
            .iter()
            .map(|repo| repo.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Built-in MOD", "Disabled other-region MOD", "Detached MOD"]
    );
    assert_eq!(bundle.repository_sources, config.repository_sources);
    assert!(!bundle.repositories[1].enabled);
    assert_eq!(bundle.repositories[1].region.as_deref(), Some(other_region));
    assert_eq!(
        bundle.repositories[1].source_id.as_deref(),
        Some("other-region-child")
    );
    let mut imported = default_config();
    service.import_bundle(&mut imported, &bundle, "overwrite");
    assert_eq!(imported.repositories.len(), 3);

    let text = service
        .export_bundle(&config, Some(vec!["builtin".into()]))
        .unwrap();
    let selected = service.validate_bundle(&text).unwrap();
    assert_eq!(selected.repositories.len(), 1);
    assert_eq!(selected.repositories[0].name, "Built-in MOD");
}

#[test]
fn repository_bundle_export_follows_editor_order_and_roundtrips() {
    let service = ConfigService {
        root: std::env::temp_dir(),
        cache: std::sync::Arc::new(std::sync::Mutex::new(None)),
        proxies: Default::default(),
    };
    let location = crate::repository_sources::SourceLocation::parse("index.toml").unwrap();
    let mut release = crate::repository_sources::parse_document(
        REGION_REPOSITORY,
        &location,
        &Default::default(),
    )
    .unwrap()
    .repositories
    .remove(0);
    release.name_i18n.insert("en".into(), "Release MOD".into());
    release.group_i18n.insert("zh-CN".into(), "测试分组".into());
    release.note = "多行备注\n[[repositories]]\n\"quoted\" \\ path".into();
    release.note_i18n.insert("en".into(), release.note.clone());
    release
        .note_i18n
        .insert("zh-CN".into(), release.note.clone());
    release.group = "测试分组".into();
    release.version_regex = r"^1\.\d+\.\d+$".into();
    release.download_threads = 16;
    release.network_plan =
        toml::from_str("international = { release = { download_threads = 4 } }").unwrap();
    let mut raw = release.clone();
    raw.name = "Raw MOD".into();
    raw.name_i18n.insert("zh-CN".into(), "Raw MOD".into());
    raw.download_mode = ModDownloadMode::RepositoryFile;
    raw.branch = "main".into();
    raw.file_path = "bin/Raw MOD.dll".into();
    raw.channel = Channel::All;
    raw.version_regex = r"^1\.2\.\d+(\.\d+)?$".into();
    raw.asset_rules.clear();
    let source = RepositorySource {
        note_i18n: Default::default(),
        network_plan: Default::default(),
        id: "export-source".into(),
        nickname: "自定义源".into(),
        nickname_i18n: std::collections::BTreeMap::from([("en".into(), "Source".into())]),
        url: "https://example.com/source.toml".into(),
        region: None,
        builtin: false,
        parent_id: None,
    };
    release.source_id = Some(source.id.clone());
    raw.source_id = Some(source.id.clone());
    let mut config = default_config();
    config.repository_sources.push(source);
    config.repositories = vec![
        mod_repository_from_export(&release, "release".into(), false),
        mod_repository_from_export(&raw, "raw".into(), false),
    ];
    // The editor keeps drafts for both modes; Release starts with main.
    config.repositories[0].branch = "main".into();
    config.repositories[0].file_path = "bin/UnusedDraft.dll".into();
    let text = service.export_bundle(&config, None).unwrap();
    assert!(!text.contains("\ndownload_threads ="));
    let restored = service.validate_bundle(&text).unwrap();
    assert_eq!(
        serde_json::to_value(&restored.repositories).unwrap(),
        serde_json::to_value([&release, &raw]).unwrap()
    );
    assert_eq!(restored.repository_sources, config.repository_sources);
    assert_eq!(
        restored.repository_sources.last().unwrap().id,
        "export-source"
    );
    let entries = text
        .split("\n[[repositories]]\n# 仓库基本信息\n")
        .skip(1)
        .collect::<Vec<_>>();
    for (entry, keys) in [
        (
            entries[0],
            vec![
                "name =",
                "name_i18n.",
                "provider =",
                "type =",
                "network_plan =",
                "group =",
                "group_i18n.",
                "username =",
                "repository =",
                "enabled =",
                "download_mode =",
                "channel =",
                "max_release_count =",
                "asset_rules =",
                "plugin_guid =",
                "version_regex =",
                "install_target =",
                "note =",
                "note_i18n.",
            ],
        ),
        (
            entries[1],
            vec![
                "download_mode =",
                "branch =",
                "file_path =",
                "plugin_guid =",
                "local_product_name =",
                "local_original_filename =",
                "version_regex =",
                "install_target =",
                "note =",
                "note_i18n.",
            ],
        ),
    ] {
        let positions = keys
            .iter()
            .map(|key| entry.find(key).unwrap_or_else(|| panic!("missing {key}")))
            .collect::<Vec<_>>();
        assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
    }
    assert!(!entries[1].contains("\nchannel ="));
    assert!(!entries[1].contains("\nasset_rules ="));
    let exported: toml::Value = toml::from_str(&text).unwrap();
    for repository in exported["repositories"].as_array().unwrap() {
        assert!(repository.get("url").is_none());
        assert_eq!(repository["username"].as_str(), Some("example"));
        assert_eq!(repository["repository"].as_str(), Some("mod"));
    }
    assert_eq!(
        exported["repository_sources"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["url"]
            .as_str(),
        Some("https://example.com/source.toml")
    );
    let parsed =
        crate::repository_sources::parse_document(&text, &location, &Default::default()).unwrap();
    assert_eq!(parsed.repositories.len(), 2);
    assert_eq!(parsed.repositories[0].note, release.note);
    service.import_bundle(&mut config, &restored, "overwrite");
    assert_eq!(
        config
            .repositories
            .iter()
            .map(|repo| repo.id.as_str())
            .collect::<Vec<_>>(),
        ["release", "raw"]
    );
}

#[test]
fn source_notes_survive_parsing_bundle_storage_import_and_public_projection() {
    let service =
        ConfigService::for_test(std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()));
    let location = crate::repository_sources::SourceLocation::parse("index.toml").unwrap();
    let document=crate::repository_sources::parse_document("[[repository_sources]]\nid='custom-note'\nnickname='Custom'\nurl='https://example.com/source.toml'\nregion='chinese_mainland'\n[repository_sources.note_i18n]\nen='English source note'\nzh-CN='源说明'\n",&location,&Default::default()).unwrap();
    let declaration = &document.repository_sources[0];
    let mut config = default_config();
    let source = RepositorySource {
        network_plan: Default::default(),
        id: declaration.id.clone(),
        nickname: declaration.nickname.clone(),
        nickname_i18n: declaration.nickname_i18n.clone(),
        note_i18n: declaration.note_i18n.clone(),
        url: declaration.url.clone(),
        region: declaration.region.clone(),
        builtin: false,
        parent_id: None,
    };
    config.repository_sources.push(source.clone());
    let restored: AppConfig = toml::from_str(&toml::to_string(&config).unwrap()).unwrap();
    assert_eq!(
        restored.repository_sources.last().unwrap().note_i18n,
        source.note_i18n
    );
    assert_eq!(
        service
            .public_config(&restored)
            .repository_sources
            .last()
            .unwrap()
            .note_i18n,
        source.note_i18n
    );
    let bundle = service
        .validate_bundle(
            &service
                .export_bundle(&config, None)
                .unwrap()
                .replace("region = \"cn\"", "region = \"chinese_mainland\""),
        )
        .unwrap();
    assert_eq!(
        bundle.repository_sources.last().unwrap().note_i18n,
        source.note_i18n
    );
    assert_eq!(
        bundle.repository_sources.last().unwrap().region.as_deref(),
        Some("cn")
    );
    let mut imported = default_config();
    service.import_bundle(&mut imported, &bundle, "overwrite");
    assert_eq!(
        imported.repository_sources.last().unwrap().note_i18n,
        source.note_i18n
    );
}
