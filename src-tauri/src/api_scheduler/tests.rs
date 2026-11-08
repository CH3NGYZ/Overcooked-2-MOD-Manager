use super::*;

fn local_scheduler() -> Arc<Scheduler> {
    Arc::new(Scheduler {
        queue: Mutex::new(Queue::default()),
        changed: Notify::new(),
        window: Duration::from_millis(80),
    })
}

#[tokio::test]
async fn sliding_window_admits_three_then_queues_and_prioritizes_installed_across_domains() {
    let scheduler = local_scheduler();
    for _ in 0..3 {
        scheduler.acquire(String::new()).await;
    }
    let started = Arc::new(Mutex::new(Vec::new()));
    let ordinary = {
        let s = scheduler.clone();
        let result = started.clone();
        tokio::spawn(async move {
            s.acquire("ordinary".into()).await;
            result.lock().unwrap().push("ordinary");
            "ordinary"
        })
    };
    tokio::task::yield_now().await;
    installed().lock().unwrap().insert(
        "scheduler-test",
        ["bound-mod".into(), "bound-map".into()].into(),
    );
    let mut tasks = Vec::new();
    for key in ["bound-mod", "bound-map"] {
        let s = scheduler.clone();
        let result = started.clone();
        tasks.push(tokio::spawn(async move {
            s.acquire(key.into()).await;
            result.lock().unwrap().push(key);
        }));
    }
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(!ordinary.is_finished());
    for task in tasks {
        task.await.unwrap();
    }
    assert_eq!(ordinary.await.unwrap(), "ordinary");
    assert_eq!(
        *started.lock().unwrap(),
        vec!["bound-mod", "bound-map", "ordinary"]
    );
    installed().lock().unwrap().remove("scheduler-test");
    let q = scheduler.queue.lock().unwrap();
    assert!(q.waiting.is_empty());
    assert_eq!(q.started.len(), 3);
}

#[tokio::test]
async fn cancelled_waiters_are_removed_and_providers_have_independent_queues() {
    assert!(!Arc::ptr_eq(
        scheduler(&Provider::Github),
        scheduler(&Provider::Gitcode)
    ));
    let s = local_scheduler();
    for _ in 0..3 {
        s.acquire(String::new()).await;
    }
    let task = {
        let s = s.clone();
        tokio::spawn(async move { s.acquire(String::new()).await })
    };
    tokio::task::yield_now().await;
    assert_eq!(s.queue.lock().unwrap().waiting.len(), 1);
    task.abort();
    let _ = task.await;
    assert!(s.queue.lock().unwrap().waiting.is_empty());
}

#[tokio::test]
async fn seven_requests_obey_every_sliding_window_not_calendar_buckets() {
    let s = scheduler(&Provider::Gitcode);
    let mut starts = Vec::new();
    for _ in 0..7 {
        s.acquire(String::new()).await;
        starts.push(Instant::now());
    }
    for i in 3..starts.len() {
        assert!(starts[i].duration_since(starts[i - 3]) >= Duration::from_millis(999));
    }
    assert!(starts[6].duration_since(starts[0]) >= Duration::from_millis(1999));
}

#[test]
fn official_and_proxy_coordinates_share_installed_keys_including_unicode_paths() {
    for provider in [Provider::Github, Provider::Gitee, Provider::Gitcode] {
        let expected = repository_key(&provider, "维护者", "Map+Bundle&Tools");
        for address in [
            "https://api.example/repos/%E7%BB%B4%E6%8A%A4%E8%80%85/Map+Bundle&Tools/releases",
            "https://proxy.example/api/v5/repos/%E7%BB%B4%E6%8A%A4%E8%80%85/Map%2BBundle%26Tools/releases?page=1",
        ] {
            assert_eq!(
                request_key(&provider, &url::Url::parse(address).unwrap()),
                expected
            );
        }
    }
}
