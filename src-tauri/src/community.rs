use crate::archive::InstallManifest;
use crate::commands::AppState;
use crate::models::{
    AppConfig, CommunityDiscovery, GitHubProxy, Provider, PublicConfig, RepositoryConfig,
    RepositorySource,
};
use crate::{github_proxies, releases};
use futures_util::StreamExt;
use reqwest::{Client, StatusCode, header::HeaderMap};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, State};
use url::Url;

pub const SOURCE_ID: &str = "github-community";
pub const GITEE_SOURCE_ID: &str = "gitee-community";
pub const SEARCH_ENDPOINT: &str = "https://api.github.com/search/repositories";
pub const PAGE_SIZE: usize = 10;
const SEARCH_QUERY: &str = "topic:overcooked-2 topic:bepinex topic:mod";
const RESULT_LIMIT: usize = 1000;
const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const SEARCH_INTERVAL: Duration = Duration::from_millis(6100);

pub fn is_repository(repo: &RepositoryConfig) -> bool {
    repo.source_id.as_deref().is_some_and(is_source)
}

pub fn is_source(id: &str) -> bool {
    matches!(id, SOURCE_ID | GITEE_SOURCE_ID)
}

pub fn source_id(provider: &Provider) -> &'static str {
    match provider {
        Provider::Github => SOURCE_ID,
        Provider::Gitee => GITEE_SOURCE_ID,
    }
}

pub fn provider_matches_region(provider: &Provider, region: &str) -> bool {
    *provider != Provider::Gitee || region == crate::config::REGION_CHINESE_MAINLAND
}

pub(crate) fn ensure_repository_region(
    repo: &RepositoryConfig,
    region: &str,
) -> Result<(), String> {
    if is_repository(repo) && !provider_matches_region(&repo.provider, region) {
        return Err("Gitee 社区仓库仅在中国大陆地区可用".into());
    }
    Ok(())
}

pub fn is_source_archive(repo: &RepositoryConfig, tag: &str, asset_name: &str) -> bool {
    repo.provider == Provider::Gitee
        && repo.source_id.as_deref() == Some(GITEE_SOURCE_ID)
        && !tag.trim().is_empty()
        && asset_name
            .trim()
            .eq_ignore_ascii_case(&format!("{}.zip", tag.trim()))
}

pub fn source(provider: &Provider) -> RepositorySource {
    let name = match provider {
        Provider::Github => "GitHub",
        Provider::Gitee => "Gitee",
    };
    RepositorySource {
        id: source_id(provider).into(),
        nickname: format!("{name} 社区仓库"),
        nickname_i18n: [
            ("zh-CN".into(), format!("{name} 社区仓库")),
            ("zh-TW".into(), format!("{name} 社群倉庫")),
            ("en".into(), format!("{name} community repositories")),
            ("ko".into(), format!("{name} 커뮤니티 저장소")),
        ]
        .into(),
        url: match provider {
            Provider::Github => SEARCH_ENDPOINT,
            Provider::Gitee => crate::gitee_community::TOPIC_ENDPOINT,
        }
        .into(),
        region: (*provider == Provider::Gitee)
            .then(|| crate::config::REGION_CHINESE_MAINLAND.into()),
        builtin: false,
        parent_id: None,
    }
}

pub struct CommunitySearch {
    github: SearchState,
    gitee: SearchState,
}

impl Default for CommunitySearch {
    fn default() -> Self {
        Self {
            github: SearchState::new(Provider::Github),
            gitee: SearchState::new(Provider::Gitee),
        }
    }
}

impl CommunitySearch {
    fn for_provider(&self, provider: &Provider) -> &SearchState {
        match provider {
            Provider::Github => &self.github,
            Provider::Gitee => &self.gitee,
        }
    }
}

struct SearchState {
    limits: tokio::sync::Mutex<SearchRateLimits>,
    cancelled: AtomicBool,
}

impl SearchState {
    fn new(provider: Provider) -> Self {
        Self {
            limits: tokio::sync::Mutex::new(SearchRateLimits {
                interval: match provider {
                    Provider::Github => SEARCH_INTERVAL,
                    Provider::Gitee => Duration::from_secs(1),
                },
                provider,
                ..Default::default()
            }),
            cancelled: AtomicBool::new(false),
        }
    }
}

