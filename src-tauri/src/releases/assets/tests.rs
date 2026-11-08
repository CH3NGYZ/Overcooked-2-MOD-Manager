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
