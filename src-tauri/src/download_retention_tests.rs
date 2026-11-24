use super::*;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Live(Arc<AtomicUsize>);
impl Drop for Live {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

async fn retained_sources(count: usize) {
    let live = Arc::new(AtomicUsize::new(0));
    let max_live = Arc::new(AtomicUsize::new(0));
    let observed = Arc::new(Mutex::new(Vec::new()));
    let result = race_candidates(
        (0..count)
            .map(|i| DownloadCandidate {
                route: DownloadRoute::Proxy,
                url: format!("https://source{i}.example/file"),
                identity: None,
            })
            .collect(),
        || false,
        |candidate, _| {
            let live = live.clone();
            let max_live = max_live.clone();
            let observed = observed.clone();
            async move {
                let current = live.fetch_add(1, Ordering::SeqCst) + 1;
                max_live.fetch_max(current, Ordering::SeqCst);
                let _guard = Live(live.clone());
                let winner = count - 1;
                if candidate.url == format!("https://source{winner}.example/file") {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    observed.lock().unwrap().push(live.load(Ordering::SeqCst));
                    Ok(vec![42])
                } else {
                    std::future::pending::<Result<Vec<u8>, String>>().await
                }
            }
        },
        None,
        RacePolicy {
            window: Duration::from_millis(30),
            tick: Duration::from_millis(2),
            ..RacePolicy::default()
        },
        |_| {},
    );
    let result = tokio::time::timeout(Duration::from_secs(3), result)
        .await
        .expect("the retained candidate must be allowed to finish")
        .unwrap();
    assert_eq!(result, vec![42]);
    assert_eq!(*observed.lock().unwrap(), vec![count.min(3)]);
    assert_eq!(max_live.load(Ordering::SeqCst), count.min(3));
    assert_eq!(
        live.load(Ordering::SeqCst),
        0,
        "winner cleanup must drop every loser"
    );
}

#[tokio::test]
async fn final_one_two_and_three_sources_survive_multiple_slow_windows() {
    for count in 1..=3 {
        retained_sources(count).await;
    }
}

#[tokio::test(start_paused = true)]
async fn four_candidates_replace_only_one_laggard_and_keep_the_final_three() {
    let started = AtomicUsize::new(0);
    let active = Arc::new(AtomicUsize::new(0));
    let race = race_candidates(
        (0..4)
            .map(|id| DownloadCandidate {
                route: DownloadRoute::Proxy,
                url: format!("https://source{id}.example/file"),
                identity: None,
            })
            .collect(),
        || false,
        |candidate, reporter| {
            started.fetch_add(1, Ordering::SeqCst);
            let active = active.clone();
            async move {
                let alive = active.fetch_add(1, Ordering::SeqCst) + 1;
                let _guard = Live(active.clone());
                assert!(alive <= 3);
                if candidate.url.contains("source3") {
                    reporter.received(1);
                    // Even after a full window, the last spare must not leave
                    // another slot empty or abort the now-stalled replacement.
                    tokio::time::sleep(Duration::from_millis(120)).await;
                    assert_eq!(active.load(Ordering::SeqCst), 3);
                    return Ok(vec![42]);
                }
                let bytes = if candidate.url.contains("source0") {
                    8192
                } else {
                    16
                };
                loop {
                    reporter.received(bytes);
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }
        },
        None,
        RacePolicy {
            window: Duration::from_millis(30),
            tick: Duration::from_millis(2),
            ..RacePolicy::default()
        },
        |_| {},
    );
    let winner = tokio::time::timeout(Duration::from_secs(2), race)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(winner, [42]);
    assert_eq!(started.load(Ordering::SeqCst), 4);
    assert_eq!(active.load(Ordering::SeqCst), 0);
}
