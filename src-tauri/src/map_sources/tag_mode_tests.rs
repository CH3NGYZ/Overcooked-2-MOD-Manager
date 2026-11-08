use super::assets::synthesize_releases;
use super::document::parse_document;
use super::identity::map_tag;
use super::releases::select_map_stage_asset;
use crate::models::MapRepositoryConfig;
use crate::models::Provider;
use crate::models::ReleaseAsset;
use crate::models::ReleaseInfo;
use crate::repository_sources::SourceLocation;

const VERSION_RULE: &str = r#"{ type = "regex", value = '^rw_v(?P<version>[0-9.]+)\.zip$' }"#;

fn entry(rules: &str, extra: &str) -> MapRepositoryConfig {
    let text = format!(
        r#"
[[maps]]
name = "RW"
provider = "github"
username = "u"
repository = "r"
level_set_name = "RW_LevelSet"
author = "RW"
{extra}
asset_rules = [{rules}]
"#
    );
    parse_document(
        &text,
        &SourceLocation::parse("maps.toml").unwrap(),
        &Default::default(),
    )
    .unwrap()
    .maps
    .into_iter()
    .next()
    .unwrap()
}

fn tag_entry(extra: &str) -> MapRepositoryConfig {
    entry(VERSION_RULE, extra)
}

fn release(tag: &str, assets: &[&str]) -> ReleaseInfo {
    ReleaseInfo {
        release_id: Some(7),
        tag_name: tag.to_string(),
        version: None,
        name: "地图合集".to_string(),
        body: "更新说明".to_string(),
        published_at: Some("2026-01-01T00:00:00Z".to_string()),
        prerelease: true,
        draft: false,
        assets: assets
            .iter()
            .map(|name| ReleaseAsset {
                name: name.to_string(),
                size: 1024,
                download_count: None,
                download_url: format!("https://github.com/u/r/releases/download/{tag}/{name}"),
                digest: None,
            })
            .collect(),
    }
}

fn tag_names(releases: &[ReleaseInfo]) -> Vec<&str> {
    releases
        .iter()
        .map(|release| release.tag_name.as_str())
        .collect()
}

#[test]
fn synthetic_versions_retain_only_their_own_split_companions_primary_first() {
    let map = tag_entry("tag = \"maps\"");
    let releases = synthesize_releases(
        &map,
        "maps",
        &[release(
            "maps",
            &["rw_v1.0.z02", "rw_v2.0.zip", "rw_v1.0.zip", "rw_v1.0.z01"],
        )],
    )
    .unwrap();
    assert_eq!(releases[0].assets.len(), 1);
    assert_eq!(
        releases[1]
            .assets
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>(),
        ["rw_v1.0.zip", "rw_v1.0.z01", "rw_v1.0.z02"]
    );
    assert_eq!(
        select_map_stage_asset(&map, &releases[1], None)
            .unwrap()
            .name,
        "rw_v1.0.zip"
    );
}

