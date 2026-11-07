use crate::framework::DIYLEVEL_DIR;
use crate::level_sets::info_cache::info_cache_parses;
use crate::level_sets::operations::locate_level_set_directory;
use crate::level_sets::operations::remove_level_set;
use crate::level_sets::paths::DISABLED_ROOT;
use crate::level_sets::paths::LEVELS_DIR;
use crate::level_sets::paths::disabled_root;
use crate::level_sets::scan::scan_level_sets;
use std::path::Path;
use std::path::PathBuf;

/// 每个用例独立根目录，避免并行测试互相踩；用例结束自己删掉。
fn workspace(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("oc2-lv-{tag}-{}", uuid::Uuid::new_v4()));
    let game = root.join("game");
    let recycle = root.join("recycle");
    let manifests = root.join("manifests");
    std::fs::create_dir_all(game.join(DIYLEVEL_DIR).join(LEVELS_DIR)).unwrap();
    std::fs::create_dir_all(disabled_root(&game)).unwrap();
    std::fs::create_dir_all(&recycle).unwrap();
    std::fs::create_dir_all(&manifests).unwrap();
    (root, game, recycle)
}

/// 缓存的命中与失效：同一文件第二次扫描不再解析，内容变了必须重新解析。
///
/// 只有命中没有失效就是脏数据 —— 用户替换关卡集后界面还显示旧名字。
/// 所以这里同时验证：命中时计数不涨，文件换掉后计数 +1 且错误内容跟着变。
#[test]
fn info_cache_parses_once_until_the_file_changes() {
    let (root, game, _recycle) = workspace("cache");
    let set_dir = game.join(DIYLEVEL_DIR).join(LEVELS_DIR).join("Cached");
    std::fs::create_dir_all(&set_dir).unwrap();
    let info = set_dir.join("info");
    std::fs::write(&info, b"not a bundle").unwrap();

    let first = scan_level_sets(&game);
    assert_eq!(first.len(), 1);
    let first_error = first[0].error.clone().expect("非 AssetBundle 应报错");
    assert_eq!(
        info_cache_parses(&info),
        Some(1),
        "首次扫描必须真正解析一次"
    );

    let second = scan_level_sets(&game);
    assert_eq!(second[0].error.as_deref(), Some(first_error.as_str()));
    assert_eq!(info_cache_parses(&info), Some(1), "文件没变时不该重复解析");

    // 换掉内容且长度不同：len/mtime 指纹失效，必须重新解析并拿到新结果。
    std::fs::write(&info, b"UnityFS\0\0\0\0\0\0\0\0\0\0\0\0").unwrap();
    let third = scan_level_sets(&game);
    assert_ne!(
        third[0].error.as_deref(),
        Some(first_error.as_str()),
        "文件被替换后应重新解析并拿到新结果"
    );
    assert_eq!(info_cache_parses(&info), Some(2), "指纹变化应再解析一次");

    std::fs::remove_dir_all(&root).unwrap();
}

/// 扫描要带回关卡集目录的真实绝对路径。
///
/// 详情面板把它与目录名合成「路径」一行；停用集必须指向 `disabled/`，
/// 否则用户照抄出来的路径在磁盘上根本不存在。坏掉的关卡集（info 解析
/// 失败）同样要带路径 —— 认不出元数据不等于找不到目录。
#[test]
fn scan_reports_the_absolute_directory_path_for_both_sides() {
    let (root, game, _recycle) = workspace("path");
    let enabled_dir = game.join(DIYLEVEL_DIR).join(LEVELS_DIR).join("EnabledSet");
    let disabled_dir = disabled_root(&game).join("DisabledSet");
    std::fs::create_dir_all(&enabled_dir).unwrap();
    std::fs::create_dir_all(&disabled_dir).unwrap();
    std::fs::write(enabled_dir.join("info"), b"not a bundle").unwrap();
    std::fs::write(disabled_dir.join("info"), b"not a bundle").unwrap();

    let sets = scan_level_sets(&game);
    assert_eq!(sets.len(), 2);
    let enabled = sets
        .iter()
        .find(|set| set.dir_name == "EnabledSet")
        .expect("应扫到启用集");
    let disabled = sets
        .iter()
        .find(|set| set.dir_name == "DisabledSet")
        .expect("应扫到停用集");

    assert!(enabled.enabled);
    assert_eq!(enabled.path, enabled_dir, "启用集应指向 levels/<目录名>");
    assert!(enabled.path.is_absolute(), "路径必须是绝对路径");
    assert!(
        enabled
            .path
            .ends_with(Path::new(LEVELS_DIR).join("EnabledSet"))
    );

    assert!(!disabled.enabled);
    assert_eq!(
        disabled.path, disabled_dir,
        "停用集应指向 disabled/<目录名>"
    );
    assert!(disabled.path.is_absolute(), "停用集路径也必须是绝对路径");
    assert!(
        disabled
            .path
            .ends_with(Path::new(DISABLED_ROOT).join("DisabledSet"))
    );

    std::fs::remove_dir_all(&root).unwrap();
}

