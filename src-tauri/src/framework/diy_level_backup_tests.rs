use super::*;

#[test]
fn backup_copies_only_the_diy_level_folder() {
    let game = std::env::temp_dir().join(format!("oc2-diy-backup-game-{}", uuid::Uuid::new_v4()));
    let backups =
        std::env::temp_dir().join(format!("oc2-diy-backup-root-{}", uuid::Uuid::new_v4()));
    // 框架目录：DLL + 一个地图集
    std::fs::create_dir_all(game.join("BepInEx/plugins/OC2DIYLevel/levels/PackA")).unwrap();
    std::fs::write(
        game.join("BepInEx/plugins/OC2DIYLevel/DIYLevel.dll"),
        b"dll",
    )
    .unwrap();
    std::fs::write(
        game.join("BepInEx/plugins/OC2DIYLevel/levels/PackA/map.txt"),
        b"map",
    )
    .unwrap();
    // 同级还有别的东西，绝不能被备份进来
    std::fs::create_dir_all(game.join("BepInEx/plugins/OtherMod")).unwrap();
    std::fs::write(game.join("BepInEx/plugins/OtherMod/other.dll"), b"other").unwrap();
    std::fs::create_dir_all(game.join("BepInEx/core")).unwrap();
    std::fs::write(game.join("BepInEx/core/BepInEx.dll"), b"core").unwrap();

    let manifests =
        std::env::temp_dir().join(format!("oc2-diy-manifests-{}", uuid::Uuid::new_v4()));
    let backup =
        backup_framework(&game, &backups, &DIYLEVEL_BACKUP, &manifests, "diy-repo").unwrap();
    assert!(backup.starts_with(&backups), "备份必须落在备份根下");
    assert!(
        backup
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(DIYLEVEL_BACKUP_PREFIX),
        "备份目录名必须带前缀，恢复/删除才认得出"
    );
    /*
     * 备份内保留**游戏相对路径**：`BepInEx/plugins/OC2DIYLevel/...`。
     * 恢复时按同一相对路径拷回，不需要为每个框架单独记映射。
     */
    const IN_BACKUP: &str = "BepInEx/plugins/OC2DIYLevel";
    assert_eq!(
        std::fs::read(backup.join(IN_BACKUP).join("DIYLevel.dll")).unwrap(),
        b"dll"
    );
    assert_eq!(
        std::fs::read(backup.join(IN_BACKUP).join("levels/PackA/map.txt")).unwrap(),
        b"map"
    );
    // 兄弟插件与加载器核心都不在备份里
    assert!(!backup.join("BepInEx/plugins/OtherMod").exists());
    assert!(!backup.join("BepInEx/core").exists());

    /*
     * 清单快照必须一起进备份：这是「恢复后还能正常卸载」的前提。
     * 没有现成清单时按磁盘实际文件现造一份。
     */
    let manifest = read_backup_manifest(&backup).expect("备份必须带清单快照");
    assert_eq!(manifest.repository_id, "diy-repo");
    assert!(
        manifest
            .files
            .iter()
            .any(|file| file.ends_with("OC2DIYLevel/DIYLevel.dll")),
        "清单应包含框架主文件：{:?}",
        manifest.files
    );

    // 列表能列出它
    let listed = list_diy_level_backups(&backups);
    assert_eq!(listed.len(), 1);
    assert!(listed[0].contains("DIYLevel.backup-"));

    std::fs::remove_dir_all(&game).unwrap();
    std::fs::remove_dir_all(&backups).unwrap();
    // manifests 目录可能从未被创建（没有现成清单时不需要它）
    let _ = std::fs::remove_dir_all(&manifests);
}

#[test]
fn backup_requires_the_diy_level_folder_to_exist() {
    let game = std::env::temp_dir().join(format!("oc2-diy-missing-{}", uuid::Uuid::new_v4()));
    let backups = std::env::temp_dir().join(format!("oc2-diy-none-{}", uuid::Uuid::new_v4()));
    let manifests = std::env::temp_dir().join(format!("oc2-diy-m-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(game.join("BepInEx/plugins")).unwrap();
    assert!(
        backup_framework(&game, &backups, &DIYLEVEL_BACKUP, &manifests, "diy").is_err(),
        "没有 OC2DIYLevel 目录时必须报错，而不是生成空备份"
    );
    std::fs::remove_dir_all(&game).unwrap();
}

#[test]
fn only_timestamped_backups_inside_the_backup_root_are_accepted() {
    let root = std::env::temp_dir().join(format!("oc2-diy-valid-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join("DIYLevel.backup-20261029120000-abc")).unwrap();
    std::fs::create_dir_all(root.join("SomethingElse")).unwrap();
    // 备份根之外的目录，名字对也不能用
    let outside = std::env::temp_dir().join(format!("oc2-diy-outside-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(outside.join("DIYLevel.backup-20261029120000-abc")).unwrap();

    let valid = root.join("DIYLevel.backup-20261029120000-abc");
    assert!(validate_diy_level_backup_in(&valid, &root).is_ok());
    // 名字不带前缀
    assert!(validate_diy_level_backup_in(&root.join("SomethingElse"), &root).is_err());
    // 父目录不是备份根
    assert!(
        validate_diy_level_backup_in(&outside.join("DIYLevel.backup-20261029120000-abc"), &root)
            .is_err()
    );

    std::fs::remove_dir_all(&root).unwrap();
    std::fs::remove_dir_all(&outside).unwrap();
}
