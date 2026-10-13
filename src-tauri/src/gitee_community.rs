use crate::community::{SearchRateLimits, SearchResults, repository_from_coordinates};
use crate::models::{CommunityDiscovery, Provider, RepositoryConfig};
use scraper::{ElementRef, Html, Selector};
use std::collections::HashSet;
use std::future::Future;
use std::path::PathBuf;
use std::sync::OnceLock;
use tauri::{AppHandle, Runtime};
use url::Url;

pub const TOPIC_ENDPOINT: &str = "https://gitee.com/explore/topic/overcooked-2";
const MAX_PAGES: usize = 100;
const MAX_REPOSITORIES: usize = 1000;

struct TopicSelectors {
    list: Selector,
    repository: Selector,
    description: Selector,
    topics: Selector,
    pagination: Selector,
}

fn selectors() -> &'static TopicSelectors {
    static SELECTORS: OnceLock<TopicSelectors> = OnceLock::new();
    SELECTORS.get_or_init(|| TopicSelectors {
        list: Selector::parse(".explore-repo__list").expect("fixed list selector"),
        repository: Selector::parse("a.project-namespace-path[href]")
            .expect("fixed repository selector"),
        description: Selector::parse(".project-desc").expect("fixed description selector"),
        topics: Selector::parse("a.project-label-item-box[href]").expect("fixed topic selector"),
        pagination: Selector::parse(
            ".pagination a[href], .pager a[href], .pagination-container a[href], a[rel=next]",
        )
        .expect("fixed pagination selector"),
    })
}

fn trusted_gitee_link(href: &str) -> Option<Url> {
    let base = Url::parse(TOPIC_ENDPOINT).ok()?;
    let url = base.join(href).ok()?;
    (url.scheme() == "https"
        && url.host_str() == Some("gitee.com")
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none())
    .then_some(url)
}

fn repository_coordinates(item: ElementRef<'_>) -> Option<(String, String)> {
    let link = item.select(&selectors().repository).next()?;
    let url = trusted_gitee_link(link.value().attr("href")?)?;
    if url.query().is_some() || url.fragment().is_some() || url.path().contains('%') {
        return None;
    }
    let segments = url.path_segments()?.collect::<Vec<_>>();
    let [owner, repository] = segments.as_slice() else {
        return None;
    };
    if matches!(
        *owner,
        "explore" | "api" | "assets" | "static" | "login" | "signup"
    ) || item
        .value()
        .attr("data-username")
        .is_some_and(|username| !username.eq_ignore_ascii_case(owner))
    {
        return None;
    }
    crate::releases::repository_coordinates(owner, repository).ok()
}

fn has_required_topics(item: ElementRef<'_>) -> bool {
    let topics = item
        .select(&selectors().topics)
        .filter_map(|link| trusted_gitee_link(link.value().attr("href")?))
        .filter_map(|url| {
            let segments = url.path_segments()?.collect::<Vec<_>>();
            match segments.as_slice() {
                ["explore", "topic", topic] => Some(topic.to_ascii_lowercase()),
                _ => None,
            }
        })
        .collect::<HashSet<_>>();
    ["overcooked-2", "bepinex", "mod"]
        .iter()
        .all(|topic| topics.contains(*topic))
}

struct TopicPage {
    repositories: Vec<RepositoryConfig>,
    has_next: bool,
    incomplete: bool,
}

