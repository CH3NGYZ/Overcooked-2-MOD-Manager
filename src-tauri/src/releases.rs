use crate::models::{
    AssetRule, AssetRuleType, Channel, GitHubProxy, Provider, ReleaseAsset, ReleaseInfo,
    RepositoryConfig,
};
use globset::GlobBuilder;
use regex::Regex;
use reqwest::{Client, StatusCode};
use serde_json::Value;
use url::Url;

/// Stable identifier for Cloudflare rules and access logs. The package
/// version makes every request identify the exact desktop client release.
pub const APP_USER_AGENT: &str = concat!(
    "Overcooked 2 MOD Manager/",
    env!("CARGO_PKG_VERSION"),
    " (client=desktop; os=Windows; arch=x64; tauri=2)"
);

#[derive(Debug, thiserror::Error)]
pub enum ReleaseError {
    #[error("Release operation failed: {0}")]
    InvalidUrl(String),
    #[error("Release operation failed: {0}")]
    Http(String),
    #[error("Release operation failed: {0}")]
    Status(StatusCode),
    #[error("Release operation failed: {0}")]
    Json(String),
}

const API_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
const REPOSITORY_API_ATTEMPTS: usize = 3;

impl ReleaseError {
    pub fn is_forbidden(&self) -> bool {
        matches!(self, Self::Status(status) if *status == StatusCode::FORBIDDEN)
    }
}

fn release_error_kind(error: &ReleaseError) -> &'static str {
    match error {
        ReleaseError::InvalidUrl(_) => "invalid_url",
        ReleaseError::Http(_) => "http",
        ReleaseError::Status(status) if *status == StatusCode::FORBIDDEN => "http_403",
        ReleaseError::Status(_) => "http_status",
        ReleaseError::Json(_) => "json",
    }
}

fn api_target_kind(
    official_url: &str,
    target_url: &str,
    request_token: Option<&str>,
) -> &'static str {
    if target_url == official_url {
        if request_token.is_some() {
            "official_token"
        } else {
            "official_direct"
        }
    } else {
        "proxy"
    }
}

fn is_official_gitee_api_url(value: &str) -> bool {
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    matches!(url.host_str(), Some("gitee.com" | "www.gitee.com"))
        && url.path().starts_with("/api/v5/")
}

fn gitee_api_url_with_token(url: &str, token: &str) -> Result<String, ReleaseError> {
    let mut parsed =
        Url::parse(url).map_err(|error| ReleaseError::InvalidUrl(error.to_string()))?;
    parsed.query_pairs_mut().append_pair("access_token", token);
    Ok(parsed.into())
}

pub fn repository_coordinates(
    username: &str,
    repository: &str,
) -> Result<(String, String), ReleaseError> {
    let username = username.trim();
    let repository = repository.trim();
    if username.is_empty()
        || repository.is_empty()
        || username
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
        || repository
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
        || username.contains(['/', '\\', ':', '?', '#'])
        || repository.contains(['/', '\\', ':', '?', '#'])
        || username == "."
        || username == ".."
        || repository == "."
        || repository == ".."
    {
        return Err(ReleaseError::InvalidUrl(
            "项目用户和仓库必须是安全的单一路径片段".into(),
        ));
    }
    Ok((username.to_owned(), repository.to_owned()))
}

pub fn repository_homepage(
    provider: &Provider,
    username: &str,
    repository: &str,
) -> Result<String, ReleaseError> {
    let (username, repository) = repository_coordinates(username, repository)?;
    let base = match provider {
        Provider::Github => "https://github.com/",
        Provider::Gitee => "https://gitee.com/",
    };
    let mut url = Url::parse(base).expect("fixed repository host is a valid URL");
    url.path_segments_mut()
        .expect("fixed repository host accepts paths")
        .pop_if_empty()
        .push(&username)
        .push(&repository);
    Ok(url.into())
}

pub fn repository_release_api_url(
    provider: &Provider,
    username: &str,
    repository: &str,
    max_release_count: u32,
) -> Result<String, ReleaseError> {
    let (username, repository) = repository_coordinates(username, repository)?;
    let base = match provider {
        Provider::Github => "https://api.github.com/repos/",
        Provider::Gitee => "https://gitee.com/api/v5/repos/",
    };
    let mut url = Url::parse(base).expect("fixed Release API host is a valid URL");
    url.path_segments_mut()
        .expect("fixed Release API host accepts paths")
        .pop_if_empty()
        .push(&username)
        .push(&repository)
        .push("releases");
    url.query_pairs_mut()
        .append_pair("per_page", &max_release_count.clamp(1, 100).to_string())
        .append_pair("direction", "desc")
        .append_pair("sort", "created");
    Ok(url.into())
}

