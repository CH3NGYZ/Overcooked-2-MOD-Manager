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
        assert!(repository_release_api_url(&Provider::Github, username, "repo").is_err());
    }
}

#[test]
fn release_api_urls_are_assembled_from_structured_repository_fields() {
    assert_eq!(
        repository_release_api_url(&Provider::Github, "sinai-dev", "UnityExplorer").unwrap(),
        "https://api.github.com/repos/sinai-dev/UnityExplorer/releases?per_page=50&page=1&direction=desc&sort=created"
    );
    assert_eq!(
        repository_release_api_url(
            &Provider::Gitee,
            "ch3ngyz",
            "Overcooked-2-ChefRotation-Releases"
        )
        .unwrap(),
        "https://gitee.com/api/v5/repos/ch3ngyz/Overcooked-2-ChefRotation-Releases/releases?per_page=50&page=1&direction=desc&sort=created"
    );
    assert!(
        repository_release_api_url(&Provider::Github, "user", "repo")
            .unwrap()
            .contains("per_page=50")
    );
}

/// 所有列表从 page=1 开始，统一每页 50 条。
#[test]
fn paged_release_api_url_always_includes_the_page() {
    let first = repository_release_api_url_page(&Provider::Github, "owner", "repo", None).unwrap();
    assert_eq!(
        first,
        "https://api.github.com/repos/owner/repo/releases?per_page=50&page=1&direction=desc&sort=created"
    );
    // page = 1 与不传等价
    assert_eq!(
        repository_release_api_url_page(&Provider::Github, "owner", "repo", Some(1)).unwrap(),
        first
    );
    assert_eq!(
        repository_release_api_url_page(&Provider::Github, "owner", "repo", Some(2)).unwrap(),
        "https://api.github.com/repos/owner/repo/releases?per_page=50&page=2&direction=desc&sort=created"
    );
    // 页大小统一由共享常量控制
    assert!(
        repository_release_api_url_page(&Provider::Github, "owner", "repo", Some(3))
            .unwrap()
            .contains("per_page=50")
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