#[tokio::test]
async fn bundle_refresh_projects_each_card_from_one_repository_snapshot() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let cache = crate::release_cache::RepoReleaseCache::default();
    let calls = AtomicUsize::new(0);
    let map = tag_entry("tag = \"maps\"");
    for pass in ["source-tree", "first-card", "second-card"] {
        let snapshot = cache
            .list("github:u:r".into(), Some(pass), false, || async {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(vec![release("maps", &["rw_v1.0.zip", "rw_v2.0.zip"])])
            })
            .await
            .unwrap();
        let projected = super::releases::project_map_releases(&map, &snapshot).unwrap();
        assert_eq!(tag_names(&projected), ["2.0", "1.0"]);
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    // The floating full refresh bypasses the snapshot once; all derived cards
    // in that pass still share it even when they each request force=true.
    for _ in 0..3 {
        cache
            .list(
                "github:u:r".into(),
                Some("floating-button"),
                true,
                || async {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(vec![release("maps", &["rw_v3.0.zip"])])
                },
            )
            .await
            .unwrap();
    }
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn tag_mode_auto_select_keeps_a_free_text_version_asset() {
    // 版本表末项是 `0.78-fix`，规则只写数字段，自动匹配按理会落空
    let map = tag_entry("tag = \"maps\"");
    let mut staged = release("maps", &["rw_v0.78-fix.zip"]);
    staged.release_id = None;
    staged.tag_name = "0.78-fix".to_string();
    staged.version = Some("0.78-fix".to_string());
    assert!(!crate::releases::asset_matches(
        &staged.assets[0],
        &map.asset_rules
    ));
    assert_eq!(
        select_map_stage_asset(&map, &staged, None).map(|asset| asset.name.as_str()),
        Some("rw_v0.78-fix.zip")
    );
    // 明确指定的资产名仍要精确匹配，错名字不能偷偷装成别的文件
    assert!(select_map_stage_asset(&map, &staged, Some("rw_v0.78.zip")).is_none());
}

#[test]
fn release_mode_still_filters_assets_by_rules() {
    // 没写 tag 就是普通 Release 模式：规则不认的资产不能自动选中
    let map = tag_entry("");
    let unmatched = release("v0.78", &["rw_v0.78-fix.zip"]);
    assert!(select_map_stage_asset(&map, &unmatched, None).is_none());
    assert_eq!(
        select_map_stage_asset(&map, &unmatched, Some("rw_v0.78-fix.zip"))
            .map(|asset| asset.name.as_str()),
        Some("rw_v0.78-fix.zip")
    );
    let matched = release("v0.78", &["rw_v0.78.zip"]);
    assert_eq!(
        select_map_stage_asset(&map, &matched, None).map(|asset| asset.name.as_str()),
        Some("rw_v0.78.zip")
    );
}

#[test]
fn one_tag_becomes_one_release_per_asset() {
    // 现实形态：一个 tag 里放同一张图的多个版本，文件名带版本号
    let map = tag_entry("tag = \"maps\"");
    let releases = synthesize_releases(
        &map,
        "maps",
        &[release("maps", &["rw_v0.22.zip", "rw_v0.78.zip"])],
    )
    .unwrap();
    assert_eq!(tag_names(&releases), ["0.78", "0.22"]);
    assert_eq!(releases[0].version.as_deref(), Some("0.78"));
    assert_eq!(releases[0].name, "v0.78");
    assert_eq!(releases[0].assets.len(), 1);
    assert_eq!(releases[0].assets[0].name, "rw_v0.78.zip");
    /*
     * 两条版本共用同一个真实 Release，`release_id` 留空是刻意的：
     * 前端拿它当列表键，重复的 ID 会让两条版本互相覆盖。
     */
    assert_eq!(releases[0].release_id, None);
    // 渠道由作者在条目里声明，不再由 Release 的预发布标记决定
    assert!(!releases[0].prerelease);
    // 说明与时间仍来自真实 Release：版本列表要显示「这次更新了什么」
    assert_eq!(releases[0].body, "更新说明");
    assert_eq!(
        releases[0].published_at.as_deref(),
        Some("2026-01-01T00:00:00Z")
    );
}

#[test]
fn assets_that_do_not_match_the_rule_are_skipped() {
    let map = tag_entry("tag = \"maps\"");
    let releases = synthesize_releases(
        &map,
        "maps",
        &[release(
            "maps",
            &["rw_v0.22.zip", "readme.md", "source.zip"],
        )],
    )
    .unwrap();
    assert_eq!(tag_names(&releases), ["0.22"]);
}

#[test]
fn the_declared_latest_version_goes_first_without_reordering_the_rest() {
    /*
     * 作者认为 0.45 才是最新（0.7 与 0.69 那种「按数字比会排错」的现实问题）。
     * 置顶只影响第一条，其余仍按版本号降序 —— 作者只说「它是最新」，没说
     * 其余版本怎么排。
     */
    let map = tag_entry("tag = \"maps\"\nlatest_version = \"0.45\"");
    let releases = synthesize_releases(
        &map,
        "maps",
        &[release(
            "maps",
            &["rw_v0.22.zip", "rw_v0.45.zip", "rw_v0.78.zip"],
        )],
    )
    .unwrap();
    assert_eq!(tag_names(&releases), ["0.45", "0.78", "0.22"]);
}

#[test]
fn versions_sort_by_numeric_segments() {
    // 0.10 比 0.9 新：纯字符串比较会反着排
    let map = tag_entry("tag = \"maps\"");
    let releases = synthesize_releases(
        &map,
        "maps",
        &[release("maps", &["rw_v0.9.zip", "rw_v0.10.zip"])],
    )
    .unwrap();
    assert_eq!(tag_names(&releases), ["0.10", "0.9"]);
}

#[test]
fn an_asset_hosted_document_may_not_declare_sub_sources() {
    let text = r#"
[[maps]]
name = "M"
provider = "github"
username = "u"
repository = "r"
level_set_name = "S"
author = "A"
enabled = true
asset_rules = [{ type = "exact", value = "map.zip" }]

[[map_sources]]
id = "child"
nickname = "Child"
url = "child.toml"
"#;
    let asset =
        SourceLocation::parse("https://github.com/RW/RW/releases/download/maps/map-sources.toml")
            .unwrap();
    let error = parse_document(text, &asset, &Default::default()).unwrap_err();
    assert!(
        format!("{error:?}").contains("mapSource.assetDocumentSubSources"),
        "{error:?}"
    );
    // 放在普通地址上的同一份文档仍然可以声明子源
    let plain = SourceLocation::parse("maps.toml").unwrap();
    assert!(parse_document(text, &plain, &Default::default()).is_ok());
}

#[test]
fn max_release_count_limits_how_many_versions_are_exposed() {
    let map = tag_entry("tag = \"maps\"\nmax_release_count = 2");
    let releases = synthesize_releases(
        &map,
        "maps",
        &[release(
            "maps",
            &["rw_v0.22.zip", "rw_v0.45.zip", "rw_v0.78.zip"],
        )],
    )
    .unwrap();
    // 截断留最新的两个，而不是列表里的前两个
    assert_eq!(tag_names(&releases), ["0.78", "0.45"]);
}

#[test]
fn a_tag_that_is_not_in_the_repository_is_reported() {
    let map = tag_entry("tag = \"maps\"");
    let err = synthesize_releases(&map, "typo", &[release("maps", &["rw_v0.22.zip"])]).unwrap_err();
    assert!(err.contains("mapSource.releaseTagNotFound"), "{err}");
}

#[test]
fn a_tag_without_any_matching_asset_is_reported() {
    let map = tag_entry("tag = \"maps\"");
    let err = synthesize_releases(&map, "maps", &[release("maps", &["notes.txt"])]).unwrap_err();
    assert!(err.contains("mapSource.noAssetInTag"), "{err}");
}

#[test]
fn two_assets_that_yield_the_same_version_are_rejected() {
    /*
     * 版本号同时是「安装哪个资产」的标识。两个资产抽出同一个号，
     * 安装时就只能瞎选一个 —— 宁可在刷新时明确报错。
     */
    let map = entry(
        r#"{ type = "regex", value = '^rw_v(?P<version>[0-9.]+)(?:_alt)?\.zip$' }"#,
        "tag = \"maps\"",
    );
    let err = synthesize_releases(
        &map,
        "maps",
        &[release("maps", &["rw_v0.22.zip", "rw_v0.22_alt.zip"])],
    )
    .unwrap_err();
    assert!(err.contains("mapSource.duplicateVersion"), "{err}");
}

#[test]
fn a_declared_latest_version_must_exist_in_the_tag() {
    // 声明了一个不存在的版本：界面会把幽灵版本标成最新，点安装找不到资产
    let map = tag_entry("tag = \"maps\"\nlatest_version = \"9.9\"");
    let err = synthesize_releases(&map, "maps", &[release("maps", &["rw_v0.22.zip"])]).unwrap_err();
    assert!(err.contains("mapSource.latestVersionMissing"), "{err}");
}

#[test]
fn a_version_rule_without_anchors_is_rejected_at_parse_time() {
    let text = format!(
        r#"
[[maps]]
name = "RW"
provider = "github"
username = "u"
repository = "r"
level_set_name = "RW_LevelSet"
author = "RW"
tag = "maps"
asset_rules = [{{ type = "regex", value = 'rw_v(?P<version>[0-9.]+)\.zip' }}]
"#
    );
    let err = parse_document(
        &text,
        &SourceLocation::parse("maps.toml").unwrap(),
        &Default::default(),
    )
    .unwrap_err();
    // 不锚定就会「同一个文件匹配出两个版本号」，必须在加载时就拒绝
    assert!(err.to_string().contains("mapSource.regexNotAnchored"));
}

#[test]
fn a_rule_without_a_version_group_is_rejected_at_parse_time() {
    let text = format!(
        r#"
[[maps]]
name = "RW"
provider = "github"
username = "u"
repository = "r"
level_set_name = "RW_LevelSet"
author = "RW"
tag = "maps"
asset_rules = [{{ type = "regex", value = '^rw_v[0-9.]+\.zip$' }}]
"#
    );
    let err = parse_document(
        &text,
        &SourceLocation::parse("maps.toml").unwrap(),
        &Default::default(),
    )
    .unwrap_err();
    assert!(err.to_string().contains("mapSource.assetRuleNeedsVersion"));
}

#[test]
fn an_exact_rule_is_enough_for_the_legacy_single_release_mode() {
    // 没写 tag 的条目还是老语义：一条 Release 一个资产，不需要抽版本号
    let map = entry(r#"{ type = "exact", value = "rw.zip" }"#, "");
    assert_eq!(map_tag(&map), None);
}

#[test]
fn bundle_coordinates_are_inherited_only_by_entries_that_omit_them() {
    let text = r#"
[bundle]
provider = "gitee"
username = "QingyuQingxue"
repository = "OC2-Map-Bundle"

[[maps]]
name = "A"
level_set_name = "A_Set"
author = "Qingyu"
asset_rules = [{ type = "exact", value = "a.zip" }]

[[maps]]
name = "B"
provider = "github"
username = "other"
repository = "repo"
level_set_name = "B_Set"
author = "Other"
asset_rules = [{ type = "exact", value = "b.zip" }]
"#;
    let doc = parse_document(
        text,
        &SourceLocation::parse("maps.toml").unwrap(),
        &Default::default(),
    )
    .unwrap();
    assert_eq!(doc.maps[0].provider, Provider::Gitee);
    assert_eq!(doc.maps[0].username, "QingyuQingxue");
    assert_eq!(doc.maps[0].repository, "OC2-Map-Bundle");
    // 显式写了坐标的条目不受 bundle 影响
    assert_eq!(doc.maps[1].provider, Provider::Github);
    assert_eq!(doc.maps[1].username, "other");
    assert!(doc.bundle.is_none(), "bundle 已展开，不再留在文档上");
}

#[test]
fn asset_versions_keep_the_authors_order_and_accept_any_version_text() {
    /*
     * 现实里的两个坑：`3.4.1-fix` 这种后缀按文件名的版本规则抽不出来；
     * 「0.7 比 0.69 新」按数字段也排不对。排序表就是作者替这两件事下的
     * 结论，读侧只按表走（表尾最新 → 输出反转）。
     */
    let map = tag_entry(&format!(
        "tag = \"maps\"\nasset_versions = [\n{}\n]",
        [
            r#"{ mapset_version = "0.7", info_version = "0.7" }"#,
            r#"{ mapset_version = "3.4.1-fix", info_version = "3.4.1" }"#,
        ]
        .join(",\n"),
    ));
    let releases = synthesize_releases(
        &map,
        "maps",
        &[release("maps", &["rw_v0.7.zip", "rw_v3.4.1-fix.zip"])],
    )
    .unwrap();
    assert_eq!(tag_names(&releases), ["3.4.1-fix", "0.7"]);
    assert_eq!(releases[0].assets[0].name, "rw_v3.4.1-fix.zip");
}

#[test]
fn assets_outside_the_table_stay_hidden() {
    let map = tag_entry(&format!(
        "tag = \"maps\"\nasset_versions = [ {} ]",
        r#"{ mapset_version = "0.22" }"#,
    ));
    let releases = synthesize_releases(
        &map,
        "maps",
        &[release(
            "maps",
            &["rw_v0.22.zip", "rw_v0.45.zip", "rw_v0.78.zip"],
        )],
    )
    .unwrap();
    // 有版本表时它就是唯一公开清单：未登记的附件（上传后未保存资料）不自动暴露
    assert_eq!(tag_names(&releases), ["0.22"]);
    // 没有版本表的旧 Release 仍按规则扫描全部附件，并按数字段降序
    let legacy = synthesize_releases(
        &tag_entry("tag = \"maps\""),
        "maps",
        &[release(
            "maps",
            &["rw_v0.22.zip", "rw_v0.45.zip", "rw_v0.78.zip"],
        )],
    )
    .unwrap();
    assert_eq!(tag_names(&legacy), ["0.78", "0.45", "0.22"]);
}

#[test]
fn a_table_entry_without_its_asset_is_skipped() {
    let map = tag_entry(&format!(
        "tag = \"maps\"\nasset_versions = [ {}, {} ]",
        r#"{ mapset_version = "0.22" }"#, r#"{ mapset_version = "0.99" }"#,
    ));
    let releases =
        synthesize_releases(&map, "maps", &[release("maps", &["rw_v0.22.zip"])]).unwrap();
    // 表里写了但文件已经删掉的版本不能凭空出现在列表里
    assert_eq!(tag_names(&releases), ["0.22"]);
    // 表里一项都对不上时报错，不退回扫描表外附件
    let only_missing = tag_entry(&format!(
        "tag = \"maps\"\nasset_versions = [ {} ]",
        r#"{ mapset_version = "0.99" }"#,
    ));
    let error = synthesize_releases(&only_missing, "maps", &[release("maps", &["rw_v0.22.zip"])])
        .unwrap_err();
    assert!(error.starts_with("mapSource.noAssetInTag"), "{error}");
}

#[test]
fn a_bundle_without_coordinates_is_rejected() {
    let text = r#"
[bundle]
username = ""
repository = "OC2-Map-Bundle"

[[maps]]
name = "A"
level_set_name = "A_Set"
author = "Qingyu"
asset_rules = [{ type = "exact", value = "a.zip" }]
"#;
    let err = parse_document(
        text,
        &SourceLocation::parse("maps.toml").unwrap(),
        &Default::default(),
    )
    .unwrap_err();
    assert!(err.to_string().contains("mapSource.missingCoordinates"));
}

#[test]
fn map_document_inherits_plans_and_release_probe_keeps_map_role() {
    let text = r#"
type = "map"
network_plan.mainland = "github-public"
network_plan.international = "github-public"
[[maps]]
provider = "github"
username = "u"
repository = "r"
level_set_name = "PlanMap"
asset_rules = [{type = "exact", value = "map.zip"}]
network_plan.mainland.release.download_threads = 4
[[map_sources]]
id = "child"
nickname = "Child"
mode = "release-tags"
provider = "github"
username = "u"
repository = "r"
"#;
    let origin = SourceLocation::parse("maps.toml").unwrap();
    let document = parse_document(text, &origin, &Default::default()).unwrap();
    let probe = super::releases::release_probe(&document.maps[0]);
    assert_eq!(probe.kind, crate::models::RepositoryKind::Map);
    let policy = crate::network_policy::NetworkPolicy::shared().unwrap();
    let plan = policy
        .resolve(
            &probe.provider,
            probe.kind,
            "chinese_mainland",
            &probe.network_plan,
        )
        .unwrap();
    assert_eq!(plan.release.download_threads, 4);
    let child = super::release_tags::declaration_probe(&document.map_sources[0]);
    assert_eq!(child.kind, crate::models::RepositoryKind::Map);
    let plan = policy
        .resolve(
            &child.provider,
            child.kind,
            "chinese_mainland",
            &child.network_plan,
        )
        .unwrap();
    assert_eq!(plan.id, "github-public");
    let parent = toml::from_str("mainland = 'github-public'").unwrap();
    let sparse = text.replace("network_plan.mainland = \"github-public\"\n", "");
    let inherited = parse_document(&sparse, &origin, &parent).unwrap();
    assert_eq!(
        inherited.maps[0].network_plan,
        document.maps[0].network_plan
    );

    assert!(
        parse_document(
            &text.replace("type = \"map\"", "type = \"mod\""),
            &origin,
            &Default::default()
        )
        .is_err()
    );
    assert!(
        parse_document(
            &text.replace("[[maps]]", "[[maps]]\ntype = \"mod\""),
            &origin,
            &Default::default()
        )
        .is_err()
    );
}
