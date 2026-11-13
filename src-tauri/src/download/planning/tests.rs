use super::*;

const DIRECT: &str = "https://github.com/example/mod/releases/download/v1/mod.zip";

#[test]
fn remote_url_kind_follows_host_and_path() {
    // Release 资产：两个平台的 /releases/download/ 路径
    for url in [
        "https://github.com/CH3NGYZ/demo/releases/download/v1.0.0/demo.zip",
        "https://gitee.com/ch3ngyz/demo/releases/download/v1.0.0/demo.zip",
        "https://github.com/CH3NGYZ/demo/releases/download/v1.0.0/sub/demo.dll",
    ] {
        assert_eq!(
            classify_remote_url(url),
            RemoteUrlKind::ReleaseAsset,
            "{url}"
        );
    }
    // Raw：raw.githubusercontent 与两个平台的仓库 raw 入口
    for url in [
        "https://raw.githubusercontent.com/CH3NGYZ/demo/main/README.md",
        "https://github.com/CH3NGYZ/demo/raw/main/maps/plus.zip",
        "https://github.com/CH3NGYZ/demo/raw/refs/heads/main/maps/plus.zip",
        "https://gitee.com/ch3ngyz/demo/raw/main/maps/plus.zip",
    ] {
        assert_eq!(classify_remote_url(url), RemoteUrlKind::Raw, "{url}");
    }
    // 其它直链：普通站点、仓库主页、以及只出现在查询串里的关键词都不改类别
    for url in [
        "https://example.com/files/map.zip",
        "https://github.com/CH3NGYZ/demo",
        "https://github.com/CH3NGYZ/demo/releases",
        "https://example.com/?next=/releases/download/v1/x.zip",
        "https://github.com/CH3NGYZ/demo/blob/main/README.md",
        "ftp://github.com/CH3NGYZ/demo/releases/download/v1/x.zip",
        "not a url",
    ] {
        assert_eq!(classify_remote_url(url), RemoteUrlKind::Other, "{url}");
    }
}

#[test]
fn remote_provider_comes_from_the_url_host() {
    assert_eq!(
        provider_for_remote_url("https://gitee.com/ch3ngyz/demo/raw/main/a.zip"),
        Some(Provider::Gitee)
    );
    assert_eq!(
        provider_for_remote_url("https://raw.githubusercontent.com/o/r/main/a.zip"),
        Some(Provider::Github)
    );
    for url in [
        "https://example.com/a.zip",
        "https://github.com.evil.example/a.zip",
        "ftp://github.com/o/r",
        "not a URL",
    ] {
        assert_eq!(provider_for_remote_url(url), None, "{url}");
    }
}

#[test]
fn shared_candidate_ordering_deduplicates_each_automatic_mode() {
    let proxy_a = "https://proxy-a.example/asset".to_string();
    let proxy_b = "https://proxy-b.example/asset".to_string();
    let duplicated = vec![
        DIRECT.to_string(),
        proxy_a.clone(),
        proxy_a.clone(),
        proxy_b.clone(),
        DIRECT.to_string(),
    ];
    let direct_first =
        plan_asset_download_candidates(ProxyMode::AutoDirect, DIRECT, duplicated.clone()).unwrap();
    let proxy_first =
        plan_asset_download_candidates(ProxyMode::AutoProxy, DIRECT, duplicated).unwrap();
    assert_eq!(
        direct_first
            .iter()
            .map(|candidate| candidate.url.as_str())
            .collect::<Vec<_>>(),
        [DIRECT, &proxy_a, &proxy_b]
    );
    assert_eq!(
        proxy_first
            .iter()
            .map(|candidate| candidate.url.as_str())
            .collect::<Vec<_>>(),
        [DIRECT, &proxy_a, &proxy_b]
    );
    assert!(plan_asset_download_candidates(ProxyMode::Fixed, DIRECT, Vec::new()).is_err());
}

