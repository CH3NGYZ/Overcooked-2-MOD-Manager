use super::error::ReleaseError;
use super::error::release_error_kind;
use super::projection::parse_release_json;
use crate::api_rate_gate::{ApiRateGate, GateKind, RateLimitSignal, body_reports_rate_limit};
use crate::api_scheduler::ApiRequestExt;
use crate::models::Provider;
use crate::models::ReleaseInfo;
use crate::network_policy::{NetworkPolicy, Step};
use reqwest::Client;
use reqwest::StatusCode;
use serde_json::Value;

const REPOSITORY_API_ATTEMPTS: usize = 3;

fn gate_kind<'a>(official_url: &str, target_url: &'a str, token: Option<&str>) -> GateKind<'a> {
    if target_url != official_url {
        GateKind::Proxy(target_url)
    } else if token.is_some() {
        GateKind::Token
    } else {
        GateKind::Anonymous
    }
}

pub(super) fn api_target_kind(
    official_url: &str,
    target_url: &str,
    request_token: Option<&str>,
) -> &'static str {
    gate_kind(official_url, target_url, request_token).label()
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

pub(super) struct RepositoryApiTarget<'a> {
    url: &'a str,
    provider: &'a Provider,
    token: Option<&'a str>,
    gate: Option<(&'a ApiRateGate, GateKind<'a>)>,
}

impl<'a> RepositoryApiTarget<'a> {
    pub(super) fn new(
        official: &str,
        url: &'a str,
        provider: &'a Provider,
        token: Option<&'a str>,
        gate: Option<&'a ApiRateGate>,
    ) -> Self {
        Self {
            url,
            provider,
            token,
            gate: gate.map(|gate| (gate, gate_kind(official, url, token))),
        }
    }

    fn check(&self) -> Result<(), ReleaseError> {
        if let Some((gate, kind)) = self.gate
            && let Err(wait) = gate.check(self.provider, kind)
        {
            log::warn!(
                "Release API 被限流闸门跳过: provider={:?} channel={} 剩余等待={}秒",
                self.provider,
                kind.label(),
                wait.as_secs()
            );
            return Err(ReleaseError::Cooldown);
        }
        Ok(())
    }

    fn record_limit(&self, status: StatusCode, signal: &RateLimitSignal) {
        if let Some((gate, kind)) = self.gate
            && let Some(block) = gate.record_limit(self.provider, kind, status, signal)
        {
            log::warn!(
                "Release API 触发限流锁: provider={:?} channel={} 锁定={}秒",
                self.provider,
                kind.label(),
                block.as_secs()
            );
        }
    }
}

pub(super) async fn fetch_release_api_with_retries(
    client: &Client,
    target: RepositoryApiTarget<'_>,
) -> Result<(Vec<ReleaseInfo>, usize), ReleaseError> {
    let provider = target.provider;
    fetch_api_with_retries(client, target, |value| {
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
    target: RepositoryApiTarget<'_>,
    parse: impl Fn(&Value) -> Result<T, ReleaseError>,
) -> Result<T, ReleaseError> {
    let provider = target.provider;
    let mut last_error = None;
    for attempt in 0..REPOSITORY_API_ATTEMPTS {
        log::debug!(
            "Release API 请求: provider={:?} attempt={}/{} token_attached={}",
            provider,
            attempt + 1,
            REPOSITORY_API_ATTEMPTS,
            target.token.is_some()
        );
        match fetch_api_json(client, &target)
            .await
            .and_then(|value| parse(&value))
        {
            Ok(result) => {
                if let Some((gate, kind)) = target.gate {
                    gate.record_success(provider, kind);
                }
                log::debug!(
                    "API 请求成功: provider={:?} attempt={}",
                    provider,
                    attempt + 1
                );
                return Ok(result);
            }
            Err(ReleaseError::Cooldown) => return Err(ReleaseError::Cooldown),
            Err(error)
                if error.is_forbidden()
                    || *provider == Provider::Gitcode && error.is_too_many_requests() =>
            {
                if let ReleaseError::RateLimited { status, signal } = &error {
                    target.record_limit(*status, signal);
                }
                log::warn!(
                    "Release API 停止当前目标重试: provider={:?} attempt={} error_kind={}",
                    provider,
                    attempt + 1,
                    release_error_kind(&error)
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
    if let ReleaseError::RateLimited { status, signal } = &error {
        target.record_limit(*status, signal);
    }
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
    target: &RepositoryApiTarget<'_>,
) -> Result<Value, ReleaseError> {
    let RepositoryApiTarget {
        url,
        provider,
        token,
        ..
    } = *target;
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
    let response = request
        .send_api_checked(provider.clone(), || target.check())
        .await?;
    log::trace!(
        "收到 Release API 响应: provider={:?} status={}",
        provider,
        response.status()
    );
    let status = response.status();
    if !status.is_success() {
        // 403/429 可能是限流：把响应头与正文交给闸门判定，并上报给调用方
        if status == StatusCode::FORBIDDEN || status == StatusCode::TOO_MANY_REQUESTS {
            let mut signal = RateLimitSignal::from_headers(provider, status, response.headers());
            if *provider != Provider::Gitcode || status != StatusCode::TOO_MANY_REQUESTS {
                signal.body_limit_message =
                    body_reports_rate_limit(&response.text().await.unwrap_or_default());
            }
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
