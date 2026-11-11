use super::*;
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};

#[derive(Default)]
pub(super) struct Gate(Mutex<bool>, Condvar);
impl Gate {
    pub(super) fn wait(&self) {
        let (open, timed) = self
            .1
            .wait_timeout_while(self.0.lock().unwrap(), Duration::from_secs(5), |open| {
                !*open
            })
            .unwrap();
        assert!(*open && !timed.timed_out(), "worker release timed out");
    }
    pub(super) fn release(&self) {
        *self.0.lock().unwrap() = true;
        self.1.notify_all();
    }
}
fn seed(name: &str) -> Store {
    let mut store = Store::test_seed();
    store.default_template = name.into();
    store
}

#[test]
fn bounds_actual_operations_and_initializes_each_stamp_once() {
    for workers in [1, 2, 4] {
        let pool = Pool::new(crate::concurrency::resolve_concurrency(workers, 4));
        let initialized = AtomicUsize::new(0);
        let running = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let gate = Gate::default();
        let (entered, received) = mpsc::channel();
        std::thread::scope(|scope| {
            let mut tasks = vec![];
            for _ in 0..8 {
                let (pool, initialized, running, peak, gate, entered) =
                    (&pool, &initialized, &running, &peak, &gate, entered.clone());
                tasks.push(scope.spawn(move || {
                    pool.run(
                        0,
                        "same-stamp".into(),
                        || {
                            initialized.fetch_add(1, Ordering::SeqCst);
                            Ok(seed("native"))
                        },
                        |store| {
                            let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                            peak.fetch_max(now, Ordering::SeqCst);
                            entered.send(()).unwrap();
                            gate.wait();
                            assert_eq!(store.default_template, "native");
                            running.fetch_sub(1, Ordering::SeqCst);
                            Ok(())
                        },
                    )
                }));
            }
            let admitted = (0..workers)
                .map(|_| received.recv_timeout(Duration::from_secs(3)).is_ok())
                .all(|entered| entered);
            let extra = received.recv_timeout(Duration::from_millis(50)).is_ok();
            gate.release();
            for task in tasks {
                task.join().unwrap().unwrap();
            }
            assert!(admitted, "the configured workers must overlap");
            assert!(!extra, "a worker exceeded the configured bound");
        });
        assert_eq!(peak.load(Ordering::SeqCst), workers as usize);
        assert_eq!(initialized.load(Ordering::SeqCst), 1);
        let state = pool.state.lock().unwrap();
        assert_eq!(state.active, 0);
        assert!(state.idle.is_some());
        assert!(
            state
                .seed
                .as_ref()
                .unwrap()
                .blocks
                .shares_storage_with(&state.idle.as_ref().unwrap().blocks)
        );
    }
}

#[test]
fn successful_lease_is_reused_but_failed_mutable_state_is_discarded() {
    let pool = Pool::new(1);
    pool.run(
        0,
        "stamp".into(),
        || Ok(seed("seed")),
        |store| {
            store.default_template = "warm".into();
            Ok(())
        },
    )
    .unwrap();
    let failure: Result<(), String> = pool.run(
        0,
        "stamp".into(),
        || unreachable!(),
        |store| {
            assert_eq!(store.default_template, "warm");
            store.default_template = "partial".into();
            Err("expected read failure".into())
        },
    );
    assert!(failure.is_err());
    pool.run(
        0,
        "stamp".into(),
        || unreachable!(),
        |store| {
            assert_eq!(store.default_template, "seed");
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(pool.state.lock().unwrap().active, 0);
}

#[test]
fn initialization_errors_and_panics_release_the_permit_and_allow_retry() {
    let pool = Pool::new(1);
    let failed: Result<(), String> = pool.run(0, "stamp".into(), || Err("read".into()), |_| Ok(()));
    assert!(failed.is_err());
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        pool.run::<()>(0, "stamp".into(), || panic!("decoder"), |_| unreachable!())
    }));
    assert!(panicked.is_err());
    {
        let state = pool.state.lock().unwrap();
        assert_eq!(state.active, 0);
        assert!(!state.initializing);
    }
    pool.run(0, "stamp".into(), || Ok(seed("retry")), |_| Ok(()))
        .unwrap();
}