pub fn repository_file_url(
    provider: &Provider,
    username: &str,
    repository: &str,
    branch: &str,
    file_path: &str,
) -> Result<String, ReleaseError> {
    let homepage = repository_homepage(provider, username, repository)?;
    let branch = branch.trim();
    let file_path = file_path.trim();
    if file_path.contains(['*', '?']) {
        return Err(ReleaseError::InvalidUrl(
            "仓库文件模式只支持具体文件路径，不支持通配和正则".into(),
        ));
    }
    if branch.is_empty()
        || file_path.is_empty()
        || branch.starts_with(['/', '\\'])
        || file_path.starts_with(['/', '\\'])
        || branch
            .split(['/', '\\'])
            .any(|part| part.is_empty() || part == "." || part == "..")
        || file_path
            .split(['/', '\\'])
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(ReleaseError::InvalidUrl(
            "仓库文件分支或相对路径不安全".into(),
        ));
    }
    let mut url =
        Url::parse(&homepage).map_err(|error| ReleaseError::InvalidUrl(error.to_string()))?;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| ReleaseError::InvalidUrl("无法创建仓库文件地址".into()))?;
        segments.pop_if_empty();
        segments.push("raw");
        if *provider == Provider::Github {
            segments.push("refs");
            segments.push("heads");
        }
        for part in branch.split('/') {
            segments.push(part);
        }
        for part in file_path.split(['/', '\\']) {
            segments.push(part);
        }
    }
    Ok(url.into())
}

pub fn repository_file_name(file_path: &str) -> Result<String, ReleaseError> {
    let encoded = file_path
        .rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| ReleaseError::InvalidUrl("仓库文件路径缺少文件名".into()))?;
    let bytes = encoded.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = bytes
                .get(index + 1..index + 3)
                .ok_or_else(|| ReleaseError::InvalidUrl("仓库文件名编码无效".into()))?;
            let value = u8::from_str_radix(std::str::from_utf8(hex).unwrap_or(""), 16)
                .map_err(|_| ReleaseError::InvalidUrl("仓库文件名编码无效".into()))?;
            decoded.push(value);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    let name = String::from_utf8(decoded)
        .map_err(|_| ReleaseError::InvalidUrl("仓库文件名不是有效 UTF-8".into()))?;
    if name.is_empty() || name.contains(['/', '\\', ':']) || name == "." || name == ".." {
        return Err(ReleaseError::InvalidUrl("仓库文件名不安全".into()));
    }
    Ok(name)
}

pub fn parse_version(tag: &str) -> Option<semver::Version> {
    let cleaned = tag.trim().trim_start_matches(['v', 'V']);
    semver::Version::parse(cleaned).ok().or_else(|| {
        let re = Regex::new(r"(\d+(?:\.\d+){1,3}(?:[-+][0-9A-Za-z.-]+)?)").ok()?;
        let value = re.captures(cleaned)?.get(1)?.as_str();
        let numeric = value.split(['-', '+']).next()?;
        let parts = numeric
            .split('.')
            .map(str::parse::<u64>)
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        let suffix = value.get(numeric.len()..).unwrap_or_default();
        let normalized = match parts.as_slice() {
            [major, minor] => format!("{major}.{minor}.0{suffix}"),
            [major, minor, patch] => format!("{major}.{minor}.{patch}{suffix}"),
            [major, minor, patch, build] => format!("{major}.{minor}.{patch}-{build}{suffix}"),
            _ => return None,
        };
        semver::Version::parse(&normalized).ok()
    })
}

pub fn filter_channel(mut releases: Vec<ReleaseInfo>, channel: &Channel) -> Vec<ReleaseInfo> {
    let before = releases.len();
    releases.retain(|release| {
        !release.draft
            && match channel {
                Channel::Stable => !release.prerelease,
                Channel::Beta => release.prerelease,
                Channel::All => true,
            }
    });
    releases.sort_by(|a, b| {
        match (
            a.version.as_deref().and_then(parse_version),
            b.version.as_deref().and_then(parse_version),
        ) {
            (Some(left), Some(right)) => right.cmp(&left),
            _ => b.published_at.cmp(&a.published_at),
        }
    });
    log::debug!(
        "Release 渠道筛选完成: channel={:?} before={} after={}",
        channel,
        before,
        releases.len()
    );
    releases
}

pub fn asset_matches(asset: &ReleaseAsset, rules: &[AssetRule]) -> bool {
    rules.iter().any(|rule| match rule.rule_type {
        AssetRuleType::Exact => asset.name.eq_ignore_ascii_case(&rule.value),
        AssetRuleType::Glob => GlobBuilder::new(&rule.value)
            .case_insensitive(true)
            .build()
            .ok()
            .map(|glob| glob.compile_matcher().is_match(&asset.name))
            .unwrap_or(false),
        AssetRuleType::Regex => Regex::new(&rule.value)
            .ok()
            .map(|regex| regex.is_match(&asset.name))
            .unwrap_or(false),
    })
}