fn parse_page(html: &str, page: usize) -> Result<TopicPage, String> {
    let document = Html::parse_document(html);
    let list = document
        .select(&selectors().list)
        .next()
        .ok_or("Gitee 社区网页结构无效或需要验证，请稍后重试")?;
    let mut result = TopicPage {
        repositories: Vec::new(),
        has_next: false,
        incomplete: false,
    };
    // Only direct list items are repositories; avatars, scripts and unrelated links are not.
    for item in list.children().filter_map(ElementRef::wrap) {
        if !item.value().has_class(
            "explore-repo__list-item",
            scraper::CaseSensitivity::CaseSensitive,
        ) {
            continue;
        }
        let Some((owner, name)) = repository_coordinates(item) else {
            result.incomplete = true;
            continue;
        };
        if !has_required_topics(item) {
            continue;
        }
        let note = item
            .select(&selectors().description)
            .next()
            .map(|description| {
                description
                    .value()
                    .attr("title")
                    .filter(|value| !value.trim().is_empty())
                    .map(str::to_string)
                    .unwrap_or_else(|| description.text().collect::<String>())
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();
        if let Some(repository) =
            repository_from_coordinates(&Provider::Gitee, &owner, &name, "", &note)
        {
            result.repositories.push(repository);
        } else {
            result.incomplete = true;
        }
    }
    for link in document.select(&selectors().pagination) {
        let next_page = link
            .value()
            .attr("href")
            .and_then(trusted_gitee_link)
            .filter(|url| url.path().trim_end_matches('/') == "/explore/topic/overcooked-2")
            .and_then(|url| {
                let pages = url
                    .query_pairs()
                    .filter(|(key, _)| key == "page")
                    .map(|(_, value)| value.parse::<usize>().ok())
                    .collect::<Vec<_>>();
                match pages.as_slice() {
                    [] => Some(1),
                    [Some(page)] if *page > 0 => Some(*page),
                    _ => None,
                }
            });
        if let Some(next_page) = next_page {
            result.has_next |= next_page > page;
        } else {
            result.incomplete = true;
        }
    }
    Ok(result)
}

fn page_url(endpoint: &str, page: usize) -> Result<String, String> {
    let mut url = Url::parse(endpoint).map_err(|_| "Gitee 社区网页地址无效")?;
    url.query_pairs_mut()
        .clear()
        .append_pair("page", &page.to_string());
    Ok(url.into())
}

pub(crate) async fn search_all<R: Runtime>(
    app: &AppHandle<R>,
    data_directory: PathBuf,
    limits: &mut SearchRateLimits,
) -> Result<SearchResults, String> {
    // Creating the WebView starts navigation, so defer it until the rate gate opens.
    let browser = tokio::sync::OnceCell::new();
    let browser = &browser;
    search_pages(TOPIC_ENDPOINT, limits, |url, page| {
        let data_directory = data_directory.clone();
        async move {
            let browser = browser
                .get_or_try_init(|| async {
                    crate::gitee_webview::TopicBrowser::open(
                        app,
                        url.clone(),
                        data_directory,
                        Default::default(),
                    )
                })
                .await?;
            browser.read_page(url, page > 1).await
        }
    })
    .await
}

async fn search_pages<F, Fut>(
    endpoint: &str,
    limits: &mut SearchRateLimits,
    mut fetch: F,
) -> Result<SearchResults, String>
where
    F: FnMut(Url, usize) -> Fut,
    Fut: Future<Output = Result<String, String>>,
{
    let mut results = SearchResults {
        repositories: Vec::new(),
        discovery: CommunityDiscovery::default(),
    };
    let mut seen = HashSet::new();
    for page in 1..=MAX_PAGES {
        let url = page_url(endpoint, page)?;
        log::info!(
            "正在请求 Gitee 社区网页：页={}，地址={}",
            page,
            crate::repository_sources::safe_log_address(&url)
        );
        limits.before_request("official").await?;
        let url = Url::parse(&url).map_err(|_| "Gitee 社区网页地址无效")?;
        let html = fetch(url, page).await?;
        let parsed = parse_page(&html, page)?;
        results.discovery.incomplete_results |= parsed.incomplete;
        let mut added = 0;
        for repository in parsed.repositories {
            if seen.insert(repository.id.clone()) {
                results.repositories.push(repository);
                added += 1;
                if results.repositories.len() >= MAX_REPOSITORIES {
                    results.discovery.truncated = true;
                    break;
                }
            }
        }
        log::info!(
            "Gitee 社区网页解析完成：页={}，新增={}，已发现={}，存在后续页={}",
            page,
            added,
            results.repositories.len(),
            parsed.has_next
        );
        if results.discovery.truncated || !parsed.has_next {
            break;
        }
        if page == MAX_PAGES {
            results.discovery.truncated = true;
        }
    }
    results.discovery.total_count = results.repositories.len() as u64;
    results.discovery.repository_ids = results
        .repositories
        .iter()
        .map(|repo| repo.id.clone())
        .collect();
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ATTACHED_LIST: &str = include_str!("../../tests/fixtures/gitee-topic.html");

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
            let html = format!(
                "<div class='explore-repo__list'></div><a rel='next' href='{href}'>Next</a>"
            );
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
        let result = search_pages(TOPIC_ENDPOINT, &mut SearchRateLimits::for_gitee_test(), |_, page| {
            std::future::ready(if page == 1 {
                Ok(format!("<div class='explore-repo__list'>{}</div><a rel='next' href='?page=2'>Next</a>", list_item("/Owner/Mod", "Mod")))
            } else { Err("verification failed".into()) })
        }).await;
        assert!(matches!(result, Err(error) if error == "verification failed"));
    }

    #[tokio::test]
    async fn endless_pagination_is_bounded_and_marked_incomplete_instead_of_claiming_all_results() {
        let mut pages = 0;
        let result = search_pages(TOPIC_ENDPOINT, &mut SearchRateLimits::for_gitee_test(), |_, page| {
            pages += 1;
            std::future::ready(Ok(format!("<div class='explore-repo__list'>{}</div><a rel='next' href='?page={}'>Next</a>",
                list_item(&format!("/Owner/Mod{page}"), "Mod"), page + 1)))
        }).await.unwrap();
        assert_eq!(pages, MAX_PAGES);
        assert_eq!(result.repositories.len(), MAX_PAGES);
        assert!(result.discovery.truncated);
    }
}
