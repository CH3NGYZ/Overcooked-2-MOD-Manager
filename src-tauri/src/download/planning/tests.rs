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
        ordered_asset_download_candidates(ProxyMode::AutoDirect, DIRECT, duplicated.clone());
    let proxy_first = ordered_asset_download_candidates(ProxyMode::AutoProxy, DIRECT, duplicated);
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
}

#[test]
fn pasted_urls_use_platform_and_region_defaults_without_forwarding_queries() {
    use crate::models::RepositoryKind;
    for (url, first_proxy, global_path) in [
        (
            DIRECT,
            "https://git.yylx.win/https://github.com/example/mod/releases/download/v1/mod.zip",
            "/example/mod/releases/download/v1/mod.zip",
        ),
        (
            "https://github.com/example/mod/releases/latest/download/mod.zip",
            "https://git.yylx.win/https://github.com/example/mod/releases/latest/download/mod.zip",
            "/example/mod/releases/latest/download/mod.zip",
        ),
        (
            "https://raw.githubusercontent.com/acme/mod/refs/heads/main/bin/Hello%20World.dll",
            "https://wget.la/https://raw.githubusercontent.com/acme/mod/main/bin/Hello%20World.dll",
            "/acme/mod/raw/refs/heads/main/bin/Hello%20World.dll",
        ),
    ] {
        for kind in [RepositoryKind::Mod, RepositoryKind::Map] {
            let parsed = Url::parse(url).unwrap();
            let url_kind = classify_remote_url(url);
            let mainland = plan_pasted_file(&parsed, url_kind, kind, "cn").unwrap();
            assert_eq!(mainland.candidates[0].url, url);
            assert_eq!(mainland.candidates[1].url, first_proxy);
            assert_eq!(mainland.download_threads, 1);
            assert_eq!(mainland.execution.max_parallelism, 3);
            assert!(mainland.execution.race);
            let global = plan_pasted_file(&parsed, url_kind, kind, "global").unwrap();
            assert_eq!(global.candidates[0].url, mainland.candidates[0].url);
            assert_ne!(
                global.candidates[0].identity,
                mainland.candidates[0].identity
            );
            assert_eq!(global.candidates.len(), 3);
            for (candidate, host) in global.candidates[1..]
                .iter()
                .zip(["oc2gh.ch3ng.top", "oc2gh.5882778.xyz"])
            {
                assert_eq!(candidate.route, DownloadRoute::Proxy);
                assert_eq!(candidate.url, format!("https://{host}{global_path}"));
            }
            assert!(global.execution.race);
            assert_eq!(global.execution.max_parallelism, 3);
        }
    }
    for url in [
        "https://gitee.com/acme/mod/releases/download/v1/mod.zip",
        "https://raw.atomgit.com/acme/mod/raw/main/bin/mod.dll",
        "https://github.com/example/mod/releases/download/v1/mod.zip?token=opaque",
    ] {
        let parsed = Url::parse(url).unwrap();
        let plan =
            plan_pasted_file(&parsed, classify_remote_url(url), RepositoryKind::Mod, "cn").unwrap();
        assert_eq!(
            plan.candidates,
            [DownloadCandidate {
                route: DownloadRoute::Direct,
                url: url.into(),
                identity: None,
            }]
        );
    }
}

#[test]
fn generic_file_threads_preserve_platform_region_and_configured_count() {
    let mainland = crate::config::REGION_CHINESE_MAINLAND;
    let international = crate::config::REGION_INTERNATIONAL;
    for provider in [Provider::Gitee, Provider::Gitcode] {
        for region in [mainland, international] {
            assert_eq!(generic_download_threads(Some(&provider), region, 16), 1);
        }
    }
    assert_eq!(
        generic_download_threads(Some(&Provider::Github), mainland, 16),
        16
    );
    assert_eq!(
        generic_download_threads(Some(&Provider::Github), international, 16),
        1
    );
    assert_eq!(generic_download_threads(None, mainland, 16), 16);
    assert_eq!(generic_download_threads(None, mainland, 0), 1);
    assert_eq!(generic_download_threads(None, mainland, 64), 32);
}

