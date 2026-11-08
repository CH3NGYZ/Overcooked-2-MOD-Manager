use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

async fn run(
    cache: &RepoReleaseCache,
    key: &str,
    pass_id: Option<&str>,
    calls: &AtomicUsize,
) -> Arc<Vec<ReleaseInfo>> {
    cache
        .list(key.to_string(), pass_id, false, || async {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(Vec::new())
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn second_read_without_pass_reuses_the_ttl_entry() {
    let cache = RepoReleaseCache::default();
    let calls = AtomicUsize::new(0);
    let first = run(&cache, "github:a:b", None, &calls).await;
    let second = run(&cache, "github:a:b", None, &calls).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(Arc::ptr_eq(&first, &second));
}

#[tokio::test]
async fn same_pass_requests_once() {
    /*
     * 一次点击里「读源树（校验 TOML）→ 逐条刷 Release」会先后要同一份
     * 列表，必须是同一次 API 请求 —— 这是用户明确要求的「点一次只打一次」。
     */
    let cache = RepoReleaseCache::default();
    let calls = AtomicUsize::new(0);
    let first = run(&cache, "github:a:b", Some("p1"), &calls).await;
    let second = run(&cache, "github:a:b", Some("p1"), &calls).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(Arc::ptr_eq(&first, &second));
}

#[tokio::test]
async fn cached_result_from_a_scan_is_reused_by_a_later_pass() {
    /*
     * 启动时没有 pass 的那次读取，与随后用户点刷新（新 pass）之间的间隔
     * 通常不到一秒：两次请求同一份刚拿到的列表没有信息增量，只会烧额度。
     */
    let cache = RepoReleaseCache::default();
    let calls = AtomicUsize::new(0);
    run(&cache, "github:a:b", None, &calls).await;
    run(&cache, "github:a:b", Some("p1"), &calls).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn different_repositories_do_not_share_entries() {
    let cache = RepoReleaseCache::default();
    let calls = AtomicUsize::new(0);
    run(&cache, "github:a:one", None, &calls).await;
    run(&cache, "github:a:two", None, &calls).await;
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn failure_is_not_cached() {
    let cache = RepoReleaseCache::default();
    let calls = AtomicUsize::new(0);
    let failed = cache
        .list("github:a:b".to_string(), None, false, || async {
            calls.fetch_add(1, Ordering::SeqCst);
            // 故意用一个非 tag 的失败串：缓存只关心「失败就不写」，与用户可见文案无关
            Err("boom".to_string())
        })
        .await;
    assert!(failed.is_err());
    run(&cache, "github:a:b", None, &calls).await;
    assert_eq!(calls.load(Ordering::SeqCst), 2, "失败不能挡住下一次重试");
}

#[test]
fn repository_cache_key_normalizes_case() {
    assert_eq!(
        crate::releases::repository_cache_key(
            crate::models::Provider::Github,
            " CH3NGYZ ",
            "Overcooked-2-MOD-Manager-Repos"
        ),
        crate::releases::repository_cache_key(
            crate::models::Provider::Github,
            "ch3ngyz",
            "overcooked-2-mod-manager-repos"
        )
    );
}

#[test]
fn repository_cache_key_separates_providers() {
    assert_ne!(
        crate::releases::repository_cache_key(crate::models::Provider::Github, "a", "b"),
        crate::releases::repository_cache_key(crate::models::Provider::Gitee, "a", "b")
    );
}

#[tokio::test]
async fn forced_refresh_replaces_old_snapshot_and_keeps_same_pass_single_flight() {
    let cache = RepoReleaseCache::default();
    let calls = AtomicUsize::new(0);
    let key = "gitcode:a:b".to_string();
    let old = run(&cache, &key, Some("old-pass"), &calls).await;
    let read = || {
        cache.list(key.clone(), Some("new-pass"), true, || async {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(Vec::new())
        })
    };
    let (first, concurrent) = tokio::join!(read(), read());
    let first = first.unwrap();
    assert!(
        !Arc::ptr_eq(&old, &first),
        "force must discard the previous TTL snapshot"
    );
    assert!(Arc::ptr_eq(&first, &concurrent.unwrap()));
    let later = run(&cache, &key, None, &calls).await;
    assert!(
        Arc::ptr_eq(&first, &later),
        "ordinary reads must see the replacement"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn forced_refresh_without_pass_never_reuses_ttl() {
    let cache = RepoReleaseCache::default();
    let calls = AtomicUsize::new(0);
    for _ in 0..2 {
        cache
            .list("github:a:b".into(), None, true, || async {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(Vec::new())
            })
            .await
            .unwrap();
    }
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}