pub(crate) struct SearchRateLimits {
    provider: Provider,
    interval: Duration,
    last_request: HashMap<String, Instant>,
    cooldown: HashMap<String, Instant>,
}

impl Default for SearchRateLimits {
    fn default() -> Self {
        Self {
            provider: Provider::Github,
            interval: SEARCH_INTERVAL,
            last_request: HashMap::new(),
            cooldown: HashMap::new(),
        }
    }
}

impl SearchRateLimits {
    pub(crate) async fn before_request(&mut self, route: &str) -> Result<(), String> {
        let now = Instant::now();
        if self
            .cooldown
            .get(route)
            .is_some_and(|deadline| *deadline > now)
        {
            return Err(self.rate_limit_message().into());
        }
        let deadline = self
            .last_request
            .get(route)
            .map(|last| *last + self.interval)
            .unwrap_or(now);
        if deadline > now {
            log::info!(
                "社区搜索等待配额：平台={:?}，渠道={}，等待秒数={}",
                self.provider,
                route,
                deadline.duration_since(now).as_secs() + 1
            );
            tokio::time::sleep(deadline.duration_since(now)).await;
        }
        self.last_request.insert(route.into(), Instant::now());
        Ok(())
    }

    pub(crate) fn observe(&mut self, route: &str, status: StatusCode, headers: &HeaderMap) {
        if let Some(delay) = rate_limit_delay(status, headers) {
            let deadline = Instant::now() + delay;
            self.cooldown
                .entry(route.into())
                .and_modify(|current| *current = (*current).max(deadline))
                .or_insert(deadline);
        }
    }

    pub(crate) fn rate_limit_message(&self) -> &'static str {
        match self.provider {
            Provider::Github => "GitHub 搜索触发速率限制，请稍后刷新",
            Provider::Gitee => "Gitee 网页请求触发速率限制，请稍后刷新",
        }
    }

    #[cfg(test)]
    pub(crate) fn for_gitee_test() -> Self {
        Self {
            provider: Provider::Gitee,
            interval: Duration::ZERO,
            ..Default::default()
        }
    }
}

fn rate_limit_delay(status: StatusCode, headers: &HeaderMap) -> Option<Duration> {
    let retry_after = headers
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value.parse::<u64>().ok().or_else(|| {
                chrono::DateTime::parse_from_rfc2822(value)
                    .ok()
                    .map(|date| (date.timestamp() - chrono::Utc::now().timestamp()).max(0) as u64)
            })
        });
    let reset = (headers
        .get("x-ratelimit-remaining")
        .and_then(|value| value.to_str().ok())
        == Some("0"))
    .then(|| {
        headers
            .get("x-ratelimit-reset")
            .and_then(|value| value.to_str().ok())?
            .parse::<i64>()
            .ok()
    })
    .flatten()
    .map(|reset| (reset - chrono::Utc::now().timestamp()).max(0) as u64 + 1);
    retry_after
        .into_iter()
        .chain(reset)
        .max()
        .map(|seconds| Duration::from_secs(seconds.min(24 * 60 * 60)))
        .or_else(|| {
            matches!(
                status,
                StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS
            )
            .then_some(Duration::from_secs(60))
        })
}

fn page_url(endpoint: &str, page: usize) -> Result<String, String> {
    let mut url = Url::parse(endpoint).map_err(|_| "GitHub 社区搜索地址无效")?;
    url.query_pairs_mut()
        .clear()
        .append_pair("q", SEARCH_QUERY)
        .append_pair("per_page", &PAGE_SIZE.to_string())
        .append_pair("page", &page.to_string());
    Ok(url.into())
}

fn repository_from_search(item: &Value) -> Option<RepositoryConfig> {
    if item.get("private")?.as_bool()?
        || !["overcooked-2", "bepinex", "mod"].iter().all(|topic| {
            item.get("topics")
                .and_then(Value::as_array)
                .is_some_and(|topics| topics.iter().any(|value| value.as_str() == Some(topic)))
        })
    {
        return None;
    }
    let owner = item.get("owner")?.get("login")?.as_str()?;
    let name = item.get("name")?.as_str()?;
    repository_from_coordinates(
        &Provider::Github,
        owner,
        name,
        item.get("default_branch")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        item.get("description")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    )
}

