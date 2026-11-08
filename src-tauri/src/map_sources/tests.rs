use super::community::community_map_label;
use super::community::{
    GITEE_COMMUNITY_MAP_SOURCE_ID, GITHUB_COMMUNITY_MAP_SOURCE_ID, community_map_declaration,
};
use super::document::{MapSourceMode, parse_document};
use super::identity::map_identity;
use super::identity::map_repository_id;
use super::release_tags::declaration_probe;
use super::state::map_is_available;
use super::state::merge_maps_keeping_runtime_state;
use super::state::replace_maps_keeping_runtime_state;
use crate::models::CommunityMapRepository;
use crate::models::Provider;
use crate::repository_sources::SourceLocation;

#[test]
fn community_map_repositories_are_declared_as_release_tag_sources() {
    let repository = CommunityMapRepository {
        provider: Provider::Gitee,
        username: "Owner".to_string(),
        repository: "Maps".to_string(),
    };
    let declaration = community_map_declaration(&repository);
    assert_eq!(declaration.mode, MapSourceMode::ReleaseTags);
    assert_eq!(declaration.provider, Some(Provider::Gitee));
    assert_eq!(declaration.username, "Owner");
    assert_eq!(declaration.repository, "Maps");
    assert_eq!(declaration.nickname, "Owner/Maps");
    assert_eq!(declaration.builtin, Some(false));
    assert!(declaration.map.is_none());
    assert_eq!(declaration.page_size, None);
    assert_eq!(declaration.max_pages, None);
    /*
     * `declaration_probe` 的字段形状是 `expect` 出来的（它只用 serde 投影
     * 一个 RepositoryConfig）：组合出错的声明会在第一次刷新时 panic。
     * 这里把这条路径也走一遍，纯本地、不联网。
     */
    let probe = declaration_probe(&declaration);
    assert_eq!(probe.provider, Provider::Gitee);
    assert_eq!(probe.username, "Owner");
    assert_eq!(probe.repository, "Maps");
}

#[test]
fn community_map_labels_share_one_source_per_platform_with_five_languages() {
    let github = community_map_label(&Provider::Github);
    let gitee = community_map_label(&Provider::Gitee);
    assert_eq!(github.id, GITHUB_COMMUNITY_MAP_SOURCE_ID);
    assert_eq!(gitee.id, GITEE_COMMUNITY_MAP_SOURCE_ID);
    for label in [github, gitee] {
        for locale in ["zh-CN", "zh-TW", "en", "ko", "ja"] {
            let name = label
                .name_i18n
                .get(locale)
                .unwrap_or_else(|| panic!("缺少 {locale} 的源名"));
            assert!(!name.trim().is_empty());
        }
        assert!(!label.name.trim().is_empty());
    }
}

fn minimal(name: &str, author: &str, set: &str) -> String {
    format!(
        r#"
[[maps]]
name = "{name}"
provider = "github"
username = "u"
repository = "r"
level_set_name = "{set}"
author = "{author}"
enabled = true
asset_rules = [{{ type = "exact", value = "map.zip" }}]
"#
    )
}

fn bundled() -> SourceLocation {
    SourceLocation::parse("maps.toml").unwrap()
}

#[test]
fn parses_a_minimal_map_document() {
    let doc = parse_document(
        &minimal("RW", "RW", "RW_LevelSet"),
        &bundled(),
        &Default::default(),
    )
    .unwrap();
    assert_eq!(doc.maps.len(), 1);
    assert_eq!(doc.maps[0].level_set_name, "RW_LevelSet");
    assert_eq!(doc.maps[0].author, "RW");
}

#[test]
fn identity_is_author_plus_level_set_name_case_insensitive() {
    let doc = parse_document(
        &minimal("RW", "RW", "RW_LevelSet"),
        &bundled(),
        &Default::default(),
    )
    .unwrap();
    assert_eq!(map_identity(&doc.maps[0]), "rw\u{1}rw_levelset");
}

#[test]
fn rejects_duplicate_identity_within_one_document() {
    // 同一个 author + level_set_name 出现两次：安装时会两份抢同一个目录
    let text = format!(
        "{}{}",
        minimal("A", "RW", "Same"),
        minimal("B", "rw", "same")
    );
    let err = parse_document(&text, &bundled(), &Default::default()).unwrap_err();
    assert!(err.to_string().contains("mapSource.duplicateEntry"));
}

#[test]
fn same_set_name_under_different_authors_is_not_a_duplicate() {
    let text = format!(
        "{}{}",
        minimal("A", "Alice", "Shared"),
        minimal("B", "Bob", "Shared")
    );
    assert!(parse_document(&text, &bundled(), &Default::default()).is_ok());
}

#[test]
fn requires_level_set_name() {
    let text = minimal("M", "A", "").replace("level_set_name = \"\"", "");
    let err = parse_document(&text, &bundled(), &Default::default()).unwrap_err();
    assert!(err.to_string().contains("mapSource.missingLevelSetName"));
}