#[test]
fn fixed_mode_uses_only_selected_proxy_in_the_typed_route() {
    use crate::models::{NetworkPlanSelection, RepositoryKind};
    let default = NetworkPlanSelection::default();
    let plan = |selected_id| {
        plan_repository_download(
            &Provider::Github,
            RepositoryKind::Mod,
            &default,
            "cn",
            DIRECT,
            None,
            Some(ProxyMode::Fixed),
            selected_id,
        )
    };
    let selected = plan(Some("oc2gh-release-public-edgeone")).unwrap();
    assert_eq!(
        selected.candidates,
        [DownloadCandidate {
            route: DownloadRoute::Proxy,
            url: "https://oc2gh.5882778.xyz/example/mod/releases/download/v1/mod.zip".into(),
            identity: None,
        }]
    );
    for selected_id in [None, Some(" "), Some("oc2gh-raw-edgeone")] {
        assert_eq!(
            plan(selected_id).err().as_deref(),
            Some("proxy.noGlobalProxy")
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
    assert_eq!(
        plan.candidates
            .iter()
            .map(|candidate| (candidate.route, candidate.url.as_str()))
            .collect::<Vec<_>>(),
        [
            (
                DownloadRoute::Proxy,
                "https://oc2gh.ch3ng.top/QingyuQingxue/OC2-Map-Bundle/releases/download/test/map.zip",
            ),
            (
                DownloadRoute::Proxy,
                "https://oc2gh.5882778.xyz/QingyuQingxue/OC2-Map-Bundle/releases/download/test/map.zip",
            ),
        ]
    );
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
    let selection: NetworkPlanSelection = toml::from_str(
        "international = { raw = { download_threads = 8, route = [{ kind = 'direct' }], race = false, max_parallelism = 1 } }",
    )
    .unwrap();
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

#[test]
fn map_source_documents_preserve_proxy_only_order_and_raw_url_encoding() {
    use crate::models::{NetworkPlanSelection, RepositoryKind};
    let selection: NetworkPlanSelection = toml::from_str(
        "mainland = 'qingyuqingxue-map-proxy-only'\ninternational = 'qingyuqingxue-map-proxy-only'",
    )
    .unwrap();
    for region in ["cn", "global"] {
        for (is_raw, url, suffix) in [
            (
                true,
                "https://raw.githubusercontent.com/QingyuQingxue/OC2-Map-Bundle/main/a%20b.toml",
                "QingyuQingxue/OC2-Map-Bundle/raw/refs/heads/main/a%20b.toml",
            ),
            (
                false,
                "https://github.com/QingyuQingxue/OC2-Map-Bundle/releases/download/test/maps.toml",
                "QingyuQingxue/OC2-Map-Bundle/releases/download/test/maps.toml",
            ),
        ] {
            let plan = plan_source_document(
                &Provider::Github,
                RepositoryKind::Map,
                &selection,
                region,
                url,
                is_raw,
            )
            .unwrap();
            assert_eq!(
                plan.candidates
                    .into_iter()
                    .map(|candidate| (candidate.route, candidate.url))
                    .collect::<Vec<_>>(),
                [
                    (
                        DownloadRoute::Proxy,
                        format!("https://oc2gh.ch3ng.top/{suffix}")
                    ),
                    (
                        DownloadRoute::Proxy,
                        format!("https://oc2gh.5882778.xyz/{suffix}")
                    ),
                ]
            );
            assert!(!plan.execution.race);
            assert_eq!(plan.execution.max_parallelism, 1);
        }
    }
}

#[test]
fn repository_document_queries_stay_on_explicit_direct_routes() {
    use crate::models::{NetworkPlanSelection, RepositoryKind};
    let proxy_only = toml::from_str("mainland = 'qingyuqingxue-map-proxy-only'").unwrap();
    for (is_raw, url) in [
        (
            true,
            "https://raw.githubusercontent.com/acme/mod/main/maps.toml?token=opaque",
        ),
        (
            false,
            "https://github.com/acme/mod/releases/download/v1/maps.toml?token=opaque",
        ),
    ] {
        for kind in [RepositoryKind::Mod, RepositoryKind::Map] {
            let plan = plan_source_document(
                &Provider::Github,
                kind,
                &NetworkPlanSelection::default(),
                "cn",
                url,
                is_raw,
            )
            .unwrap();
            assert_eq!(
                plan.candidates,
                [DownloadCandidate {
                    route: DownloadRoute::Direct,
                    url: url.into(),
                    identity: None,
                }]
            );
        }
        let rejected = plan_source_document(
            &Provider::Github,
            RepositoryKind::Map,
            &proxy_only,
            "cn",
            url,
            is_raw,
        );
        assert_eq!(rejected.err().as_deref(), Some("proxy.noGlobalProxy"));
    }
    let fixed = plan_repository_download(
        &Provider::Github,
        RepositoryKind::Mod,
        &NetworkPlanSelection::default(),
        "cn",
        "https://github.com/acme/mod/releases/download/v1/mod.zip?token=opaque",
        None,
        Some(ProxyMode::Fixed),
        Some("git-yylx"),
    );
    assert_eq!(fixed.err().as_deref(), Some("proxy.noGlobalProxy"));
}
