use crate::asset_bundle::LevelSetMetadata;
use crate::level_sets::install::existing_level_set_metadata;
use crate::level_sets::install::install_from_zip;
use crate::level_sets::paths::disabled_root;
use crate::level_sets::paths::levels_root;
use crate::level_sets::seed_info_metadata;
use std::path::PathBuf;

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

#[test]
fn existing_level_set_metadata_reads_the_disabled_root_too() {
    /*
     * 曾经的缺陷：只看 `levels/`。同名目录只在 `BepInEx/cache/modmanager_disabled_maps/` 时会得到
     * 「读不出身份」，于是另一张同名地图也被 `levelSet.targetExists`
     * 拒绝 —— 而本地根本没有那张图的副本可卸，用户无路可走。
     */
    let root =
        std::env::temp_dir().join(format!("oc2-lv-disabled-lookup-{}", uuid::Uuid::new_v4()));
    let levels = levels_root(&crate::level_sets::paths::test_roots(&root)).unwrap();
    let disabled = disabled_root(&root);
    std::fs::create_dir_all(&levels).unwrap();
    let stale = disabled.join("GAODA");
    std::fs::create_dir_all(&stale).unwrap();
    let info = stale.join("info");
    std::fs::write(&info, b"not-a-real-bundle").unwrap();
    seed_info_metadata(&info, metadata("GAODA", Some("胡闹糕中"), "GAODA", "1.0"));

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
    std::fs::create_dir_all(levels_root(&crate::level_sets::paths::test_roots(&game)).unwrap())
        .unwrap();
    let stale = disabled_root(&game).join("RW_LevelSet");
    std::fs::create_dir_all(&stale).unwrap();
    let info = stale.join("info");
    std::fs::write(&info, b"not-a-real-bundle").unwrap();
    seed_info_metadata(
        &info,
        metadata("AnotherMap", Some("另一张图"), "Someone", "2.0"),
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
    assert!(
        levels_root(&crate::level_sets::paths::test_roots(&game))
            .unwrap()
            .join(&installed[0])
            .is_dir()
    );
    assert!(stale.is_dir(), "停用根里的用户数据不能被改动");

    std::fs::remove_dir_all(&root).unwrap();
}
