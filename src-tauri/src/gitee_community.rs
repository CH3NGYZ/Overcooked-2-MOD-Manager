use crate::community::{SearchRateLimits, SearchResults, repository_from_coordinates};
use crate::models::{CommunityDiscovery, CommunityMapRepository, ModRepositoryConfig, Provider};
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

/// 一个列表项带的 topic（小写）。
fn topics_of(item: ElementRef<'_>) -> HashSet<String> {
    item.select(&selectors().topics)
        .filter_map(|link| trusted_gitee_link(link.value().attr("href")?))
        .filter_map(|url| {
            let segments = url.path_segments()?.collect::<Vec<_>>();
            match segments.as_slice() {
                ["explore", "topic", topic] => Some(topic.to_ascii_lowercase()),
                _ => None,
            }
        })
        .collect()
}

/// 列表项该进哪一栏。
enum TopicKind {
    Mod,
    Map,
}

/// 准入与分流：两个必填 topic 都在才收，再看 `mod` / `map` 决定去处。
///
/// `mod` 优先：同时标了两个 topic 的仓库按 MOD 仓库收，否则它的 Release
/// 会被 MOD 页与地图页各刷一遍。两个分流 topic 都没标的列表项静默跳过 ——
/// 只是「用 BepInEx 的胡闹厨房仓库」，不是发现失败。
fn classify_topics(topics: &HashSet<String>) -> Option<TopicKind> {
    if !crate::community::REQUIRED_TOPICS
        .iter()
        .all(|topic| topics.contains(*topic))
    {
        return None;
    }
    if topics.contains("mod") {
        return Some(TopicKind::Mod);
    }
    topics.contains("map").then_some(TopicKind::Map)
}

struct TopicPage {
    repositories: Vec<ModRepositoryConfig>,
    map_repositories: Vec<CommunityMapRepository>,
    has_next: bool,
    incomplete: bool,
}

fn parse_page(html: &str, page: usize) -> Result<TopicPage, String> {
    let document = Html::parse_document(html);
    let list = document
        .select(&selectors().list)
        .next()
        .ok_or("community.giteeWebInvalid")?;
    let mut result = TopicPage {
        repositories: Vec::new(),
        map_repositories: Vec::new(),
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
        let Some(kind) = classify_topics(&topics_of(item)) else {
            continue;
        };
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
        match kind {
            TopicKind::Mod => {
                if let Some(repository) =
                    repository_from_coordinates(&Provider::Gitee, &owner, &name, "", &note)
                {
                    result.repositories.push(repository);
                } else {
                    result.incomplete = true;
                }
            }
            TopicKind::Map => result.map_repositories.push(CommunityMapRepository {
                provider: Provider::Gitee,
                username: owner,
                repository: name,
            }),
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
    let mut url = Url::parse(endpoint).map_err(|_| "community.giteeWebUrlInvalid")?;
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
        map_repositories: Vec::new(),
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
        let url = Url::parse(&url).map_err(|_| "community.giteeWebUrlInvalid")?;
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
        for repository in parsed.map_repositories {
            let key = format!(
                "{}/{}",
                repository.username.to_ascii_lowercase(),
                repository.repository.to_ascii_lowercase()
            );
            if seen.insert(key) {
                results.map_repositories.push(repository);
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
    /*
     * `total_count` 是「这次发现到多少仓库」（两类都算），与 MOD 仓库 ID
     * 列表不同：地图源仓库不进 `repositories`，但用户眼里它们同样是发现结果。
     */
    results.discovery.total_count =
        (results.repositories.len() + results.map_repositories.len()) as u64;
    results.discovery.repository_ids = results
        .repositories
        .iter()
        .map(|repo| repo.id.clone())
        .collect();
    Ok(results)
}

#[cfg(test)]
mod tests;
