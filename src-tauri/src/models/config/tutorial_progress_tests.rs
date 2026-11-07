/// 前端每走一步都会整批发一次；并集去重是唯一合并规则。
#[test]
fn watched_step_ids_are_merged_as_a_deduped_union() {
    let mut config = crate::config::default_config();
    config.record_tutorial_steps(&["intro".into(), "sidebar".into()]);
    // 重播时又发了一遍老 id，并带上新 id：老的不能重复、顺序不动。
    config.record_tutorial_steps(&[
        " sidebar ".into(),
        "sidebar-mods-online".into(),
        String::new(),
    ]);
    assert_eq!(
        config.tutorial_watched_steps,
        vec!["intro", "sidebar", "sidebar-mods-online"]
    );
}

/// 旧配置缺字段时反序列化成空集合，序列化时整个键都不写。
#[test]
fn missing_field_round_trips_as_empty_and_stays_out_of_json() {
    let raw = serde_json::to_value(crate::config::default_config()).unwrap();
    assert!(raw.get("tutorial_watched_steps").is_none());
    let mut config = crate::config::default_config();
    config.record_tutorial_steps(&["intro".into()]);
    let raw = serde_json::to_value(&config).unwrap();
    assert_eq!(raw["tutorial_watched_steps"], serde_json::json!(["intro"]));
}
