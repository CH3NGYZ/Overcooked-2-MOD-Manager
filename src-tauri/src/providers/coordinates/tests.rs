use super::*;

#[test]
fn repository_homepage_is_built_from_provider_username_and_repository() {
    assert_eq!(
        repository_homepage(&Provider::Github, "gua248", "ManyRecipes").unwrap(),
        "https://github.com/gua248/ManyRecipes"
    );
    assert_eq!(
        repository_homepage(&Provider::Gitee, " ch3ngyz ", "repo").unwrap(),
        "https://gitee.com/ch3ngyz/repo"
    );
}

#[test]
fn repository_homepage_rejects_unsafe_path_segments() {
    assert!(repository_homepage(&Provider::Github, "owner/name", "repo").is_err());
    assert!(repository_homepage(&Provider::Github, "owner", "repo name").is_err());
    assert!(repository_homepage(&Provider::Github, "", "repo").is_err());
    for username in [
        "../owner",
        "https://github.com/owner",
        "owner?token=value",
        "owner#fragment",
        "owner\\repo",
        "..",
    ] {
        assert!(repository_homepage(&Provider::Github, username, "repo").is_err());
        assert!(repository_release_api_url(&Provider::Github, username, "repo", 10).is_err());
    }
}

#[test]
fn release_api_urls_are_assembled_from_structured_repository_fields() {
    assert_eq!(
        repository_release_api_url(&Provider::Github, "sinai-dev", "UnityExplorer", 10).unwrap(),
        "https://api.github.com/repos/sinai-dev/UnityExplorer/releases?per_page=10&direction=desc&sort=created"
    );
    assert_eq!(
        repository_release_api_url(
            &Provider::Gitee,
            "ch3ngyz",
            "Overcooked-2-ChefRotation-Releases",
            0
        )
        .unwrap(),
        "https://gitee.com/api/v5/repos/ch3ngyz/Overcooked-2-ChefRotation-Releases/releases?per_page=1&direction=desc&sort=created"
    );
    assert!(
        repository_release_api_url(&Provider::Github, "user", "repo", 500)
            .unwrap()
            .contains("per_page=100")
    );
}

/// 翻页只在第 2 页起追加 `page`：第 1 页的地址被既有断言与代理模板逐字匹配。
#[test]
fn paged_release_api_url_only_adds_page_after_the_first() {
    let first =
        repository_release_api_url_page(&Provider::Github, "owner", "repo", 50, None).unwrap();
    assert_eq!(
        first,
        "https://api.github.com/repos/owner/repo/releases?per_page=50&direction=desc&sort=created"
    );
    // page = 1 与不传等价
    assert_eq!(
        repository_release_api_url_page(&Provider::Github, "owner", "repo", 50, Some(1)).unwrap(),
        first
    );
    assert_eq!(
        repository_release_api_url_page(&Provider::Github, "owner", "repo", 50, Some(2)).unwrap(),
        "https://api.github.com/repos/owner/repo/releases?per_page=50&page=2&direction=desc&sort=created"
    );
    // 每页最多 100：地图源的 page_size 允许配置，越界要被夹住
    assert!(
        repository_release_api_url_page(&Provider::Github, "owner", "repo", 1000, Some(3))
            .unwrap()
            .contains("per_page=100")
    );
}

#[test]
fn repository_file_urls_follow_provider_raw_layout() {
    assert_eq!(
        repository_file_url(
            &Provider::Github,
            "gua248",
            "Overcooked2-ManyRecipes",
            "master",
            "bin/Release/OC2ManyRecipes.dll",
        )
        .unwrap(),
        "https://github.com/gua248/Overcooked2-ManyRecipes/raw/refs/heads/master/bin/Release/OC2ManyRecipes.dll"
    );
    assert_eq!(
        repository_file_url(
            &Provider::Gitee,
            "ch3ngyz",
            "example",
            "main",
            "bin/example.zip",
        )
        .unwrap(),
        "https://gitee.com/ch3ngyz/example/raw/main/bin/example.zip"
    );
}

#[test]
fn repository_file_names_are_decoded_and_path_safe() {
    assert_eq!(
        repository_file_name("bin/My%20MOD.dll").unwrap(),
        "My MOD.dll"
    );
    assert!(repository_file_name("bin/%2Fetc.dll").is_err());
    assert!(
        repository_file_url(&Provider::Github, "gua248", "repo", "main", "../secret.dll").is_err()
    );
}
