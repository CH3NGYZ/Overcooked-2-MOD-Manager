use crate::models::*;

#[test]
fn dotted_assignment_is_rejected_because_relations_is_a_table_array() {
    let dotted = r#"
name = "x"
name_i18n.zh-CN = "x"
group_i18n.zh-CN = ""
note_i18n.zh-CN = ""
provider = "github"
username = "a"
repository = "b"
install_target = "BepInEx/plugins"
enabled = true
relations.guid = "com.bepis.bepinex.configurationmanager"
relations.min_version = ""
relations.optional = false
[[asset_rules]]
type = "exact"
value = "a.zip"
"#;
    let error = toml::from_str::<RepositoryExport>(dotted).unwrap_err();
    assert!(error.to_string().contains("expected a sequence"), "{error}");
}

/// 正确写法：`[[relations]]` 表数组，可重复声明多条关系。
#[test]
fn array_of_tables_accepts_multiple_relations() {
    let explicit = r#"
name = "x"
name_i18n.zh-CN = "x"
group_i18n.zh-CN = ""
note_i18n.zh-CN = ""
provider = "github"
username = "a"
repository = "b"
install_target = "BepInEx/plugins"
enabled = true
[[asset_rules]]
type = "exact"
value = "a.zip"

[[relations]]
guid = "com.bepis.bepinex.configurationmanager"

[[relations]]
kind = "conflicts"
guid = "com.example.other"
min_version = "1.8.7"
max_version = "2.0"
"#;
    let v = toml::from_str::<RepositoryExport>(explicit).expect("表数组写法应当成功");
    assert_eq!(v.relations.len(), 2);
    // 省略 kind 时按依赖处理：历史写法（[[requires]] 只写 guid）继续可读
    assert_eq!(v.relations[0].kind, RelationKind::Requires);
    assert_eq!(
        v.relations[0].guid,
        "com.bepis.bepinex.configurationmanager"
    );
    assert_eq!(v.relations[1].kind, RelationKind::Conflicts);
    assert_eq!(v.relations[1].version, ">=1.8.7,<=2.0");
    assert!(!v.relations[1].optional);
}

/// `[[requires]]` 作为历史字段名继续可读（alias，不需要作者改文档）。
#[test]
fn legacy_requires_field_still_parses() {
    let legacy = r#"
name = "x"
name_i18n.zh-CN = "x"
group_i18n.zh-CN = ""
note_i18n.zh-CN = ""
provider = "github"
username = "a"
repository = "b"
install_target = "BepInEx/plugins"
enabled = true
[[asset_rules]]
type = "exact"
value = "a.zip"

[[requires]]
guid = "com.example.dep"
min_version = "1.8.7"
optional = true
"#;
    let v = toml::from_str::<RepositoryExport>(legacy).expect("旧字段名应当继续可读");
    assert_eq!(v.relations.len(), 1);
    assert_eq!(v.relations[0].kind, RelationKind::Requires);
    assert_eq!(v.relations[0].version, ">=1.8.7");
    assert!(v.relations[0].optional);
}

#[test]
fn unknown_relation_kind_is_skipped_instead_of_failing_the_document() {
    let document = r#"
name = "x"
name_i18n.zh-CN = "x"
group_i18n.zh-CN = ""
note_i18n.zh-CN = ""
provider = "github"
username = "a"
repository = "b"
install_target = "BepInEx/plugins"
enabled = true
[[asset_rules]]
type = "exact"
value = "a.zip"

[[relations]]
kind = "loads_after"
guid = "com.example.future"

[[relations]]
kind = "requires"
guid = "com.example.dep"
"#;
    let v = toml::from_str::<RepositoryExport>(document).expect("未知 kind 不应让文档失败");
    assert_eq!(v.relations.len(), 1);
    assert_eq!(v.relations[0].guid, "com.example.dep");
}

/// 未知的**顶层**字段也不该让源文档失败（旧客户端读新文档）。
#[test]
fn unknown_top_level_field_is_ignored() {
    let document = r#"
name = "x"
name_i18n.zh-CN = "x"
group_i18n.zh-CN = ""
note_i18n.zh-CN = ""
provider = "github"
username = "a"
repository = "b"
install_target = "BepInEx/plugins"
enabled = true
some_future_field = "whatever"
[[asset_rules]]
type = "exact"
value = "a.zip"
"#;
    let v = toml::from_str::<RepositoryExport>(document).expect("未知字段不应让文档失败");
    assert_eq!(v.name, "x");
}

#[test]
fn repository_field_after_relations_is_reported_as_misplaced() {
    let misplaced = r#"
name_i18n.zh-CN = "x"
group_i18n.zh-CN = ""
note_i18n.zh-CN = ""
provider = "github"
username = "a"
repository = "b"
enabled = true
[[asset_rules]]
type = "exact"
value = "a.zip"

[[relations]]
kind = "requires"
guid = "com.example.dep"

# 写在了关系块之后 —— 实际属于关系条目
install_target = "BepInEx/plugins"
"#;
    let error = toml::from_str::<RepositoryExport>(misplaced)
        .expect_err("仓库字段出现在关系块之后应当报错");
    let text = error.to_string();
    assert!(text.contains("install_target"), "{text}");
    assert!(text.contains("写在"), "{text}");
}

/// 正确顺序（仓库字段都在关系块之前）照常解析。
#[test]
fn repository_fields_before_relations_still_parse() {
    let correct = r#"
name_i18n.zh-CN = "x"
group_i18n.zh-CN = ""
note_i18n.zh-CN = ""
provider = "github"
username = "a"
repository = "b"
install_target = "BepInEx/plugins"
enabled = true
[[asset_rules]]
type = "exact"
value = "a.zip"

[[relations]]
kind = "requires"
guid = "com.example.dep"
min_version = "1.0"
"#;
    let v = toml::from_str::<RepositoryExport>(correct).expect("正确顺序应当成功");
    assert_eq!(v.relations.len(), 1);
    assert_eq!(v.install_target, "BepInEx/plugins");
}
