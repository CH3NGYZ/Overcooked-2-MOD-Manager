use crate::asset_bundle::LevelSetMetadata;
use crate::level_sets::conflicts::conflicts_with_installed;
use crate::level_sets::conflicts::find_install_conflicts;
use crate::level_sets::identity::level_set_display_name;
use crate::level_sets::info_cache::InfoCacheEntry;
use crate::level_sets::info_cache::info_cache;
use crate::level_sets::info_cache::info_cache_key;
use crate::level_sets::install::existing_level_set_metadata;
use crate::level_sets::install::install_from_zip;
use crate::level_sets::paths::disabled_root;
use crate::level_sets::paths::levels_root;
use crate::level_sets::scan::LevelSetOnDisk;
use crate::level_sets::zip_inspect::ImportedLevelSet;
use crate::level_sets::zip_inspect::inspect_zip;
use crate::models::LevelSetLayout;
use std::path::Path;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

fn metadata(name: &str, name_zh: Option<&str>, author: &str, version: &str) -> LevelSetMetadata {
    LevelSetMetadata {
        name: name.into(),
        name_zh: name_zh.map(str::to_string),
        author: author.into(),
        version: version.into(),
        uid: format!("uid-{name}"),
        base_uid: None,
        levels: Vec::new(),
    }
}

fn installed(dir_name: &str, enabled: bool, metadata: LevelSetMetadata) -> LevelSetOnDisk {
    LevelSetOnDisk {
        dir_name: dir_name.into(),
        enabled,
        path: PathBuf::from(dir_name),
        metadata: Some(metadata),
        error: None,
        required_runtime: None,
        plugins: Vec::new(),
    }
}

fn imported(dir_name: &str, metadata: LevelSetMetadata) -> ImportedLevelSet {
    ImportedLevelSet {
        dir_name: dir_name.into(),
        metadata,
        files: Vec::new(),
        plugins: Vec::new(),
        layout: LevelSetLayout::Flattened,
    }
}

/// 往 `info` 的进程内缓存里塞一份伪造解析结果。
///
/// 造一份真的 AssetBundle 不现实，而身份判定只吃 `read_info_metadata` 的
/// 返回值：指纹（长度 + mtime）对上就直接命中，不会真的去解析。
fn inject_info_cache(info_path: &Path, metadata: LevelSetMetadata) {
    let stats = std::fs::metadata(info_path).unwrap();
    let mtime_nanos = stats
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|delta| delta.as_nanos())
        .unwrap_or(0);
    info_cache().lock().unwrap().insert(
        info_cache_key(info_path),
        InfoCacheEntry {
            len: stats.len(),
            mtime_nanos,
            result: Ok(metadata),
            parses: 0,
        },
    );
}

#[test]
fn conflicts_report_every_installed_copy_in_both_roots() {
    // 启用 + 停用各一份：两份都要卸，关着的那份也要报（否则装完仍有两份）。
    let installed = vec![
        installed(
            "RW_LevelSet",
            true,
            metadata("RW_LevelSet", None, "RW", "0.73"),
        ),
        installed(
            "RW_LevelSet_copy",
            false,
            metadata("RW_LevelSet", None, "RW", "0.71"),
        ),
    ];
    let sets = vec![imported(
        "RW_LevelSet",
        metadata("RW_LevelSet", None, "RW", "0.73"),
    )];

    let conflicts = conflicts_with_installed(&installed, &sets);
    assert_eq!(conflicts.len(), 2);
    assert_eq!(conflicts[0].handle, "RW_LevelSet");
    assert_eq!(conflicts[0].name, "RW_LevelSet");
    assert_eq!(conflicts[0].version, "0.73");
    assert!(conflicts[0].enabled);
    assert!(conflicts[0].same_version);
    assert_eq!(conflicts[1].handle, "RW_LevelSet_copy");
    assert!(!conflicts[1].enabled);
    assert!(!conflicts[1].same_version, "0.71 不是 0.73");
}

#[test]
fn conflicts_ignore_same_named_sets_with_another_identity() {
    // 实测「胡闹糕中」与「胡闹达学」的英文名与作者都叫 GAODA，只有中文名不同。
    let installed = vec![installed(
        "GAODA",
        true,
        metadata("GAODA", Some("胡闹糕中"), "GAODA", "1.0"),
    )];
    let other_map = vec![imported(
        "GAODA",
        metadata("GAODA", Some("胡闹达学"), "GAODA", "1.0"),
    )];
    assert!(conflicts_with_installed(&installed, &other_map).is_empty());

    // 反过来：目录名不同但身份相同，照样是同一张图（目录名不是身份）
    let renamed_copy = vec![imported(
        "GAODA_0405",
        metadata("GAODA", Some("胡闹糕中"), "GAODA", "1.0"),
    )];
    assert_eq!(conflicts_with_installed(&installed, &renamed_copy).len(), 1);
}