pub(crate) fn repository_from_coordinates(
    provider: &Provider,
    owner: &str,
    name: &str,
    branch: &str,
    note: &str,
) -> Option<RepositoryConfig> {
    let (owner, name) = releases::repository_coordinates(owner, name).ok()?;
    let identity = format!(
        "{}/{}",
        owner.to_ascii_lowercase(),
        name.to_ascii_lowercase()
    );
    let prefix = match provider {
        Provider::Github => "community",
        Provider::Gitee => "gitee-community",
    };
    let id = format!("{prefix}-{:x}", Sha256::digest(identity.as_bytes()));
    serde_json::from_value(serde_json::json!({
        "id": id, "name": format!("{owner}/{name}"), "provider": provider, "username": owner,
        "repository": name, "branch": branch, "note": note,
        "region": (*provider == Provider::Gitee).then_some(crate::config::REGION_CHINESE_MAINLAND),
        "source_id": source_id(provider), "install_target": "BepInEx/plugins", "channel": "all",
        "asset_rules": [{"type": "regex", "value": "^.+\\.(?:[dD][lL][lL]|[zZ][iI][pP])$"}],
        "download_threads": 16, "enabled": true, "builtin": false
    }))
    .ok()
}

async fn fetch_page(
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
        .send()
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
                "GitHub 搜索触发速率限制，请稍后刷新".into()
            } else {
                format!("GitHub 社区搜索 HTTP {status}")
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
        return Err("GitHub 社区搜索响应过大".into());
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| error.without_url().to_string())?;
        if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err("GitHub 社区搜索响应过大".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let value = serde_json::from_slice(&bytes).map_err(|_| "GitHub 社区搜索响应格式无效")?;
    Ok((value, has_next))
}

pub(crate) struct SearchResults {
    pub(crate) repositories: Vec<RepositoryConfig>,
    pub(crate) discovery: CommunityDiscovery,
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
    let mut last_error = "没有可用的 GitHub API 地址".to_string();
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
            Ok(_) => last_error = "GitHub 社区搜索响应格式无效".into(),
            Err(error) => last_error = error,
        }
    }
    Err(last_error)
}

async fn search_all(
    client: &Client,
    endpoint: &str,
    token: Option<&str>,
    proxies: &[GitHubProxy],
    limits: &mut SearchRateLimits,
) -> Result<SearchResults, String> {
    let mut results = SearchResults {
        repositories: Vec::new(),
        discovery: CommunityDiscovery::default(),
    };
    let mut seen = HashSet::new();
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
            if let Some(repo) = repository_from_search(item) {
                if seen.insert(repo.id.clone()) {
                    results.repositories.push(repo);
                }
            } else {
                results.discovery.incomplete_results = true;
            }
        }
        log::info!(
            "GitHub 社区搜索分页完成：页={}，已发现={}，API总数={}",
            page,
            results.repositories.len(),
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
    if results.repositories.len() < results.discovery.total_count.min(RESULT_LIMIT as u64) as usize
    {
        results.discovery.incomplete_results = true;
    }
    Ok(results)
}

