use super::*;
use crate::map_note::NOTE_BLOCK_OPEN;
use crate::models::AssetVersionEntry;

#[test]
fn publishing_rejects_invalid_relation_overrides_before_writing() {
    let mut metadata: MapPublishMetadata = serde_json::from_value(serde_json::json!({
        "release_relations": [{ "version": ">=0.9", "relations": [
            { "kind": "requires", "guid": "dep", "version": ">=1.7" }
        ] }]
    }))
    .unwrap();
    assert!(validate_publish_relations(&metadata).is_ok());
    for version in ["", "*", "~=1", ">=2,<1"] {
        metadata.release_relations[0].version = version.to_string();
        assert_eq!(
            validate_publish_relations(&metadata).unwrap_err(),
            "mapPublish.invalidRelations"
        );
    }
    metadata.release_relations[0].version = "==0.1".to_string();
    metadata.release_relations[0].relations[0].version = ">=2,<1".to_string();
    assert!(validate_publish_relations(&metadata).is_err());
    metadata.release_relations[0].relations.clear();
    assert!(validate_publish_relations(&metadata).is_err());
    metadata.release_relations.clear();
    metadata.requires.insert("dep".to_string(), "".to_string());
    assert!(validate_publish_relations(&metadata).is_ok());
}

#[test]
fn release_metadata_survives_the_round_trip_through_the_block() {
    let metadata = MapPublishMetadata {
        level_set_name: "Gaoda".to_string(),
        level_set_name_zh: "高達".to_string(),
        author: "GAODA".to_string(),
        asset: "gaoda-${version}.zip".to_string(),
        requires: [("dev.gua.diylevel".to_string(), ">=4.0.0".to_string())]
            .into_iter()
            .collect(),
        ..MapPublishMetadata::default()
    };
    let body = render_release_body(&metadata, "首个版本");
    let block = crate::map_note::parse_release_note(&body)
        .unwrap()
        .expect("rendered body must contain a block");
    let parsed = publish_metadata_from_note(&block.metadata);
    assert_eq!(parsed.level_set_name, "Gaoda");
    assert_eq!(parsed.level_set_name_zh, "高達");
    assert_eq!(parsed.asset, "gaoda-${version}.zip");
    assert_eq!(
        parsed.requires.get("dev.gua.diylevel").map(String::as_str),
        Some(">=4.0.0")
    );
    assert_eq!(block.body.trim(), "首个版本");
    assert!(body.starts_with(NOTE_BLOCK_OPEN));
}

#[test]
fn the_table_tail_overrides_the_stale_latest_versions() {
    let mut metadata = MapPublishMetadata {
        latest_mapset_version: "0.7".to_string(),
        latest_info_version: "0.7".to_string(),
        asset_versions: vec![
            AssetVersionEntry {
                mapset_version: "0.7".to_string(),
                info_version: "0.7".to_string(),
            },
            AssetVersionEntry {
                mapset_version: " 0.69 ".to_string(),
                info_version: "0.69".to_string(),
            },
        ],
        ..MapPublishMetadata::default()
    };
    apply_table_tail(&mut metadata);
    assert_eq!(metadata.latest_mapset_version, "0.69");
    assert_eq!(metadata.latest_info_version, "0.69");
    // 表尾那一格的版本号空着时不能把已有值抹成空
    metadata.asset_versions.push(AssetVersionEntry::default());
    apply_table_tail(&mut metadata);
    assert_eq!(metadata.latest_mapset_version, "0.69");
    // 空表完全不动：老源的 latest_* 仍然只有一个来源
    metadata.asset_versions.clear();
    apply_table_tail(&mut metadata);
    assert_eq!(metadata.latest_mapset_version, "0.69");
}
