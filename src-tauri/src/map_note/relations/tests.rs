use super::super::block::parse_release_note;
use super::super::test_support::body;
use super::*;

#[test]
fn the_three_relation_tables_become_relations() {
    let block = parse_release_note(&body(
        r#"level_set_name = "A"
requires = { "dev.gua.overcooked.diylevel" = ">=0.10.0" }
compatible = { "dev.gua.overcooked.diylevel" = ">=0.10.0,<=0.11.9" }
conflicts = { "oc2.oc2diylevelruntimewloader" = { version = ">=4.0.0", prerelease = true } }
"#,
        "",
    ))
    .unwrap()
    .unwrap();
    let relations = block.metadata.relations();
    assert_eq!(relations.len(), 3);
    let requires = relations
        .iter()
        .find(|relation| relation.kind == RelationKind::Requires)
        .unwrap();
    assert_eq!(requires.version, ">=0.10.0");
    let compatible = relations
        .iter()
        .find(|relation| relation.kind == RelationKind::Compatible)
        .unwrap();
    assert_eq!(compatible.version, ">=0.10.0,<=0.11.9");
    let conflicts = relations
        .iter()
        .find(|relation| relation.kind == RelationKind::Conflicts)
        .unwrap();
    assert_eq!(conflicts.version, ">=4.0.0");
    assert_eq!(conflicts.prerelease, Some(true));
}

#[derive(serde::Deserialize)]
struct Holder {
    value: RelationSpec,
}

/// 关系值规范化到 `version` 文本：两种写法落到同一段话上。
fn normalized(toml_value: &str) -> Option<String> {
    let holder: Holder = toml::from_str(&format!("value = {toml_value}")).ok()?;
    holder.value.spec().map(|(version, _, _)| version)
}

#[test]
fn relation_specs_are_normalized_to_one_version_string() {
    assert_eq!(normalized("\"*\""), Some(String::new()));
    assert_eq!(normalized("\"\""), Some(String::new()));
    assert_eq!(normalized("\">=1.0\""), Some(">=1.0".into()));
    assert_eq!(normalized("\">=1.0,<2.0\""), Some(">=1.0,<2.0".into()));
    // 单个 `=` 统一成 `==`，开区间保留（版本比较是离散的，边界明确）
    assert_eq!(normalized("\"=1.2\""), Some("==1.2".into()));
    assert_eq!(normalized("\">1.0\""), Some(">1.0".into()));
    // 显式表同样接受，旧 min/max 折成区间
    assert_eq!(
        normalized("{ min_version = \"1.0\", max_version = \"2.0\" }"),
        Some(">=1.0,<=2.0".into())
    );
}

#[test]
fn malformed_relation_specs_are_rejected() {
    // 缺比较符、`~=`、空条件都不是合法区间，绝不能静默当成「不限」
    assert_eq!(normalized("\"1.0\""), None);
    assert_eq!(normalized("\"~=1.0\""), None);
    assert_eq!(normalized("\">=1.0,\""), None);
}

#[test]
fn a_bad_relation_is_skipped_instead_of_dropping_the_map() {
    let block = parse_release_note(&body(
        r#"level_set_name = "A"
requires = { "com.example.badloader" = "1.0", "com.example.loader" = ">=2.0" }
"#,
        "",
    ))
    .unwrap()
    .unwrap();
    let relations = block.metadata.relations();
    assert_eq!(relations.len(), 1);
    assert_eq!(relations[0].guid, "com.example.loader");
}

/// 块里的 `[[release_relations]]` 按版本覆盖依赖，内层是块自己的三张表写法。
#[test]
fn release_relations_override_the_relations_for_that_version() {
    let block = parse_release_note(&body(
            r#"level_set_name = "A"
requires = { "dev.gua.overcooked.diylevel" = ">=0.10.0" }

[[release_relations]]
version = ">=2.0.0"
requires = { "dev.gua.overcooked.diylevel" = ">=0.12.0" }
compatible = { "com.example.sorting" = "*" }

[[release_relations]]
version = ">=2.1.0,<3.0.0"
relations = [
  { kind = "requires", guid = "dev.gua.overcooked.diylevel", version = ">=0.13.0", optional = true },
]

[[release_relations]]
version = "*"
requires = { "dev.gua.overcooked.diylevel" = ">=9.9.9" }

[[release_relations]]
requires = { "dev.gua.overcooked.diylevel" = ">=9.9.9" }
"#,
            "",
        ))
        .unwrap()
        .unwrap();
    let overrides = block.metadata.release_relation_overrides();
    // 不限版本与没写区间的那两张被丢掉，只留真正限定了版本的两张
    assert_eq!(overrides.len(), 2, "{overrides:?}");
    assert_eq!(overrides[0].version, ">=2.0.0");
    assert_eq!(overrides[0].relations.len(), 2);
    assert!(
        overrides[0]
            .relations
            .iter()
            .any(|relation| relation.kind == RelationKind::Compatible)
    );
    assert_eq!(overrides[1].version, ">=2.1.0,<3.0.0");
    assert_eq!(overrides[1].relations.len(), 1);
    assert_eq!(overrides[1].relations[0].version, ">=0.13.0");
    // 覆盖表里的 optional 只对依赖有意义
    assert!(overrides[1].relations[0].optional);
}
