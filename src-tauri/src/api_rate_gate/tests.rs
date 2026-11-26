use super::*;

fn github_signal(reset_in: u64) -> RateLimitSignal {
    RateLimitSignal {
        body_limit_message: true,
        remaining_exhausted: true,
        reset_at: Some(now_epoch_secs() + reset_in),
        retry_after: None,
    }
}

#[test]
fn gitee_forbidden_without_message_does_not_lock() {
    let gate = ApiRateGate::default();
    let signal = RateLimitSignal {
        body_limit_message: false,
        remaining_exhausted: true,
        reset_at: None,
        retry_after: None,
    };
    assert!(
        gate.record_limit(
            &Provider::Gitee,
            GateKind::Anonymous,
            StatusCode::FORBIDDEN,
            &signal
        )
        .is_none()
    );
    assert!(gate.check(&Provider::Gitee, GateKind::Anonymous).is_ok());
}

#[test]
fn gitee_rate_limit_locks_anonymous_for_five_minutes() {
    let gate = ApiRateGate::default();
    let signal = RateLimitSignal {
        body_limit_message: true,
        ..Default::default()
    };
    let block = gate
        .record_limit(
            &Provider::Gitee,
            GateKind::Anonymous,
            StatusCode::FORBIDDEN,
            &signal,
        )
        .expect("should lock");
    assert_eq!(block, GITEE_ANONYMOUS_BLOCK);
    let remaining = gate
        .remaining(&Provider::Gitee, GateKind::Anonymous)
        .expect("should be locked");
    assert!(remaining <= GITEE_ANONYMOUS_BLOCK);
    // Token 通道不受匿名锁影响
    assert!(gate.check(&Provider::Gitee, GateKind::Token).is_ok());
}

#[test]
fn github_uses_reset_header_for_lock_duration() {
    let gate = ApiRateGate::default();
    let block = gate
        .record_limit(
            &Provider::Github,
            GateKind::Anonymous,
            StatusCode::FORBIDDEN,
            &github_signal(30),
        )
        .expect("should lock");
    assert!(block >= Duration::from_secs(25) && block <= Duration::from_secs(35));
}

#[test]
fn github_retry_after_takes_priority() {
    let gate = ApiRateGate::default();
    let signal = RateLimitSignal {
        body_limit_message: true,
        remaining_exhausted: false,
        reset_at: Some(now_epoch_secs() + 600),
        retry_after: Some(3),
    };
    let block = gate
        .record_limit(
            &Provider::Github,
            GateKind::Anonymous,
            StatusCode::FORBIDDEN,
            &signal,
        )
        .expect("should lock");
    assert_eq!(block, Duration::from_secs(3));
}

#[test]
fn github_secondary_limit_backs_off_exponentially() {
    let gate = ApiRateGate::default();
    let signal = RateLimitSignal {
        body_limit_message: true,
        ..Default::default()
    };
    let first = gate
        .record_limit(
            &Provider::Github,
            GateKind::Anonymous,
            StatusCode::FORBIDDEN,
            &signal,
        )
        .unwrap();
    // 立刻再命中一次：退避时间应翻倍
    let second = gate
        .record_limit(
            &Provider::Github,
            GateKind::Anonymous,
            StatusCode::FORBIDDEN,
            &signal,
        )
        .unwrap();
    assert!(second > first, "second={second:?} first={first:?}");
}

#[test]
fn success_clears_lock_and_backoff() {
    let gate = ApiRateGate::default();
    gate.record_limit(
        &Provider::Github,
        GateKind::Anonymous,
        StatusCode::FORBIDDEN,
        &github_signal(60),
    );
    assert!(
        gate.remaining(&Provider::Github, GateKind::Anonymous)
            .is_some()
    );
    gate.record_success(&Provider::Github, GateKind::Anonymous);
    assert!(gate.check(&Provider::Github, GateKind::Anonymous).is_ok());
}

#[test]
fn github_proxy_requests_ignore_the_gate() {
    let gate = ApiRateGate::default();
    let proxy = GateKind::Proxy("https://proxy.example.invalid/repos/owner/repo/releases");
    let signal = RateLimitSignal {
        body_limit_message: true,
        ..Default::default()
    };
    assert!(
        gate.record_limit(&Provider::Github, proxy, StatusCode::FORBIDDEN, &signal)
            .is_none()
    );
    assert!(gate.check(&Provider::Github, proxy).is_ok());
}

#[test]
fn body_detection_matches_known_messages() {
    assert!(body_reports_rate_limit("Rate Limit Exceeded"));
    assert!(body_reports_rate_limit(
        "You have exceeded a secondary rate limit"
    ));
    assert!(!body_reports_rate_limit("Not Found"));
}