pub fn select_install_asset<'a>(
    repo: &RepositoryConfig,
    release: &'a ReleaseInfo,
    asset_name: Option<&str>,
) -> Option<&'a ReleaseAsset> {
    release.assets.iter().find(|asset| {
        !crate::community::is_source_archive(repo, &release.tag_name, &asset.name)
            && asset_name
                .map(|name| asset.name == name)
                .unwrap_or_else(|| asset_matches(asset, &repo.asset_rules))
    })
}

pub async fn fetch_releases(
    client: &Client,
    repo: &RepositoryConfig,
    token: Option<&str>,
    official_api_only: bool,
    api_proxies: &[GitHubProxy],
) -> Result<Vec<ReleaseInfo>, ReleaseError> {
    let max_release_count = repo.max_release_count.clamp(1, 100);
    let official = match repository_release_api_url(
        &repo.provider,
        &repo.username,
        &repo.repository,
        max_release_count,
    ) {
        Ok(url) => url,
        Err(error) => {
            log::warn!(
                "Release API 仓库地址校验失败: repository_id={} provider={:?} error_kind={}",
                repo.id,
                repo.provider,
                release_error_kind(&error)
            );
            return Err(error);
        }
    };
    let request_targets = api_request_targets(&official, token, official_api_only, api_proxies);
    log::debug!(
        "开始获取 Release: repository_id={} provider={:?} channel={:?} max_release_count={} token_configured={} official_api_only={} api_target_count={}",
        repo.id,
        repo.provider,
        repo.channel,
        max_release_count,
        token.is_some(),
        official_api_only,
        request_targets.len()
    );
    fetch_releases_from_targets(client, repo, &official, request_targets).await
}

async fn fetch_releases_from_targets(
    client: &Client,
    repo: &RepositoryConfig,
    official: &str,
    request_targets: Vec<(String, Option<&str>)>,
) -> Result<Vec<ReleaseInfo>, ReleaseError> {
    let mut last_error = None;
    let mut skip_gitee_official = false;
    for (index, (url, request_token)) in request_targets.into_iter().enumerate() {
        if skip_gitee_official && url == official {
            continue;
        }
        let target_kind = api_target_kind(official, &url, request_token);
        log::debug!(
            "尝试 Release API: repository_id={} provider={:?} target_kind={} target_index={} token_attached={}",
            repo.id,
            repo.provider,
            target_kind,
            index + 1,
            request_token.is_some()
        );
        match fetch_release_api_with_retries(client, &url, &repo.provider, request_token).await {
            Ok(mut releases) => {
                for release in &mut releases {
                    release.assets.retain(|asset| {
                        !crate::community::is_source_archive(repo, &release.tag_name, &asset.name)
                    });
                }
                log::info!(
                    "Release API 获取成功: repository_id={} provider={:?} target_kind={} release_count={}",
                    repo.id,
                    repo.provider,
                    target_kind,
                    releases.len()
                );
                return Ok(releases);
            }
            Err(error) => {
                log::warn!(
                    "Release API 目标失败: repository_id={} provider={:?} target_kind={} error_kind={}",
                    repo.id,
                    repo.provider,
                    target_kind,
                    release_error_kind(&error)
                );
                if repo.provider == Provider::Gitee && error.is_forbidden() {
                    if url != official {
                        log::warn!(
                            "Gitee API 代理返回 403，结束刷新: repository_id={}",
                            repo.id
                        );
                        return Err(error);
                    }
                    log::debug!(
                        "Gitee 官方 API 返回 403，跳过官方 Token 目标并转 API 代理: repository_id={}",
                        repo.id
                    );
                    skip_gitee_official = true;
                }
                last_error = Some(error);
            }
        }
    }
    let error = last_error.expect("release API request targets must not be empty");
    log::error!(
        "Release API 所有目标均失败: repository_id={} provider={:?} error_kind={}",
        repo.id,
        repo.provider,
        release_error_kind(&error)
    );
    Err(error)
}

