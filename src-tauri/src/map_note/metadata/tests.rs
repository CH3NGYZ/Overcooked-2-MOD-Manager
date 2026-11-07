use super::super::block::parse_release_note;
use super::super::publish::publish_metadata_from_note;
use super::super::test_support::body;
use super::*;

#[test]
fn the_legacy_latest_version_field_still_parses() {
    // 改名不能作废已发布的 Release note：旧名必须继续认得，否则那些地图
    // 的「这就是最新版」直接变成没写，退回按 tag 比较。
    let text = body(
        r#"level_set_name = "GAODA"
author = "GAODA"
latest_version = "0.42"
"#,
        "",
    );
    let note = parse_release_note(&text).unwrap().unwrap();
    assert_eq!(note.metadata.latest_mapset_version.as_deref(), Some("0.42"));
}

#[test]
fn latest_info_version_is_trimmed_and_empty_dropped() {
    let mut metadata = MapNoteMetadata::default();
    assert_eq!(metadata.normalized_info_version(), None);
    // 只有空白等于没写：空串版本号进到前端会变成「一直在更新」
    metadata.latest_info_version = Some("   ".to_string());
    assert_eq!(metadata.normalized_info_version(), None);
    metadata.latest_info_version = Some(" 0.69 ".to_string());
    assert_eq!(metadata.normalized_info_version().as_deref(), Some("0.69"));
}

#[test]
fn the_table_tail_supplies_the_derived_latest_versions() {
    /*
     * 表尾最新，块里的 `latest_*` 是上一版的残留：作者改了顺序却不改那两个
     * 标量时，读侧必须按表走，否则刚发的新版会被当成旧版。
     */
    let text = body(
        r#"level_set_name = "GAODA"
author = "GAODA"
latest_mapset_version = "0.7"
latest_info_version = "0.7"

asset_versions = [
  { mapset_version = "0.7", info_version = "0.7" },
  { mapset_version = "0.69", info_version = "0.69-fix" },
]
"#,
        "",
    );
    let note = parse_release_note(&text).unwrap().unwrap();
    assert_eq!(
        note.metadata.derived_mapset_version().as_deref(),
        Some("0.69")
    );
    assert_eq!(
        note.metadata.derived_info_version().as_deref(),
        Some("0.69-fix")
    );
    // 折回上传页形态时表一起带走，否则作者一编辑就把顺序丢了
    let metadata = publish_metadata_from_note(&note.metadata);
    assert_eq!(metadata.latest_mapset_version, "0.69");
    assert_eq!(metadata.latest_info_version, "0.69-fix");
    assert_eq!(metadata.asset_versions.len(), 2);
    assert_eq!(metadata.asset_versions[1].mapset_version, "0.69");
}

#[test]
fn an_empty_table_tail_means_unknown_instead_of_borrowing_older_versions() {
    let mut metadata = MapNoteMetadata {
        latest_mapset_version: Some("0.7".to_string()),
        latest_info_version: Some("0.7".to_string()),
        asset_versions: vec![
            AssetVersionEntry {
                mapset_version: "0.7".to_string(),
                info_version: "0.7".to_string(),
            },
            AssetVersionEntry {
                mapset_version: "  ".to_string(),
                info_version: String::new(),
            },
        ],
        ..MapNoteMetadata::default()
    };
    // 版本表非空时，末项即权威；末项空值不能从旧标量或更早版本借值。
    assert_eq!(metadata.derived_mapset_version(), None);
    assert_eq!(metadata.derived_info_version(), None);
    // 只有完全没有版本表时才读旧标量字段。
    metadata.asset_versions.clear();
    assert_eq!(metadata.derived_mapset_version().as_deref(), Some("0.7"));
    assert_eq!(metadata.derived_info_version().as_deref(), Some("0.7"));
}