#[test]
fn gitcode_429_needs_no_extra_evidence_but_plain_403_does() {
    let gate = ApiRateGate::default();
    let signal = RateLimitSignal::default();
    let provider = Provider::Gitcode;
    assert_eq!(
        gate.record_limit(
            &provider,
            GateKind::Anonymous,
            StatusCode::FORBIDDEN,
            &signal
        ),
        None
    );
    assert_eq!(
        gate.record_limit(
            &provider,
            GateKind::Anonymous,
            StatusCode::TOO_MANY_REQUESTS,
            &signal
        ),
        Some(Duration::from_secs(60))
    );
    assert!(gate.check(&provider, GateKind::Anonymous).is_err());
    assert!(gate.check(&provider, GateKind::Token).is_ok());
    let proxy = GateKind::Proxy("https://proxy.example.invalid/repos/owner/repo/releases");
    assert!(gate.check(&provider, proxy).is_ok());
    assert!(gate.check(&Provider::Github, GateKind::Anonymous).is_ok());
}

#[test]
fn gitcode_proxy_cooldown_is_endpoint_specific_and_expires() {
    let gate = ApiRateGate::default();
    let provider = Provider::Gitcode;
    let proxy = GateKind::Proxy("https://proxy.example.invalid/repos/first/repo/releases?page=1");
    let same_proxy =
        GateKind::Proxy("https://proxy.example.invalid/repos/second/repo/releases?page=2");
    let another_proxy = GateKind::Proxy("https://other.example.invalid/repos/first/repo/releases");
    let signal = RateLimitSignal {
        retry_after: Some(120),
        ..Default::default()
    };
    assert_eq!(
        gate.record_limit(&provider, proxy, StatusCode::FORBIDDEN, &signal),
        None
    );
    assert_eq!(
        gate.record_limit(&provider, proxy, StatusCode::TOO_MANY_REQUESTS, &signal),
        Some(Duration::from_secs(120))
    );
    assert!(gate.check(&provider, same_proxy).is_err());
    assert!(gate.check(&provider, another_proxy).is_ok());
    assert!(gate.check(&provider, GateKind::Anonymous).is_ok());
    assert!(gate.check(&provider, GateKind::Token).is_ok());
    gate.state.lock().unwrap().gitcode_proxies.insert(
        proxy_origin("https://proxy.example.invalid").unwrap(),
        Instant::now() - Duration::from_secs(1),
    );
    assert!(gate.check(&provider, same_proxy).is_ok());
}

#[test]
fn gitcode_429_prefers_retry_after_then_future_reset_and_caps_explicit_hints() {
    let provider = Provider::Gitcode;
    let retry_date = (chrono::Utc::now() + chrono::TimeDelta::hours(48))
        .format("%a, %d %b %Y %H:%M:%S GMT")
        .to_string();
    let now = now_epoch_secs();
    let headers = |retry_after: Option<&str>, reset_at: u64| {
        let mut headers = HeaderMap::new();
        if let Some(value) = retry_after {
            headers.insert("retry-after", value.parse().unwrap());
        }
        headers.insert("x-ratelimit-reset", reset_at.to_string().parse().unwrap());
        headers
    };
    let record = |retry_after, reset_at| {
        let signal = RateLimitSignal::from_headers(
            &provider,
            StatusCode::TOO_MANY_REQUESTS,
            &headers(retry_after, reset_at),
        );
        ApiRateGate::default()
            .record_limit(
                &provider,
                GateKind::Token,
                StatusCode::TOO_MANY_REQUESTS,
                &signal,
            )
            .unwrap()
            .as_secs()
    };
    assert_eq!(record(Some("125"), now + 900), 125);
    assert_eq!(record(Some(&retry_date), now + 900), 86_400);
    assert_eq!(record(None, now + 200_000), 86_400);
    assert!((899..=900).contains(&record(Some("invalid"), now + 900)));
    assert_eq!(record(Some("0"), now - 1), 60);
}

#[test]
fn gitcode_late_replies_do_not_shorten_or_clear_an_active_cooldown() {
    let gate = ApiRateGate::default();
    let provider = Provider::Gitcode;
    let signal = RateLimitSignal {
        retry_after: Some(600),
        ..Default::default()
    };
    gate.record_limit(
        &provider,
        GateKind::Token,
        StatusCode::TOO_MANY_REQUESTS,
        &signal,
    );
    gate.record_limit(
        &provider,
        GateKind::Token,
        StatusCode::TOO_MANY_REQUESTS,
        &RateLimitSignal::default(),
    );
    gate.record_success(&provider, GateKind::Token);
    assert!(gate.remaining(&provider, GateKind::Token).unwrap() > Duration::from_secs(550));
    assert!(gate.check(&provider, GateKind::Anonymous).is_ok());
}

#[test]
fn gitcode_channel_reopens_after_deadline() {
    let gate = ApiRateGate::default();
    gate.state.lock().unwrap().gitcode_anonymous = Some(Instant::now() - Duration::from_secs(1));
    assert!(gate.check(&Provider::Gitcode, GateKind::Anonymous).is_ok());
}