fn merge_results(
    config: &mut AppConfig,
    provider: &Provider,
    results: SearchResults,
    manifests: &[InstallManifest],
) {
    let source_id = source_id(provider);
    let belongs_to_search = |repo: &RepositoryConfig| repo.source_id.as_deref() == Some(source_id);
    let official = config
        .repositories
        .iter()
        .filter(|repo| repo.builtin && repo.provider == *provider && !is_repository(repo))
        .map(|repo| {
            (
                repo.username.to_ascii_lowercase(),
                repo.repository.to_ascii_lowercase(),
            )
        })
        .collect::<HashSet<_>>();
    let is_installed = |repo: &RepositoryConfig| {
        manifests.iter().any(|manifest| {
            manifest.repository_id == repo.id
                || manifest
                    .repository_binding
                    .as_ref()
                    .is_some_and(|binding| binding.matches_repository(repo))
        })
    };
    let mut discovery = results.discovery;
    let mut visible_ids = HashSet::new();
    let mut previous = config
        .repositories
        .iter()
        .filter(|repo| belongs_to_search(repo))
        .map(|repo| (repo.id.clone(), repo.clone()))
        .collect::<HashMap<_, _>>();
    config.repositories.retain(|repo| !belongs_to_search(repo));
    for mut repo in results.repositories {
        if let Some(old) = previous.remove(&repo.id) {
            repo.enabled = old.enabled;
        }
        let already_official = official.contains(&(
            repo.username.to_ascii_lowercase(),
            repo.repository.to_ascii_lowercase(),
        ));
        if !already_official {
            visible_ids.insert(repo.id.clone());
        }
        // Installed duplicates retain their original channel and update binding, but stay hidden.
        if !already_official || is_installed(&repo) {
            config.repositories.push(repo);
        }
    }
    let mut retained = previous
        .into_values()
        .filter(is_installed)
        .collect::<Vec<_>>();
    retained.sort_by(|left, right| left.id.cmp(&right.id));
    for repo in &mut retained {
        repo.cached_releases.clear();
        repo.last_error = None;
        repo.etag = None;
    }
    config.repositories.extend(retained);
    config
        .repository_sources
        .retain(|source| source.id != source_id);
    config.repository_sources.push(source(provider));
    discovery
        .repository_ids
        .retain(|id| visible_ids.contains(id));
    match provider {
        Provider::Github => config.community_discovery = Some(discovery),
        Provider::Gitee => config.gitee_community_discovery = Some(discovery),
    }
}

#[tauri::command]
pub async fn refresh_community_repositories(
    app: AppHandle,
    state: State<'_, AppState>,
    provider: Option<Provider>,
) -> Result<PublicConfig, String> {
    let provider = provider.unwrap_or(Provider::Github);
    if !provider_matches_region(&provider, crate::config::current_user_region()) {
        return Err("Gitee 社区仓库仅在中国大陆地区可用".into());
    }
    let search = state.community.for_provider(&provider);
    let mut limits = search.limits.try_lock().map_err(|_| "社区仓库正在搜索")?;
    search.cancelled.store(false, Ordering::Release);
    let config = {
        let _guard = state.operation.lock().map_err(|_| "操作锁异常")?;
        state.config.load_or_default()
    };
    let token = (provider == Provider::Github)
        .then(|| state.config.api_token(&config, &Provider::Github))
        .flatten();
    let results = tokio::select! {
        results = async {
            match provider {
                Provider::Github => {
                    let client = releases::build_client(config.network.timeout_seconds).map_err(|error| error.to_string())?;
                    search_all(&client, SEARCH_ENDPOINT, token.as_deref(), &config.github_proxies.github_api_proxies, &mut limits).await
                },
                Provider::Gitee => crate::gitee_community::search_all(&app, state.config.cache_dir().join("gitee-webview"), &mut limits).await,
            }
        } => results?,
        _ = async { while !search.cancelled.load(Ordering::Acquire) { tokio::time::sleep(Duration::from_millis(100)).await; } } => return Err("社区仓库搜索已取消".into()),
    };
    let _cache_guard = state.repository_cache.lock().await;
    let _guard = state.operation.lock().map_err(|_| "操作锁异常")?;
    if search.cancelled.load(Ordering::Acquire) {
        return Err("社区仓库搜索已取消".into());
    }
    let mut config = state.config.load_or_default();
    let manifests = crate::archive::read_manifests(&state.config.manifests_dir());
    merge_results(&mut config, &provider, results, &manifests);
    state
        .config
        .save(&config)
        .map_err(|error| error.to_string())?;
    Ok(state.config.public_config(&config))
}

