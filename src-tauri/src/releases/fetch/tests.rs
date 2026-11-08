use super::super::test_support::{ApiTestServer, api_test_client, api_test_repository};
use super::*;

#[tokio::test]
#[ignore = "requires public GitCode API proxy network access"]
async fn real_gitcode_proxy_reads_complete_release_pages() {
    let client = crate::releases::build_client(30).unwrap();
    let mut repo = api_test_repository();
    repo.provider = Provider::Gitcode;
    repo.username = "CH3NGY2".into();
    repo.repository = "OC2-Map-Bundle".into();
    let config = crate::config::default_config();
    let proxies = crate::providers::api_proxies(&config, &repo.provider);
    let started = std::time::Instant::now();
    let releases = crate::releases::pagination::collect_release_pages(|page| {
        let official = crate::providers::repository_release_api_url_page(
            &repo.provider,
            &repo.username,
            &repo.repository,
            Some(page),
        )
        .unwrap();
        let targets = crate::github_proxies::proxy_urls(proxies, &official, false, None)
            .into_iter()
            .map(|url| (url, None))
            .collect();
        let client = &client;
        let repo = &repo;
        async move {
            let result = fetch_releases_from_targets(client, repo, &official, targets).await?;
            println!(
                "GitCode proxy page={page} raw_count={} projected_count={}",
                result.1,
                result.0.len()
            );
            Ok::<_, ReleaseError>(result)
        }
    })
    .await
    .unwrap();
    assert!(
        !releases.is_empty(),
        "the public bundle must contain release declarations"
    );
    assert!(
        releases
            .iter()
            .any(|release| release.body.contains("<!-- oc2-map"))
    );
    println!(
        "GitCode proxy total={} elapsed_ms={}",
        releases.len(),
        started.elapsed().as_millis()
    );
}

#[tokio::test]
async fn release_api_success_stops_retries_and_remaining_targets() {
    let official = ApiTestServer::new(vec![("200 OK", "[]")]);
    let proxy = ApiTestServer::new(vec![("200 OK", "[]")]);
    let targets = vec![
        (official.url.clone(), None),
        (official.url.clone(), Some("test-token")),
        (proxy.url.clone(), None),
    ];
    assert!(
        fetch_releases_from_targets(
            &api_test_client(),
            &api_test_repository(),
            &official.url,
            targets
        )
        .await
        .is_ok()
    );
    assert_eq!(official.requests().len(), 1);
    assert!(proxy.requests().is_empty());
}

#[tokio::test]
async fn gitee_official_403_skips_token_and_proxy_success_stops_fallback() {
    let official = ApiTestServer::new(vec![("403 Forbidden", "")]);
    let proxy = ApiTestServer::new(vec![("500 Internal Server Error", ""), ("200 OK", "[]")]);
    let remaining = ApiTestServer::new(vec![("200 OK", "[]")]);
    let targets = vec![
        (official.url.clone(), None),
        (official.url.clone(), Some("test-token")),
        (proxy.url.clone(), None),
        (remaining.url.clone(), None),
    ];
    assert!(
        fetch_releases_from_targets(
            &api_test_client(),
            &api_test_repository(),
            &official.url,
            targets
        )
        .await
        .is_ok()
    );
    assert_eq!(official.requests().len(), 1);
    assert_eq!(proxy.requests().len(), 2);
    assert!(remaining.requests().is_empty());
    assert!(proxy.requests().iter().all(|request| {
        !request.to_ascii_lowercase().contains("authorization:") && !request.contains("test-token")
    }));
}

#[tokio::test]
async fn gitee_proxy_403_ends_the_refresh_immediately() {
    let official = ApiTestServer::new(vec![("403 Forbidden", "")]);
    let proxy = ApiTestServer::new(vec![("403 Forbidden", "")]);
    let remaining = ApiTestServer::new(vec![("200 OK", "[]")]);
    let targets = vec![
        (official.url.clone(), None),
        (proxy.url.clone(), None),
        (remaining.url.clone(), None),
    ];
    let error = fetch_releases_from_targets(
        &api_test_client(),
        &api_test_repository(),
        &official.url,
        targets,
    )
    .await
    .unwrap_err();
    assert!(error.is_forbidden());
    assert_eq!(official.requests().len(), 1);
    assert_eq!(proxy.requests().len(), 1);
    assert!(remaining.requests().is_empty());
}

