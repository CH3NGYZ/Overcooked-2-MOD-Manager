use super::catalog::SearchHit;
use super::catalog::SearchResults;
use super::catalog::classify_search_item;
use super::state::SearchRateLimits;
use crate::api_scheduler::ApiRequestExt;
use crate::github_proxies;
use crate::models::CommunityDiscovery;
use crate::models::GitHubProxy;
use futures_util::StreamExt;
use reqwest::Client;
use reqwest::StatusCode;
use serde_json::Value;
use std::collections::HashSet;
use std::time::Duration;
use url::Url;

pub const SEARCH_ENDPOINT: &str = "https://api.github.com/search/repositories";

pub const PAGE_SIZE: usize = 10;

/// 搜索式只锁定两个必然成立的 topic。
///
/// `mod` / `map` 不是准入条件而是**分流条件**：写进搜索式就只能搜到标了
/// 三个标签的仓库 —— 只标了 `overcooked-2` + `bepinex` + `map` 的地图源
/// 会被整个漏掉，而这恰恰是新版地图源约定的第三个标签。准入与分流分开，
/// 作者怎么标都不会让仓库消失。
pub(super) const SEARCH_QUERY: &str = "topic:overcooked-2 topic:bepinex";

const RESULT_LIMIT: usize = 1000;

const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

fn page_url(endpoint: &str, page: usize) -> Result<String, String> {
    let mut url = Url::parse(endpoint).map_err(|_| "community.searchUrlInvalid")?;
    url.query_pairs_mut()
        .clear()
        .append_pair("q", SEARCH_QUERY)
        .append_pair("per_page", &PAGE_SIZE.to_string())
        .append_pair("page", &page.to_string());
    Ok(url.into())
}

pub(super) async fn fetch_page(
    client: &Client,
    url: &str,
    token: Option<&str>,
    route: &str,
    limits: &mut SearchRateLimits,
) -> Result<(Value, bool), String> {
    limits.before_request(route).await?;
    let mut request = client
        .get(url)
        .timeout(Duration::from_secs(5))
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header(
            reqwest::header::CACHE_CONTROL,
            "no-cache, no-store, max-age=0",
        )
        .header(reqwest::header::PRAGMA, "no-cache");
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    let response = request
        .send_api(crate::models::Provider::Github)
        .await
        .map_err(|error| error.without_url().to_string())?;
    let status = response.status();
    limits.observe(route, status, response.headers());
    log::info!("GitHub 社区搜索响应：渠道={}，状态={}", route, status);
    if !status.is_success() {
        return Err(
            if matches!(
                status,
                StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS
            ) {
                "community.searchRateLimited".into()
            } else {
                crate::locale_tag::with_params(
                    "community.searchHttpStatus",
                    &[("status", &status.to_string())],
                )
            },
        );
    }
    let has_next = response
        .headers()
        .get(reqwest::header::LINK)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|link| link.split(',').any(|part| part.contains("rel=\"next\"")));
    if response
        .content_length()
        .is_some_and(|size| size > MAX_RESPONSE_BYTES as u64)
    {
        return Err("community.searchResponseTooLarge".into());
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| error.without_url().to_string())?;
        if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err("community.searchResponseTooLarge".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let value = serde_json::from_slice(&bytes).map_err(|_| "community.searchResponseInvalid")?;
    Ok((value, has_next))
}

struct SearchTarget<'a> {
    url: String,
    token: Option<&'a str>,
    route: String,
}

fn search_targets<'a>(
    official: &str,
    token: Option<&'a str>,
    proxies: &[GitHubProxy],
) -> Vec<SearchTarget<'a>> {
    let mut targets = vec![SearchTarget {
        url: official.into(),
        token,
        route: "official".into(),
    }];
    let mut seen = HashSet::from([official.to_string()]);
    for proxy in proxies {
        for url in github_proxies::proxy_urls(std::slice::from_ref(proxy), official, false, None) {
            if seen.insert(url.clone()) {
                targets.push(SearchTarget {
                    url,
                    token: None,
                    route: format!("proxy-{}", proxy.id),
                });
            }
        }
    }
    targets
}