#[tauri::command]
pub fn cancel_community_search(state: State<'_, AppState>, provider: Option<Provider>) {
    state
        .community
        .for_provider(&provider.unwrap_or(Provider::Github))
        .cancelled
        .store(true, Ordering::Release);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Channel, RepositoryBinding};
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn item(owner: &str, name: &str) -> Value {
        serde_json::json!({"owner": {"login": owner}, "name": name, "private": false,
            "topics": ["overcooked-2", "bepinex", "mod"], "default_branch": "develop"})
    }

    fn page(items: Vec<Value>, total: u64) -> String {
        serde_json::json!({"items": items, "total_count": total, "incomplete_results": false})
            .to_string()
    }

    fn server(
        responses: Vec<(u16, String, String)>,
    ) -> (String, std::thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for (status, headers, body) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") {
                    let mut chunk = [0; 2048];
                    let count = stream.read(&mut chunk).unwrap();
                    assert!(count > 0);
                    request.extend_from_slice(&chunk[..count]);
                }
                requests.push(String::from_utf8(request).unwrap());
                write!(stream, "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}", body.len()).unwrap();
            }
            requests
        });
        (url, task)
    }

    fn limits() -> SearchRateLimits {
        SearchRateLimits {
            interval: Duration::ZERO,
            ..Default::default()
        }
    }

    #[test]
    fn gitee_discovery_is_mainland_only_while_github_is_global() {
        for region in [
            crate::config::REGION_CHINESE_MAINLAND,
            crate::config::REGION_INTERNATIONAL,
            "",
            "unknown",
        ] {
            assert!(provider_matches_region(&Provider::Github, region));
            assert_eq!(
                provider_matches_region(&Provider::Gitee, region),
                region == crate::config::REGION_CHINESE_MAINLAND
            );
        }
        assert_eq!(
            source(&Provider::Gitee).region.as_deref(),
            Some(crate::config::REGION_CHINESE_MAINLAND)
        );
        assert_eq!(source(&Provider::Github).region, None);
    }

    #[test]
    fn search_description_is_preserved_when_present_and_optional_when_missing() {
        let mut response = item("Owner", "Mod");
        response["description"] = "Community repository description".into();
        let repo = repository_from_search(&response).unwrap();
        assert_eq!(repo.note, "Community repository description");
        response["description"] = Value::Null;
        assert!(repository_from_search(&response).unwrap().note.is_empty());
        response.as_object_mut().unwrap().remove("description");
        assert!(repository_from_search(&response).unwrap().note.is_empty());
    }

    #[test]
    fn search_entries_are_public_three_topic_dll_zip_repositories_with_stable_ids() {
        let repo = repository_from_search(&item("Owner", "Mod")).unwrap();
        assert_eq!(
            repo.id,
            repository_from_search(&item("owner", "MOD")).unwrap().id
        );
        assert_eq!(repo.source_id.as_deref(), Some(SOURCE_ID));
        assert_eq!(repo.branch, "develop");
        assert!(repo.group.is_empty() && repo.note.is_empty());
        assert!(repo.plugin_guid.is_empty() && repo.local_product_name.is_empty());
        for (name, expected) in [
            ("Mod.dll", true),
            ("Mod.ZIP", true),
            ("notes.md", false),
            ("Mod.zip.sig", false),
        ] {
            let asset = serde_json::from_value(
                serde_json::json!({"name":name,"size":1,"download_url":"https://example.com/file"}),
            )
            .unwrap();
            assert_eq!(releases::asset_matches(&asset, &repo.asset_rules), expected);
        }
        let mut invalid = item("Owner", "Mod");
        invalid["private"] = true.into();
        assert!(repository_from_search(&invalid).is_none());
        invalid["private"] = false.into();
        invalid["topics"] = serde_json::json!(["overcooked-2", "bepinex"]);
        assert!(repository_from_search(&invalid).is_none());
        assert!(repository_from_search(&item("../owner", "Mod")).is_none());
    }

    #[test]
    fn community_assets_exclude_github_generated_source_archives() {
        let repo = repository_from_search(&item("Owner", "Mod")).unwrap();
        let releases = releases::parse_release_json(
            &serde_json::json!([{
                "tag_name": "v1.0.0", "draft": false, "prerelease": false,
                "zipball_url": "https://api.github.com/repos/Owner/Mod/zipball/v1.0.0",
                "tarball_url": "https://api.github.com/repos/Owner/Mod/tarball/v1.0.0",
                "assets": [
                    {"name": "Mod.ZiP", "browser_download_url": "https://example.com/Mod.ZiP"},
                    {"name": "Mod.DLL", "browser_download_url": "https://example.com/Mod.DLL"},
                    {"name": "README.md", "browser_download_url": "https://example.com/README.md"}
                ]
            }]),
            &Provider::Github,
        )
        .unwrap();
        assert_eq!(releases.len(), 1);
        let matched = releases[0]
            .assets
            .iter()
            .filter(|asset| releases::asset_matches(asset, &repo.asset_rules))
            .map(|asset| asset.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(matched, ["Mod.ZiP", "Mod.DLL"]);
        assert!(releases[0].assets.iter().all(|asset| {
            !asset.download_url.contains("zipball") && !asset.download_url.contains("tarball")
        }));
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
    async fn rate_limited_routes_are_not_retried_before_their_cooldown() {
        let (url, task) = server(vec![(429, "Retry-After: 120\r\n".into(), "{}".into())]);
        let client = Client::new();
        let mut limits = limits();
        assert!(
            fetch_page(&client, &url, None, "official", &mut limits)
                .await
                .is_err()
        );
        assert!(
            fetch_page(&client, &url, None, "official", &mut limits)
                .await
                .is_err()
        );
        assert_eq!(task.join().unwrap().len(), 1);
        let headers = HeaderMap::from_iter([
            (
                reqwest::header::HeaderName::from_static("x-ratelimit-remaining"),
                "0".parse().unwrap(),
            ),
            (
                reqwest::header::HeaderName::from_static("x-ratelimit-reset"),
                (chrono::Utc::now().timestamp() + 180)
                    .to_string()
                    .parse()
                    .unwrap(),
            ),
            (reqwest::header::RETRY_AFTER, "120".parse().unwrap()),
        ]);
        assert!(rate_limit_delay(StatusCode::OK, &headers).unwrap() >= Duration::from_secs(180));
        assert_eq!(
            rate_limit_delay(StatusCode::FORBIDDEN, &HeaderMap::new()),
            Some(Duration::from_secs(60))
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

    #[test]
    fn community_merge_preserves_official_entries_and_installed_repositories_missing_from_search() {
        let mut official = repository_from_search(&item("Owner", "Official")).unwrap();
        official.id = "official-mod".into();
        official.source_id = None;
        official.builtin = true;
        official.note = "official description".into();
        let mut retained = repository_from_search(&item("Owner", "Installed")).unwrap();
        retained.enabled = false;
        let removed = repository_from_search(&item("Owner", "Removed")).unwrap();
        let mut config: AppConfig = serde_json::from_value(serde_json::json!({
            "schema_version":1,"ui_zoom":1.5,"onboarding_seen_steps":[],
            "game":{"path":"","auto_detect":false},
            "network":{"timeout_seconds":5,"download_threads":1,"max_retry_count":3,"check_updates_on_start":false},
            "launch":{"ask_user_to_start_from_steam":false,"auto_restore_winhttp":false},
            "auth":{"github_token":"","gitee_token":"","official_api_only":true},
            "repositories":[official, retained, removed]
        })).unwrap();
        let mut manifest = crate::archive::new_manifest("old-id", "nightly", None, None, vec![]);
        manifest.repository_binding = RepositoryBinding::from_release(&retained, "nightly");
        let found = repository_from_search(&item("Owner", "Official")).unwrap();
        merge_results(
            &mut config,
            &Provider::Github,
            SearchResults {
                discovery: CommunityDiscovery {
                    repository_ids: vec![found.id.clone()],
                    total_count: 1,
                    ..Default::default()
                },
                repositories: vec![found],
            },
            &[manifest],
        );
        assert_eq!(config.repositories.len(), 2);
        assert_eq!(config.repositories[0].id, "official-mod");
        assert_eq!(config.repositories[0].note, "official description");
        assert!(
            config
                .repositories
                .iter()
                .any(|repo| repo.id == retained.id && !repo.enabled)
        );
        assert!(!config.repositories.iter().any(|repo| repo.id == removed.id));
        assert!(
            config
                .community_discovery
                .as_ref()
                .unwrap()
                .repository_ids
                .is_empty()
        );
        assert_eq!(config.repository_sources.last().unwrap().id, SOURCE_ID);
        assert_eq!(manifest_binding_channel(&retained), Channel::All);
    }

    #[test]
    fn official_dedup_preserves_installed_channels_and_does_not_hide_custom_or_gitee_coordinates() {
        let mut official = repository_from_search(&item("Owner", "Listed")).unwrap();
        official.id = "official-stable".into();
        official.source_id = Some("official-child".into());
        official.builtin = true;
        official.enabled = false;
        official.channel = Channel::Stable;
        let installed = repository_from_search(&item("owner", "listed")).unwrap();
        let mut manifest =
            crate::archive::new_manifest(&installed.id, "v1.0.0", None, None, vec![]);
        manifest.repository_binding = RepositoryBinding::from_release(&installed, "v1.0.0");
        let binding = manifest.repository_binding.clone();

        let mut custom = repository_from_search(&item("Owner", "Custom")).unwrap();
        custom.id = "custom".into();
        custom.source_id = None;
        let mut gitee = repository_from_search(&item("Owner", "GiteeOnly")).unwrap();
        gitee.id = "gitee".into();
        gitee.source_id = Some("official-child".into());
        gitee.provider = Provider::Gitee;
        gitee.builtin = true;
        let found = vec![
            repository_from_search(&item("OWNER", "LISTED")).unwrap(),
            repository_from_search(&item("Owner", "Custom")).unwrap(),
            repository_from_search(&item("Owner", "GiteeOnly")).unwrap(),
        ];
        let visible_ids = found[1..]
            .iter()
            .map(|repo| repo.id.clone())
            .collect::<Vec<_>>();
        let mut config: AppConfig = serde_json::from_value(serde_json::json!({
            "schema_version":1,"ui_zoom":1.5,"onboarding_seen_steps":[],
            "game":{"path":"","auto_detect":false},
            "network":{"timeout_seconds":5,"download_threads":1,"max_retry_count":3,"check_updates_on_start":false},
            "launch":{"ask_user_to_start_from_steam":false,"auto_restore_winhttp":false},
            "auth":{"github_token":"","gitee_token":"","official_api_only":true},
            "repositories":[official, custom, gitee, installed.clone()]
        })).unwrap();
        merge_results(
            &mut config,
            &Provider::Github,
            SearchResults {
                discovery: CommunityDiscovery {
                    repository_ids: found.iter().map(|repo| repo.id.clone()).collect(),
                    total_count: 3,
                    ..Default::default()
                },
                repositories: found,
            },
            std::slice::from_ref(&manifest),
        );
        assert_eq!(
            config.community_discovery.as_ref().unwrap().repository_ids,
            visible_ids
        );
        let hidden = config
            .repositories
            .iter()
            .find(|repo| repo.id == installed.id)
            .unwrap();
        assert_eq!(hidden.channel, Channel::All);
        assert!(hidden.enabled);
        assert_eq!(manifest.repository_binding, binding);
        assert!(binding.as_ref().unwrap().matches_repository(hidden));
        assert!(
            !binding
                .as_ref()
                .unwrap()
                .matches_repository(&config.repositories[0])
        );

        // A later empty search must not erase an installed duplicate's update entry.
        merge_results(
            &mut config,
            &Provider::Github,
            SearchResults {
                repositories: vec![],
                discovery: CommunityDiscovery::default(),
            },
            &[manifest],
        );
        assert!(
            config
                .repositories
                .iter()
                .any(|repo| repo.id == installed.id)
        );
    }

    fn manifest_binding_channel(repo: &RepositoryConfig) -> Channel {
        RepositoryBinding::from_release(repo, "tag")
            .unwrap()
            .channel
    }

    #[tokio::test]
    async fn provider_search_locks_cancellation_and_rate_budgets_are_independent() {
        let searches = CommunitySearch::default();
        let github = searches.for_provider(&Provider::Github);
        let gitee = searches.for_provider(&Provider::Gitee);
        let github_limits = github.limits.try_lock().unwrap();
        let gitee_limits = gitee.limits.try_lock().unwrap();
        assert_eq!(github_limits.interval, SEARCH_INTERVAL);
        assert_eq!(gitee_limits.interval, Duration::from_secs(1));
        github.cancelled.store(true, Ordering::Release);
        assert!(!gitee.cancelled.load(Ordering::Acquire));
        github.cancelled.store(false, Ordering::Release);
        gitee.cancelled.store(true, Ordering::Release);
        assert!(!github.cancelled.load(Ordering::Acquire));
    }

    #[test]
    fn gitee_merge_preserves_github_catalogue_and_uses_provider_scoped_official_dedup() {
        let github = repository_from_search(&item("Owner", "Listed")).unwrap();
        let found = repository_from_coordinates(
            &Provider::Gitee,
            "owner",
            "listed",
            "",
            "Gitee description",
        )
        .unwrap();
        assert_ne!(github.id, found.id);
        let mut official = found.clone();
        official.id = "official-gitee".into();
        official.builtin = true;
        official.source_id = Some("official-child".into());
        official.channel = Channel::Stable;
        let unique =
            repository_from_coordinates(&Provider::Gitee, "Owner", "New", "", "New description")
                .unwrap();
        let mut manifest = crate::archive::new_manifest(&found.id, "v1.0.0", None, None, vec![]);
        manifest.repository_binding = RepositoryBinding::from_release(&found, "v1.0.0");
        let mut config: AppConfig = serde_json::from_value(serde_json::json!({
            "schema_version":1,"ui_zoom":1.5,"onboarding_seen_steps":[],
            "game":{"path":"","auto_detect":false},
            "network":{"timeout_seconds":5,"download_threads":1,"max_retry_count":3,"check_updates_on_start":false},
            "launch":{"ask_user_to_start_from_steam":false,"auto_restore_winhttp":false},
            "auth":{"github_token":"","gitee_token":"","official_api_only":true},
            "repositories":[github, official, found],
            "community_discovery":{"repository_ids":[github.id],"total_count":1,"incomplete_results":false,"truncated":false}
        })).unwrap();
        let github_before = serde_json::to_value(&config.repositories[0]).unwrap();
        let discovery_before = serde_json::to_value(&config.community_discovery).unwrap();
        merge_results(
            &mut config,
            &Provider::Gitee,
            SearchResults {
                discovery: CommunityDiscovery {
                    repository_ids: vec![found.id.clone(), unique.id.clone()],
                    total_count: 2,
                    ..Default::default()
                },
                repositories: vec![found.clone(), unique.clone()],
            },
            std::slice::from_ref(&manifest),
        );
        assert_eq!(
            serde_json::to_value(&config.repositories[0]).unwrap(),
            github_before
        );
        assert_eq!(
            serde_json::to_value(&config.community_discovery).unwrap(),
            discovery_before
        );
        assert_eq!(
            config
                .gitee_community_discovery
                .as_ref()
                .unwrap()
                .repository_ids,
            vec![unique.id]
        );
        let hidden = config
            .repositories
            .iter()
            .find(|repo| repo.id == found.id)
            .unwrap();
        assert_eq!(hidden.channel, Channel::All);
        assert!(
            manifest
                .repository_binding
                .as_ref()
                .unwrap()
                .matches_repository(hidden)
        );
        assert!(is_source(
            config.repository_sources.last().unwrap().id.as_str()
        ));
        let round_trip: AppConfig = toml::from_str(&toml::to_string(&config).unwrap()).unwrap();
        assert_eq!(
            round_trip
                .gitee_community_discovery
                .unwrap()
                .repository_ids
                .len(),
            1
        );
        assert_eq!(
            serde_json::to_value(&round_trip.community_discovery).unwrap(),
            discovery_before
        );

        merge_results(
            &mut config,
            &Provider::Gitee,
            SearchResults {
                repositories: vec![],
                discovery: CommunityDiscovery::default(),
            },
            &[manifest],
        );
        assert!(config.repositories.iter().any(|repo| repo.id == found.id));
        assert_eq!(
            serde_json::to_value(&config.repositories[0]).unwrap(),
            github_before
        );
    }
}