#[tokio::test]
async fn gitee_official_and_proxy_other_errors_exhaust_three_attempts_each() {
    let official = ApiTestServer::new(vec![("500 Internal Server Error", ""); 3]);
    let proxy = ApiTestServer::new(vec![("502 Bad Gateway", ""); 3]);
    let targets = vec![(official.url.clone(), None), (proxy.url.clone(), None)];
    assert!(
        fetch_releases_from_targets(
            &api_test_client(),
            &api_test_repository(),
            &official.url,
            targets
        )
        .await
        .is_err()
    );
    assert_eq!(official.requests().len(), 3);
    assert_eq!(proxy.requests().len(), 3);
}

#[test]
fn gitee_community_source_tag_zip_is_removed_after_real_api_parsing() {
    let repo =
        crate::community::repository_from_coordinates(&Provider::Gitee, "Owner", "Mod", "", "")
            .unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
            let server = ApiTestServer::new(vec![("200 OK", r#"[{"tag_name":"v1.8.7.15","assets":[
                {"name":"v1.8.7.15.zip","browser_download_url":"https://gitee.com/Owner/Mod/archive/v1.8.7.15.zip"},
                {"name":"Mod.zip","browser_download_url":"https://gitee.com/Owner/Mod/releases/download/v1.8.7.15/Mod.zip"},
                {"name":"Mod.dll","browser_download_url":"https://gitee.com/Owner/Mod/releases/download/v1.8.7.15/Mod.dll"}
            ]}]"#)]);
            let releases = fetch_releases_from_targets(&api_test_client(), &repo, &server.url,
                vec![(server.url.clone(), None)]).await.unwrap().0;
            assert_eq!(releases[0].assets.iter().map(|asset| asset.name.as_str()).collect::<Vec<_>>(), ["Mod.zip", "Mod.dll"]);
            assert_eq!(server.requests().len(), 1);
        });
}

#[tokio::test]
async fn all_providers_paginate_using_raw_response_counts_and_explicit_page_one() {
    let first = serde_json::to_string(
        &(0..50)
            .map(|index| {
                if index == 0 {
                    serde_json::json!({"assets": []})
                } else {
                    serde_json::json!({"tag_name": format!("v1.0.{index}"), "assets": []})
                }
            })
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let last = r#"[{"tag_name":"v2.0.0","assets":[]}]"#;
    for provider in [Provider::Github, Provider::Gitee, Provider::Gitcode] {
        let server = ApiTestServer::new(vec![
            ("200 OK", Box::leak(first.clone().into_boxed_str())),
            ("200 OK", last),
        ]);
        let client = api_test_client();
        let mut repo = api_test_repository();
        repo.provider = provider.clone();
        let releases = crate::releases::pagination::collect_release_pages(|page| {
            let official = crate::providers::repository_release_api_url_page(
                &provider,
                "owner",
                "repo",
                Some(page),
            )
            .unwrap();
            let query = url::Url::parse(&official)
                .unwrap()
                .query()
                .unwrap()
                .to_string();
            let target = format!("{}?{}", server.url, query);
            let client = &client;
            let repo = &repo;
            async move {
                fetch_releases_from_targets(client, repo, &target, vec![(target.clone(), None)])
                    .await
            }
        })
        .await
        .unwrap();
        assert_eq!(
            releases.len(),
            50,
            "{provider:?}: 49 valid entries on the first page plus one on the second"
        );
        assert_eq!(releases.last().unwrap().tag_name, "v2.0.0");
        let requests = server.requests();
        assert_eq!(requests.len(), 2, "{provider:?}");
        assert!(
            requests[0].starts_with("GET /releases?per_page=50&page=1&"),
            "{}",
            requests[0]
        );
        assert!(
            requests[1].starts_with("GET /releases?per_page=50&page=2&"),
            "{}",
            requests[1]
        );
    }
}
