use super::*;

#[test]
fn workers_fill_all_slots_and_refill_before_the_slow_task_finishes() {
    use std::sync::{Condvar, Mutex};
    use std::time::Duration;
    // The first task cannot finish until a later task (outside the first wave)
    // has run. A chunked executor or serial implementation fails with a timeout.
    let signal = (Mutex::new(false), Condvar::new());
    let first_wave = std::sync::Barrier::new(4);
    let results = run_bounded(&(0..12).collect::<Vec<_>>(), 4, |index, _| {
        if index < 4 {
            first_wave.wait();
        }
        if index == 0 {
            let (lock, wake) = &signal;
            let (ready, _) = wake
                .wait_timeout_while(lock.lock().unwrap(), Duration::from_secs(3), |ready| {
                    !*ready
                })
                .unwrap();
            assert!(
                *ready,
                "free workers must refill without waiting for the slow task"
            );
        }
        if index == 8 {
            *signal.0.lock().unwrap() = true;
            signal.1.notify_all();
        }
        index
    });
    assert!(results.iter().all(|(_, value)| value.is_some()));
}
#[test]
fn resolve_concurrency_treats_zero_as_all_cores_and_clamps_above() {
    assert_eq!(resolve_concurrency(0, 8), 8);
    assert_eq!(resolve_concurrency(4, 8), 4);
    assert_eq!(resolve_concurrency(1, 8), 1);
    assert_eq!(resolve_concurrency(99, 8), 8, "超过核心数按核心数算");
    assert_eq!(resolve_concurrency(0, 0), 1, "核心数兜底至少 1");
}

#[test]
fn run_bounded_never_exceeds_the_worker_limit() {
    let items: Vec<usize> = (0..16).collect();
    let in_flight = AtomicUsize::new(0);
    let peak = AtomicUsize::new(0);
    let results = run_bounded(&items, 4, |_, _| {
        let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        peak.fetch_max(now, Ordering::SeqCst);
        std::thread::sleep(std::time::Duration::from_millis(20));
        in_flight.fetch_sub(1, Ordering::SeqCst);
        true
    });
    assert!(peak.load(Ordering::SeqCst) <= 4, "并发峰值不得超过 4");
    assert_eq!(results.len(), 16);
    assert!(
        results
            .iter()
            .all(|(index, value)| value.is_some() && *index < 16)
    );
}

#[test]
fn run_bounded_keeps_request_order_and_isolates_panics() {
    let items: Vec<usize> = (0..6).collect();
    let results = run_bounded(&items, 3, |index, _| {
        if index == 2 {
            panic!("故意 panic");
        }
        std::thread::sleep(std::time::Duration::from_millis(10u64 * (6 - index) as u64));
        index
    });
    let indexes: Vec<usize> = results.iter().map(|(index, _)| *index).collect();
    assert_eq!(indexes, vec![0, 1, 2, 3, 4, 5], "必须按原下标升序返回");
    assert!(results[2].1.is_none(), "panic 的那一条只丢自己");
    assert!(results[0].1.is_some() && results[5].1.is_some());
}
