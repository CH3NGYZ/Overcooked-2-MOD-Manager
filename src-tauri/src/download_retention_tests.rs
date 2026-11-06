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
                // The first three stay below the speed floor for multiple windows.
                // Four candidates must replace just one connection, not all three.
                let winner = if count == 4 { 3 } else { count - 1 };
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

#[tokio::test]
async fn four_candidates_replace_only_one_slow_source_and_keep_the_final_three() {
    retained_sources(4).await;
}
