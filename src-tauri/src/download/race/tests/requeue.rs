use super::*;

#[tokio::test(start_paused = true)]
async fn requeue_keeps_full_slots_and_runs_after_waiting_sources() {
    let candidates = ["failed", "leader", "reserve-one", "reserve-two"].map(candidate);
    let starts = RefCell::new(Vec::new());
    let attempts = Cell::new(0);
    let race_id = RefCell::new(None);
    let enqueued = Cell::new(false);
    let saw_queued = Cell::new(false);
    let result = archive::observe_download_progress(
        true,
        |_| {},
        race_candidates(
            candidates.into(),
            || false,
            |candidate, reporter| {
                let host = source_host(&candidate.url).unwrap();
                let route = host.strip_suffix(".example").unwrap().to_owned();
                starts.borrow_mut().push(route.clone());
                let first_attempt = route == "failed" && attempts.replace(attempts.get() + 1) == 0;
                async move {
                    match route.as_str() {
                        "failed" if first_attempt => {
                            receive(&reporter, 1);
                            tokio::time::sleep(Duration::from_millis(100)).await;
                            Err("download.httpStatus?status=503".into())
                        }
                        "failed" => {
                            assert_eq!(reporter.snapshot().downloaded, 0);
                            Ok(vec![42])
                        }
                        "leader" => loop {
                            receive(&reporter, 50);
                            tokio::time::sleep(Duration::from_millis(50)).await;
                        },
                        "reserve-two" => {
                            for _ in 0..10 {
                                receive(&reporter, 1);
                                tokio::time::sleep(Duration::from_millis(50)).await;
                            }
                            Err("download.firstByteTimeout".into())
                        }
                        _ => {
                            tokio::time::sleep(Duration::from_millis(500)).await;
                            Err("download.firstByteTimeout".into())
                        }
                    }
                }
            },
            Some(context(None, Some(10_000))),
            RacePolicy {
                parallelism: 2,
                window: Duration::from_millis(100),
                tick: Duration::from_millis(25),
                ..Default::default()
            },
            |event| {
                let race = event.race.unwrap();
                let source = &race.sources[0];
                assert_eq!(source.id, 1);
                if source.can_requeue && !enqueued.replace(true) {
                    assert_eq!(source.phase, DownloadSourcePhase::Failed);
                    assert_eq!(source.downloaded, 1);
                    assert_eq!(source.error.as_deref(), Some("download.httpStatus"));
                    assert_eq!(race.waiting, 1);
                    assert!(request(&race.id, RaceAction::Requeue(1)));
                    assert!(!request(&race.id, RaceAction::Requeue(1)));
                    race_id.replace(Some(race.id.clone()));
                }
                if source.phase == DownloadSourcePhase::Queued {
                    saw_queued.set(true);
                    assert!(event.downloaded >= 50);
                    assert_eq!(race.primary_id, Some(2));
                    assert_eq!(source.downloaded, 0);
                    assert!(source.error.is_none());
                    assert!(!source.can_requeue);
                    assert!(!request(&race.id, RaceAction::Requeue(1)));
                    assert_eq!(attempts.get(), 1);
                }
                if event.finished {
                    assert_eq!(race.primary_id, Some(1));
                    assert_eq!(race.waiting, 0);
                    assert!(!race.sources.iter().any(|row| row.id == 2));
                    assert!(
                        race.sources
                            .iter()
                            .any(|row| { row.id == 4 && row.phase == DownloadSourcePhase::Failed })
                    );
                    assert!(race.sources.iter().all(|row| !row.can_requeue));
                    assert!(!request(&race.id, RaceAction::Requeue(3)));
                }
            },
        ),
    )
    .await;
    assert_eq!(result, Ok(vec![42]));
    assert!(saw_queued.get());
    assert_eq!(attempts.get(), 2);
    assert_eq!(
        starts.into_inner(),
        ["failed", "leader", "reserve-one", "reserve-two", "failed"]
    );
    assert!(!request(
        &race_id.into_inner().unwrap(),
        RaceAction::Requeue(3)
    ));
}

