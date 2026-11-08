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
        gate.record_forbidden(&Provider::Gitee, GateKind::Anonymous, &signal)
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
        .record_forbidden(&Provider::Gitee, GateKind::Anonymous, &signal)
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
        .record_forbidden(&Provider::Github, GateKind::Anonymous, &github_signal(30))
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
        .record_forbidden(&Provider::Github, GateKind::Anonymous, &signal)
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
        .record_forbidden(&Provider::Github, GateKind::Anonymous, &signal)
        .unwrap();
    // 立刻再命中一次：退避时间应翻倍
    let second = gate
        .record_forbidden(&Provider::Github, GateKind::Anonymous, &signal)
        .unwrap();
    assert!(second > first, "second={second:?} first={first:?}");
}

#[test]
fn success_clears_lock_and_backoff() {
    let gate = ApiRateGate::default();
    gate.record_forbidden(&Provider::Github, GateKind::Anonymous, &github_signal(60));
    assert!(
        gate.remaining(&Provider::Github, GateKind::Anonymous)
            .is_some()
    );
    gate.record_success(&Provider::Github, GateKind::Anonymous);
    assert!(gate.check(&Provider::Github, GateKind::Anonymous).is_ok());
}

#[test]
fn proxy_requests_ignore_the_gate() {
    let gate = ApiRateGate::default();
    let signal = RateLimitSignal {
        body_limit_message: true,
        ..Default::default()
    };
    assert!(
        gate.record_forbidden(&Provider::Github, GateKind::Proxy, &signal)
            .is_none()
    );
    assert!(gate.check(&Provider::Github, GateKind::Proxy).is_ok());
}

#[test]
fn body_detection_matches_known_messages() {
    assert!(body_reports_rate_limit("Rate Limit Exceeded"));
    assert!(body_reports_rate_limit(
        "You have exceeded a secondary rate limit"
    ));
    assert!(!body_reports_rate_limit("Not Found"));
}
