use crate::framework::DIYLEVEL_DIR;
use crate::level_sets::identity::sanitize_dir_name;
use crate::level_sets::operations::set_all_level_sets_enabled;
use crate::level_sets::operations::set_level_set_enabled;
use crate::level_sets::paths::LEVELS_DIR;
use crate::level_sets::paths::disabled_root;
use crate::level_sets::paths::is_safe_relative;
use crate::level_sets::scan::locate_info_file;
use crate::level_sets::zip_info::is_info_file;
use crate::level_sets::zip_info::is_junk_entry;

#[test]
fn rejects_unsafe_directory_names() {
    let root = std::env::temp_dir().join(format!("oc2-lv-name-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join(DIYLEVEL_DIR).join(LEVELS_DIR)).unwrap();

    // 目录名必须是单一路径片段：这些都应该被拒绝
    for name in ["..", "../evil", "a/b", "a\\b", ""] {
        let result = set_level_set_enabled(&root, name, false);
        assert!(result.is_err(), "应拒绝目录名 {name:?}");
    }
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn toggling_moves_the_whole_directory_between_levels_and_disabled() {
    let root = std::env::temp_dir().join(format!("oc2-lv-move-{}", uuid::Uuid::new_v4()));
    let levels = root.join(DIYLEVEL_DIR).join(LEVELS_DIR);
    let set = levels.join("MySet");
    std::fs::create_dir_all(&set).unwrap();
    std::fs::write(set.join("info_MySet"), b"placeholder").unwrap();

    // 停用：整个目录移到 BepInEx/cache/modmanager_disabled_maps/
    let moved = set_level_set_enabled(&root, "MySet", false).unwrap();
    assert!(!set.exists(), "原目录应已移走");
    assert!(
        moved.is_dir(),
        "应出现在 BepInEx/cache/modmanager_disabled_maps/"
    );
    assert!(moved.join("info_MySet").is_file(), "目录内容要跟着走");

    // 启用：移回 levels/
    let back = set_level_set_enabled(&root, "MySet", true).unwrap();
    assert_eq!(back, set);
    assert!(back.join("info_MySet").is_file());

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn bulk_toggle_moves_every_set_and_skips_noop_ones() {
    let root = std::env::temp_dir().join(format!("oc2-lv-bulk-{}", uuid::Uuid::new_v4()));
    let base = root.join(DIYLEVEL_DIR);
    for name in ["A", "B"] {
        std::fs::create_dir_all(base.join(LEVELS_DIR).join(name)).unwrap();
    }
    std::fs::create_dir_all(disabled_root(&root).join("C")).unwrap();

    // 停用：只移动当前启用的 A、B，已经在 disabled 的 C 不参与
    let changed = set_all_level_sets_enabled(&root, false).unwrap();
    assert_eq!(changed, vec!["A".to_string(), "B".to_string()]);
    for name in ["A", "B", "C"] {
        assert!(
            disabled_root(&root).join(name).is_dir(),
            "{name} 应在 disabled"
        );
    }

    // 全部已是禁用态：再关一次不产生移动
    assert!(set_all_level_sets_enabled(&root, false).unwrap().is_empty());

    // 启用：三个都回到 levels
    let back = set_all_level_sets_enabled(&root, true).unwrap();
    assert_eq!(back.len(), 3);
    for name in ["A", "B", "C"] {
        assert!(
            base.join(LEVELS_DIR).join(name).is_dir(),
            "{name} 应在 levels"
        );
    }

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn refuses_to_overwrite_an_existing_set_at_the_target() {
    let root = std::env::temp_dir().join(format!("oc2-lv-clash-{}", uuid::Uuid::new_v4()));
    let base = root.join(DIYLEVEL_DIR);
    std::fs::create_dir_all(base.join(LEVELS_DIR).join("Dup")).unwrap();
    std::fs::create_dir_all(disabled_root(&root).join("Dup")).unwrap();

    // 两边同名：覆盖会静默丢掉用户已有的那个，必须拒绝
    let result = set_level_set_enabled(&root, "Dup", false);
    assert!(result.is_err(), "目标已存在时应拒绝");
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn locates_fixed_info_before_prefixed_variants() {
    let root = std::env::temp_dir().join(format!("oc2-lv-info-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    // 固定名优先：DLL 只认 `info`
    std::fs::write(root.join("info"), b"a").unwrap();
    std::fs::write(root.join("info_Other"), b"b").unwrap();
    assert_eq!(locate_info_file(&root), Some(root.join("info")));

    // 只有带前缀的唯一一个时才用它
    std::fs::remove_file(root.join("info")).unwrap();
    assert_eq!(locate_info_file(&root), Some(root.join("info_Other")));

    // 多个带前缀的：不猜
    std::fs::write(root.join("info_Another"), b"c").unwrap();
    assert_eq!(locate_info_file(&root), None);
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn sanitizes_level_set_names_into_directory_names() {
    assert_eq!(
        sanitize_dir_name("RW_LevelSet").as_deref(),
        Some("RW_LevelSet")
    );
    assert_eq!(sanitize_dir_name("a/b:c").as_deref(), Some("a_b_c"));
    assert_eq!(sanitize_dir_name("  "), None);
    assert_eq!(sanitize_dir_name("..."), None);
}

#[test]
fn junk_and_info_detection() {
    assert!(is_junk_entry("__MACOSX/x/._info"));
    assert!(is_junk_entry("a/.DS_Store"));
    assert!(!is_junk_entry("levels/a/info"));
    assert!(is_info_file("info"));
    assert!(is_info_file("info_rw"));
    assert!(!is_info_file("s_rw_1"));
}

#[test]
fn rejects_paths_that_escape_the_set_directory() {
    assert!(!is_safe_relative("../evil"));
    assert!(!is_safe_relative("/abs/path"));
    assert!(!is_safe_relative("C:/windows"));
    assert!(!is_safe_relative(""));
    assert!(is_safe_relative("s_rw_1"));
    assert!(is_safe_relative("sub/dir/file.dll"));
}
