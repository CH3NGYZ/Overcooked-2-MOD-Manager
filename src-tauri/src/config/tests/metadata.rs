use crate::config::ConfigService;
use crate::config::bundle::BUNDLE_FORMAT;
use crate::config::defaults::default_config;
use crate::config::identity::mod_repository_from_export;
use crate::models::*;
use std::fs;
use std::path::Path;
use uuid::Uuid;

use super::REGION_REPOSITORY;

#[test]
fn source_nickname_translations_survive_config_and_bundle_round_trips() {
    let service = ConfigService {
        root: std::env::temp_dir(),
        cache: std::sync::Arc::new(std::sync::Mutex::new(None)),
        proxies: Default::default(),
    };
    let legacy: RepositorySource = toml::from_str(
        r#"
id = "custom-translated-source"
nickname = "Fallback name"
url = "https://example.com/mods.toml"
"#,
    )
    .unwrap();
    assert!(legacy.nickname_i18n.is_empty());
    let mut source = legacy;
    source.nickname_i18n = std::collections::BTreeMap::from([
        ("zh-CN".into(), "中文昵称".into()),
        ("en".into(), "English name".into()),
    ]);
    let mut config = default_config();
    config.repository_sources.push(source.clone());
    let serialized = toml::to_string(&config).unwrap();
    let restored: AppConfig = toml::from_str(&serialized).unwrap();
    let public = service.public_config(&restored);
    assert_eq!(
        public
            .repository_sources
            .iter()
            .find(|item| item.id == source.id),
        Some(&source)
    );
    let exported = service.export_bundle(&restored, None).unwrap();
    let bundle = service.validate_bundle(&exported).unwrap();
    assert_eq!(bundle.repository_sources, restored.repository_sources);
    let mut imported = default_config();
    service.import_bundle(&mut imported, &bundle, "overwrite");
    assert_eq!(
        imported
            .repository_sources
            .iter()
            .find(|item| item.id == source.id),
        Some(&source)
    );
}

#[test]
fn legacy_repository_metadata_migrates_into_chinese_maps_and_persists_empty_fields() {
    let service = ConfigService {
        root: Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-config-load")
            .join(Uuid::new_v4().to_string()),
        cache: std::sync::Arc::new(std::sync::Mutex::new(None)),
        proxies: Default::default(),
    };
    let location = crate::repository_sources::SourceLocation::parse("fixture.toml").unwrap();
    let mut config = default_config();
    let export = crate::repository_sources::parse_document(
        REGION_REPOSITORY,
        &location,
        &Default::default(),
    )
    .unwrap()
    .repositories
    .remove(0);
    config.repositories = vec![mod_repository_from_export(
        &export,
        "legacy-repository".into(),
        false,
    )];
    let mut legacy = toml::Value::try_from(&config).unwrap();
    let repository = legacy["repositories"][0].as_table_mut().unwrap();
    repository.remove("name_i18n");
    repository.remove("group_i18n");
    repository.remove("note_i18n");
    repository.insert("name".into(), toml::Value::String("Legacy name".into()));
    repository.insert("group".into(), toml::Value::String("Legacy group".into()));
    repository.insert("note".into(), toml::Value::String(String::new()));
    fs::create_dir_all(&service.root).unwrap();
    fs::write(
        service.config_path(),
        toml::to_string_pretty(&legacy).unwrap(),
    )
    .unwrap();

    let loaded = service.load().unwrap();
    let repository = &loaded.repositories[0];
    assert_eq!(repository.name_i18n["zh-CN"], "Legacy name");
    assert_eq!(repository.group_i18n["zh-CN"], "Legacy group");
    assert_eq!(repository.note_i18n["zh-CN"], "");
    assert_eq!(repository.name, repository.name_i18n["zh-CN"]);
    assert_eq!(repository.group, repository.group_i18n["zh-CN"]);
    assert_eq!(repository.note, repository.note_i18n["zh-CN"]);

    let saved = fs::read_to_string(service.config_path()).unwrap();
    let saved_value: toml::Value = toml::from_str(&saved).unwrap();
    let saved_repository = &saved_value["repositories"][0];
    assert_eq!(
        saved_repository["name_i18n"]["zh-CN"].as_str(),
        Some("Legacy name")
    );
    assert_eq!(
        saved_repository["group_i18n"]["zh-CN"].as_str(),
        Some("Legacy group")
    );
    assert_eq!(saved_repository["note_i18n"]["zh-CN"].as_str(), Some(""));
    let reloaded = service.load().unwrap();
    assert_eq!(reloaded.repositories[0].note_i18n["zh-CN"], "");
    assert_eq!(fs::read_to_string(service.config_path()).unwrap(), saved);
}

