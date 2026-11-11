use crate::config::ConfigService;
use crate::config::availability::REGION_CHINESE_MAINLAND;
use crate::config::availability::REGION_INTERNATIONAL;
use crate::config::availability::repository_source_matches_region;
use crate::config::defaults::default_config;
use crate::config::identity::mod_repository_from_export;
use crate::config::migrations::contains_legacy_repository_proxy_fields;
use crate::config::sources::builtin_repository_sources;
use crate::config::sources::merge_builtin_repository_sources;
use crate::config::sources::synchronize_builtin_repository_sources;
use crate::models::*;
use std::fs;
use std::path::Path;
use uuid::Uuid;

use super::REGION_REPOSITORY;

#[test]
fn config_reads_preserve_remote_children_and_load_later_disk_changes() {
    let text = r#"
[[repository_sources]]
id = "china"
nickname = "China"
url = "mods.toml"
region = "chinese_mainland"
[repository_sources.nickname_i18n]
en = "China Mods"
[[repository_sources]]
id = "international"
nickname = "International"
url = "mods.toml"
region = "international"
[repository_sources.nickname_i18n]
en = "International Mods"
[[repository_sources]]
id = "shared"
nickname = "Shared"
url = "shared.toml"
"#;
    let service = ConfigService::for_test(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-config-load")
            .join(Uuid::new_v4().to_string()),
    );
    let mut config = default_config();
    let root = config.repository_sources[0].clone();
    let location = crate::repository_sources::SourceLocation::parse(&root.url).unwrap();
    let document =
        crate::repository_sources::parse_document(text, &location, &Default::default()).unwrap();
    for source in document.repository_sources {
        config.repository_sources.push(RepositorySource {
            network_plan: Default::default(),
            id: source.id,
            nickname: source.nickname,
            nickname_i18n: source.nickname_i18n,
            url: source.url,
            region: source.region,
            builtin: true,
            parent_id: Some(root.id.clone()),
        });
    }
    service.save(&config).unwrap();
    let loaded = service.load().unwrap();
    assert_eq!(loaded.repository_sources, config.repository_sources);
    for (region, expected_ids) in [
        (REGION_CHINESE_MAINLAND, vec!["china", "shared"]),
        (REGION_INTERNATIONAL, vec!["international", "shared"]),
    ] {
        let sources = loaded
            .repository_sources
            .iter()
            .filter(|source| {
                source.parent_id.is_some() && repository_source_matches_region(source, region)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            sources
                .iter()
                .map(|source| source.id.as_str())
                .collect::<Vec<_>>(),
            expected_ids
        );
        assert!(
            sources
                .iter()
                .all(|source| source.builtin
                    && source.parent_id.as_deref() == Some(root.id.as_str()))
        );
        assert_eq!(
            sources[0].nickname_i18n["en"],
            if region == REGION_CHINESE_MAINLAND {
                "China Mods"
            } else {
                "International Mods"
            }
        );
    }
    config.repository_sources[1].nickname = "Remote changed name".into();
    config.ui_zoom = 1.8;
    service.save(&config).unwrap();
    let reloaded = service.load().unwrap();
    assert_eq!(reloaded.repository_sources, config.repository_sources);
    assert_eq!(reloaded.ui_zoom, 1.8);
    fs::remove_dir_all(service.root).unwrap();
}

#[test]
fn official_index_follows_region_and_preserves_custom_sources() {
    let custom = RepositorySource {
        network_plan: Default::default(),
        id: "regional-custom".into(),
        nickname: "Custom".into(),
        nickname_i18n: Default::default(),
        url: "https://example.com/custom.toml".into(),
        region: None,
        builtin: false,
        parent_id: None,
    };
    let mut config = default_config();
    config.repository_sources.push(custom.clone());
    for (region, expected) in [
        (
            REGION_CHINESE_MAINLAND,
            crate::models::DataProvider::Gitee.document("mods.toml"),
        ),
        (
            REGION_INTERNATIONAL,
            crate::models::DataProvider::Github.document("mods.toml"),
        ),
        (
            REGION_CHINESE_MAINLAND,
            crate::models::DataProvider::Gitee.document("mods.toml"),
        ),
    ] {
        merge_builtin_repository_sources(
            &mut config,
            builtin_repository_sources(DataProvider::for_region(region)),
        );
        assert_eq!(config.repository_sources.len(), 2);
        assert_eq!(
            config.repository_sources[0].id,
            crate::repository_sources::DEFAULT_INDEX_ID
        );
        assert_eq!(config.repository_sources[0].url, expected);
        assert!(config.repository_sources[0].builtin);
        assert_eq!(config.repository_sources[1], custom);
        assert!(!merge_builtin_repository_sources(
            &mut config,
            builtin_repository_sources(DataProvider::for_region(region))
        ));
    }
}

#[test]
fn builtin_source_migration_removes_obsolete_trees_and_preserves_custom_sources() {
    let mut config = default_config();
    let old_root = RepositorySource {
        network_plan: Default::default(),
        id: "old-builtin-root".into(),
        nickname: "Old built-in source".into(),
        nickname_i18n: Default::default(),
        url: "https://example.com/old.toml".into(),
        region: None,
        builtin: true,
        parent_id: None,
    };
    let mut new_root = old_root.clone();
    new_root.id = "new-builtin-root".into();
    new_root.url = "new.toml".into();
    let mut child = old_root.clone();
    child.id = "old-child".into();
    child.parent_id = Some(old_root.id.clone());
    let mut grandchild = child.clone();
    grandchild.id = "old-grandchild".into();
    grandchild.parent_id = Some(child.id.clone());
    grandchild.builtin = false;
    let mut custom = old_root.clone();
    custom.id = "custom-source".into();
    custom.url = "https://example.com/custom.toml".into();
    custom.builtin = false;
    config.repository_sources = vec![old_root, child, grandchild, custom.clone()];
    let export: ModRepositoryExport = toml::from_str(
        r#"
name = "Custom MOD"
name_i18n.zh-CN = "Custom MOD"
group_i18n.zh-CN = ""
note_i18n.zh-CN = ""
provider = "github"
username = "example"
repository = "mod"
channel = "all"
install_target = "BepInEx/plugins"
plugin_guid = "com.example.mod"
enabled = true
[[asset_rules]]
type = "exact"
value = "MOD.dll"
"#,
    )
    .unwrap();
    let mut old_fallback = mod_repository_from_export(&export, "fallback".into(), true);
    old_fallback.source_id = None;
    let mut old_source_repo = mod_repository_from_export(&export, "old-repo".into(), false);
    old_source_repo.source_id = Some("old-grandchild".into());
    let mut custom_repo = mod_repository_from_export(&export, "custom-repo".into(), false);
    custom_repo.source_id = Some(custom.id.clone());
    config.repositories = vec![old_fallback, old_source_repo, custom_repo];
    assert!(merge_builtin_repository_sources(
        &mut config,
        vec![new_root.clone()]
    ));
    assert_eq!(config.repository_sources.len(), 2);
    assert_eq!(config.repository_sources[1], custom);
    assert_eq!(config.repositories.len(), 1);
    assert_eq!(config.repositories[0].id, "custom-repo");
    assert!(!merge_builtin_repository_sources(
        &mut config,
        vec![new_root]
    ));
}

#[test]
fn all_providers_select_the_official_root_and_preserve_custom_sources() {
    for provider in [
        DataProvider::Github,
        DataProvider::Gitee,
        DataProvider::Builtin,
    ] {
        let mut config = default_config();
        config.repository_sources.push(RepositorySource {
            network_plan: Default::default(),
            id: "custom".into(),
            nickname: "Custom".into(),
            nickname_i18n: Default::default(),
            url: "https://example.com/custom.toml".into(),
            region: None,
            builtin: false,
            parent_id: None,
        });
        config.data_provider = provider;
        synchronize_builtin_repository_sources(&mut config);
        let root = &config.repository_sources[0];
        assert_eq!(root.id, crate::repository_sources::DEFAULT_INDEX_ID);
        assert_eq!(root.url, provider.document("mods.toml"));
        assert!(root.builtin && root.parent_id.is_none());
        assert_eq!(config.repository_sources[1].id, "custom");
        assert!(!synchronize_builtin_repository_sources(&mut config));
    }
}

#[test]
fn first_load_synchronizes_builtin_sources_even_when_removing_legacy_proxy_fields() {
    for has_legacy_proxy_fields in [false, true] {
        let service = ConfigService {
            root: Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target/test-config-load")
                .join(Uuid::new_v4().to_string()),
            cache: std::sync::Arc::new(std::sync::Mutex::new(None)),
            proxies: Default::default(),
        };
        let mut config = default_config();
        let mut expected_sources = config.repository_sources.clone();
        let old_root = RepositorySource {
            network_plan: Default::default(),
            id: "obsolete-builtin".into(),
            nickname: "内置仓库源".into(),
            nickname_i18n: Default::default(),
            url: "https://example.com/obsolete.toml".into(),
            region: None,
            builtin: true,
            parent_id: None,
        };
        let mut old_child = old_root.clone();
        old_child.id = "obsolete-child".into();
        old_child.parent_id = Some(old_root.id.clone());
        // 旧配置只有单语昵称：加载后应自动回填 zh-CN（见 normalize_configured_regions）
        let custom = RepositorySource {
            network_plan: Default::default(),
            id: "custom-source".into(),
            nickname: "Custom source".into(),
            nickname_i18n: Default::default(),
            url: "https://example.com/custom.toml".into(),
            region: None,
            builtin: false,
            parent_id: None,
        };
        let mut expected_custom = custom.clone();
        expected_custom
            .nickname_i18n
            .insert("zh-CN".into(), "Custom source".into());
        expected_sources.push(expected_custom);
        config.repository_sources = vec![old_root.clone(), old_child.clone(), custom.clone()];
        let location = crate::repository_sources::SourceLocation::parse("fixture.toml").unwrap();
        let document = crate::repository_sources::parse_document(
            REGION_REPOSITORY,
            &location,
            &Default::default(),
        )
        .unwrap();
        config.repositories = [
            ("obsolete-mod", Some(old_root.id), true),
            ("obsolete-child-mod", Some(old_child.id), true),
            ("obsolete-fallback-mod", None, true),
            ("current-mod", Some(expected_sources[0].id.clone()), true),
            ("custom-mod", Some(custom.id), false),
        ]
        .into_iter()
        .map(|(id, source_id, builtin)| {
            let mut repository =
                mod_repository_from_export(&document.repositories[0], id.into(), builtin);
            repository.source_id = source_id;
            repository
        })
        .collect();
        let mut fixture = toml::Value::try_from(&config).unwrap();
        if has_legacy_proxy_fields {
            fixture["repositories"][0].as_table_mut().unwrap().insert(
                "api_proxy_mode".into(),
                toml::Value::String("direct".into()),
            );
        }
        fs::create_dir_all(&service.root).unwrap();
        fs::write(
            service.config_path(),
            toml::to_string_pretty(&fixture).unwrap(),
        )
        .unwrap();

        let loaded = service.load().unwrap();
        assert_eq!(loaded.repository_sources, expected_sources);
        assert_eq!(
            loaded
                .repositories
                .iter()
                .map(|repository| repository.id.as_str())
                .collect::<Vec<_>>(),
            vec!["current-mod", "custom-mod"]
        );
        let saved = fs::read_to_string(service.config_path()).unwrap();
        assert!(!contains_legacy_repository_proxy_fields(&saved));
        assert!(!saved.contains("obsolete-"));
        let reloaded = service.load().unwrap();
        assert_eq!(reloaded.repository_sources, expected_sources);
        assert_eq!(fs::read_to_string(service.config_path()).unwrap(), saved);
    }
}
