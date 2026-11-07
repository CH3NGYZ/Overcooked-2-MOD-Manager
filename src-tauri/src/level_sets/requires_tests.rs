use crate::framework::DIYLEVEL_DIR;
use crate::level_sets::paths::LEVELS_DIR;
use crate::level_sets::requirements::REQUIRES_FILE;
use crate::level_sets::requirements::read_required_runtime;
use crate::level_sets::requirements::runtime_relation;
use crate::level_sets::scan::scan_level_sets;

#[test]
fn reads_the_runtime_requirement_from_requires_txt() {
    let root = std::env::temp_dir().join(format!("oc2-lv-req-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();

    // 没有文件 = 没有外部 runtime 要求，界面不显示关系卡片
    assert_eq!(read_required_runtime(&root), None);

    // 实测的真实内容：纯版本号（eon_v0.2.1 / tutu / mianhua_v1.4.2 都是它）
    std::fs::write(root.join(REQUIRES_FILE), "3.10.2\n").unwrap();
    assert_eq!(read_required_runtime(&root), Some("3.10.2".into()));

    // 空行与 `#` 注释行要跳过（同目录的 stub_levels.txt 就带说明行）
    std::fs::write(
        root.join(REQUIRES_FILE),
        "# 需要 Web Runtime\n\n  3.8.8  \n",
    )
    .unwrap();
    assert_eq!(read_required_runtime(&root), Some("3.8.8".into()));

    // 全是注释 → 等同于没有要求
    std::fs::write(root.join(REQUIRES_FILE), "# nothing\n\n  \n").unwrap();
    assert_eq!(read_required_runtime(&root), None);

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn runtime_relation_matches_the_online_declaration_shape() {
    /*
     * 必须与地图源里那条 `guid = "oc2.oc2diylevelruntimewloader"` +
     * `min_version = "3.10.2"` 完全同形：依赖表判定、缺依赖提示与跳转
     * 都靠 guid 匹配，形状不同会让两份声明各自算一遍。
     */
    let relation = runtime_relation("3.10.2").expect("有版本号就该产出关系");
    assert_eq!(relation.kind, crate::models::RelationKind::Requires);
    assert_eq!(relation.guid, "oc2.oc2diylevelruntimewloader");
    assert_eq!(relation.version, ">=3.10.2", "只声明下限");
    assert!(!relation.optional, "缺依赖要提示，不能静默");
    assert!(relation.prerelease.is_none());

    // 空版本号不能造出一条「不限版本」的关系
    assert!(runtime_relation("   ").is_none());
}

#[test]
fn scanning_reads_requires_txt_even_when_info_is_broken() {
    // info 坏掉时开关照样要用，外观要求也不该因此丢掉
    let game = std::env::temp_dir().join(format!("oc2-lv-req-scan-{}", uuid::Uuid::new_v4()));
    let set = game.join(DIYLEVEL_DIR).join(LEVELS_DIR).join("Broken");
    std::fs::create_dir_all(&set).unwrap();
    std::fs::write(set.join("info_broken"), b"not a bundle").unwrap();
    std::fs::write(set.join(REQUIRES_FILE), "3.10.2").unwrap();

    let sets = scan_level_sets(&game);
    assert_eq!(sets.len(), 1);
    assert!(sets[0].metadata.is_none(), "info 本身读不出来");
    assert_eq!(sets[0].required_runtime.as_deref(), Some("3.10.2"));

    std::fs::remove_dir_all(&game).unwrap();
}