/// Build the ordered API fallback sequence for one provider. The official
/// endpoint is tried without credentials first, then again with the configured
/// token, and only then are that provider's configured proxy endpoints used.
/// A Gitee official 403 skips the token target; a Gitee proxy 403 ends the run.
/// Credentials are never sent to a proxy endpoint.
fn api_request_targets<'a>(
    official_url: &str,
    token: Option<&'a str>,
    _official_api_only: bool,
    api_proxies: &[GitHubProxy],
) -> Vec<(String, Option<&'a str>)> {
    let mut targets = vec![(official_url.to_string(), None)];
    if token.is_some() {
        targets.push((official_url.to_string(), token));
    }
    targets.extend(
        crate::github_proxies::proxy_urls(api_proxies, official_url, false, None)
            .into_iter()
            .map(|url| (url, None)),
    );
    targets
}

async fn fetch_release_api_with_retries(
    client: &Client,
    url: &str,
    provider: &Provider,
    token: Option<&str>,
) -> Result<Vec<ReleaseInfo>, ReleaseError> {
    fetch_api_with_retries(client, url, provider, token, |value| {
        parse_release_json(value, provider)
    })
    .await
}

async fn fetch_api_with_retries<T>(
    client: &Client,
    url: &str,
    provider: &Provider,
    token: Option<&str>,
    parse: impl Fn(&Value) -> Result<T, ReleaseError>,
) -> Result<T, ReleaseError> {
    let mut last_error = None;
    for attempt in 0..REPOSITORY_API_ATTEMPTS {
        log::debug!(
            "Release API 请求: provider={:?} attempt={}/{} token_attached={}",
            provider,
            attempt + 1,
            REPOSITORY_API_ATTEMPTS,
            token.is_some()
        );
        match fetch_api_json(client, url, provider, token)
            .await
            .and_then(|value| parse(&value))
        {
            Ok(result) => {
                log::debug!(
                    "API 请求成功: provider={:?} attempt={}",
                    provider,
                    attempt + 1
                );
                return Ok(result);
            }
            Err(error) if error.is_forbidden() => {
                log::warn!(
                    "Release API 返回 403，停止当前目标重试: provider={:?} attempt={}",
                    provider,
                    attempt + 1
                );
                return Err(error);
            }
            Err(error) => {
                log::debug!(
                    "Release API 请求失败: provider={:?} attempt={} error_kind={}",
                    provider,
                    attempt + 1,
                    release_error_kind(&error)
                );
                last_error = Some(error);
                if attempt + 1 < REPOSITORY_API_ATTEMPTS {
                    log::debug!(
                        "Release API 将重试: provider={:?} next_attempt={}/{}",
                        provider,
                        attempt + 2,
                        REPOSITORY_API_ATTEMPTS
                    );
                    continue;
                }
            }
        }
    }
    let error = last_error.expect("repository API attempts must not be empty");
    log::warn!(
        "Release API 重试耗尽: provider={:?} attempts={} error_kind={}",
        provider,
        REPOSITORY_API_ATTEMPTS,
        release_error_kind(&error)
    );
    Err(error)
}

/// Fetches a release list from an already selected API endpoint. This is used
/// by the application updater so its provider/proxy order is explicit.
pub async fn fetch_release_api(
    client: &Client,
    url: &str,
    provider: &Provider,
    token: Option<&str>,
) -> Result<Vec<ReleaseInfo>, ReleaseError> {
    let value = fetch_api_json(client, url, provider, token).await?;
    parse_release_json(&value, provider)
}

async fn fetch_api_json(
    client: &Client,
    url: &str,
    provider: &Provider,
    token: Option<&str>,
) -> Result<Value, ReleaseError> {
    log::trace!(
        "发送 Release API 请求: provider={:?} token_attached={}",
        provider,
        token.is_some()
    );
    let request_url = match (provider, token) {
        (Provider::Gitee, Some(token)) if is_official_gitee_api_url(url) => {
            gitee_api_url_with_token(url, token)?
        }
        _ => url.to_string(),
    };
    let mut request = client
        .get(&request_url)
        .timeout(API_REQUEST_TIMEOUT)
        .header("User-Agent", APP_USER_AGENT)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header(
            reqwest::header::CACHE_CONTROL,
            "no-cache, no-store, max-age=0",
        )
        .header(reqwest::header::PRAGMA, "no-cache");
    if let Some(token) = token {
        request = match provider {
            Provider::Github => request.bearer_auth(token),
            Provider::Gitee => request,
        };
    }
    let response = request.send().await.map_err(|e| {
        let error = ReleaseError::Http(e.without_url().to_string());
        log::debug!(
            "Release API 网络请求异常: provider={:?} error_kind={}",
            provider,
            release_error_kind(&error)
        );
        error
    })?;
    log::trace!(
        "收到 Release API 响应: provider={:?} status={}",
        provider,
        response.status()
    );
    if !response.status().is_success() {
        return Err(ReleaseError::Status(response.status()));
    }
    let value = response.json::<Value>().await.map_err(|e| {
        let error = ReleaseError::Json(e.without_url().to_string());
        log::warn!(
            "Release API JSON 解析失败: provider={:?} error_kind={}",
            provider,
            release_error_kind(&error)
        );
        error
    })?;
    Ok(value)
}

