use super::*;
use crate::archive::DownloadRoute;
use crate::network_policy::{Access, Intent, Proxy, TokenTransport};

fn proxy(id: &str, template: &str) -> Proxy {
    Proxy {
        id: id.into(),
        name: id.into(),
        provider: Provider::Github,
        intent: Intent::Release,
        access: Access::Public,
        template: template.into(),
        token_transport: TokenTransport::None,
        description: String::new(),
        enabled: true,
        regions: Vec::new(),
    }
}

fn candidates() -> Vec<(usize, DownloadCandidate)> {
    (0..5)
        .map(|id| {
            let url = format!("https://host{id}.test/asset.zip?key=secret");
            let identity = RouteIdentity::for_candidate(
                "cn",
                &Provider::Github,
                HistoryPurpose::Release,
                &url,
                None,
            );
            (
                id,
                DownloadCandidate {
                    route: DownloadRoute::Direct,
                    url,
                    identity,
                },
            )
        })
        .collect()
}

fn success(session: &RaceSession, candidate: &DownloadCandidate, transfer_ms: u64) {
    let identity = candidate.identity.as_ref().unwrap();
    assert!(session.started(identity, false));
    session.succeeded(
        identity,
        1024 * 1024,
        Some(Duration::from_millis(42)),
        Duration::from_millis(transfer_ms),
    );
}

#[test]
fn route_key_ignores_asset_and_sensitive_url_parts_but_not_origin_or_template() {
    let direct = |url, purpose| {
        RouteIdentity::for_candidate("cn", &Provider::Github, purpose, url, None).unwrap()
    };
    let template = |proxy: &Proxy| {
        RouteIdentity::for_candidate(
            "cn",
            &Provider::Github,
            HistoryPurpose::Release,
            "https://public.test",
            Some(proxy),
        )
        .unwrap()
    };
    let first = direct(
        "https://gh.test/users/mod.zip?signature=secret",
        HistoryPurpose::Release,
    );
    assert_eq!(
        first,
        direct("https://gh.test/other/next.zip", HistoryPurpose::Release)
    );
    assert_ne!(
        first,
        direct("https://gh.test:83/release.zip", HistoryPurpose::Release)
    );
    assert_ne!(
        first,
        direct("https://gh.test/release.zip", HistoryPurpose::Raw)
    );
    assert_ne!(
        first,
        direct("https://gh.test/release.zip", HistoryPurpose::Installer)
    );
    assert_eq!(
        first.with_purpose(HistoryPurpose::Framework),
        direct("https://gh.test/large.zip", HistoryPurpose::Framework)
    );
    assert_ne!(
        template(&proxy("mirror", "https://one.test/{url}")),
        template(&proxy("mirror", "https://one.test/new-template/{url}"))
    );
    assert_ne!(
        template(&proxy("mirror", "https://one.test/{url}")),
        template(&proxy("other", "https://one.test/{url}"))
    );
}

#[test]
fn uncommitted_or_expired_samples_never_reorder_the_authorized_pool() {
    let workspace = tempfile::tempdir().unwrap();
    let history = Arc::new(DownloadHistory::new(workspace.path().into()));
    let mut candidates = candidates();
    let candidate = candidates[4].1.clone();
    let identity = candidate.identity.as_ref().unwrap();
    let session = history.session();
    success(&session, &candidate, 80);
    assert!(session.order(&mut candidates, None).is_none());
    assert_eq!(candidates[0].0, 0);
    assert!(!session.history_preferred(identity));
    session.commit();
    let session = history.session();
    assert!(session.order(&mut candidates, None).is_none());
    assert_eq!(candidates[0].0, 4);
    assert!(session.history_preferred(identity));
    lock(&history.ledger)
        .entries
        .get_mut(&identity.key())
        .unwrap()
        .timing
        .as_mut()
        .unwrap()
        .verified_at = now_ms() - HISTORY_LIFETIME_MS;
    candidates.sort_by_key(|(id, _)| *id);
    assert!(history.session().order(&mut candidates, None).is_none());
    assert_eq!(candidates[0].0, 0);
    assert_ne!(lock(&history.ledger).entries[&identity.key()].tried_at, 0);
}

#[test]
fn fourth_real_race_reserves_one_probe_across_volumes_and_retries() {
    let workspace = tempfile::tempdir().unwrap();
    let history = Arc::new(DownloadHistory::new(workspace.path().into()));
    for _ in 0..3 {
        let session = history.session();
        let mut plan = candidates();
        assert!(session.order(&mut plan, None).is_none());
        assert!(session.order(&mut plan, None).is_none());
        success(&session, &plan[0].1, 80);
        session.commit();
    }
    let session = history.session();
    let mut first = candidates();
    assert_eq!(session.order(&mut first, None), Some(1));
    let mut second = candidates();
    assert_eq!(session.order(&mut second, None), Some(1));
    assert!(session.started(first[1].1.identity.as_ref().unwrap(), true));
    assert!(!session.started(second[1].1.identity.as_ref().unwrap(), true));
    assert_eq!(session.order(&mut candidates(), None), None);
    assert_ne!(
        lock(&history.ledger).entries[&first[1].1.identity.as_ref().unwrap().key()].tried_at,
        0
    );
    for _ in 0..3 {
        assert_eq!(history.session().order(&mut candidates(), None), None);
    }
    assert_eq!(history.session().order(&mut candidates(), None), Some(2));
}

