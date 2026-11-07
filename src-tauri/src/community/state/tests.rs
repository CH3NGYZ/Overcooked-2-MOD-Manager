use super::super::github::fetch_page;
use super::super::test_support::server;
use super::*;
use reqwest::Client;
use std::sync::atomic::Ordering;

pub(in crate::community) fn limits() -> SearchRateLimits {
    SearchRateLimits {
        interval: Duration::ZERO,
        ..Default::default()
    }
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
