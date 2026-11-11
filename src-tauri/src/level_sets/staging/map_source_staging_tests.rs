use super::*;
use crate::archive;
use crate::level_sets::view::{bind_map_relation_snapshot, level_set_to_info};
use crate::models::MapInstallOrigin;

#[test]
fn discard_accepts_only_files_inside_the_cache_directory() {
    let root = std::env::temp_dir().join(format!("oc2-stage-{}", uuid::Uuid::new_v4()));
    let cache = root.join("cache");
    fs::create_dir_all(&cache).unwrap();
    let staged = cache.join("staged.zip");
    fs::write(&staged, b"zip").unwrap();
    let outside = root.join("user.zip");
    fs::write(&outside, b"zip").unwrap();

    // 缓存目录内的普通文件：解析成真实路径，交给调用方删除
    assert_eq!(
        cache_file_to_discard(&cache, &staged.to_string_lossy()).unwrap(),
        Some(fs::canonicalize(&staged).unwrap())
    );
    // 已经不在了：视为已清理，不报错
    assert!(
        cache_file_to_discard(&cache, &cache.join("missing.zip").to_string_lossy())
            .unwrap()
            .is_none()
    );
    // 缓存目录外的文件、以及目录本身都不是可丢弃的暂存文件
    assert!(cache_file_to_discard(&cache, &outside.to_string_lossy()).is_err());
    assert!(cache_file_to_discard(&cache, &cache.to_string_lossy()).is_err());
    assert!(outside.exists(), "缓存目录外的文件不能被删除");
    fs::remove_dir_all(root).unwrap();
}

fn zip_level_set(name: &str, name_zh: Option<&str>, author: &str) -> ZipLevelSet {
    serde_json::from_value(serde_json::json!({
        "dir_name": name,
        "name": name,
        "name_zh": name_zh,
        "author": author,
        "version": "0.73",
    }))
    .unwrap()
}

fn map_repository(level_set_name: &str, name_zh: &str, author: &str) -> MapRepositoryConfig {
    serde_json::from_value(serde_json::json!({
        "provider": "github",
        "username": "CH3NGYZ",
        "repository": "Overcooked-2-MOD-Manager-Repos",
        "level_set_name": level_set_name,
        "level_set_name_zh": name_zh,
        "author": author,
    }))
    .unwrap()
}

#[test]
fn a_matching_map_release_is_accepted_without_the_chinese_name() {
    // 老地图源不写中文名：退回「英文集名 + 作者」比较，与安装前一样能过
    let map = map_repository("RW_LevelSet", "", "RW");
    assert!(validate_map_release_sets(&[zip_level_set("RW_LevelSet", None, "RW")], &map).is_ok());
    // 中文名两边都有才比较：写对了也要能过
    let map = map_repository("GAODA", "胡闹糕中", "GAODA");
    assert!(
        validate_map_release_sets(&[zip_level_set("GAODA", Some("胡闹糕中"), "GAODA")], &map)
            .is_ok()
    );
}

#[test]
fn a_mismatched_map_release_is_rejected_before_the_user_confirms() {
    // 声明与实际不一致：装进去会让绑定错位、更新检测全失效，必须在暂存阶段失败
    let map = map_repository("GAODA", "胡闹糕中", "GAODA");
    let error =
        validate_map_release_sets(&[zip_level_set("GAODA", Some("胡闹达学"), "GAODA")], &map)
            .unwrap_err();
    assert!(
        error.starts_with("levelSet.zipMismatch?expected="),
        "错误必须是带 expected/actual 的 tag：{error}"
    );
    assert!(error.contains("actual="), "{error}");
}

#[test]
fn a_multi_set_map_release_is_accepted_when_declared_set_is_present() {
    let map = map_repository("RW_LevelSet", "", "RW");
    assert!(
        validate_map_release_sets(
            &[
                zip_level_set("RW_LevelSet", None, "RW"),
                zip_level_set("Other", None, "Other"),
            ],
            &map,
        )
        .is_ok()
    );
}

