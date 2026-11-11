use crate::config::ConfigService;
use crate::config::availability::current_user_region;
use crate::config::availability::repository_source_matches_region;
use crate::config::defaults::CONFIG_SCHEMA_VERSION;
use crate::config::defaults::default_config;
use crate::config::defaults::default_ui_zoom;
use crate::config::migrations::migrate_schema;
use crate::config::sources::builtin_repository_sources;
use crate::models::*;
use std::fs;
use std::path::Path;
use uuid::Uuid;

#[test]
fn ui_zoom_defaults_to_125_and_preserves_stored_choices() {
    assert_eq!(default_ui_zoom(), 1.25);
    assert_eq!(default_config().ui_zoom, 1.25);
    let mut value = serde_json::to_value(default_config()).unwrap();
    value.as_object_mut().unwrap().remove("ui_zoom");
    let missing: AppConfig = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(missing.ui_zoom, 1.25);
    for zoom in [1.0, 1.25, 1.5, 1.75, 2.0] {
        value["ui_zoom"] = serde_json::json!(zoom);
        let stored: AppConfig = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(stored.ui_zoom, zoom);
    }
}

#[test]
fn default_config_follows_the_remote_index() {
    let config = default_config();
    assert!(config.repositories.is_empty());
    assert_eq!(config.repository_sources.len(), 1);
    let root = &config.repository_sources[0];
    assert_eq!(root.id, crate::repository_sources::DEFAULT_INDEX_ID);
    assert_eq!(
        root.url,
        DataProvider::Github.document("mods.toml"),
        "默认读远端索引，内置快照只是读不到时的回退"
    );
    assert!(matches!(
        crate::repository_sources::SourceLocation::parse(&root.url).unwrap(),
        crate::repository_sources::SourceLocation::Remote(_)
    ));
    assert_eq!(config.data_provider, DataProvider::Github);
    // 内置地图索引只有 release-tags 源：条目来自各 Release note，离线解析
    // 不出地图（首启/断网地图页为空，联网刷新后才有）。
    assert!(
        config.map_repositories.is_empty(),
        "内置地图索引没有内联条目，离线不应凭空生成地图"
    );
    assert!(config.repository_sources.iter().all(|source| source.builtin
        && source.parent_id.is_none()
        && repository_source_matches_region(source, current_user_region())));
    let serialized = toml::to_string(&config).unwrap();
    assert!(!serialized.contains("\n[proxies"));
}

#[test]
fn upgrading_old_source_switches_drops_them_and_uses_the_session_provider() {
    for schema in [1, 2] {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-config-load")
            .join(Uuid::new_v4().to_string());
        let service = ConfigService::for_test(root.clone());
        service.select_data_provider(DataProvider::Gitee);
        let mut old = default_config();
        old.schema_version = schema;
        old.repository_sources = builtin_repository_sources(DataProvider::Builtin);
        let mut old = toml::Value::try_from(&old).unwrap();
        for field in [
            "builtin_sources_only",
            "builtin_map_sources_only",
            "builtin_proxies_only",
            "builtin_network_plans_only",
        ] {
            old.as_table_mut()
                .unwrap()
                .insert(field.into(), toml::Value::Boolean(true));
        }
        fs::create_dir_all(&root).unwrap();
        fs::write(service.config_path(), toml::to_string(&old).unwrap()).unwrap();
        let loaded = service.load().unwrap();
        assert_eq!(loaded.schema_version, CONFIG_SCHEMA_VERSION);
        assert_eq!(loaded.data_provider, DataProvider::Gitee);
        assert_eq!(
            loaded.repository_sources[0].url,
            DataProvider::Gitee.document("mods.toml")
        );
        let saved = fs::read_to_string(service.config_path()).unwrap();
        assert!(saved.contains("schema_version = 3"));
        assert!(!saved.contains("builtin_"));
        assert!(!saved.contains("data_provider"));
        assert!(!migrate_schema(&mut loaded.clone()));
        fs::remove_dir_all(root).unwrap();
    }
}
