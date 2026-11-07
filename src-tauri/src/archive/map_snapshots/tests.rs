use super::*;
use crate::archive::test_support::{requires, temp_manifest_dir};
use crate::archive::{new_manifest, read_manifests, write_manifest};
use std::collections::BTreeMap;

/// 地图快照与安装清单同目录，但它不是 TOML，不能被当成一份清单。
#[test]
fn map_relation_snapshots_are_not_install_manifests() {
    let dir = temp_manifest_dir("manifest-mixed");
    write_manifest(
        &dir,
        &new_manifest(
            "repo",
            "v1.0.0",
            BTreeMap::new(),
            BTreeMap::new(),
            vec!["BepInEx/plugins/a.dll".into()],
            Vec::new(),
        ),
    )
    .unwrap();
    let mut snapshots = crate::models::MapRelationSnapshots::default();
    snapshots.entries.insert(
        "map".into(),
        crate::models::MapRelationSnapshot {
            repository_id: "map".into(),
            level_set_name: "Set".into(),
            level_set_name_zh: String::new(),
            author: "Author".into(),
            group_i18n: BTreeMap::new(),
            group: String::new(),
            relations: vec![requires("dep.guid", "1.0.0")],
            release_relations: Vec::new(),
            installed_mapset_version: String::new(),
        },
    );
    write_map_relation_snapshots(&dir, &snapshots).unwrap();
    assert_eq!(read_manifests(&dir).len(), 1, "只该读到那份 TOML 清单");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// 地图快照写读往返；文件损坏或不存在时返回默认值，绝不 panic。
#[test]
fn map_relation_snapshots_round_trip_and_tolerate_broken_files() {
    let dir = temp_manifest_dir("map-relations");
    let mut snapshots = crate::models::MapRelationSnapshots::default();
    snapshots.entries.insert(
        "map".into(),
        crate::models::MapRelationSnapshot {
            repository_id: "map".into(),
            level_set_name: "Set".into(),
            level_set_name_zh: String::new(),
            author: "Author".into(),
            group_i18n: BTreeMap::new(),
            group: String::new(),
            relations: vec![requires("dep.guid", "1.0.0")],
            release_relations: Vec::new(),
            installed_mapset_version: String::new(),
        },
    );
    write_map_relation_snapshots(&dir, &snapshots).unwrap();
    let loaded = read_map_relation_snapshots(&dir);
    assert_eq!(loaded.entries.len(), 1);
    assert_eq!(loaded.entries["map"].level_set_name, "Set");
    assert_eq!(
        loaded.entries["map"].relations,
        vec![requires("dep.guid", "1.0.0")]
    );

    // 损坏：返回默认值而不是 Err/panic
    std::fs::write(dir.join("map-relations.json"), b"{ not json").unwrap();
    assert!(read_map_relation_snapshots(&dir).entries.is_empty());
    // 不存在：同样返回默认值
    assert!(
        read_map_relation_snapshots(&dir.join("missing"))
            .entries
            .is_empty()
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