#[tokio::test(start_paused = true)]
async fn a_slow_route_can_rejoin_while_locked_without_interrupting_the_leader() {
    let candidates = ["leader", "slow", "reserve"].map(candidate);
    let slow_url = candidates[1].url.clone();
    let reserve_url = candidates[2].url.clone();
    let slow_attempts = Cell::new(0);
    let reserve_attempts = Cell::new(0);
    let slow_dropped = Cell::new(false);
    let mut locked = false;
    let mut requeued = false;
    let mut unlocked = false;
    let tick = Duration::from_millis(25);
    let race = race_candidates(
        candidates.into(),
        || false,
        |candidate, reporter| {
            let slow_dropped = &slow_dropped;
            let is_slow = candidate.url == slow_url;
            let is_reserve = candidate.url == reserve_url;
            let slow_restart = is_slow && slow_attempts.replace(slow_attempts.get() + 1) > 0;
            let reserve_restart =
                is_reserve && reserve_attempts.replace(reserve_attempts.get() + 1) > 0;
            async move {
                if slow_restart {
                    assert!(slow_dropped.get());
                    assert_eq!(reporter.snapshot().downloaded, 0);
                    return Ok(vec![42]);
                }
                if is_reserve {
                    if reserve_restart {
                        tokio::time::sleep(tick).await;
                        return Err("download.firstByteTimeout".into());
                    }
                    return pending().await;
                }
                let _guard = is_slow.then(|| SetOnDrop(slow_dropped));
                loop {
                    receive(&reporter, if is_slow { 16 } else { 1024 * 4 });
                    tokio::time::sleep(tick).await;
                }
            }
        },
        Some(context(None, Some(1024 * 1024))),
        RacePolicy {
            parallelism: 2,
            window: Duration::from_millis(100),
            tick,
            ..RacePolicy::default()
        },
        |event| {
            let race = event.race.unwrap();
            let slow = race.sources.iter().find(|source| source.id == 2).unwrap();
            if !locked
                && race.can_lock
                && slow.phase == DownloadSourcePhase::Slow
                && race.sources.iter().any(|source| source.id == 3)
            {
                assert!(slow.can_requeue);
                assert!(slow.downloaded > 0);
                assert!(request(&race.id, RaceAction::Lock(1)));
                locked = true;
            } else if race.locked_id == Some(1) && !requeued {
                assert!(!race.sources.iter().any(|source| source.id == 3));
                assert_eq!(race.waiting, 1);
                assert!(slow.can_requeue);
                assert!(request(&race.id, RaceAction::Requeue(2)));
                requeued = true;
            } else if race.locked_id == Some(1)
                && slow.phase == DownloadSourcePhase::Queued
                && !unlocked
            {
                assert_eq!(race.waiting, 2);
                assert_eq!(race.primary_id, Some(1));
                assert_eq!(slow.downloaded, 0);
                assert!(!slow.can_requeue);
                assert!(request(&race.id, RaceAction::Unlock));
                unlocked = true;
            }
        },
    );
    let result = archive::observe_download_progress(true, |_| {}, async {
        tokio::time::timeout(Duration::from_secs(2), race)
            .await
            .unwrap()
    })
    .await;
    assert_eq!(result, Ok(vec![42]));
    assert!(locked && requeued && unlocked);
    assert_eq!(reserve_attempts.get(), 2);
    assert_eq!(slow_attempts.get(), 2);
}

#[tokio::test(start_paused = true)]
async fn a_pending_requeue_cannot_restart_a_cancelled_or_exhausted_race() {
    for cancel in [false, true] {
        let cancelled = Cell::new(false);
        let starts = Cell::new(0);
        let race_id = RefCell::new(None);
        let race = archive::observe_download_progress(
            true,
            |_| {},
            race_candidates(
                ["first", "second"].map(candidate).into(),
                || cancelled.get(),
                |candidate, _| {
                    starts.set(starts.get() + 1);
                    async move {
                        if candidate.url.contains("second") {
                            tokio::time::sleep(Duration::from_secs(1)).await;
                        }
                        Err::<Vec<u8>, _>("download.firstByteTimeout".into())
                    }
                },
                Some(context(None, None)),
                RacePolicy::default(),
                |event| {
                    let race = event.race.unwrap();
                    if cancel && race.sources[0].can_requeue {
                        assert!(request(&race.id, RaceAction::Requeue(1)));
                        cancelled.set(true);
                        race_id.replace(Some(race.id));
                    } else if !cancel
                        && race
                            .sources
                            .iter()
                            .all(|row| row.phase == DownloadSourcePhase::Failed)
                    {
                        assert!(race.sources.iter().all(|row| !row.can_requeue));
                        assert!(!request(&race.id, RaceAction::Requeue(1)));
                        race_id.replace(Some(race.id));
                    }
                },
            ),
        );
        let result = tokio::time::timeout(Duration::from_secs(2), race)
            .await
            .expect("cancelled or exhausted race must return to the caller");
        assert_eq!(
            result,
            Err(if cancel {
                "download.cancelled"
            } else {
                "download.firstByteTimeout"
            }
            .into())
        );
        assert_eq!(starts.get(), 2);
        assert!(!request(
            &race_id.into_inner().unwrap(),
            RaceAction::Requeue(1)
        ));
    }
}