#[test]
fn operation_panic_discards_lease_without_poisoning_the_pool() {
    let pool = Pool::new(1);
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        pool.run::<()>(
            0,
            "stamp".into(),
            || Ok(seed("seed")),
            |store| {
                store.default_template = "partial".into();
                panic!("texture decoder");
            },
        )
    }));
    assert!(panicked.is_err());
    pool.run(
        0,
        "stamp".into(),
        || unreachable!(),
        |store| {
            assert_eq!(store.default_template, "seed");
            Ok(())
        },
    )
    .unwrap();
}

#[test]
fn old_completion_cannot_replace_the_new_resource_stamp() {
    let pool = Pool::new(2);
    let gate = Gate::default();
    let (entered, received) = mpsc::channel();
    std::thread::scope(|scope| {
        let old = scope.spawn(|| {
            pool.run::<()>(
                0,
                "old".into(),
                || Ok(seed("old")),
                |store| {
                    entered.send(()).unwrap();
                    gate.wait();
                    store.default_template = "old completed last".into();
                    Ok(())
                },
            )
        });
        received.recv_timeout(Duration::from_secs(3)).unwrap();
        pool.run(
            0,
            "new".into(),
            || Ok(seed("new")),
            |store| {
                assert_eq!(store.default_template, "new");
                store.default_template = "new warm".into();
                Ok(())
            },
        )
        .unwrap();
        gate.release();
        old.join().unwrap().unwrap();
    });
    pool.run(
        0,
        "new".into(),
        || unreachable!(),
        |store| {
            assert_eq!(store.default_template, "new warm");
            Ok(())
        },
    )
    .unwrap();
}

#[test]
fn invalidated_initializer_cannot_install_a_stale_seed() {
    let pool = Pool::new(2);
    let gate = Gate::default();
    let (entered, received) = mpsc::channel();
    std::thread::scope(|scope| {
        let old = scope.spawn(|| {
            pool.run::<()>(
                0,
                "old".into(),
                || {
                    entered.send(()).unwrap();
                    gate.wait();
                    Ok(seed("old"))
                },
                |_| panic!("invalidated seed must not reach conversion"),
            )
        });
        received.recv_timeout(Duration::from_secs(3)).unwrap();
        pool.run(0, "new".into(), || Ok(seed("new")), |_| Ok(()))
            .unwrap();
        gate.release();
        assert!(old.join().unwrap().is_err());
    });
    let state = pool.state.lock().unwrap();
    assert_eq!(state.active, 0);
    assert_eq!(state.seed.as_ref().unwrap().default_template, "new");
    assert!(!state.initializing);
}

#[test]
fn clearing_drops_shared_and_mutable_caches_and_rejects_late_work_from_the_old_epoch() {
    let pool = Pool::new(1);
    pool.run(
        0,
        "same-files".into(),
        || Ok(seed("original")),
        |store| {
            store.default_template = "warm".into();
            Ok(())
        },
    )
    .unwrap();
    let old_seed = Arc::downgrade(pool.state.lock().unwrap().seed.as_ref().unwrap());
    pool.clear(2);
    assert!(old_seed.upgrade().is_none());
    {
        let state = pool.state.lock().unwrap();
        assert!(state.seed.is_none() && state.idle.is_none());
        assert_eq!(state.active, 0);
    }
    assert!(
        pool.run::<()>(
            0,
            "same-files".into(),
            || panic!("old initializer"),
            |_| panic!("old conversion")
        )
        .is_err()
    );
    pool.run(
        2,
        "same-files".into(),
        || Ok(seed("fresh")),
        |store| {
            assert_eq!(store.default_template, "fresh");
            Ok(())
        },
    )
    .unwrap();
}

