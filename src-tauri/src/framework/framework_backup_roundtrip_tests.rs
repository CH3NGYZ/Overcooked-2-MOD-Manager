use super::*;
use crate::bepinex::install::BEPINEX_MANIFEST_ID;

/// 造一个最小 BepInEx 框架目录（含核心程序集与根目录加载器文件）。
fn write_bepinex(game: &Path) {
    std::fs::create_dir_all(game.join("BepInEx/core")).unwrap();
    std::fs::write(game.join("BepInEx/core/BepInEx.dll"), b"core").unwrap();
    std::fs::write(game.join("winhttp.dll"), b"loader").unwrap();
    std::fs::write(game.join("doorstop_config.ini"), b"cfg").unwrap();
    // MOD 目录：属于用户数据，必须一起备份但不能被当成框架文件
    std::fs::create_dir_all(game.join("BepInEx/plugins/SomeMod")).unwrap();
    std::fs::write(game.join("BepInEx/plugins/SomeMod/mod.dll"), b"mod").unwrap();
}

#[test]
fn bepinex_backup_restores_root_loader_files_and_manifest() {
    let root = std::env::temp_dir().join(format!("oc2-fw-rt-{}", uuid::Uuid::new_v4()));
    let game = root.join("game");
    let backups = root.join("backups");
    let manifests = root.join("manifests");
    std::fs::create_dir_all(&game).unwrap();
    std::fs::create_dir_all(&manifests).unwrap();
    write_bepinex(&game);

    let backup = backup_framework(
        &game,
        &backups,
        &BEPINEX_BACKUP,
        &manifests,
        BEPINEX_MANIFEST_ID,
    )
    .unwrap();

    /*
     * 备份必须包含游戏根目录的加载器文件。
     * 早期只拷 `BepInEx/`，恢复后 winhttp.dll 缺失 —— 用户在界面看到的
     * 就是「恢复完了但框架没装好」。
     */
    assert!(backup.join("winhttp.dll").is_file(), "备份缺 winhttp.dll");
    assert!(
        backup.join("doorstop_config.ini").is_file(),
        "备份缺 doorstop_config.ini"
    );
    assert!(
        backup.join("BepInEx/core/BepInEx.dll").is_file(),
        "备份缺核心程序集"
    );

    // 模拟卸载：清掉游戏目录里的框架（含根文件）
    std::fs::remove_dir_all(game.join("BepInEx")).unwrap();
    std::fs::remove_file(game.join("winhttp.dll")).unwrap();
    std::fs::remove_file(game.join("doorstop_config.ini")).unwrap();
    assert!(!crate::bepinex::installed(&game));

    // 恢复
    restore_framework_backup(
        &game,
        &backup,
        &BEPINEX_BACKUP,
        &manifests,
        BEPINEX_MANIFEST_ID,
    )
    .unwrap();

    // 核心程序集与根文件都必须回来
    assert!(
        crate::bepinex::installed(&game),
        "恢复后必须重新判定为已安装"
    );
    assert!(game.join("winhttp.dll").is_file(), "恢复后缺 winhttp.dll");
    assert!(
        game.join("doorstop_config.ini").is_file(),
        "恢复后缺 doorstop_config.ini"
    );
    assert!(
        game.join("BepInEx/plugins/SomeMod/mod.dll").is_file(),
        "恢复必须带回 plugins 里的 MOD"
    );

    // 清单必须写回：这是「恢复后还能再卸载」的关键
    let manifest =
        archive::read_manifest(&manifests, BEPINEX_MANIFEST_ID).expect("恢复后必须存在安装清单");
    assert!(
        manifest.files.iter().any(|file| file == "winhttp.dll"),
        "清单应包含根目录加载器文件：{:?}",
        manifest.files
    );
    assert!(
        manifest
            .files
            .iter()
            .any(|file| file.ends_with("BepInEx/core/BepInEx.dll")),
        "清单应包含核心程序集：{:?}",
        manifest.files
    );

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn restoring_without_a_backup_manifest_still_writes_one() {
    /*
     * 旧版本产生的备份没有清单快照。恢复时必须按磁盘实际文件现造一份，
     * 否则用户升级管理器后恢复旧备份，再卸载仍然会报
     * 「没有安装清单，拒绝删除未知文件」。
     */
    let root = std::env::temp_dir().join(format!("oc2-fw-legacy-{}", uuid::Uuid::new_v4()));
    let game = root.join("game");
    let backups = root.join("backups");
    let manifests = root.join("manifests");
    std::fs::create_dir_all(&game).unwrap();
    std::fs::create_dir_all(&manifests).unwrap();
    write_bepinex(&game);

    let backup = backup_framework(
        &game,
        &backups,
        &BEPINEX_BACKUP,
        &manifests,
        BEPINEX_MANIFEST_ID,
    )
    .unwrap();
    // 模拟旧备份：删掉清单快照
    std::fs::remove_file(backup.join(FRAMEWORK_BACKUP_MANIFEST)).unwrap();
    assert!(read_backup_manifest(&backup).is_none());

    // 清空管理器清单目录，模拟「从未装过、也没有清单」
    std::fs::remove_dir_all(&manifests).unwrap();
    std::fs::create_dir_all(&manifests).unwrap();

    restore_framework_backup(
        &game,
        &backup,
        &BEPINEX_BACKUP,
        &manifests,
        BEPINEX_MANIFEST_ID,
    )
    .unwrap();
    let manifest = archive::read_manifest(&manifests, BEPINEX_MANIFEST_ID)
        .expect("旧备份恢复后也必须现造出清单");
    assert!(!manifest.files.is_empty());

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn diy_level_backup_never_touches_the_bepinex_loader() {
    /*
     * DIYLevel 的备份范围必须严格是 `BepInEx/plugins/OC2DIYLevel`：
     * `BepInEx/core` 归 BepInEx 自己的备份管，两边都备份会互相覆盖。
     */
    let root = std::env::temp_dir().join(format!("oc2-fw-diy-{}", uuid::Uuid::new_v4()));
    let game = root.join("game");
    let backups = root.join("backups");
    let manifests = root.join("manifests");
    std::fs::create_dir_all(&game).unwrap();
    std::fs::create_dir_all(&manifests).unwrap();
    write_bepinex(&game);
    std::fs::create_dir_all(game.join("BepInEx/plugins/OC2DIYLevel/levels/PackA")).unwrap();
    std::fs::write(
        game.join("BepInEx/plugins/OC2DIYLevel/DIYLevel.dll"),
        b"diy",
    )
    .unwrap();

    let backup = backup_framework(&game, &backups, &DIYLEVEL_BACKUP, &manifests, "diy-id").unwrap();
    // 只有 DIYLevel 目录，没有 core、没有 winhttp.dll
    assert!(
        backup
            .join("BepInEx/plugins/OC2DIYLevel/DIYLevel.dll")
            .is_file()
    );
    assert!(!backup.join("BepInEx/core").exists());
    assert!(!backup.join("winhttp.dll").exists());

    std::fs::remove_dir_all(&root).unwrap();
}

/// 恢复 BepInEx 后，`.before-restore-*` 暂存目录必须被删掉。
///
/// 它落在游戏根目录（`BepInEx.before-restore-<ts>`）。留着不只是占空间：
/// MOD 扫描会把里面的 DLL 当成插件，用户看到「刚恢复完就多出一批重复 MOD」。
#[test]
fn bepinex_restore_removes_the_staging_directory() {
    let root = std::env::temp_dir().join(format!("oc2-fw-stage-{}", uuid::Uuid::new_v4()));
    let game = root.join("game");
    let backups = root.join("backups");
    let manifests = root.join("manifests");
    std::fs::create_dir_all(&game).unwrap();
    std::fs::create_dir_all(&manifests).unwrap();
    write_bepinex(&game);

    let backup = backup_framework(
        &game,
        &backups,
        &BEPINEX_BACKUP,
        &manifests,
        BEPINEX_MANIFEST_ID,
    )
    .unwrap();
    // 备份之后改动目录，让「恢复前内容」与备份内容不同
    std::fs::write(game.join("BepInEx/core/Extra.dll"), b"extra").unwrap();

    restore_framework_backup(
        &game,
        &backup,
        &BEPINEX_BACKUP,
        &manifests,
        BEPINEX_MANIFEST_ID,
    )
    .unwrap();

    let leftovers: Vec<_> = std::fs::read_dir(&game)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".before-restore-"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "恢复后不应残留暂存目录：{leftovers:?}"
    );

    std::fs::remove_dir_all(&root).unwrap();
}

/// 恢复 DIYLevel 后同样不能残留 —— 它的暂存目录在 `plugins/` 里面，
/// 不删就必然被 MOD 扫描当成插件（比 BepInEx 的情况更严重）。
#[test]
fn diy_level_restore_removes_the_staging_directory_inside_plugins() {
    let root = std::env::temp_dir().join(format!("oc2-diy-stage-{}", uuid::Uuid::new_v4()));
    let game = root.join("game");
    let backups = root.join("backups");
    let manifests = root.join("manifests");
    std::fs::create_dir_all(&game).unwrap();
    std::fs::create_dir_all(&manifests).unwrap();

    let diy = game.join(DIYLEVEL_DIR);
    std::fs::create_dir_all(&diy).unwrap();
    std::fs::write(diy.join("OC2DIYLevel.dll"), b"v1").unwrap();

    let backup =
        backup_framework(&game, &backups, &DIYLEVEL_BACKUP, &manifests, "oc2diylevel").unwrap();
    // 备份后放一个不同版本，确保恢复前的目录非空
    std::fs::write(diy.join("OC2DIYLevel.dll"), b"v2").unwrap();

    restore_framework_backup(&game, &backup, &DIYLEVEL_BACKUP, &manifests, "oc2diylevel").unwrap();

    let plugins = game.join("BepInEx/plugins");
    let leftovers: Vec<_> = std::fs::read_dir(&plugins)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".before-restore-"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "DIYLevel 恢复后 plugins 下不应残留暂存目录：{leftovers:?}"
    );
    // 恢复出来的内容必须是备份那一刻的
    assert_eq!(std::fs::read(diy.join("OC2DIYLevel.dll")).unwrap(), b"v1");

    std::fs::remove_dir_all(&root).unwrap();
}
