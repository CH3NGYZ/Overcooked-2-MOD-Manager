use super::error::ReleaseError;
use super::error::release_error_kind;
use super::transport::api_target_kind;
use super::transport::fetch_api_with_retries;
use super::transport::fetch_release_api_with_retries;
use super::transport::repository_api_targets;
use super::transport::{ApiTargets, RepositoryApiRoute, RepositoryApiTarget};
use crate::api_rate_gate::{ApiRateGate, global_gate};
use crate::models::Provider;
use crate::models::ReleaseInfo;
use crate::providers::repository_file_url;
use crate::providers::repository_release_api_url;
use crate::providers::repository_release_api_url_page;
use crate::release_cache::RepoReleaseCache;
use crate::releases::RepositoryRequest;
use reqwest::Client;
use serde_json::Value;
use std::sync::Arc;
use url::Url;

pub(crate) async fn cached_releases(
    cache: &RepoReleaseCache,
    client: &Client,
    repo: &RepositoryRequest,
    region: &str,
    token: Option<&str>,
    pass_id: Option<&str>,
    force: bool,
) -> Result<Arc<Vec<ReleaseInfo>>, String> {
    let ResolvedReleaseRoute { route, first, key } =
        resolve_release_route(repo, region, token).map_err(|error| error.to_string())?;
    cache
        .list(key, pass_id, force, || async {
            fetch_releases_on_route(client, repo, &route, first, token)
                .await
                .map_err(|error| error.to_string())
        })
        .await
}

#[cfg(test)]
pub async fn fetch_releases(
    client: &Client,
    repo: &RepositoryRequest,
    region: &str,
    token: Option<&str>,
) -> Result<Vec<ReleaseInfo>, ReleaseError> {
    let result = resolve_release_route(repo, region, token)?;
    fetch_releases_on_route(client, repo, &result.route, result.first, token).await
}

struct ResolvedReleaseRoute<'a> {
    route: RepositoryApiRoute,
    first: (String, ApiTargets<'a>),
    key: String,
}

fn resolve_release_route<'a>(
    repo: &RepositoryRequest,
    region: &str,
    token: Option<&'a str>,
) -> Result<ResolvedReleaseRoute<'a>, ReleaseError> {
    let official = release_page_url(repo, 1)?;
    let route = RepositoryApiRoute::resolve(repo, region)?;
    let targets = route.targets(&official, token)?;
    let cache_url = repository_release_api_url_page(
        &repo.provider,
        &repo.username.to_lowercase(),
        &repo.repository.to_lowercase(),
        Some(1),
    )?;
    let key = if cache_url == official {
        release_route_cache_key(repo, &official, &targets)
    } else {
        release_route_cache_key(repo, &cache_url, &route.targets(&cache_url, token)?)
    };
    Ok(ResolvedReleaseRoute {
        route,
        first: (official, targets),
        key,
    })
}

fn release_route_cache_key(
    repo: &RepositoryRequest,
    official: &str,
    targets: &ApiTargets<'_>,
) -> String {
    use sha2::{Digest, Sha256};
    use std::sync::OnceLock;

    static CACHE_SECRET: OnceLock<uuid::Uuid> = OnceLock::new();
    let mut hash = Sha256::new();
    hash.update(CACHE_SECRET.get_or_init(uuid::Uuid::new_v4).as_bytes());
    hash.update([u8::from(crate::community::filters_source_archives(
        &repo.provider,
        repo.source_id.as_deref(),
    ))]);
    for (url, token) in targets {
        hash.update([u8::from(url == official), u8::from(token.is_some())]);
        hash.update((url.len() as u64).to_le_bytes());
        hash.update(url);
        if let Some(token) = token {
            hash.update((token.len() as u64).to_le_bytes());
            hash.update(token);
        }
    }
    let repo = super::repository_cache_key(repo.provider.clone(), &repo.username, &repo.repository);
    format!("{repo}:{:x}", hash.finalize())
}

#[cfg(test)]
pub(crate) fn release_cache_key_for_test(
    repo: &RepositoryRequest,
    region: &str,
    token: Option<&str>,
) -> String {
    resolve_release_route(repo, region, token).unwrap().key
}