#[test]
fn reload_uses_only_complete_records_without_urls_and_respects_cache_root() {
    let first_root = tempfile::tempdir().unwrap();
    let second_root = tempfile::tempdir().unwrap();
    let history = Arc::new(DownloadHistory::new(first_root.path().into()));
    let first = history.session();
    let second = history.session();
    let known = candidates();
    success(&first, &known[4].1, 100);
    success(&second, &known[3].1, 300);
    second.commit();
    first.commit();
    history.flush();
    let stored = fs::read_to_string(&history.path).unwrap();
    for forbidden in ["https://", "asset.zip", "secret"] {
        assert!(!stored.contains(forbidden));
    }
    assert!(!stored.contains(&format!("host{}.test", 3)));
    let reloaded = Arc::new(DownloadHistory::new(first_root.path().into()));
    let isolated = Arc::new(DownloadHistory::new(second_root.path().into()));
    let mut plan = candidates();
    reloaded.session().order(&mut plan, None);
    assert_eq!(plan[0].0, 3);
    let mut authorized = candidates();
    authorized.retain(|(id, _)| *id != 3);
    reloaded.session().order(&mut authorized, None);
    assert_eq!(authorized[0].0, 4);
    plan.sort_by_key(|(id, _)| *id);
    isolated.session().order(&mut plan, None);
    assert_eq!(plan[0].0, 0);

    let session = reloaded.session();
    success(&session, &known[2].1, 500);
    session.commit();
    reloaded.flush();
    let mut plan = candidates();
    Arc::new(DownloadHistory::new(first_root.path().into()))
        .session()
        .order(&mut plan, None);
    assert_eq!(plan[0].0, 2);

    fs::write(&history.path, "not JSON").unwrap();
    assert!(
        lock(&DownloadHistory::new(first_root.path().into()).ledger)
            .entries
            .is_empty()
    );
}

#[test]
fn abandoning_a_run_persists_attempts_but_not_uncommitted_timing() {
    let workspace = tempfile::tempdir().unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let history = Arc::new(DownloadHistory::new(workspace.path().into()));
    let mut pool = candidates();
    let identity = pool[4].1.identity.clone().unwrap();
    runtime.block_on(async {
        let session = history.session();
        assert_eq!(session.order(&mut pool, None), None);
        success(&session, &pool[4].1, 100);
        assert_eq!(session.verified_speed(&identity), None);
    });
    drop(runtime);
    let persisted = read(&history.path).unwrap();
    let entry = &persisted.entries[&identity.key()];
    assert_ne!(entry.tried_at, 0);
    assert!(entry.timing.is_none());
}

#[test]
fn latest_multipart_winner_becomes_preferred_only_after_commit() {
    let workspace = tempfile::tempdir().unwrap();
    let history = Arc::new(DownloadHistory::new(workspace.path().into()));
    let session = history.session();
    let mut plan = candidates();
    success(&session, &plan[3].1, 80);
    success(&session, &plan[1].1, 300);
    assert_eq!(history.session().order(&mut plan, None), None);
    assert_eq!(plan[0].0, 0);

    session.commit();
    assert_eq!(history.session().order(&mut plan, None), None);
    assert_eq!(plan[0].0, 1);
}

#[test]
fn backup_ranking_estimates_completion_or_uses_verified_speed_then_latency() {
    let workspace = tempfile::tempdir().unwrap();
    let history = Arc::new(DownloadHistory::new(workspace.path().into()));
    let session = history.session();
    let plan = candidates();
    for (index, wait_ms, transfer_ms) in [
        (1, 10, 1_000),
        (3, 200, 100),
        (4, 150, 100),
        (2, 500, 2_000),
    ] {
        let identity = plan[index].1.identity.as_ref().unwrap();
        assert!(session.started(identity, false));
        session.succeeded(
            identity,
            1024 * 1024,
            Some(Duration::from_millis(wait_ms)),
            Duration::from_millis(transfer_ms),
        );
    }
    session.commit();
    for (expected, order) in [
        (Some(1024), [2, 1, 4, 3, 0]),
        (Some(20 * 1024 * 1024), [2, 4, 3, 1, 0]),
        (None, [2, 4, 3, 1, 0]),
    ] {
        let mut plan = plan.clone();
        assert!(history.session().order(&mut plan, expected).is_none());
        assert_eq!(
            plan.into_iter().map(|(id, _)| id).collect::<Vec<_>>(),
            order
        );
    }
}