/// 定位必须跟随 `enabled` 决定去 `levels/` 还是 `disabled/` 找。
///
/// 传错一侧就会「打开了别的目录」或「删掉了本该保留的副本」，所以这条
/// 用同名目录只存在于一侧来验证，而不是只看是否成功。
#[test]
fn locating_respects_the_enabled_side() {
    let (root, game, _recycle) = workspace("locate");
    let enabled_dir = game.join(DIYLEVEL_DIR).join(LEVELS_DIR).join("Same");
    std::fs::create_dir_all(&enabled_dir).unwrap();

    let located = locate_level_set_directory(&game, "Same", true).unwrap();
    assert_eq!(located, enabled_dir.canonicalize().unwrap());
    assert_eq!(
        locate_level_set_directory(&game, "Same", false).unwrap_err(),
        "levelSet.notFound",
        "停用侧没有同名目录时必须报 notFound"
    );

    std::fs::remove_dir_all(&root).unwrap();
}

/// 开着回收站时，删除是「搬进回收站」而不是真删：记录里要能看到这两个文件。
#[test]
fn deleting_moves_the_set_into_the_recycle_bin() {
    let (root, game, recycle) = workspace("recycle");
    let manifests = root.join("manifests");
    let set = game.join(DIYLEVEL_DIR).join(LEVELS_DIR).join("MySet");
    std::fs::create_dir_all(&set).unwrap();
    std::fs::write(set.join("info"), b"placeholder").unwrap();
    std::fs::write(set.join("s_myset_1"), b"level").unwrap();

    remove_level_set(&recycle, &manifests, &game, "MySet", true, true).unwrap();

    assert!(!set.exists(), "关卡集目录应已移走");
    let records = crate::recycle_bin::list(&recycle).unwrap();
    assert_eq!(records.len(), 1, "应产生一条回收记录");
    assert_eq!(records[0].file_count, 2, "两个文件都应进回收记录");

    std::fs::remove_dir_all(&root).unwrap();
}

/// 关掉回收站时直接删干净，且不留下空回收记录。
#[test]
fn deleting_without_recycling_removes_the_directory() {
    let (root, game, recycle) = workspace("purge");
    let manifests = root.join("manifests");
    let set = disabled_root(&game).join("MySet");
    std::fs::create_dir_all(&set).unwrap();
    std::fs::write(set.join("info"), b"placeholder").unwrap();

    // enabled=false：在 disabled/ 下找
    remove_level_set(&recycle, &manifests, &game, "MySet", false, false).unwrap();

    assert!(!set.exists(), "关卡集目录应已删除");
    assert!(
        crate::recycle_bin::list(&recycle).unwrap().is_empty(),
        "关闭回收时不该产生记录"
    );

    std::fs::remove_dir_all(&root).unwrap();
}

/// 不安全的目录名必须在碰盘之前就被拒；不存在的目录报 notFound 而不是报别的错。
#[test]
fn deleting_rejects_unsafe_names_and_missing_sets() {
    let (root, game, recycle) = workspace("guard");
    let manifests = root.join("manifests");

    for name in ["", "..", "../escape", "a/b", "C:evil"] {
        let result = remove_level_set(&recycle, &manifests, &game, name, true, true);
        assert!(result.is_err(), "应拒绝不安全的目录名 {name:?}");
    }
    assert_eq!(
        remove_level_set(&recycle, &manifests, &game, "Missing", true, true).unwrap_err(),
        "levelSet.notFound"
    );

    std::fs::remove_dir_all(&root).unwrap();
}
