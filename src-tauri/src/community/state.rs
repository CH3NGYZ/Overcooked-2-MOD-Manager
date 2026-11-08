use crate::models::Provider;
use reqwest::StatusCode;
use reqwest::header::HeaderMap;
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::time::Duration;
use std::time::Instant;

const SEARCH_INTERVAL: Duration = Duration::from_millis(6100);

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
    pub(super) fn for_provider(&self, provider: &Provider) -> &SearchState {
        match provider {
            Provider::Github => &self.github,
            Provider::Gitee => &self.gitee,
        }
    }
}

pub(super) struct SearchState {
    pub(super) limits: tokio::sync::Mutex<SearchRateLimits>,
    pub(super) cancelled: AtomicBool,
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
            Provider::Github => "community.searchRateLimited",
            Provider::Gitee => "community.giteeWebRateLimited",
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

#[cfg(test)]
mod test_support;
#[cfg(test)]
pub(super) mod tests;