pub async fn fetch_repository_default_branch(
    client: &Client,
    repo: &RepositoryConfig,
    token: Option<&str>,
    api_proxies: &[GitHubProxy],
) -> Result<String, ReleaseError> {
    let mut url = Url::parse(&repository_release_api_url(
        &repo.provider,
        &repo.username,
        &repo.repository,
        1,
    )?)
    .map_err(|error| ReleaseError::InvalidUrl(error.to_string()))?;
    url.path_segments_mut()
        .expect("validated repository API URL")
        .pop();
    url.set_query(None);
    let official = url.to_string();
    let mut last_error = None;
    let mut skip_official = false;
    for (url, request_token) in api_request_targets(&official, token, false, api_proxies) {
        if skip_official && url == official {
            continue;
        }
        log::info!(
            "正在读取仓库默认分支：仓库={}，渠道={}",
            repo.id,
            api_target_kind(&official, &url, request_token)
        );
        let fetched =
            fetch_api_with_retries(client, &url, &repo.provider, request_token, |value| {
                let branch = value
                    .get("default_branch")
                    .and_then(Value::as_str)
                    .filter(|branch| !branch.trim().is_empty())
                    .ok_or_else(|| ReleaseError::Json("仓库默认分支缺失".into()))?;
                repository_file_url(
                    &repo.provider,
                    &repo.username,
                    &repo.repository,
                    branch,
                    "README.md",
                )?;
                Ok(branch.to_string())
            })
            .await;
        match fetched {
            Ok(branch) => return Ok(branch),
            Err(error) => {
                if repo.provider == Provider::Gitee && error.is_forbidden() {
                    if url != official {
                        return Err(error);
                    }
                    skip_official = true;
                }
                last_error = Some(error);
            }
        }
    }
    Err(last_error.expect("repository API always has an official target"))
}

