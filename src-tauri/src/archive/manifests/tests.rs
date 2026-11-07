use super::*;
use crate::archive::test_support::{requires, temp_manifest_dir};

/*
 * 旧清单只有单语 `package_name` / `group` / `note`。
 *
 * 新结构删掉了这三个字段，如果读取时不迁移，用户在 MOD 管理页会看到
 * 空白的包名和分组 —— 等于升级后旧 MOD 的标题全丢了。
 * `ManifestWire` 负责把单语值折进 `zh-CN` 键。
 */
#[test]
fn legacy_single_language_fields_fold_into_zh_cn() {
    let text = r#"
repository_id = "old"
release_tag = "v1.0.0"
package_name = "旧包名"
group = "旧分组"
note = "旧备注"
prerelease = false
installed_at = "2026-01-01T00:00:00Z"
files = ["BepInEx/plugins/Old.dll"]
"#;
    let manifest: InstallManifest = toml::from_str(text).unwrap();
    assert_eq!(manifest.package_name_i18n["zh-CN"], "旧包名");
    assert_eq!(manifest.group_i18n["zh-CN"], "旧分组");
    assert_eq!(manifest.note_i18n["zh-CN"], "旧备注");
}

/// 新旧字段并存时以多语言字段为准，不被单语值覆盖。
#[test]
fn explicit_i18n_wins_over_legacy_fields() {
    let text = r#"
repository_id = "mixed"
release_tag = "v1.0.0"
package_name = "旧包名"
package_name_i18n.zh-CN = "新包名"
package_name_i18n.en = "New name"
group = "旧分组"
group_i18n.zh-CN = "新分组"
note = "旧备注"
note_i18n.zh-CN = "新备注"
installed_at = "2026-01-01T00:00:00Z"
files = []
"#;
    let manifest: InstallManifest = toml::from_str(text).unwrap();
    assert_eq!(manifest.package_name_i18n["zh-CN"], "新包名");
    assert_eq!(manifest.package_name_i18n["en"], "New name");
    assert_eq!(manifest.group_i18n["zh-CN"], "新分组");
    assert_eq!(manifest.note_i18n["zh-CN"], "新备注");
}

/// 空值与空白不写入 zh-CN，避免出现「有翻译但是空的」。
#[test]
fn blank_legacy_values_are_ignored() {
    let text = r#"
repository_id = "blank"
release_tag = ""
package_name = "   "
group = ""
installed_at = "2026-01-01T00:00:00Z"
files = []
"#;
    let manifest: InstallManifest = toml::from_str(text).unwrap();
    assert!(manifest.package_name_i18n.is_empty());
    assert!(manifest.group_i18n.is_empty());
    assert!(manifest.note_i18n.is_empty());
}

/// 写出时不再产生单语字段。
#[test]
fn serialized_manifest_has_no_legacy_single_language_fields() {
    let mut manifest = new_manifest(
        "p",
        "v1",
        single_language("包名 - GitHub"),
        single_language("备注"),
        vec!["a.dll".into()],
        Vec::new(),
    );
    manifest.group_i18n = single_language("分组");
    let text = toml::to_string_pretty(&manifest).unwrap();
    assert!(!text.contains("package_name ="), "{text}");
    assert!(!text.contains("\ngroup ="), "{text}");
    assert!(!text.contains("\nnote ="), "{text}");
    assert!(text.contains("package_name_i18n"), "{text}");
}

/// `primary_translation` 优先中文，缺失时取第一个非空值。
#[test]
fn primary_translation_prefers_chinese_then_any() {
    let mut map = BTreeMap::new();
    map.insert("en".to_string(), "English".to_string());
    map.insert("ja".to_string(), "日本語".to_string());
    assert_eq!(primary_translation(&map), "English");
    map.insert("zh-CN".to_string(), "中文".to_string());
    assert_eq!(primary_translation(&map), "中文");
    assert_eq!(primary_translation(&BTreeMap::new()), "");
}

/// `relations` 在 IPC/JSON 里的形状：只带非空字段，前端按原样展示。
#[test]
fn relation_serializes_with_only_its_non_empty_fields() {
    assert_eq!(
        serde_json::to_value(requires("dep.guid", "1.0.0")).unwrap(),
        serde_json::json!({
            "kind": "requires",
            "guid": "dep.guid",
            "version": ">=1.0.0",
        })
    );
    let mut optional = requires("dep.guid", "");
    optional.kind = crate::models::RelationKind::Conflicts;
    optional.prerelease = Some(true);
    optional.optional = true;
    assert_eq!(
        serde_json::to_value(&optional).unwrap(),
        serde_json::json!({
            "kind": "conflicts",
            "guid": "dep.guid",
            "prerelease": true,
            "optional": true,
        })
    );
}

/// 依赖快照跟着安装清单一起落盘、一起读回来。
#[test]
fn manifest_round_trips_relation_snapshot() {
    let dir = temp_manifest_dir("manifest-relations");
    let manifest = new_manifest(
        "repo",
        "v1.0.0",
        BTreeMap::new(),
        BTreeMap::new(),
        vec!["BepInEx/plugins/a.dll".into()],
        vec![requires("dep.guid", "1.0.0")],
    );
    write_manifest(&dir, &manifest).unwrap();
    let loaded = read_manifest(&dir, "repo").expect("清单应能读回");
    assert_eq!(loaded.relations, vec![requires("dep.guid", "1.0.0")]);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// 旧清单没有 `relations` 字段：读成空表，而不是解析失败。
#[test]
fn legacy_manifest_without_relations_reads_as_empty() {
    let text = r#"
repository_id = "legacy"
release_tag = "v1.0.0"
installed_at = "2026-01-01T00:00:00Z"
files = ["BepInEx/plugins/a.dll"]
"#;
    let manifest: InstallManifest = toml::from_str(text).unwrap();
    assert!(manifest.relations.is_empty());
}
