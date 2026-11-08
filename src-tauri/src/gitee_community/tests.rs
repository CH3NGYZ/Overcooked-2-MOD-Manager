use super::*;

const ATTACHED_LIST: &str = include_str!("../../../tests/fixtures/gitee-topic.html");

#[test]
fn attached_list_extracts_coordinates_descriptions_and_case_insensitive_three_topics() {
    let parsed = parse_page(ATTACHED_LIST, 1).unwrap();
    assert_eq!(parsed.repositories.len(), 2);
    assert_eq!(parsed.repositories[0].username, "ch3ngyz");
    assert_eq!(
        parsed.repositories[0].repository,
        "Overcooked-2-HostUtilities-Beta-Releases"
    );
    assert_eq!(
        parsed.repositories[1].repository,
        "Overcooked-2-SkipLoading"
    );
    assert_eq!(parsed.repositories[1].note, "胡闹厨房2 模组 & MOD");
    assert!(
        parsed
            .repositories
            .iter()
            .all(|repo| repo.provider == Provider::Gitee
                && repo.source_id.as_deref() == Some(crate::community::GITEE_SOURCE_ID)
                && repo.branch.is_empty()
                && repo.group.is_empty())
    );
    assert!(!parsed.incomplete && !parsed.has_next);
}

#[test]
fn missing_list_is_an_error_but_a_real_empty_list_is_valid() {
    assert!(parse_page("<html><script>challenge()</script></html>", 1).is_err());
    let empty = parse_page("<div class='explore-repo__list'></div>", 1).unwrap();
    assert!(empty.repositories.is_empty());
    assert!(!empty.incomplete);
}

fn list_item(href: &str, name: &str) -> String {
    format!(
        r#"<div class="item explore-repo__list-item">
          <a class="project-namespace-path" href="{href}">{name}</a>
          <div class="project-desc" title="Description &amp; text"></div>
          <a class="project-label-item-box" href="/explore/topic/overcooked-2">topic</a>
          <a class="project-label-item-box" href="/explore/topic/BepInEx">topic</a>
          <a class="project-label-item-box" href="/explore/topic/MOD">topic</a>
        </div>"#
    )
}

#[test]
fn repository_links_are_scoped_to_direct_items_and_trusted_coordinates() {
    let good = list_item("/Owner/Mod", "Valid");
    let invalid = [
        "https://example.com/Owner/Mod",
        "http://gitee.com/Owner/Mod",
        "https://user@gitee.com/Owner/Mod",
        "https://gitee.com:444/Owner/Mod",
        "/Owner/Mod/tree/main",
        "/Owner/Mod?access_token=test",
        "/Owner/Mod#readme",
        "/Owner/%2fMod",
        "/api/v5",
    ]
    .iter()
    .map(|href| list_item(href, "Invalid"))
    .collect::<String>();
    let html = format!(
        "{good}<div class='explore-repo__list'><section>{good}</section>{invalid}{good}</div>"
    );
    let parsed = parse_page(&html, 1).unwrap();
    assert_eq!(parsed.repositories.len(), 1);
    assert_eq!(parsed.repositories[0].username, "Owner");
    assert_eq!(parsed.repositories[0].note, "Description & text");
    assert!(parsed.incomplete);
    let mismatched = good.replace(
        "class=\"item explore-repo__list-item\"",
        "class=\"item explore-repo__list-item\" data-username=\"Other\"",
    );
    assert!(
        parse_page(
            &format!("<div class='explore-repo__list'>{mismatched}</div>"),
            1
        )
        .unwrap()
        .repositories
        .is_empty()
    );
}

#[test]
fn map_topic_repositories_are_discovered_separately_from_mod_repositories() {
    let raw = |href: &str, topics: &[&str]| {
        let links = topics
            .iter()
            .map(|topic| {
                format!("<a class='project-label-item-box' href='/explore/topic/{topic}'>t</a>")
            })
            .collect::<String>();
        format!(
            "<div class='item explore-repo__list-item'>\
                 <a class='project-namespace-path' href='{href}'>x</a>\
                 <div class='project-desc' title='desc'></div>{links}</div>"
        )
    };
    let html = format!(
        "<div class='explore-repo__list'>{}{}{}{}</div>",
        raw("/Owner/Mod", &["overcooked-2", "bepinex", "mod"]),
        raw("/Owner/MapPack", &["overcooked-2", "bepinex", "map"]),
        raw("/Owner/Both", &["overcooked-2", "bepinex", "mod", "map"]),
        raw("/Owner/Plain", &["overcooked-2", "bepinex"]),
    );
    let parsed = parse_page(&html, 1).unwrap();
    assert_eq!(
        parsed
            .repositories
            .iter()
            .map(|repo| repo.repository.as_str())
            .collect::<Vec<_>>(),
        ["Mod", "Both"]
    );
    assert_eq!(parsed.map_repositories.len(), 1);
    assert_eq!(parsed.map_repositories[0].username, "Owner");
    assert_eq!(parsed.map_repositories[0].repository, "MapPack");
    assert_eq!(parsed.map_repositories[0].provider, Provider::Gitee);
    // 两个分流 topic 都没标不是发现失败
    assert!(!parsed.incomplete);
}