#[test]
fn clearing_active_workers_never_returns_old_mutable_or_shared_resources_to_the_new_pool() {
    let pool = Pool::new(2);
    let gate = Gate::default();
    let (entered, received) = mpsc::channel();
    std::thread::scope(|scope| {
        let old = scope.spawn(|| {
            pool.run(
                0,
                "same-files".into(),
                || Ok(seed("old")),
                |store| {
                    entered.send(()).unwrap();
                    gate.wait();
                    store.default_template = "old completed last".into();
                    Ok(())
                },
            )
        });
        received.recv_timeout(Duration::from_secs(3)).unwrap();
        pool.clear(2);
        assert_eq!(pool.state.lock().unwrap().active, 1);
        pool.run(
            2,
            "same-files".into(),
            || Ok(seed("new")),
            |store| {
                store.default_template = "new warm".into();
                Ok(())
            },
        )
        .unwrap();
        gate.release();
        old.join().unwrap().unwrap();
    });
    pool.run(
        2,
        "same-files".into(),
        || unreachable!(),
        |store| {
            assert_eq!(store.default_template, "new warm");
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(pool.state.lock().unwrap().active, 0);
}

#[test]
fn clearing_an_initializing_worker_cannot_restore_its_seed_after_the_clear_completes() {
    let pool = Pool::new(2);
    let gate = Gate::default();
    let (entered, received) = mpsc::channel();
    std::thread::scope(|scope| {
        let old = scope.spawn(|| {
            pool.run::<()>(
                0,
                "same-files".into(),
                || {
                    entered.send(()).unwrap();
                    gate.wait();
                    Ok(seed("old"))
                },
                |_| panic!("invalidated initializer must not convert"),
            )
        });
        received.recv_timeout(Duration::from_secs(3)).unwrap();
        pool.clear(2);
        pool.run(2, "same-files".into(), || Ok(seed("fresh")), |_| Ok(()))
            .unwrap();
        gate.release();
        assert!(old.join().unwrap().is_err());
    });
    let state = pool.state.lock().unwrap();
    assert_eq!(state.seed.as_ref().unwrap().default_template, "fresh");
    assert_eq!(state.active, 0);
    assert!(!state.initializing);
}

#[test]
#[ignore = "requires installed game resources; reads only, no game process"]
fn real_resource_workers_share_metadata_and_preserve_distinctive_parts() {
    let game = std::path::Path::new("E:/SteamLibrary/steamapps/common/Overcooked! 2");
    let paths = super::super::reader::sources(game);
    let stamp = crate::diychef::preview::fingerprint::files(&paths).unwrap();
    let pool = Pool::new(2);
    let initialized = AtomicUsize::new(0);
    let running = AtomicUsize::new(0);
    let peak = AtomicUsize::new(0);
    let started = std::time::Instant::now();
    std::thread::scope(|scope| {
        let tasks: Vec<_> = [("Chef_Racoon", "Wheelchair"), ("Chef_Ghost", "Accessories")]
            .into_iter()
            .map(|(name, special)| {
                let (pool, paths, stamp, initialized, running, peak) =
                    (&pool, &paths, &stamp, &initialized, &running, &peak);
                scope.spawn(move || {
                    pool.run(
                        0,
                        stamp.clone(),
                        || {
                            initialized.fetch_add(1, Ordering::SeqCst);
                            Store::new(paths.clone())
                        },
                        |store| {
                            let active = running.fetch_add(1, Ordering::SeqCst) + 1;
                            peak.fetch_max(active, Ordering::SeqCst);
                            let tree = super::super::tree(store, name)?;
                            let prepared = super::super::model_parts(store, &tree, name)?;
                            running.fetch_sub(1, Ordering::SeqCst);
                            assert!(prepared.diagnostics.is_empty());
                            assert!(prepared.parts.iter().any(|part| part.name == special));
                            assert!(prepared.parts.iter().any(|part| part.name == "Body"));
                            assert!(prepared.parts.iter().all(|part| part.name != "Knife"));
                            eprintln!("{name}: {} parts", prepared.parts.len());
                            Ok(())
                        },
                    )
                })
            })
            .collect();
        for task in tasks {
            task.join().unwrap().unwrap();
        }
    });
    assert_eq!(initialized.load(Ordering::SeqCst), 1);
    assert_eq!(peak.load(Ordering::SeqCst), 2);
    let state = pool.state.lock().unwrap();
    let seed = state.seed.as_ref().unwrap();
    let idle = state.idle.as_ref().unwrap();
    assert!(Arc::ptr_eq(&seed.docs[0], &idle.docs[0]));
    assert!(seed.blocks.shares_storage_with(&idle.blocks));
    eprintln!(
        "real native resource pool: initialized=1, operation peak=2, elapsed={:?}",
        started.elapsed()
    );
}