async fn fetch_page_from_targets(
    client: &Client,
    targets: Vec<SearchTarget<'_>>,
    page: usize,
    limits: &mut SearchRateLimits,
) -> Result<(Value, bool), String> {
    let mut last_error = "community.noApiTarget".to_string();
    for target in targets {
        log::info!(
            "正在请求 GitHub 社区仓库：页={}，渠道={}，地址={}",
            page,
            target.route,
            crate::repository_sources::safe_log_address(&target.url)
        );
        match fetch_page(client, &target.url, target.token, &target.route, limits).await {
            Ok((value, has_next))
                if value.get("total_count").and_then(Value::as_u64).is_some()
                    && value
                        .get("incomplete_results")
                        .and_then(Value::as_bool)
                        .is_some()
                    && value
                        .get("items")
                        .and_then(Value::as_array)
                        .is_some_and(|items| items.len() <= PAGE_SIZE) =>
            {
                return Ok((value, has_next));
            }
            Ok(_) => last_error = "community.searchResponseInvalid".into(),
            Err(error) => last_error = error,
        }
    }
    Err(last_error)
}

pub(super) async fn search_all(
    client: &Client,
    endpoint: &str,
    token: Option<&str>,
    proxies: &[GitHubProxy],
    limits: &mut SearchRateLimits,
) -> Result<SearchResults, String> {
    let mut results = SearchResults {
        map_repositories: Vec::new(),
        repositories: Vec::new(),
        discovery: CommunityDiscovery::default(),
    };
    let mut seen = HashSet::new();
    /*
     * 「发现是否完整」只能按**读过的条目**算，不能按收下的仓库算：
     * 搜索式放开了 `mod` / `map` 的分流，绝大多数命中都是既不是 MOD 也不是
     * 地图源的仓库（`SearchHit::Skip`），用 `repositories.len()` 去比总数
     * 会永远小于 `total_count`，这个标志就失去意义了。
     */
    let mut processed = 0usize;
    for page in 1..=RESULT_LIMIT / PAGE_SIZE {
        let official = page_url(endpoint, page)?;
        let (value, has_next) = fetch_page_from_targets(
            client,
            search_targets(&official, token, proxies),
            page,
            limits,
        )
        .await?;
        let total = value["total_count"].as_u64().unwrap();
        results.discovery.total_count = results.discovery.total_count.max(total);
        results.discovery.incomplete_results |= value["incomplete_results"].as_bool().unwrap();
        results.discovery.truncated |= total > RESULT_LIMIT as u64;
        let items = value["items"].as_array().unwrap();
        for item in items {
            processed += 1;
            match classify_search_item(item) {
                SearchHit::Mod(repository) => {
                    if seen.insert(repository.id.clone()) {
                        results.repositories.push(repository);
                    }
                }
                /*
                 * 地图源仓库不进 `repositories`，没有仓库 ID，去重键用坐标。
                 * 两者共用一个 `seen` 不会冲突：仓库 ID 是 `community-<hex>`。
                 */
                SearchHit::Map(repository) => {
                    let key = format!(
                        "{}/{}",
                        repository.username.to_ascii_lowercase(),
                        repository.repository.to_ascii_lowercase()
                    );
                    if seen.insert(key) {
                        results.map_repositories.push(repository);
                    }
                }
                SearchHit::Skip => {}
                SearchHit::Invalid => results.discovery.incomplete_results = true,
            }
        }
        log::info!(
            "GitHub 社区搜索分页完成：页={}，MOD={}，地图源={}，API总数={}",
            page,
            results.repositories.len(),
            results.map_repositories.len(),
            total
        );
        if items.is_empty()
            || (!has_next && page * PAGE_SIZE >= total.min(RESULT_LIMIT as u64) as usize)
        {
            break;
        }
    }
    results.discovery.repository_ids = results
        .repositories
        .iter()
        .map(|repo| repo.id.clone())
        .collect();
    if processed < results.discovery.total_count.min(RESULT_LIMIT as u64) as usize {
        results.discovery.incomplete_results = true;
    }
    Ok(results)
}

#[cfg(test)]
mod tests;
