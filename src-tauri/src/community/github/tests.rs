use super::super::state::tests::limits;
use super::super::test_support::{item, item_with_topics, page, server};
use super::*;
use crate::models::Provider;
use std::collections::HashMap;

#[tokio::test]
async fn repositories_are_split_by_the_mod_and_map_topics() {
    let (url, task) = server(vec![(
        200,
        String::new(),
        page(
            vec![
                item_with_topics("Owner", "Mod", &["overcooked-2", "bepinex", "mod"]),
                item_with_topics("Owner", "Map", &["overcooked-2", "bepinex", "map"]),
                item_with_topics("Owner", "Both", &["overcooked-2", "bepinex", "mod", "map"]),
                item_with_topics("Owner", "Plain", &["overcooked-2", "bepinex"]),
            ],
            4,
        ),
    )]);
    let result = search_all(
        &Client::new(),
        &format!("{url}/search"),
        None,
        &[],
        &mut limits(),
    )
    .await
    .unwrap();
    task.join().unwrap();
    assert_eq!(
        result
            .repositories
            .iter()
            .map(|repo| repo.repository.as_str())
            .collect::<Vec<_>>(),
        ["Mod", "Both"]
    );
    assert_eq!(result.map_repositories.len(), 1);
    assert_eq!(result.map_repositories[0].username, "Owner");
    assert_eq!(result.map_repositories[0].repository, "Map");
    assert_eq!(result.map_repositories[0].provider, Provider::Github);
    /*
     * 既没标 `mod` 也没标 `map` 的仓库是很正常的结果（只是用 BepInEx
     * 的胡闹厨房仓库），不能算成「这次发现不完整」。
     */
    assert!(!result.discovery.incomplete_results);
}

#[test]
fn target_order_and_credentials_follow_the_configured_proxy_order() {
    let official = page_url(SEARCH_ENDPOINT, 2).unwrap();
    let proxies = ["first", "disabled", "last", "duplicate"].map(|id| GitHubProxy {
        id: id.into(),
        name: id.into(),
        template: format!(
            "https://{}.example/{{url}}",
            if id == "duplicate" { "first" } else { id }
        ),
        enabled: id != "disabled",
        regions: vec![],
        description: String::new(),
        builtin: false,
        remote: false,
    });
    let targets = search_targets(&official, Some("test-token"), &proxies);
    assert_eq!(targets.len(), 3);
    assert_eq!(targets[0].url, official);
    assert_eq!(targets[0].token, Some("test-token"));
    assert_eq!(targets[1].route, "proxy-first");
    assert_eq!(targets[2].route, "proxy-last");
    assert!(targets[1..].iter().all(|target| target.token.is_none()));
}

#[tokio::test]
async fn search_requests_every_page_with_ten_items_and_never_follows_untrusted_link_hosts() {
    for link in [
        String::new(),
        "Link: <https://untrusted.example/?page=2>; rel=\"next\"\r\n".into(),
    ] {
        let (url, task) = server(vec![
            (
                200,
                link,
                page(
                    (0..10)
                        .map(|index| item("Owner", &format!("Mod{index}")))
                        .collect(),
                    11,
                ),
            ),
            (200, String::new(), page(vec![item("Owner", "Mod10")], 11)),
        ]);
        let result = search_all(
            &Client::new(),
            &format!("{url}/search"),
            None,
            &[],
            &mut limits(),
        )
        .await
        .unwrap();
        assert_eq!(result.repositories.len(), 11);
        assert_eq!(result.discovery.repository_ids.len(), 11);
        assert!(!result.discovery.incomplete_results);
        let requests = task.join().unwrap();
        for (index, request) in requests.iter().enumerate() {
            let path = request
                .lines()
                .next()
                .unwrap()
                .split_whitespace()
                .nth(1)
                .unwrap();
            let query = Url::parse(&format!("{url}{path}")).unwrap();
            let pairs = query.query_pairs().collect::<HashMap<_, _>>();
            assert_eq!(pairs["q"], SEARCH_QUERY);
            assert_eq!(pairs["per_page"], "10");
            assert_eq!(pairs["page"], (index + 1).to_string());
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("cache-control: no-cache")
            );
        }
    }
}

#[tokio::test]
async fn failed_targets_and_malformed_json_fall_through_without_proxy_credentials() {
    let (url, task) = server(vec![
        (500, String::new(), "{}".into()),
        (200, String::new(), "{}".into()),
        (200, String::new(), page(vec![item("Owner", "Mod")], 1)),
    ]);
    let targets = ["official", "first", "last"]
        .into_iter()
        .enumerate()
        .map(|(index, route)| SearchTarget {
            url: format!("{url}/{route}"),
            route: route.into(),
            token: (index == 0).then_some("test-token"),
        })
        .collect();
    let (value, _) = fetch_page_from_targets(&Client::new(), targets, 1, &mut limits())
        .await
        .unwrap();
    assert_eq!(value["total_count"], 1);
    let requests = task.join().unwrap();
    assert!(
        requests[0]
            .to_ascii_lowercase()
            .contains("authorization: bearer test-token")
    );
    assert!(
        requests[1..]
            .iter()
            .all(|request| !request.to_ascii_lowercase().contains("authorization:"))
    );
}

#[tokio::test]
async fn capped_or_incomplete_results_are_reported_honestly() {
    let (url, task) = server(vec![(200, String::new(), page(vec![], 1001))]);
    let result = search_all(&Client::new(), &url, None, &[], &mut limits())
        .await
        .unwrap();
    assert!(result.discovery.truncated && result.discovery.incomplete_results);
    assert_eq!(result.discovery.total_count, 1001);
    task.join().unwrap();
}
