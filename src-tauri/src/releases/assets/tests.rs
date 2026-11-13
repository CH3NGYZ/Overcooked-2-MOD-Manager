use super::super::projection::parse_release_json;
use super::*;
use crate::models::Provider;

#[test]
fn cached_tag_source_archives_cannot_be_installed_even_when_explicitly_selected() {
    let mut repo =
        crate::community::repository_from_coordinates(&Provider::Gitee, "Owner", "Mod", "", "")
            .unwrap();
    let cases: Vec<(String, String, bool)> = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/gitee-source-assets.json"
    ))
    .unwrap();
    for (tag, name, allowed) in cases {
        let mut info = parse_release_json(&serde_json::json!([{"tag_name": "placeholder", "assets": [{"name": name, "browser_download_url": "https://example.com/file"}]}]), &Provider::Gitee).unwrap().remove(0);
        info.tag_name = tag;
        let selected = info.assets[0].name.clone();
        assert_eq!(
            select_install_asset(
                &crate::releases::RepositoryRequest::from(&repo),
                &repo.asset_rules,
                &info,
                None
            )
            .is_some(),
            allowed,
            "{selected}"
        );
        assert_eq!(
            select_install_asset(
                &crate::releases::RepositoryRequest::from(&repo),
                &repo.asset_rules,
                &info,
                Some(&selected)
            )
            .is_some(),
            allowed,
            "explicit {selected}"
        );
        let source_id = repo.source_id.take();
        assert!(
            select_install_asset(
                &crate::releases::RepositoryRequest::from(&repo),
                &repo.asset_rules,
                &info,
                Some(&selected)
            )
            .is_some(),
            "custom Gitee {selected}"
        );
        repo.source_id = source_id;
        repo.provider = Provider::Github;
        assert!(
            select_install_asset(
                &crate::releases::RepositoryRequest::from(&repo),
                &repo.asset_rules,
                &info,
                Some(&selected)
            )
            .is_some(),
            "GitHub {selected}"
        );
        repo.provider = Provider::Gitee;
    }
}

#[test]
fn asset_rules_match_exact_names() {
    let asset = ReleaseAsset {
        name: "HostUtilities.zip".into(),
        size: 1,
        download_count: None,
        download_url: "https://github.com/a/b/releases/download/v1/HostUtilities.zip".into(),
        digest: None,
    };
    assert!(asset_matches(
        &asset,
        &[AssetRule {
            rule_type: AssetRuleType::Exact,
            value: "HostUtilities.zip".into()
        }]
    ));
}

#[test]
fn cached_release_selection_never_substitutes_another_requested_version() {
    let releases = parse_release_json(
        &serde_json::json!([
            {"tag_name":"v2.0.0", "assets":[]},
            {"tag_name":"v1.0.0", "assets":[{"name":"mod.zip", "browser_download_url":"https://example.com/mod.zip"}]}
        ]),
        &Provider::Github,
    ).unwrap();
    assert!(select_cached_release(&[], None, None).is_none());
    assert_eq!(
        select_cached_release(&releases, None, None)
            .unwrap()
            .tag_name,
        "v2.0.0"
    );
    assert!(
        select_cached_release(&releases, None, None)
            .unwrap()
            .assets
            .is_empty()
    );
    assert_eq!(
        select_cached_release(&releases, Some("v1.0.0"), None)
            .unwrap()
            .tag_name,
        "v1.0.0"
    );
    assert!(select_cached_release(&releases, Some("missing"), None).is_none());
    assert!(select_cached_release(&releases, Some("missing"), Some("v1.0.0")).is_none());
    assert_eq!(
        select_cached_release(&releases, None, Some("1.0.0"))
            .unwrap()
            .tag_name,
        "v1.0.0"
    );
    assert!(select_cached_release(&releases, None, Some("missing")).is_none());
    assert_eq!(
        select_cached_release(&releases, Some("v2.0.0"), Some("1.0.0"))
            .unwrap()
            .tag_name,
        "v2.0.0"
    );
}

#[test]
fn cached_asset_selection_preserves_first_matching_snapshot_asset() {
    let repo =
        crate::community::repository_from_coordinates(&Provider::Github, "Owner", "Mod", "", "")
            .unwrap();
    let releases = parse_release_json(
        &serde_json::json!([{"tag_name":"v1.0.0", "assets":[
            {"name":"README.txt", "browser_download_url":"https://example.com/readme"},
            {"name":"first.zip", "browser_download_url":"https://example.com/first.zip"},
            {"name":"second.zip", "browser_download_url":"https://example.com/second.zip"}
        ]}]),
        &Provider::Github,
    )
    .unwrap();
    let release = select_cached_release(&releases, Some("v1.0.0"), None).unwrap();
    let request = RepositoryRequest::from(&repo);
    let rules = [AssetRule {
        rule_type: AssetRuleType::Glob,
        value: "*.zip".into(),
    }];
    assert_eq!(
        select_install_asset(&request, &rules, release, None)
            .unwrap()
            .name,
        "first.zip"
    );
    assert_eq!(
        select_install_asset(&request, &rules, release, Some("second.zip"))
            .unwrap()
            .name,
        "second.zip"
    );
    assert!(select_install_asset(&request, &rules, release, Some("missing.zip")).is_none());
}
