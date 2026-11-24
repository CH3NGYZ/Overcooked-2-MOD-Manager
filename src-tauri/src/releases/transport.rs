use super::error::ReleaseError;
use super::error::release_error_kind;
use super::projection::parse_release_json;
use crate::api_scheduler::ApiRequestExt;
use crate::models::Provider;
use crate::models::ReleaseInfo;
use crate::network_policy::{NetworkPolicy, Step};
use reqwest::Client;
use reqwest::StatusCode;
use serde_json::Value;

const REPOSITORY_API_ATTEMPTS: usize = 3;

pub(super) fn api_target_kind(
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

pub(super) fn repository_api_targets<'a>(
    repo: &crate::releases::RepositoryRequest,
    region: &str,
    official: &str,
    token: Option<&'a str>,
) -> Result<ApiTargets<'a>, ReleaseError> {
    RepositoryApiRoute::resolve(repo, region)?.targets(official, token)
}

pub(super) type ApiTargets<'a> = Vec<(String, Option<&'a str>)>;

pub(super) struct RepositoryApiRoute {
    policy: std::sync::Arc<NetworkPolicy>,
    steps: Vec<Step>,
}

impl RepositoryApiRoute {
    pub(super) fn resolve(
        repo: &crate::releases::RepositoryRequest,
        region: &str,
    ) -> Result<Self, ReleaseError> {
        let policy = NetworkPolicy::active().map_err(ReleaseError::Http)?;
        let plan = policy
            .resolve(&repo.provider, repo.kind, region, &repo.network_plan)
            .map_err(ReleaseError::Http)?;
        Ok(Self {
            policy,
            steps: plan.api.route,
        })
    }

    pub(super) fn targets<'a>(
        &self,
        official: &str,
        token: Option<&'a str>,
    ) -> Result<ApiTargets<'a>, ReleaseError> {
        let mut targets = Vec::new();
        for step in &self.steps {
            match step {
                Step::Official => targets.push((official.to_string(), token)),
                Step::Proxy { id } => {
                    let proxy = self
                        .policy
                        .proxies
                        .get(id)
                        .ok_or_else(|| ReleaseError::Http("proxy.validationFailed".into()))?;
                    targets.extend(
                        crate::github_proxies::proxy_urls(
                            &[proxy.legacy_entry()],
                            official,
                            false,
                            None,
                        )
                        .into_iter()
                        .filter(|url| url != official)
                        .map(|url| (url, None)),
                    );
                }
                _ => return Err(ReleaseError::Http("proxy.validationFailed".into())),
            }
        }
        let mut seen = std::collections::HashSet::new();
        targets.retain(|(url, _)| seen.insert(url.clone()));
        if targets.is_empty() {
            return Err(ReleaseError::Http("proxy.validationFailed".into()));
        }
        Ok(targets)
    }
}

pub(super) async fn fetch_release_api_with_retries(
    client: &Client,
    url: &str,
    provider: &Provider,
    token: Option<&str>,
) -> Result<(Vec<ReleaseInfo>, usize), ReleaseError> {
    fetch_api_with_retries(client, url, provider, token, |value| {
        let count = value
            .as_array()
            .ok_or_else(|| ReleaseError::Json("source.releaseResponseInvalid".into()))?
            .len();
        Ok((parse_release_json(value, provider)?, count))
    })
    .await
}

pub(super) async fn fetch_api_with_retries<T>(
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
    let request = crate::providers::api_request(client, url, provider, token)?
        .header(
            reqwest::header::CACHE_CONTROL,
            "no-cache, no-store, max-age=0",
        )
        .header(reqwest::header::PRAGMA, "no-cache");
    let response = request.send_api(provider.clone()).await.map_err(|e| {
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
    let status = response.status();
    if !status.is_success() {
        // 403/429 可能是限流：把响应头与正文交给闸门判定，并上报给调用方
        if status == StatusCode::FORBIDDEN || status == StatusCode::TOO_MANY_REQUESTS {
            let signal = crate::api_rate_gate::RateLimitSignal {
                body_limit_message: false, // 由调用方在读取正文后补充
                remaining_exhausted: response
                    .headers()
                    .get("x-ratelimit-remaining")
                    .and_then(|value| value.to_str().ok())
                    .is_some_and(|value| value.trim() == "0"),
                reset_at: response
                    .headers()
                    .get("x-ratelimit-reset")
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.trim().parse::<u64>().ok()),
                retry_after: response
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.trim().parse::<u64>().ok()),
            };
            let body = response.text().await.unwrap_or_default();
            let signal = crate::api_rate_gate::RateLimitSignal {
                body_limit_message: crate::api_rate_gate::body_reports_rate_limit(&body),
                ..signal
            };
            return Err(ReleaseError::RateLimited {
                status,
                signal: Box::new(signal),
            });
        }
        return Err(ReleaseError::Status(status));
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

#[cfg(test)]
mod test_support;
#[cfg(test)]
pub use test_support::fetch_release_api;

#[cfg(test)]
mod tests;