#[test]
fn pagination_uses_only_trusted_topic_links_and_unambiguous_page_numbers() {
    let html = "<div class='explore-repo__list'></div><div class='pagination'>
            <a href='/explore/topic/overcooked-2'>First</a><a href='?page=1'>Previous</a>
            <a href='?page=2'>Current</a><a href='?page=3'>Next</a></div>";
    let parsed = parse_page(html, 2).unwrap();
    assert!(parsed.has_next && !parsed.incomplete);
    for href in [
        "https://example.com/?page=3",
        "/api/v5/search?page=3",
        "?page=2&page=3",
        "?page=0",
        "?page=bad",
    ] {
        let html =
            format!("<div class='explore-repo__list'></div><a rel='next' href='{href}'>Next</a>");
        let parsed = parse_page(&html, 2).unwrap();
        assert!(!parsed.has_next && parsed.incomplete);
    }
}

#[tokio::test]
async fn browser_pages_follow_pagination_even_when_one_page_has_no_matching_topics() {
    let mut requested = Vec::new();
    let item = list_item("/Owner/Mod", "Mod");
    let result = search_pages(TOPIC_ENDPOINT, &mut SearchRateLimits::for_gitee_test(), |url, page| {
            requested.push(url.clone());
            let html = match page {
                1 => format!("<div class='explore-repo__list'>{item}</div><a rel='next' href='?page=2'>Next</a>"),
                2 => "<div class='explore-repo__list'></div><div class='pagination'><a href='?page=3'>Next</a></div>".into(),
                3 => format!("<div class='explore-repo__list'>{item}{}</div>", list_item("/Owner/Second", "Second")),
                _ => panic!("unexpected extra page"),
            };
            std::future::ready(Ok(html))
        }).await.unwrap();
    assert_eq!(requested.len(), 3);
    assert!(
        requested
            .iter()
            .enumerate()
            .all(|(index, url)| url.as_str() == format!("{TOPIC_ENDPOINT}?page={}", index + 1))
    );
    assert_eq!(result.discovery.total_count, 2);
    assert_eq!(result.discovery.repository_ids.len(), 2);
    assert!(!result.discovery.incomplete_results && !result.discovery.truncated);
}

#[tokio::test]
async fn a_verification_failure_never_commits_a_partial_or_empty_success() {
    let result = search_pages(
        TOPIC_ENDPOINT,
        &mut SearchRateLimits::for_gitee_test(),
        |_, page| {
            std::future::ready(if page == 1 {
                Ok(format!(
                    "<div class='explore-repo__list'>{}</div><a rel='next' href='?page=2'>Next</a>",
                    list_item("/Owner/Mod", "Mod")
                ))
            } else {
                Err("verification failed".into())
            })
        },
    )
    .await;
    assert!(matches!(result, Err(error) if error == "verification failed"));
}

#[tokio::test]
async fn endless_pagination_is_bounded_and_marked_incomplete_instead_of_claiming_all_results() {
    let mut pages = 0;
    let result = search_pages(
        TOPIC_ENDPOINT,
        &mut SearchRateLimits::for_gitee_test(),
        |_, page| {
            pages += 1;
            std::future::ready(Ok(format!(
                "<div class='explore-repo__list'>{}</div><a rel='next' href='?page={}'>Next</a>",
                list_item(&format!("/Owner/Mod{page}"), "Mod"),
                page + 1
            )))
        },
    )
    .await
    .unwrap();
    assert_eq!(pages, MAX_PAGES);
    assert_eq!(result.repositories.len(), MAX_PAGES);
    assert!(result.discovery.truncated);
}