pub fn parse_release_json(
    value: &Value,
    provider: &Provider,
) -> Result<Vec<ReleaseInfo>, ReleaseError> {
    let owned;
    let array = if let Some(array) = value.as_array() {
        array
    } else if value.is_object() {
        owned = vec![value.clone()];
        &owned
    } else {
        log::warn!(
            "Release API 响应结构无效: provider={:?} expected=array_or_object",
            provider
        );
        return Err(ReleaseError::Json("Release 响应不是数组或对象".into()));
    };
    log::debug!(
        "开始解析 Release API 响应: provider={:?} item_count={}",
        provider,
        array.len()
    );
    let mut releases = Vec::new();
    let mut skipped_without_tag = 0usize;
    for item in array {
        let tag = item
            .get("tag_name")
            .or_else(|| item.get("tagName"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if tag.is_empty() {
            skipped_without_tag += 1;
            continue;
        }
        let assets = item
            .get("assets")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|asset| {
                let name = asset.get("name").and_then(Value::as_str)?.to_string();
                let download_url = [
                    "browser_download_url",
                    "browserDownloadUrl",
                    "download_url",
                    "downloadUrl",
                    "url",
                ]
                .iter()
                .filter_map(|key| asset.get(*key).and_then(Value::as_str))
                .find(|url| !url.trim().is_empty())?
                .to_string();
                let digest = asset
                    .get("digest")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or_else(|| {
                        asset
                            .get("sha256")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    });
                Some(ReleaseAsset {
                    name,
                    size: asset
                        .get("size")
                        .and_then(|value| value.as_u64().or_else(|| value.as_str()?.parse().ok()))
                        .unwrap_or(0),
                    download_count: asset
                        .get("download_count")
                        .and_then(|value| value.as_u64().or_else(|| value.as_str()?.parse().ok())),
                    download_url,
                    digest,
                })
            })
            .collect();
        let prerelease = item
            .get("prerelease")
            .and_then(Value::as_bool)
            .unwrap_or_else(|| {
                tag.to_ascii_lowercase().contains("beta")
                    || tag.to_ascii_lowercase().contains("alpha")
            });
        let date = item
            .get("published_at")
            .or_else(|| item.get("created_at"))
            .and_then(Value::as_str)
            .map(str::to_string);
        releases.push(ReleaseInfo {
            release_id: item
                .get("id")
                .and_then(|value| value.as_u64().or_else(|| value.as_str()?.parse().ok())),
            version: parse_version(&tag).map(|value| value.to_string()),
            tag_name: tag,
            name: item
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            body: item
                .get("body")
                .or_else(|| item.get("description"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            published_at: date,
            prerelease,
            draft: item.get("draft").and_then(Value::as_bool).unwrap_or(false),
            assets,
        });
    }
    log::debug!(
        "Release API 响应解析完成: provider={:?} release_count={} skipped_without_tag={}",
        provider,
        releases.len(),
        skipped_without_tag
    );
    Ok(releases)
}

pub fn build_client(timeout_seconds: u64) -> Result<Client, ReleaseError> {
    Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .user_agent(APP_USER_AGENT)
        .build()
        .map_err(|e| {
            log::error!(
                "构建 Release API 客户端失败: requested_timeout_seconds={} error_kind=client_build",
                timeout_seconds
            );
            ReleaseError::Http(e.to_string())
        })
}

/// Builds a client for potentially large release assets. The total request
/// timeout is disabled so a download is allowed to run as long as data keeps
/// arriving. The read timeout still aborts a connection that has gone idle.
pub fn build_download_client(timeout_seconds: u64) -> Result<Client, ReleaseError> {
    let connect_timeout = std::time::Duration::from_secs(timeout_seconds.max(1));
    Client::builder()
        .connect_timeout(connect_timeout)
        .read_timeout(std::time::Duration::from_secs(120))
        .user_agent(APP_USER_AGENT)
        .build()
        .map_err(|e| {
            log::error!(
                "构建 Release 下载客户端失败: requested_timeout_seconds={} error_kind=client_build",
                timeout_seconds
            );
            ReleaseError::Http(e.to_string())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn failed_api_requests_do_not_expose_query_credentials() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let client = Client::builder().no_proxy().build().unwrap();
        let url = format!("http://{address}/api?access_token=placeholder-credential");
        let error = fetch_release_api(&client, &url, &Provider::Gitee, None)
            .await
            .unwrap_err()
            .to_string();
        assert!(!error.contains("access_token"));
        assert!(!error.contains("placeholder-credential"));
        assert!(!error.contains(&address.to_string()));
    }
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };

    struct ApiTestServer {
        url: String,
        requests: Arc<Mutex<Vec<String>>>,
        stopped: Arc<AtomicBool>,
        worker: Option<std::thread::JoinHandle<()>>,
    }

    impl ApiTestServer {
        fn new(responses: Vec<(&'static str, &'static str)>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            listener.set_nonblocking(true).unwrap();
            let requests = Arc::new(Mutex::new(Vec::new()));
            let stopped = Arc::new(AtomicBool::new(false));
            let server_requests = requests.clone();
            let server_stopped = stopped.clone();
            let worker = std::thread::spawn(move || {
                let mut index = 0;
                while !server_stopped.load(Ordering::Relaxed) {
                    let (mut stream, _) = match listener.accept() {
                        Ok(connection) => connection,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(std::time::Duration::from_millis(2));
                            continue;
                        }
                        Err(error) => panic!("test server accept failed: {error}"),
                    };
                    // Windows accepted sockets inherit the listener's mode.
                    stream.set_nonblocking(false).unwrap();
                    stream.set_read_timeout(Some(API_REQUEST_TIMEOUT)).unwrap();
                    let mut request = Vec::new();
                    let mut buffer = [0; 1024];
                    while !request.windows(4).any(|part| part == b"\r\n\r\n") {
                        let count = stream.read(&mut buffer).unwrap();
                        if count == 0 {
                            break;
                        }
                        request.extend_from_slice(&buffer[..count]);
                    }
                    server_requests
                        .lock()
                        .unwrap()
                        .push(String::from_utf8(request).unwrap());
                    // Unexpected extra requests also get a response, so a failed
                    // assertion reports the actual count instead of hanging.
                    let (status, body) = responses
                        .get(index)
                        .copied()
                        .unwrap_or(("500 Internal Server Error", ""));
                    index += 1;
                    write!(stream, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                }
            });
            Self {
                url: format!("http://{address}/releases"),
                requests,
                stopped,
                worker: Some(worker),
            }
        }

        fn requests(&self) -> Vec<String> {
            self.requests.lock().unwrap().clone()
        }
    }

    impl Drop for ApiTestServer {
        fn drop(&mut self) {
            self.stopped.store(true, Ordering::Relaxed);
            self.worker.take().unwrap().join().unwrap();
        }
    }

    fn api_test_client() -> Client {
        Client::builder().no_proxy().build().unwrap()
    }

    fn api_test_repository() -> RepositoryConfig {
        serde_json::from_value(serde_json::json!({
            "id": "retry-test", "name": "Retry test", "provider": "gitee",
            "username": "owner", "repository": "repo", "install_target": "BepInEx/plugins",
            "enabled": true, "builtin": false
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn release_api_non_403_failures_try_three_times() {
        for status in [
            "404 Not Found",
            "429 Too Many Requests",
            "500 Internal Server Error",
        ] {
            let server = ApiTestServer::new(vec![(status, ""); 3]);
            let result = fetch_release_api_with_retries(
                &api_test_client(),
                &server.url,
                &Provider::Gitee,
                None,
            )
            .await;
            assert!(result.is_err());
            assert_eq!(server.requests().len(), 3, "{status}");
        }
    }

    #[tokio::test]
    async fn release_api_403_is_never_retried_even_after_an_ordinary_failure() {
        let server = ApiTestServer::new(vec![
            ("500 Internal Server Error", ""),
            ("403 Forbidden", ""),
        ]);
        let error =
            fetch_release_api_with_retries(&api_test_client(), &server.url, &Provider::Gitee, None)
                .await
                .unwrap_err();
        assert!(error.is_forbidden());
        assert_eq!(server.requests().len(), 2);
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
            !request.to_ascii_lowercase().contains("authorization:")
                && !request.contains("test-token")
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
    fn cached_tag_source_archives_cannot_be_installed_even_when_explicitly_selected() {
        let mut repo =
            crate::community::repository_from_coordinates(&Provider::Gitee, "Owner", "Mod", "", "")
                .unwrap();
        let cases: Vec<(String, String, bool)> = serde_json::from_str(include_str!(
            "../../tests/fixtures/gitee-source-assets.json"
        ))
        .unwrap();
        for (tag, name, allowed) in cases {
            let mut info = parse_release_json(&serde_json::json!([{"tag_name": "placeholder", "assets": [{"name": name, "browser_download_url": "https://example.com/file"}]}]), &Provider::Gitee).unwrap().remove(0);
            info.tag_name = tag;
            let selected = info.assets[0].name.clone();
            assert_eq!(
                select_install_asset(&repo, &info, None).is_some(),
                allowed,
                "{selected}"
            );
            assert_eq!(
                select_install_asset(&repo, &info, Some(&selected)).is_some(),
                allowed,
                "explicit {selected}"
            );
            let source_id = repo.source_id.take();
            assert!(
                select_install_asset(&repo, &info, Some(&selected)).is_some(),
                "custom Gitee {selected}"
            );
            repo.source_id = source_id;
            repo.provider = Provider::Github;
            assert!(
                select_install_asset(&repo, &info, Some(&selected)).is_some(),
                "GitHub {selected}"
            );
            repo.provider = Provider::Gitee;
        }
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
                vec![(server.url.clone(), None)]).await.unwrap();
            assert_eq!(releases[0].assets.iter().map(|asset| asset.name.as_str()).collect::<Vec<_>>(), ["Mod.zip", "Mod.dll"]);
            assert_eq!(server.requests().len(), 1);
        });
    }

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
            assert!(repository_release_api_url(&Provider::Github, username, "repo", 10).is_err());
        }
    }

    #[test]
    fn release_api_urls_are_assembled_from_structured_repository_fields() {
        assert_eq!(
            repository_release_api_url(&Provider::Github, "sinai-dev", "UnityExplorer", 10)
                .unwrap(),
            "https://api.github.com/repos/sinai-dev/UnityExplorer/releases?per_page=10&direction=desc&sort=created"
        );
        assert_eq!(
            repository_release_api_url(
                &Provider::Gitee,
                "ch3ngyz",
                "Overcooked-2-ChefRotation-Releases",
                0
            )
            .unwrap(),
            "https://gitee.com/api/v5/repos/ch3ngyz/Overcooked-2-ChefRotation-Releases/releases?per_page=1&direction=desc&sort=created"
        );
        assert!(
            repository_release_api_url(&Provider::Github, "user", "repo", 500)
                .unwrap()
                .contains("per_page=100")
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
            repository_file_url(&Provider::Github, "gua248", "repo", "main", "../secret.dll")
                .is_err()
        );
    }

    #[test]
    fn versions_and_channels_are_supported() {
        assert_eq!(
            parse_version("v1.2.3-beta").unwrap().to_string(),
            "1.2.3-beta"
        );
        assert!(parse_version("5.4.23.5").is_some());
        let releases = vec![
            ReleaseInfo {
                release_id: None,
                tag_name: "v1.0.0".into(),
                version: Some("1.0.0".into()),
                name: String::new(),
                body: String::new(),
                published_at: None,
                prerelease: false,
                draft: false,
                assets: Vec::new(),
            },
            ReleaseInfo {
                release_id: None,
                tag_name: "v1.1.0-beta".into(),
                version: Some("1.1.0-beta".into()),
                name: String::new(),
                body: String::new(),
                published_at: None,
                prerelease: true,
                draft: false,
                assets: Vec::new(),
            },
        ];
        assert_eq!(filter_channel(releases.clone(), &Channel::Stable).len(), 1);
        assert_eq!(filter_channel(releases, &Channel::Beta).len(), 1);
    }

    #[test]
    fn asset_rules_match_exact_names() {
        let asset = ReleaseAsset {
            name: "HostUtilities.zip".into(),
            size: 1,
            download_count: None,
            download_url: "https://github.com/a/b/releases/download/v1/HostUtilities.zip".into(),
            digest: None,
        };
        assert!(asset_matches(
            &asset,
            &[AssetRule {
                rule_type: AssetRuleType::Exact,
                value: "HostUtilities.zip".into()
            }]
        ));
    }

    #[test]
    fn api_request_targets_order_direct_token_then_proxy() {
        let official = "https://api.github.com/repos/a/b/releases";
        let proxies = vec![GitHubProxy {
            id: "proxy".into(),
            name: "Proxy".into(),
            template: "https://proxy.example/{url}".into(),
            regions: Vec::new(),
            description: String::new(),
            enabled: true,
            builtin: false,
            remote: false,
        }];

        let targets = api_request_targets(official, Some("secret"), true, &proxies);
        assert_eq!(targets.len(), 3);
        assert_eq!(targets[0], (official.to_string(), None));
        assert_eq!(targets[1], (official.to_string(), Some("secret")));
        assert_eq!(
            targets[2],
            (
                "https://proxy.example/https://api.github.com/repos/a/b/releases".to_string(),
                None,
            )
        );

        let targets = api_request_targets(official, Some("secret"), false, &proxies);
        assert_eq!(targets[2].1, None);
    }

    #[test]
    fn gitee_token_uses_access_token_query_on_official_api_only() {
        let official = "https://gitee.com/api/v5/repos/a/b/releases?per_page=10";
        assert_eq!(
            gitee_api_url_with_token(official, "secret token").unwrap(),
            "https://gitee.com/api/v5/repos/a/b/releases?per_page=10&access_token=secret+token"
        );
        assert!(!is_official_gitee_api_url(
            "https://oc2gtapi.ch3ng.top/api/v5/repos/a/b/releases"
        ));
    }

    #[test]
    fn release_id_and_missing_asset_size_are_parsed() {
        let value = serde_json::json!([{
            "id": "42",
            "tag_name": "v1.0.0",
            "assets": [{
                "name": "HostUtilities.zip",
                "browser_download_url": "https://gitee.com/example/download"
            }]
        }]);
        let releases = parse_release_json(&value, &Provider::Gitee).unwrap();
        assert_eq!(releases[0].release_id, Some(42));
        assert_eq!(releases[0].assets[0].size, 0);
        assert_eq!(releases[0].assets[0].download_count, None);
    }

    #[test]
    fn release_asset_download_counts_preserve_zero_and_missing_metadata() {
        for (input, expected) in [
            (Some(serde_json::json!(42)), Some(42)),
            (Some(serde_json::json!(0)), Some(0)),
            (Some(serde_json::json!("1234")), Some(1234)),
            (None, None),
            (Some(serde_json::Value::Null), None),
            (Some(serde_json::json!(-1)), None),
            (Some(serde_json::json!(1.5)), None),
            (Some(serde_json::json!("unknown")), None),
        ] {
            let mut asset = serde_json::json!({
                "name": "example.zip", "size": 1024,
                "browser_download_url": "https://github.com/example/mod/releases/download/v1/example.zip"
            });
            if let Some(input) = input {
                asset["download_count"] = input;
            }
            let releases = parse_release_json(
                &serde_json::json!([{ "tag_name": "v1.0.0", "assets": [asset] }]),
                &Provider::Github,
            )
            .unwrap();
            let asset = &releases[0].assets[0];
            assert_eq!(asset.download_count, expected);
            let cached: ReleaseAsset =
                serde_json::from_value(serde_json::to_value(asset).unwrap()).unwrap();
            assert_eq!(cached.download_count, expected);
            assert_eq!(cached.name, "example.zip");
            assert_eq!(cached.size, 1024);
        }

        let legacy: ReleaseAsset = serde_json::from_value(serde_json::json!({
            "name": "legacy.zip", "size": 1024, "download_url": "https://example.com/legacy.zip"
        }))
        .unwrap();
        assert_eq!(legacy.download_count, None);
    }

    #[test]
    fn user_agent_identifies_client_and_version() {
        assert!(APP_USER_AGENT.starts_with("Overcooked 2 MOD Manager/"));
        assert!(APP_USER_AGENT.contains("client=desktop"));
        assert!(APP_USER_AGENT.contains("arch=x64"));
    }
}
