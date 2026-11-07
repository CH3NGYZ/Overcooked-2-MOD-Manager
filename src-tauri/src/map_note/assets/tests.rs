use super::*;

#[test]
fn the_asset_template_becomes_an_anchored_regex() {
    let rule = versioned_asset_rule(Some("daxue_v${version}.zip"), "DifferentTag").unwrap();
    assert_eq!(rule.rule_type, AssetRuleType::Regex);
    assert_eq!(
        rule.value,
        r"^daxue_v(?P<version>[0-9]+(?:\.[0-9]+)*)\.zip$"
    );
    let regex = regex::Regex::new(&rule.value).unwrap();
    let captures = regex.captures("daxue_v0.3.2.zip").unwrap();
    assert_eq!(
        captures.name("version").map(|found| found.as_str()),
        Some("0.3.2")
    );
}

#[test]
fn a_template_without_a_version_placeholder_is_reported() {
    assert!(versioned_asset_rule(Some("daxue.zip"), "daxue").is_none());
}

#[test]
fn without_a_template_the_original_tag_gives_the_rule() {
    let rule = versioned_asset_rule(None, "blni").unwrap();
    let regex = regex::Regex::new(&rule.value).unwrap();
    let captures = regex.captures("blni_v2.1.zip").expect("同一命名形状应命中");
    assert_eq!(
        captures.name("version").map(|found| found.as_str()),
        Some("2.1")
    );
    // 派生规则的版本段是通配的：同一个仓库里换版本号仍然命中（那正是要的）
    assert!(regex.captures("blni_v2.2.zip").is_some());
    // 但别的地图包不该命中
    assert!(regex.captures("daxue_v0.3.2.zip").is_none());
}

#[test]
fn default_template_preserves_tag_case_and_escapes_legacy_characters() {
    let rule = versioned_asset_rule(None, "My.Map中文").unwrap();
    let regex = regex::Regex::new(&rule.value).unwrap();
    assert!(regex.is_match("My.Map中文_v1.2.zip"));
    assert!(!regex.is_match("my.map中文_v1.2.zip"));
    assert!(!regex.is_match("MyXMap中文_v1.2.zip"));
    assert!(!regex.is_match("unrelated_v1.2.zip"));
}