#[test]
fn japanese_repository_metadata_survives_import_export_and_config_reload() {
    let service = ConfigService {
        root: Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-config-load")
            .join(Uuid::new_v4().to_string()),
        cache: std::sync::Arc::new(std::sync::Mutex::new(None)),
        proxies: Default::default(),
    };
    let location = crate::repository_sources::SourceLocation::parse("fixture.toml").unwrap();
    let text = r#"
[[repositories]]
name = "日本語名"
name_i18n.zh-CN = "中文名称"
name_i18n.ja = "日本語名"
group_i18n.zh-CN = "工具"
group_i18n.ja = "ツール"
note_i18n.zh-CN = ""
note_i18n.ja = "日本語のメモ"
provider = "github"
username = "example"
repository = "mod"
channel = "stable"
install_target = "BepInEx/plugins"
plugin_guid = "com.example.ja"
enabled = true
[[repositories.asset_rules]]
type = "exact"
value = "Japan.dll"
"#;
    let document =
        crate::repository_sources::parse_document(text, &location, &Default::default()).unwrap();
    let export = document.repositories[0].clone();
    assert_eq!(export.name_i18n["ja"], "日本語名");
    assert_eq!(export.group_i18n["ja"], "ツール");
    assert_eq!(export.note_i18n["ja"], "日本語のメモ");
    let bundle = ModRepositoryBundle {
        format: BUNDLE_FORMAT.into(),
        schema_version: 1,
        exported_at: String::new(),
        repositories: vec![export],
        repository_sources: Vec::new(),
    };
    let mut config = default_config();
    service.import_bundle(&mut config, &bundle, "overwrite");
    let export_bundle = service.export_bundle(&config, None).unwrap();
    let restored = service.validate_bundle(&export_bundle).unwrap();
    assert_eq!(restored.repositories[0].name_i18n["ja"], "日本語名");
    assert_eq!(restored.repositories[0].group_i18n["ja"], "ツール");
    assert_eq!(restored.repositories[0].note_i18n["ja"], "日本語のメモ");
    fs::create_dir_all(&service.root).unwrap();
    service.save(&config).unwrap();
    let reloaded = service.load().unwrap();
    assert_eq!(reloaded.repositories[0].name_i18n["ja"], "日本語名");
    assert_eq!(reloaded.repositories[0].group_i18n["ja"], "ツール");
    assert_eq!(reloaded.repositories[0].note_i18n["ja"], "日本語のメモ");
    assert_eq!(reloaded.repositories[0].name, "中文名称");
    fs::remove_dir_all(&service.root).unwrap();
}

#[test]
fn repository_bundle_requires_chinese_metadata_and_ignores_raw_fields() {
    let service = ConfigService {
        root: std::env::temp_dir(),
        cache: std::sync::Arc::new(std::sync::Mutex::new(None)),
        proxies: Default::default(),
    };
    let bundle_for = |metadata: &str| {
        format!(
            r#"format = "{BUNDLE_FORMAT}"
schema_version = 1
exported_at = "2026-01-01T00:00:00Z"

[[repositories]]
{metadata}
provider = "github"
username = "example"
repository = "mod"
install_target = "BepInEx/plugins"
enabled = true
[[repositories.asset_rules]]
type = "exact"
value = "mod.dll"
"#
        )
    };

    let valid = service
        .validate_bundle(&bundle_for(
            r#"name = "Raw name"
name_i18n.zh-CN = " 中文名称 "
group = "Raw group"
group_i18n.zh-CN = "中文分组"
note = "Raw note"
note_i18n.zh-CN = "中文备注""#,
        ))
        .unwrap();
    let repository = &valid.repositories[0];
    assert_eq!(repository.name, "中文名称");
    assert_eq!(repository.group, "中文分组");
    assert_eq!(repository.note, "中文备注");

    for (metadata, expected_error) in [
        (
            r#"name_i18n.zh-CN = "名称"
note_i18n.zh-CN = """#,
            "config.missingGroupI18n",
        ),
        (
            r#"name_i18n.zh-CN = "名称"
group_i18n.zh-CN = """#,
            "config.missingNoteI18n",
        ),
        (
            r#"name_i18n.zh-CN = ""
group_i18n.zh-CN = ""
note_i18n.zh-CN = """#,
            "config.emptyNameI18n",
        ),
        (
            r#"name_i18n.en = "English name"
group_i18n.zh-CN = ""
note_i18n.zh-CN = """#,
            "config.missingNameI18n",
        ),
    ] {
        let error = service.validate_bundle(&bundle_for(metadata)).unwrap_err();
        assert!(error.to_string().contains(expected_error), "{error}");
    }
}
