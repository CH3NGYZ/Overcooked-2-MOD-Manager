use crate::config::ConfigService;
use crate::config::availability::current_user_region;
use crate::config::availability::repository_source_matches_region;
use crate::config::defaults::CONFIG_SCHEMA_VERSION;
use crate::config::defaults::default_config;
use crate::config::defaults::default_ui_zoom;
use crate::config::migrations::migrate_bundled_source_switches;
use crate::config::sources::builtin_repository_sources_for_region;
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
        crate::repository_sources::default_index_url(current_user_region()),
        "默认读远端索引，内置快照只是读不到时的回退"
    );
    assert!(matches!(
        crate::repository_sources::SourceLocation::parse(&root.url).unwrap(),
        crate::repository_sources::SourceLocation::Remote(_)
    ));
    assert!(!config.builtin_sources_only, "默认不再只看内置仓库快照");
    assert!(!config.builtin_map_sources_only, "默认不再只看内置地图快照");
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

/// 缺字段的旧配置按「远端索引」补齐，显式打开内置快照的才保留用户选择。
///
/// V2 早期这两个开关默认开启；现在默认关闭，缺字段时不能再补成 `true`。
#[test]
fn legacy_configs_without_the_bundled_switches_default_to_the_remote_index() {
    let full = toml::to_string_pretty(&default_config()).unwrap();
    let without_switches = full
        .lines()
        .filter(|line| {
            !line.starts_with("builtin_sources_only")
                && !line.starts_with("builtin_map_sources_only")
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!without_switches.contains("builtin_sources_only"));
    let migrated: AppConfig = toml::from_str(&without_switches).unwrap();
    assert!(!migrated.builtin_sources_only);
    assert!(!migrated.builtin_map_sources_only);

    let mut value: toml::Value = toml::from_str(&full).unwrap();
    let table = value.as_table_mut().unwrap();
    table.insert("builtin_sources_only".into(), toml::Value::Boolean(true));
    table.insert(
        "builtin_map_sources_only".into(),
        toml::Value::Boolean(true),
    );
    let explicit: AppConfig = toml::from_str(&toml::to_string(&value).unwrap()).unwrap();
    assert!(explicit.builtin_sources_only, "显式打开必须保留用户的选择");
    assert!(explicit.builtin_map_sources_only);
}

/// 升级路径：V1 配置里写着 `true` 的两个开关，首次读取要迁成 `false`。
///
/// 只改 serde 默认值救不了已经落盘的老配置 —— 升级必须把文件里的值也改掉，
/// 并把升级后的配置写回，否则每次启动都会重跑迁移。
#[test]
fn upgrading_a_v1_config_switches_the_bundled_source_defaults_off() {
    let service = ConfigService {
        root: Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-config-load")
            .join(Uuid::new_v4().to_string()),
        cache: std::sync::Arc::new(std::sync::Mutex::new(None)),
        proxies: Default::default(),
    };
    // V1：两个开关默认开启，官方根指向编译期快照
    let mut v1 = default_config();
    v1.schema_version = 1;
    v1.builtin_sources_only = true;
    v1.builtin_map_sources_only = true;
    v1.repository_sources = builtin_repository_sources_for_region(current_user_region(), true);
    let text = toml::to_string_pretty(&v1).unwrap();
    assert!(text.contains("schema_version = 1"));
    assert!(text.contains("builtin_sources_only = true"));
    fs::create_dir_all(&service.root).unwrap();
    fs::write(service.config_path(), text).unwrap();

    let loaded = service.load().unwrap();
    assert_eq!(loaded.schema_version, CONFIG_SCHEMA_VERSION);
    assert!(!loaded.builtin_sources_only);
    assert!(!loaded.builtin_map_sources_only);
    assert_eq!(
        loaded.repository_sources[0].url,
        crate::repository_sources::default_index_url(current_user_region()),
        "迁移后官方根必须回到远端索引"
    );
    // 迁移必须落盘：否则每次启动都重跑一遍
    let saved = fs::read_to_string(service.config_path()).unwrap();
    assert!(saved.contains("schema_version = 2"));
    assert!(saved.contains("builtin_sources_only = false"));
    assert!(saved.contains("builtin_map_sources_only = false"));
    fs::remove_dir_all(service.root).unwrap();
}

/// 已经是当前版本的配置不再迁移：用户手动打开的开关必须留着。
#[test]
fn current_schema_version_keeps_the_bundled_source_switches_as_saved() {
    let mut config = default_config();
    config.builtin_sources_only = true;
    config.builtin_map_sources_only = true;
    assert!(!migrate_bundled_source_switches(&mut config));
    assert!(config.builtin_sources_only);
    assert!(config.builtin_map_sources_only);
    assert_eq!(config.schema_version, CONFIG_SCHEMA_VERSION);
}