#[test]
fn a_multi_set_map_release_still_requires_declared_identity() {
    let map = map_repository("RW_LevelSet", "", "RW");
    let error = validate_map_release_sets(
        &[
            zip_level_set("Wrong", None, "Other"),
            zip_level_set("Another", None, "Other"),
        ],
        &map,
    )
    .unwrap_err();
    assert!(
        error.starts_with("levelSet.zipMismatch?expected="),
        "{error}"
    );
}

#[test]
fn declared_primary_is_selected_even_when_bundled_map_is_first() {
    let map = map_repository("pear", "梨之盛宴", "Author");
    let sets = vec![
        zip_level_set("Music", Some("音乐厨房"), "Author"),
        zip_level_set("pear", Some("梨之盛宴"), "Author"),
    ];
    assert_eq!(validate_map_release_sets(&sets, &map).unwrap().name, "pear");
}

#[test]
#[ignore = "需要 OC2_PEAR_REAL_ZIP 指向 pear_v11.0.zip；只安装到临时目录"]
fn real_pear_bundle_install_and_scan_keep_versions_separate() {
    let path = PathBuf::from(std::env::var_os("OC2_PEAR_REAL_ZIP").expect("OC2_PEAR_REAL_ZIP"));
    let sets = crate::level_sets::inspect_zip(&path).unwrap();
    let primary = sets.iter().find(|set| set.metadata.name == "pear").unwrap();
    let bundled = sets
        .iter()
        .find(|set| set.metadata.name == "Music")
        .unwrap();
    assert_eq!(primary.metadata.version, "11.0");
    assert_eq!(bundled.metadata.version, "1.1");
    let origin = MapInstallOrigin {
        level_set_name: primary.metadata.name.clone(),
        level_set_name_zh: primary.metadata.name_zh.clone().unwrap_or_default(),
        author: primary.metadata.author.clone(),
        mapset_version: "11.0".into(),
    };
    let root = std::env::temp_dir().join(format!("oc2-real-pear-{}", uuid::Uuid::new_v4()));
    let game = root.join("game");
    let manifests = root.join("manifests");
    fs::create_dir_all(&game).unwrap();
    let selected: Vec<_> = sets.iter().map(|set| set.dir_name.clone()).collect();
    let installed = crate::level_sets::install_parsed_sets(
        &sets,
        &path,
        &crate::level_sets::paths::test_roots(&game),
        &manifests,
        &selected,
        Some(&origin),
    )
    .unwrap();
    assert_eq!(installed.len(), 2);
    let mut config = crate::config::default_config();
    let mut map = map_repository("pear", "梨之盛宴", &origin.author);
    map.id = "pear-source".into();
    config.map_repositories = vec![map];
    let snapshots = archive::read_map_relation_snapshots(&manifests);
    let mut infos: Vec<_> =
        crate::level_sets::scan_level_sets(&crate::level_sets::paths::test_roots(&game))
            .into_iter()
            .map(level_set_to_info)
            .collect();
    assert_eq!(infos.len(), 2);
    for info in &mut infos {
        bind_map_relation_snapshot(info, &config, &snapshots);
        assert!(info.error.is_none(), "{:?}", info.error);
        match info.name.as_str() {
            "pear" => {
                assert_eq!(info.version, "11.0");
                assert_eq!(info.installed_mapset_version, "11.0");
                assert_eq!(info.map_id.as_deref(), Some("pear-source"));
            }
            "Music" => {
                assert_eq!(info.version, "1.1");
                assert!(info.installed_mapset_version.is_empty());
                assert!(info.map_id.is_none());
            }
            other => panic!("unexpected level set: {other}"),
        }
        eprintln!(
            "name={} info={} installed_asset={} map_id={:?}",
            info.name, info.version, info.installed_mapset_version, info.map_id
        );
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn zip_level_set_json_carries_conflicts_and_no_exists_flag() {
    // 前端契约：`exists`（同名即冲突）已删除，改为 `conflicts` 列表 ——
    // 同名不同身份的目录允许并存，不能再用「有同名目录」当冲突判据。
    let value = serde_json::to_value(zip_level_set("RW_LevelSet", None, "RW")).unwrap();
    assert!(value.get("conflicts").is_some(), "{value}");
    assert!(value.get("exists").is_none(), "{value}");
}
