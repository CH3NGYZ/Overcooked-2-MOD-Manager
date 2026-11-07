use super::super::github::SEARCH_QUERY;
use super::super::test_support::item;
use super::*;

#[test]
fn both_community_sources_apply_to_every_region() {
    /*
     * 两个社区源都不限制地区：Gitee 社区过去只在中国大陆可用，
     * 现在与国际地区一致，地区只由仓库/来源自己的 region 表达。
     *
     * 断言写在「源 + 仓库」这条真实判定链上，而不是断言某个常量：
     * 这样以后如果重新引入平台限制，测试会因为行为变化而失败。
     */
    let mut config: crate::models::AppConfig = serde_json::from_value(serde_json::json!({
        "schema_version": 1, "ui_zoom": 1.5,
        "game": { "path": "" },
        "network": { "timeout_seconds": 5, "download_threads": 1, "max_retry_count": 3 },
        "launch": { "ask_user_to_start_from_steam": false, "auto_restore_winhttp": false },
        "auth": { "github_token": "", "gitee_token": "", "official_api_only": true },
        "repositories": []
    }))
    .unwrap();
    config.repository_sources.push(source(&Provider::Github));
    config.repository_sources.push(source(&Provider::Gitee));
    let gitee_repo = repository_from_coordinates(&Provider::Gitee, "Owner", "Mod", "", "").unwrap();
    let github_repo =
        repository_from_coordinates(&Provider::Github, "Owner", "Mod", "", "").unwrap();
    for region in [
        crate::config::REGION_CHINESE_MAINLAND,
        crate::config::REGION_INTERNATIONAL,
        "",
        "unknown",
    ] {
        assert!(crate::config::repository_matches_channel(
            &config,
            &gitee_repo,
            region
        ));
        assert!(crate::config::repository_matches_channel(
            &config,
            &github_repo,
            region
        ));
    }
    assert_eq!(source(&Provider::Gitee).region, None);
    assert_eq!(source(&Provider::Github).region, None);
}

#[test]
fn search_description_is_preserved_when_present_and_optional_when_missing() {
    let mut response = item("Owner", "Mod");
    response["description"] = "Community repository description".into();
    let repo = repository_from_search(&response).unwrap();
    assert_eq!(repo.note, "Community repository description");
    response["description"] = Value::Null;
    assert!(repository_from_search(&response).unwrap().note.is_empty());
    response.as_object_mut().unwrap().remove("description");
    assert!(repository_from_search(&response).unwrap().note.is_empty());
}

#[test]
fn search_entries_are_public_three_topic_dll_zip_repositories_with_stable_ids() {
    let repo = repository_from_search(&item("Owner", "Mod")).unwrap();
    assert_eq!(
        repo.id,
        repository_from_search(&item("owner", "MOD")).unwrap().id
    );
    assert_eq!(repo.source_id.as_deref(), Some(SOURCE_ID));
    assert_eq!(repo.branch, "develop");
    assert!(repo.group.is_empty() && repo.note.is_empty());
    assert!(repo.plugin_guid.is_empty() && repo.local_product_name.is_empty());
    for (name, expected) in [
        ("Mod.dll", true),
        ("Mod.ZIP", true),
        ("notes.md", false),
        ("Mod.zip.sig", false),
    ] {
        let asset = serde_json::from_value(
            serde_json::json!({"name":name,"size":1,"download_url":"https://example.com/file"}),
        )
        .unwrap();
        assert_eq!(releases::asset_matches(&asset, &repo.asset_rules), expected);
    }
    let mut invalid = item("Owner", "Mod");
    invalid["private"] = true.into();
    assert!(repository_from_search(&invalid).is_none());
    invalid["private"] = false.into();
    invalid["topics"] = serde_json::json!(["overcooked-2", "bepinex"]);
    assert!(repository_from_search(&invalid).is_none());
    assert!(repository_from_search(&item("../owner", "Mod")).is_none());
}

#[test]
fn search_query_only_requires_overcooked_two_and_bepinex() {
    assert_eq!(SEARCH_QUERY, "topic:overcooked-2 topic:bepinex");
    assert_eq!(REQUIRED_TOPICS, ["overcooked-2", "bepinex"]);
}

#[test]
fn community_assets_exclude_github_generated_source_archives() {
    let repo = repository_from_search(&item("Owner", "Mod")).unwrap();
    let releases = releases::parse_release_json(
        &serde_json::json!([{
            "tag_name": "v1.0.0", "draft": false, "prerelease": false,
            "zipball_url": "https://api.github.com/repos/Owner/Mod/zipball/v1.0.0",
            "tarball_url": "https://api.github.com/repos/Owner/Mod/tarball/v1.0.0",
            "assets": [
                {"name": "Mod.ZiP", "browser_download_url": "https://example.com/Mod.ZiP"},
                {"name": "Mod.DLL", "browser_download_url": "https://example.com/Mod.DLL"},
                {"name": "README.md", "browser_download_url": "https://example.com/README.md"}
            ]
        }]),
        &Provider::Github,
    )
    .unwrap();
    assert_eq!(releases.len(), 1);
    let matched = releases[0]
        .assets
        .iter()
        .filter(|asset| releases::asset_matches(asset, &repo.asset_rules))
        .map(|asset| asset.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(matched, ["Mod.ZiP", "Mod.DLL"]);
    assert!(releases[0].assets.iter().all(|asset| {
        !asset.download_url.contains("zipball") && !asset.download_url.contains("tarball")
    }));
}
