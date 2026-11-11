use super::{AppConfig, SettingsPatch};

#[test]
fn chef_preview_defaults_to_all_cores_and_reads_older_configs() {
    let default = crate::config::default_config();
    assert_eq!(default.diychef_preview_concurrency, 0);
    let mut old = serde_json::to_value(&default).unwrap();
    old.as_object_mut()
        .unwrap()
        .remove("diychef_preview_concurrency");
    let restored: AppConfig = serde_json::from_value(old).unwrap();
    assert_eq!(restored.diychef_preview_concurrency, 0);
    for value in [0, 1, 8] {
        let mut config = default.clone();
        config.diychef_preview_concurrency = value;
        let encoded = serde_json::to_value(config).unwrap();
        let decoded: AppConfig = serde_json::from_value(encoded).unwrap();
        assert_eq!(decoded.diychef_preview_concurrency, value);
    }
}

#[test]
fn chef_preview_patch_distinguishes_no_change_from_all_cores() {
    for (input, expected) in [
        (serde_json::json!({}), None),
        (
            serde_json::json!({ "diychef_preview_concurrency": 0 }),
            Some(0),
        ),
        (
            serde_json::json!({ "diychef_preview_concurrency": 3 }),
            Some(3),
        ),
    ] {
        let patch: SettingsPatch = serde_json::from_value(input).unwrap();
        assert_eq!(patch.diychef_preview_concurrency, expected);
    }
}