#[test]
fn remote_urls_pick_their_own_proxy_group_and_gitee_stays_direct() {
    let mut config = crate::config::default_config();
    config.github_proxies.release_proxies = vec![GitHubProxy {
        id: "release-only".into(),
        name: "Release".into(),
        template: "https://release.example/{url}".into(),
        regions: vec![],
        description: String::new(),
        enabled: true,
        builtin: false,
        remote: false,
    }];
    config.github_proxies.raw_proxies = vec![GitHubProxy {
        id: "raw-only".into(),
        name: "Raw".into(),
        template: "https://raw.example/{owner}/{repo}@{branch}/{path}".into(),
        regions: vec![],
        description: String::new(),
        enabled: true,
        builtin: false,
        remote: false,
    }];

    // Release 资产只用 release_proxies，raw 组不能混进来
    let release_url = "https://github.com/acme/mod/releases/download/v1/mod.zip";
    let release = plan_remote_download_candidates(
        &config,
        &Provider::Github,
        RemoteUrlKind::ReleaseAsset,
        release_url,
        crate::config::REGION_CHINESE_MAINLAND,
    )
    .unwrap();
    assert!(
        release
            .iter()
            .any(|candidate| candidate.url.contains("release.example"))
    );
    assert!(
        !release
            .iter()
            .any(|candidate| candidate.url.contains("raw.example"))
    );

    // Raw 文件只用 raw_proxies，且模板里的分支/路径来自 URL 自身
    let raw_url = "https://github.com/acme/mod/raw/main/bin/mod.dll";
    let raw = plan_remote_download_candidates(
        &config,
        &Provider::Github,
        RemoteUrlKind::Raw,
        raw_url,
        crate::config::REGION_CHINESE_MAINLAND,
    )
    .unwrap();
    assert!(
        raw.iter()
            .any(|candidate| candidate.url == "https://raw.example/acme/mod@main/bin/mod.dll")
    );
    assert!(
        !raw.iter()
            .any(|candidate| candidate.url.contains("release.example"))
    );

    // International defaults remain direct regardless of configured proxies.
    for (kind, url) in [
        (RemoteUrlKind::ReleaseAsset, release_url),
        (RemoteUrlKind::Raw, raw_url),
    ] {
        let candidates = plan_remote_download_candidates(
            &config,
            &Provider::Github,
            kind,
            url,
            crate::config::REGION_INTERNATIONAL,
        )
        .unwrap();
        assert_eq!(
            candidates,
            vec![DownloadCandidate {
                route: DownloadRoute::Direct,
                url: url.into()
            }]
        );
    }

    // Gitee 没有资产代理：只剩直连候选
    let gitee = plan_remote_download_candidates(
        &config,
        &Provider::Gitee,
        RemoteUrlKind::ReleaseAsset,
        "https://gitee.com/acme/mod/releases/download/v1/mod.zip",
        crate::config::REGION_CHINESE_MAINLAND,
    )
    .unwrap();
    assert_eq!(gitee.len(), 1);
    assert_eq!(gitee[0].route, DownloadRoute::Direct);
}

fn proxy(id: &str, host: &str, remote: bool, enabled: bool) -> GitHubProxy {
    GitHubProxy {
        id: id.into(),
        name: id.into(),
        template: format!("https://{host}/{{url}}"),
        regions: Vec::new(),
        description: String::new(),
        enabled,
        builtin: !remote,
        remote,
    }
}

#[test]
fn defaults_follow_region_and_gitee_forces_direct() {
    assert_eq!(
        selected_asset_proxy_mode(
            None,
            &Provider::Github,
            crate::config::REGION_CHINESE_MAINLAND
        ),
        ProxyMode::AutoDirect
    );
    assert_eq!(
        selected_asset_proxy_mode(None, &Provider::Github, crate::config::REGION_INTERNATIONAL),
        ProxyMode::Direct
    );
    assert_eq!(
        selected_asset_proxy_mode(
            Some(ProxyMode::AutoProxy),
            &Provider::Gitee,
            crate::config::REGION_CHINESE_MAINLAND
        ),
        ProxyMode::Direct
    );
}

#[test]
fn file_threads_follow_provider_region_and_race_without_discarding_configured_count() {
    let mainland = crate::config::REGION_CHINESE_MAINLAND;
    let international = crate::config::REGION_INTERNATIONAL;
    for provider in [Provider::Gitee, Provider::Gitcode] {
        for region in [mainland, international] {
            for racing in [false, true] {
                assert_eq!(
                    file_download_threads(Some(&provider), region, 16, racing),
                    1
                );
            }
        }
    }
    assert_eq!(
        file_download_threads(Some(&Provider::Github), mainland, 16, false),
        16
    );
    assert_eq!(
        file_download_threads(Some(&Provider::Github), mainland, 16, true),
        1
    );
    assert_eq!(
        file_download_threads(Some(&Provider::Github), international, 16, false),
        1
    );
    assert_eq!(file_download_threads(None, mainland, 16, false), 16);
    assert_eq!(file_download_threads(None, mainland, 0, false), 1);
    assert_eq!(file_download_threads(None, mainland, 64, false), 32);
    assert_eq!(
        selected_asset_proxy_mode(Some(ProxyMode::Fixed), &Provider::Github, international),
        ProxyMode::Fixed
    );
}