#[test]
fn requires_asset_rules() {
    let text = minimal("M", "A", "S").replace(
        "asset_rules = [{ type = \"exact\", value = \"map.zip\" }]",
        "",
    );
    let err = parse_document(&text, &bundled(), &Default::default()).unwrap_err();
    assert!(err.to_string().contains("mapSource.missingAssetRules"));
}

#[test]
fn region_filtering_matches_the_repository_semantics() {
    // 走 parse_document 而不是裸 from_str：后者绕过 serde 的字段默认值
    let mut map = parse_document(&minimal("M", "A", "S"), &bundled(), &Default::default())
        .unwrap()
        .maps
        .into_iter()
        .next()
        .unwrap();
    assert!(map_is_available(&map, "chinese_mainland"));
    assert!(map_is_available(&map, "international"));

    map.region = Some("chinese_mainland".into());
    assert!(map_is_available(&map, "chinese_mainland"));
    assert!(!map_is_available(&map, "international"));

    map.region = Some("international".into());
    assert!(!map_is_available(&map, "chinese_mainland"));
    assert!(map_is_available(&map, "international"));

    // 禁用一律不可见，与地区无关
    map.enabled = false;
    assert!(!map_is_available(&map, "international"));
}

/*
 * 刷新源树会整批重建地图条目。缓存必须跟着 ID 接回来，否则 5 分钟
 * TTL 形同虚设 —— 每次刷新都把整批地图的 API 重打一遍。
 *
 * 这条测试是「必须看着它红过一次」的守卫：把 224 行附近的
 * `map.cached_releases = old.cached_releases.clone()` 删掉，它会失败。
 */
#[test]
fn refreshing_the_tree_keeps_release_cache_by_stable_id() {
    let mut config = crate::config::default_config();
    let mut first = parse_document(&minimal("M", "A", "S"), &bundled(), &Default::default())
        .unwrap()
        .maps
        .into_iter()
        .next()
        .unwrap();
    first.id = map_repository_id("maps-x", &first);
    first.releases_cached_at = Some(1_700_000_000_000);
    first.cached_releases = vec![crate::models::ReleaseInfo {
        release_id: None,
        tag_name: "v1.2".into(),
        version: None,
        name: String::new(),
        body: String::new(),
        published_at: None,
        prerelease: false,
        draft: false,
        assets: Vec::new(),
    }];
    config.map_repositories = vec![first.clone()];

    // 源文档重建：同样的身份 → 同样的 ID
    let mut fresh = parse_document(&minimal("M", "A", "S"), &bundled(), &Default::default())
        .unwrap()
        .maps
        .into_iter()
        .next()
        .unwrap();
    fresh.id = map_repository_id("maps-x", &fresh);
    assert!(fresh.cached_releases.is_empty(), "重建出来的条目没有缓存");

    replace_maps_keeping_runtime_state(&mut config, vec![fresh]);
    assert_eq!(config.map_repositories.len(), 1);
    assert_eq!(
        config.map_repositories[0].cached_releases.len(),
        1,
        "刷新后必须接回 Release 缓存"
    );
    assert_eq!(
        config.map_repositories[0].releases_cached_at,
        Some(1_700_000_000_000),
        "缓存时间戳同样是命中的依据"
    );
}

/*
 * `enabled` 是作者的声明，不是用户的偏好：源文档里关掉某张图后，
 * 刷新不能让它在本地复活。管理器没有「用户手动隐藏地图」这个操作，
 * 所以要保留它反而会吃掉作者的意图。
 */
#[test]
fn refreshing_the_tree_follows_the_authors_enabled_flag() {
    let mut config = crate::config::default_config();
    let mut stale = parse_document(&minimal("M", "A", "S"), &bundled(), &Default::default())
        .unwrap()
        .maps
        .into_iter()
        .next()
        .unwrap();
    stale.id = map_repository_id("maps-x", &stale);
    stale.enabled = true;
    config.map_repositories = vec![stale];

    let mut fresh = parse_document(
        &minimal("M", "A", "S").replace("enabled = true", "enabled = false"),
        &bundled(),
        &Default::default(),
    )
    .unwrap()
    .maps
    .into_iter()
    .next()
    .unwrap();
    fresh.id = map_repository_id("maps-x", &fresh);

    replace_maps_keeping_runtime_state(&mut config, vec![fresh]);
    assert!(
        !config.map_repositories[0].enabled,
        "作者在源文档里下线的地图，刷新后不应恢复可用"
    );
}