#[test]
fn conflicts_report_each_installed_copy_once() {
    // 一个 ZIP 里有两张同身份的图：同一份已装副本只报一条，不重复展示。
    let installed = vec![installed(
        "RW_LevelSet",
        true,
        metadata("RW_LevelSet", None, "RW", "0.73"),
    )];
    let sets = vec![
        imported("RW_A", metadata("RW_LevelSet", None, "RW", "0.73")),
        imported("RW_B", metadata("RW_LevelSet", None, "RW", "0.73")),
    ];
    assert_eq!(conflicts_with_installed(&installed, &sets).len(), 1);
}

#[test]
fn display_name_falls_back_to_the_chinese_name() {
    assert_eq!(
        level_set_display_name(&metadata("RW_LevelSet", None, "RW", "1.0")),
        "RW_LevelSet"
    );
    assert_eq!(
        level_set_display_name(&metadata("", Some("胡闹糕中"), "GAODA", "1.0")),
        "胡闹糕中"
    );
    assert_eq!(
        level_set_display_name(&metadata("", None, "GAODA", "1.0")),
        ""
    );
}

#[test]
fn existing_level_set_metadata_reads_the_disabled_root_too() {
    /*
     * 曾经的缺陷：只看 `levels/`。同名目录只在 `BepInEx/cache/modmanager_disabled_maps/` 时会得到
     * 「读不出身份」，于是另一张同名地图也被 `levelSet.targetExists`
     * 拒绝 —— 而本地根本没有那张图的副本可卸，用户无路可走。
     */
    let root =
        std::env::temp_dir().join(format!("oc2-lv-disabled-lookup-{}", uuid::Uuid::new_v4()));
    let levels = levels_root(&root);
    let disabled = disabled_root(&root);
    std::fs::create_dir_all(&levels).unwrap();
    let stale = disabled.join("GAODA");
    std::fs::create_dir_all(&stale).unwrap();
    let info = stale.join("info");
    std::fs::write(&info, b"not-a-real-bundle").unwrap();
    inject_info_cache(&info, metadata("GAODA", Some("胡闹糕中"), "GAODA", "1.0"));

    let found = existing_level_set_metadata(&levels, &disabled, "GAODA").unwrap();
    assert_eq!(found.name_zh.as_deref(), Some("胡闹糕中"));
    // 两个根都没有这个目录名：读不出身份，调用方按原规则拒绝
    assert!(existing_level_set_metadata(&levels, &disabled, "Missing").is_none());

    std::fs::remove_dir_all(&root).unwrap();
}

/// 只在 `BepInEx/cache/modmanager_disabled_maps/` 里有同名目录、且身份不同时，必须换名并存而不是拒绝。
///
/// 用真实 RW 包（ZIP 里的身份来自真 `info`），停用根那份的身份用
/// `inject_info_cache` 伪造 —— 这是本修复最关键的一条端到端回归。
#[test]
#[ignore = "需要 .tmp/rw.zip，用 --ignored 单独跑"]
fn a_same_named_set_in_disabled_with_another_identity_is_renamed() {
    let zip = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.tmp/rw.zip");
    if !zip.is_file() {
        eprintln!("跳过：没有 .tmp/rw.zip");
        return;
    }
    let root = std::env::temp_dir().join(format!("oc2-lv-disabled-{}", uuid::Uuid::new_v4()));
    let game = root.join("game");
    std::fs::create_dir_all(levels_root(&game)).unwrap();
    let stale = disabled_root(&game).join("RW_LevelSet");
    std::fs::create_dir_all(&stale).unwrap();
    let info = stale.join("info");
    std::fs::write(&info, b"not-a-real-bundle").unwrap();
    inject_info_cache(
        &info,
        metadata("AnotherMap", Some("另一张图"), "Someone", "2.0"),
    );

    // 停用根里那张是另一张图：导入前不该报冲突
    let sets = inspect_zip(&zip).unwrap();
    assert!(
        find_install_conflicts(&game, &sets).is_empty(),
        "同名但身份不同的停用目录不算冲突"
    );

    let installed = install_from_zip(
        &zip,
        &game,
        &game.join("manifests"),
        &["RW_LevelSet".to_string()],
    )
    .unwrap();
    assert_eq!(installed.len(), 1);
    assert_ne!(
        installed[0], "RW_LevelSet",
        "同名不同身份应换名并存，而不是 levelSet.targetExists"
    );
    assert!(levels_root(&game).join(&installed[0]).is_dir());
    assert!(stale.is_dir(), "停用根里的用户数据不能被改动");

    std::fs::remove_dir_all(&root).unwrap();
}