#[test]
fn legacy_auto_proxy_and_auto_direct_use_the_same_order() {
    let proxies = vec![
        "https://proxy-a.example/asset".into(),
        "https://proxy-b.example/asset".into(),
    ];
    let direct_first =
        plan_asset_download_candidates(ProxyMode::AutoDirect, DIRECT, proxies.clone()).unwrap();
    assert_eq!(
        direct_first
            .iter()
            .map(|item| item.route)
            .collect::<Vec<_>>(),
        [
            DownloadRoute::Direct,
            DownloadRoute::Proxy,
            DownloadRoute::Proxy
        ]
    );
    assert_eq!(direct_first[0].url, DIRECT);
    let proxy_first =
        plan_asset_download_candidates(ProxyMode::AutoProxy, DIRECT, proxies).unwrap();
    assert_eq!(
        proxy_first
            .iter()
            .map(|item| item.route)
            .collect::<Vec<_>>(),
        [
            DownloadRoute::Direct,
            DownloadRoute::Proxy,
            DownloadRoute::Proxy
        ]
    );
    assert_eq!(proxy_first[0].url, DIRECT);
    assert_eq!(proxy_first, direct_first);
}

#[test]
fn automatic_proxy_preserves_manifest_order_without_origin_priority() {
    let entries = vec![
        proxy("bundled", "bundled.example", false, true),
        proxy("disabled", "disabled.example", true, false),
        proxy("remote", "remote.example", true, true),
    ];
    let urls = asset_proxy_urls(
        &entries,
        DIRECT,
        false,
        "",
        "",
        ProxyMode::AutoProxy,
        Some("bundled"),
    );
    assert_eq!(urls.len(), 2);
    assert!(urls[0].starts_with("https://bundled.example/"));
    assert!(urls[1].starts_with("https://remote.example/"));

    let bundled_only = asset_proxy_urls(
        &entries[..1],
        DIRECT,
        false,
        "",
        "",
        ProxyMode::AutoProxy,
        None,
    );
    assert_eq!(bundled_only.len(), 1);
    assert!(bundled_only[0].starts_with("https://bundled.example/"));
}

#[test]
fn fixed_mode_uses_only_selected_enabled_proxy() {
    let entries = vec![
        proxy("first", "first.example", true, true),
        proxy("selected", "selected.example", false, true),
        proxy("disabled", "disabled.example", false, false),
    ];
    let selected = asset_proxy_urls(
        &entries,
        DIRECT,
        false,
        "",
        "",
        ProxyMode::Fixed,
        Some("selected"),
    );
    let candidates = plan_asset_download_candidates(ProxyMode::Fixed, DIRECT, selected).unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].route, DownloadRoute::Proxy);
    assert!(candidates[0].url.starts_with("https://selected.example/"));
    assert!(plan_asset_download_candidates(ProxyMode::Fixed, DIRECT, Vec::new()).is_err());
    assert!(
        asset_proxy_urls(
            &entries,
            DIRECT,
            false,
            "",
            "",
            ProxyMode::Fixed,
            Some("disabled")
        )
        .is_empty()
    );
    for selected_id in [None, Some(" ")] {
        assert!(
            asset_proxy_urls(
                &entries,
                DIRECT,
                false,
                "",
                "",
                ProxyMode::Fixed,
                selected_id
            )
            .is_empty()
        );
    }
}

#[test]
fn repository_plan_materializes_threads_routes_and_explicit_choice_without_escaping() {
    use crate::models::{NetworkPlanSelection, RepositoryKind};
    let selection: NetworkPlanSelection =
        toml::from_str("mainland = 'qingyuqingxue-map-proxy-only'").unwrap();
    let url = "https://github.com/QingyuQingxue/OC2-Map-Bundle/releases/download/test/map.zip";
    let plan = plan_repository_download(
        &Provider::Github,
        RepositoryKind::Map,
        &selection,
        "cn",
        url,
        None,
        None,
        None,
    )
    .unwrap();
    assert_eq!(plan.candidates.len(), 1);
    assert_eq!(plan.candidates[0].route, DownloadRoute::Proxy);
    assert!(!plan.candidates[0].url.starts_with("https://github.com/"));
    assert_eq!(plan.download_threads, 1);
    assert!(!plan.execution.race);
    assert_eq!(plan.execution.max_parallelism, 1);
    assert!(
        plan_repository_download(
            &Provider::Github,
            RepositoryKind::Map,
            &selection,
            "cn",
            url,
            None,
            Some(ProxyMode::Direct),
            None
        )
        .is_err()
    );
    let selection: NetworkPlanSelection =
        toml::from_str("international = { raw = { download_threads = 8 } }").unwrap();
    let plan = plan_repository_download(
        &Provider::Github,
        RepositoryKind::Mod,
        &selection,
        "global",
        "https://github.com/user/repo/raw/refs/heads/main/a.dll",
        Some(crate::github_proxies::RawProxyContext {
            branch: "main",
            file_path: "a.dll",
        }),
        None,
        None,
    )
    .unwrap();
    assert_eq!(plan.download_threads, 8);
    assert_eq!(plan.candidates.len(), 1);
    assert!(!plan.execution.race);
}