#[test]
fn map_id_is_stable_across_rebuilds_and_unique_per_identity() {
    let build = |source: &str, author: &str, set: &str| {
        let map = parse_document(&minimal("M", author, set), &bundled(), &Default::default())
            .unwrap()
            .maps
            .into_iter()
            .next()
            .unwrap();
        map_repository_id(source, &map)
    };
    // 同一身份重复派生 → 同一个 ID（缓存与安装清单都挂在它上面）
    assert_eq!(
        build("maps-x", "RW", "RW_LevelSet"),
        build("maps-x", "RW", "RW_LevelSet")
    );
    // 换作者或换集名 → 新 ID（否则两张不同的地图会共用一份缓存）
    assert_ne!(
        build("maps-x", "RW", "RW_LevelSet"),
        build("maps-x", "RW", "RW_LevelSet2")
    );
    assert_ne!(
        build("maps-x", "RW", "RW_LevelSet"),
        build("maps-x", "Alice", "RW_LevelSet")
    );
    // 换源 → 新 ID（不同源可以各自声明同名地图）
    assert_ne!(
        build("maps-x", "RW", "RW_LevelSet"),
        build("maps-y", "RW", "RW_LevelSet")
    );
}

/// 只改展示字段（分组名）也必须判定为「有变化」。
///
/// 回归：ID 由「源 ID + author + level_set_name」派生，作者把 `group_i18n`
/// 从 `RW` 改成作者名时 ID 不变。早先的实现只比 ID 与长度，于是刷新回来的
/// 新分组被判成「没变化」、配置不写盘，界面永远显示旧分组。
#[test]
fn a_display_only_map_change_still_counts_as_change() {
    let fresh = parse_document(
        &minimal("RW", "RW", "RW_LevelSet"),
        &bundled(),
        &Default::default(),
    )
    .unwrap()
    .maps;
    let mut stored = fresh.clone();
    stored[0].id = "maps-rw::rw::rw_levelset".to_string();
    stored[0].releases_cached_at = Some(1_700_000_000);
    stored[0].last_error = Some("release.rateLimited".to_string());
    let mut renamed = stored.clone();
    renamed[0]
        .group_i18n
        .insert("zh-CN".to_string(), "RickWang".to_string());

    // 内容相同 → 不算变化，避免每次刷新都写一次配置
    let (same, changed) = merge_maps_keeping_runtime_state(stored.clone(), stored.clone());
    assert!(!changed);
    assert_eq!(same[0].releases_cached_at, Some(1_700_000_000));

    // 只改 group_i18n（身份没变）→ 必须算变化，且保留 Release 缓存
    let (merged, changed) = merge_maps_keeping_runtime_state(stored, renamed);
    assert!(
        changed,
        "只改 group_i18n 也必须写盘，否则线上分组永远不生效"
    );
    assert_eq!(
        merged[0].group_i18n.get("zh-CN").map(String::as_str),
        Some("RickWang")
    );
    assert_eq!(merged[0].releases_cached_at, Some(1_700_000_000));
    assert_eq!(merged[0].last_error.as_deref(), Some("release.rateLimited"));
}

#[test]
fn migrating_map_repository_does_not_restore_old_release_state() {
    let mut old = parse_document(&minimal("M", "A", "S"), &bundled(), &Default::default())
        .unwrap()
        .maps
        .remove(0);
    old.id = "stable-map-id".into();
    old.releases_cached_at = Some(123);
    old.last_error = Some("release.rateLimited".into());
    for field in ["provider", "owner", "repository", "tag", "asset_rule"] {
        let mut fresh = old.clone();
        fresh.releases_cached_at = None;
        fresh.last_error = None;
        match field {
            "provider" => fresh.provider = Provider::Gitcode,
            "owner" => fresh.username = "CH3NGY2".into(),
            "repository" => fresh.repository = "OC2-Map-Bundle".into(),
            "tag" => fresh.tag = Some("new-tag".into()),
            "asset_rule" => fresh.asset_rules[0].value = "new-map.zip".into(),
            _ => unreachable!(),
        }
        let (merged, changed) = merge_maps_keeping_runtime_state(vec![old.clone()], vec![fresh]);
        assert!(changed, "{field}");
        assert_eq!(merged[0].releases_cached_at, None, "{field}");
        assert_eq!(merged[0].last_error, None, "{field}");
        assert!(merged[0].cached_releases.is_empty(), "{field}");
    }
}

#[test]
fn fresh_empty_api_snapshot_replaces_previous_map_cache() {
    let mut old = parse_document(&minimal("M", "A", "S"), &bundled(), &Default::default())
        .unwrap()
        .maps
        .remove(0);
    old.id = "stable-map-id".into();
    old.releases_cached_at = Some(123);
    old.last_error = Some("release.rateLimited".into());
    old.cached_releases = vec![
        serde_json::from_value(serde_json::json!({
            "tag_name": "old", "name": "Old", "body": "", "prerelease": false,
            "draft": false, "assets": []
        }))
        .unwrap(),
    ];
    let mut fresh = old.clone();
    fresh.cached_releases.clear();
    fresh.releases_cached_at = Some(456);
    fresh.last_error = None;
    let (merged, changed) = merge_maps_keeping_runtime_state(vec![old], vec![fresh]);
    assert!(changed);
    assert!(merged[0].cached_releases.is_empty());
    assert_eq!(merged[0].releases_cached_at, Some(456));
    assert_eq!(merged[0].last_error, None);
}