async fn fetch_releases_on_route(
    client: &Client,
    repo: &RepositoryRequest,
    route: &RepositoryApiRoute,
    first: (String, ApiTargets<'_>),
    token: Option<&str>,
) -> Result<Vec<ReleaseInfo>, ReleaseError> {
    let mut first = Some(first);
    super::pagination::collect_release_pages(|page| {
        let target: Result<_, ReleaseError> = first.take().map(Ok).unwrap_or_else(|| {
            let official = release_page_url(repo, page)?;
            let targets = route.targets(&official, token)?;
            Ok((official, targets))
        });
        async move {
            let (official, request_targets) = target?;
            log::debug!(
                "开始获取 Release: repository_id={} provider={:?} per_page={} page={} token_configured={} api_target_count={}",
                repo.id,
                repo.provider,
                super::pagination::RELEASE_PAGE_SIZE,
                page,
                token.is_some(),
                request_targets.len()
            );
            fetch_releases_from_targets(client, repo, &official, request_targets, global_gate())
                .await
        }
    })
    .await
}

fn release_page_url(repo: &RepositoryRequest, page: u32) -> Result<String, ReleaseError> {
    repository_release_api_url_page(&repo.provider, &repo.username, &repo.repository, Some(page))
}

fn record_target_error(
    last_error: &mut Option<ReleaseError>,
    provider: &Provider,
    error: ReleaseError,
) {
    if *provider == Provider::Gitcode && error.is_too_many_requests() {
        *last_error = Some(ReleaseError::Cooldown);
    } else if *provider != Provider::Gitcode || !matches!(last_error, Some(ReleaseError::Cooldown))
    {
        *last_error = Some(error);
    }
}

async fn fetch_releases_from_targets(
    client: &Client,
    repo: &RepositoryRequest,
    official: &str,
    request_targets: ApiTargets<'_>,
    gate: &ApiRateGate,
) -> Result<(Vec<ReleaseInfo>, usize), ReleaseError> {
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
        let target =
            RepositoryApiTarget::new(official, &url, &repo.provider, request_token, Some(gate));
        match fetch_release_api_with_retries(client, target).await {
            Ok((mut releases, received)) => {
                for release in &mut releases {
                    release.assets.retain(|asset| {
                        !crate::community::is_source_archive(
                            &repo.provider,
                            repo.source_id.as_deref(),
                            &release.tag_name,
                            &asset.name,
                        )
                    });
                }
                log::info!(
                    "Release API 获取成功: repository_id={} provider={:?} target_kind={} release_count={}",
                    repo.id,
                    repo.provider,
                    target_kind,
                    releases.len()
                );
                return Ok((releases, received));
            }
            Err(error) => {
                log::warn!(
                    "Release API 目标失败: repository_id={} provider={:?} target_kind={} error_kind={}",
                    repo.id,
                    repo.provider,
                    target_kind,
                    release_error_kind(&error)
                );
                if repo.provider == Provider::Gitee && url == official && error.is_forbidden() {
                    log::debug!(
                        "Gitee 官方 API 返回 403，跳过剩余官方目标并转 API 代理: repository_id={}",
                        repo.id
                    );
                    skip_gitee_official = true;
                }
                record_target_error(&mut last_error, &repo.provider, error);
            }
        }
    }
    let error = last_error.unwrap_or_else(|| ReleaseError::Http("proxy.validationFailed".into()));
    log::error!(
        "Release API 所有目标均失败: repository_id={} provider={:?} error_kind={}",
        repo.id,
        repo.provider,
        release_error_kind(&error)
    );
    Err(error)
}

pub async fn fetch_repository_default_branch(
    client: &Client,
    repo: &RepositoryRequest,
    region: &str,
    token: Option<&str>,
) -> Result<String, ReleaseError> {
    let mut url = Url::parse(&repository_release_api_url(
        &repo.provider,
        &repo.username,
        &repo.repository,
    )?)
    .map_err(|error| ReleaseError::InvalidUrl(error.to_string()))?;
    url.path_segments_mut()
        .expect("validated repository API URL")
        .pop();
    url.set_query(None);
    let official = url.to_string();
    let targets = repository_api_targets(repo, region, &official, token)?;
    fetch_default_branch_from_targets(client, repo, &official, targets, global_gate()).await
}

async fn fetch_default_branch_from_targets(
    client: &Client,
    repo: &RepositoryRequest,
    official: &str,
    targets: ApiTargets<'_>,
    gate: &ApiRateGate,
) -> Result<String, ReleaseError> {
    let gate = (repo.provider == Provider::Gitcode).then_some(gate);
    let mut last_error = None;
    let mut skip_official = false;
    for (url, request_token) in targets {
        if skip_official && url == official {
            continue;
        }
        log::info!(
            "正在读取仓库默认分支：仓库={}，渠道={}",
            repo.id,
            api_target_kind(official, &url, request_token)
        );
        let target = RepositoryApiTarget::new(official, &url, &repo.provider, request_token, gate);
        let fetched = fetch_api_with_retries(client, target, |value| {
            let branch = value
                .get("default_branch")
                .and_then(Value::as_str)
                .filter(|branch| !branch.trim().is_empty())
                .ok_or_else(|| ReleaseError::Json("source.defaultBranchMissing".into()))?;
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
                if repo.provider == Provider::Gitee && url == official && error.is_forbidden() {
                    skip_official = true;
                }
                record_target_error(&mut last_error, &repo.provider, error);
            }
        }
    }
    Err(last_error.unwrap_or_else(|| ReleaseError::Http("proxy.validationFailed".into())))
}

#[cfg(test)]
mod tests;
