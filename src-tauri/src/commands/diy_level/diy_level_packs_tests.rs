use super::*;

#[test]
fn counts_only_directories_under_levels() {
    let root = std::env::temp_dir().join(format!("oc2-diy-levels-{}", uuid::Uuid::new_v4()));
    let levels = root.join("levels");
    std::fs::create_dir_all(&levels).unwrap();

    // 空目录：0 个地图集
    assert_eq!(count_level_packs(&levels), 0);

    // 两个文件夹 + 三个散文件 → 只数出来 2
    std::fs::create_dir_all(levels.join("Pack A")).unwrap();
    std::fs::create_dir_all(levels.join("Pack B")).unwrap();
    std::fs::write(levels.join("readme.txt"), b"notes").unwrap();
    std::fs::write(levels.join("preview.png"), b"png").unwrap();
    std::fs::write(levels.join("config.json"), b"{}").unwrap();
    assert_eq!(count_level_packs(&levels), 2);

    // 嵌套目录只按顶层算：Pack A 里再建一个不算新地图集
    std::fs::create_dir_all(levels.join("Pack A/inner")).unwrap();
    assert_eq!(count_level_packs(&levels), 2);

    // 目录不存在：0，不报错
    std::fs::remove_dir_all(&root).unwrap();
    assert_eq!(count_level_packs(&levels), 0);
}
