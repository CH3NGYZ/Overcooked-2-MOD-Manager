use super::*;
use crate::archive::{DownloadPhase, DownloadRoute};
use crate::download::control::{RaceAction, request};
use crate::download::history::{DownloadHistory, HistoryPurpose, RouteIdentity};
use crate::models::Provider;
use std::cell::{Cell, RefCell};
use std::time::Duration;

mod progress;
mod requeue;

struct SetOnDrop<'a>(&'a Cell<bool>);

impl Drop for SetOnDrop<'_> {
    fn drop(&mut self) {
        self.0.set(true);
    }
}

fn candidate(name: &str) -> DownloadCandidate {
    let url = format!("https://{name}.example/file");
    DownloadCandidate {
        route: DownloadRoute::Direct,
        identity: RouteIdentity::for_candidate(
            "cn",
            &Provider::Github,
            HistoryPurpose::Release,
            &url,
            None,
        ),
        url,
    }
}

fn context(session: Option<&RaceSession>, expected_size: Option<u64>) -> DownloadContext<'_> {
    DownloadContext {
        execution: None,
        label: "fixture",
        file_name: None,
        expected_size,
        race_session: session,
    }
}

fn receive(reporter: &DownloadReporter, bytes: u64) {
    reporter.received(bytes);
    let mut progress = reporter.snapshot();
    progress.downloaded += bytes;
    reporter.update(progress);
}

fn history_with_verified_preference(history: &Arc<DownloadHistory>, candidate: &DownloadCandidate) {
    let session = history.session();
    session.succeeded(
        candidate.identity.as_ref().unwrap(),
        1024 * 1024,
        Some(Duration::from_millis(10)),
        Duration::from_secs(1),
    );
    session.commit();
}

#[tokio::test(start_paused = true)]
async fn failure_of_the_scheduled_route_does_not_replace_a_healthy_display_leader() {
    let directory = tempfile::tempdir().unwrap();
    let history = Arc::new(DownloadHistory::new(directory.path().to_path_buf()));
    let primary = candidate("primary");
    history_with_verified_preference(&history, &primary);
    let session = history.session();
    let mut events = Vec::new();
    let result = race_candidates(
        vec![primary, candidate("slow-backup"), candidate("fast-backup")],
        || false,
        |candidate, reporter| async move {
            if candidate.url.contains("primary") {
                tokio::time::sleep(Duration::from_millis(21_500)).await;
                Err("download.firstByteTimeout".into())
            } else if candidate.url.contains("slow-backup") {
                receive(&reporter, 16_384);
                loop {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    receive(&reporter, 16);
                }
            } else {
                for _ in 0..24 {
                    receive(&reporter, 512);
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Ok(vec![42])
            }
        },
        Some(context(Some(&session), None)),
        RacePolicy::default(),
        |event| events.push(event),
    )
    .await
    .unwrap();
    assert_eq!(result, vec![42]);
    let takeover = events
        .iter()
        .filter(|event| !event.finished)
        .find_map(|event| {
            let race = event.race.as_ref().unwrap();
            race.sources
                .iter()
                .any(|row| row.id == 1 && row.phase == DownloadSourcePhase::Failed)
                .then_some(race)
        })
        .unwrap();
    let slower = takeover.sources.iter().find(|row| row.id == 2).unwrap();
    let faster = takeover.sources.iter().find(|row| row.id == 3).unwrap();
    assert!(slower.downloaded > faster.downloaded);
    assert!(slower.speed_bytes.unwrap() < faster.speed_bytes.unwrap());
    assert_eq!(takeover.primary_id, Some(2));
    assert_eq!(
        events.last().unwrap().race.as_ref().unwrap().primary_id,
        Some(3)
    );
}

#[tokio::test(start_paused = true)]
async fn a_third_route_waits_for_the_second_routes_own_soft_grace() {
    let directory = tempfile::tempdir().unwrap();
    let history = Arc::new(DownloadHistory::new(directory.path().to_path_buf()));
    let primary = candidate("primary");
    history_with_verified_preference(&history, &primary);
    let session = history.session();
    let started = Instant::now();
    let mut starts = Vec::new();
    let mut events = Vec::new();
    let result = race_candidates(
        vec![primary, candidate("second"), candidate("third")],
        || false,
        |candidate, _| {
            starts.push(started.elapsed().as_secs());
            async move {
                if candidate.url.contains("third") {
                    Ok(vec![3])
                } else {
                    pending().await
                }
            }
        },
        Some(context(Some(&session), None)),
        RacePolicy::default(),
        |event| events.push(event),
    )
    .await
    .unwrap();
    assert_eq!(result, vec![3]);
    assert_eq!(starts, [0, 10, 20]);
    assert!(
        events
            .iter()
            .take_while(|event| !event.finished)
            .all(|event| event.race.as_ref().unwrap().primary_id == Some(1))
    );
}

#[tokio::test(start_paused = true)]
async fn a_slower_recent_sample_and_a_body_stall_open_backup_without_aborting_primary() {
    for (steady, latest_expected_start) in [(true, 2500), (false, 3250)] {
        let directory = tempfile::tempdir().unwrap();
        let history = Arc::new(DownloadHistory::new(directory.path().to_path_buf()));
        let primary = candidate("primary");
        history_with_verified_preference(&history, &primary);
        let session = history.session();
        let started = Instant::now();
        let primary_dropped = Cell::new(false);
        let mut starts = Vec::new();
        let mut events = Vec::new();
        let result = race_candidates(
            vec![primary, candidate("backup")],
            || false,
            |candidate, reporter| {
                let primary_dropped = &primary_dropped;
                starts.push(started.elapsed().as_millis());
                async move {
                    if candidate.url.contains("primary") {
                        let _guard = SetOnDrop(primary_dropped);
                        receive(&reporter, 64 * 1024);
                        loop {
                            tokio::time::sleep(Duration::from_millis(250)).await;
                            if steady {
                                receive(&reporter, 64 * 1024);
                            }
                        }
                    } else {
                        receive(&reporter, 1024 * 1024);
                        reporter.verifying();
                        tokio::time::sleep(Duration::from_millis(300)).await;
                        assert!(!primary_dropped.get());
                        Ok(vec![4])
                    }
                }
            },
            Some(context(Some(&session), Some(1024 * 1024))),
            RacePolicy::default(),
            |event| events.push(event),
        )
        .await
        .unwrap();
        assert_eq!(result, vec![4]);
        assert_eq!(starts.len(), 2);
        assert!(
            (latest_expected_start - 250..=latest_expected_start).contains(&starts[1]),
            "{starts:?}"
        );
        assert!(
            events
                .iter()
                .any(|event| event.phase == DownloadPhase::Verifying
                    && event.race.as_ref().unwrap().primary_id == Some(2))
        );
        assert_eq!(
            events.last().unwrap().race.as_ref().unwrap().primary_id,
            Some(2)
        );
    }
}

#[tokio::test(start_paused = true)]
async fn sufficiently_sampled_slower_probe_can_return_after_primary_validation_fails() {
    let directory = tempfile::tempdir().unwrap();
    let history = Arc::new(DownloadHistory::new(directory.path().to_path_buf()));
    let primary = candidate("primary");
    let backup = candidate("backup");
    history_with_verified_preference(&history, &primary);
    let candidates = vec![primary, backup];
    for _ in 0..3 {
        let session = history.session();
        let mut plan = candidates.iter().cloned().enumerate().collect::<Vec<_>>();
        assert_eq!(session.order(&mut plan, None), None);
    }
    let session = history.session();
    let mut events = Vec::new();
    let backup_attempt = Cell::new(0);
    let result = race_candidates(
        candidates,
        || false,
        |candidate, reporter| {
            let primary = candidate.url.contains("primary");
            let retry = !primary && backup_attempt.replace(backup_attempt.get() + 1) > 0;
            async move {
                if primary {
                    for _ in 0..8 {
                        receive(&reporter, 1024 * 1024);
                        tokio::time::sleep(Duration::from_millis(250)).await;
                    }
                    reporter.verifying();
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    Err("download.checksumMismatch".to_owned())
                } else if retry {
                    receive(&reporter, 8 * 1024 * 1024);
                    Ok(vec![42])
                } else {
                    loop {
                        receive(&reporter, 128 * 1024);
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
            }
        },
        Some(context(Some(&session), Some(8 * 1024 * 1024))),
        RacePolicy::default(),
        |event| events.push(event),
    )
    .await
    .unwrap();
    assert_eq!(result, vec![42]);
    assert_eq!(backup_attempt.get(), 2);
    assert_eq!(events[0].race.as_ref().unwrap().sources.len(), 2);
    assert!(events.iter().any(|event| event.speed_bytes > 0));
    let checking = events
        .iter()
        .find(|event| event.phase == DownloadPhase::Verifying)
        .unwrap();
    assert_eq!(checking.speed_bytes, 0);
    assert!(
        checking
            .race
            .as_ref()
            .unwrap()
            .sources
            .iter()
            .any(|row| row.phase == DownloadSourcePhase::Verifying && row.speed_bytes.is_none())
    );
    assert!(events.iter().any(
        |event| event.race.as_ref().unwrap().sources.iter().any(|row| {
            row.id == 2 && row.phase == DownloadSourcePhase::Slow && row.downloaded >= 819_200
        })
    ));
    assert_eq!(
        events.last().unwrap().race.as_ref().unwrap().primary_id,
        Some(2)
    );
}

#[tokio::test(start_paused = true)]
async fn faster_probe_can_pass_validation_and_become_next_downloads_preference() {
    let directory = tempfile::tempdir().unwrap();
    let history = Arc::new(DownloadHistory::new(directory.path().to_path_buf()));
    let primary = candidate("primary");
    let preferred_id = primary.identity.clone().unwrap();
    let probe = candidate("probe");
    let probe_id = probe.identity.clone().unwrap();
    let candidates = vec![primary.clone(), probe];
    history_with_verified_preference(&history, &primary);
    for _ in 0..3 {
        let session = history.session();
        let mut plan = candidates.iter().cloned().enumerate().collect::<Vec<_>>();
        assert_eq!(session.order(&mut plan, None), None);
    }
    let session = history.session();
    let size = 8 * 1024 * 1024;
    let mut events = Vec::new();
    let result = race_candidates(
        candidates.clone(),
        || false,
        |candidate, reporter| async move {
            let primary = candidate.url.contains("primary");
            let chunk = if primary { 128 * 1024 } else { 1024 * 1024 };
            for _ in 0..size / chunk {
                receive(&reporter, chunk);
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            reporter.verifying();
            tokio::time::sleep(Duration::from_millis(300)).await;
            Ok(vec![if primary { 1 } else { 2 }])
        },
        Some(context(Some(&session), Some(size))),
        RacePolicy::default(),
        |event| events.push(event),
    )
    .await
    .unwrap();
    assert_eq!(result, vec![2]);
    assert_eq!(
        events.last().unwrap().race.as_ref().unwrap().primary_id,
        Some(2)
    );
    assert!(events.iter().any(
        |event| event.race.as_ref().unwrap().sources.iter().any(|row| {
            row.id == 2 && row.phase == DownloadSourcePhase::Verifying && row.downloaded == size
        })
    ));
    assert!(history.session().verified_speed(&probe_id).is_none());
    session.commit();
    let next = history.session();
    let mut reordered = candidates.into_iter().enumerate().collect::<Vec<_>>();
    next.order(&mut reordered, Some(size));
    assert_eq!(reordered[0].0, 1);
    assert!(next.verified_speed(&probe_id).unwrap() > next.verified_speed(&preferred_id).unwrap());
}

#[tokio::test(start_paused = true)]
async fn explicit_lock_pauses_on_failure_then_retries_or_unlocks_or_cancels() {
    for after_failure in [Some(RaceAction::Retry), Some(RaceAction::Unlock), None] {
        let cancelled = Cell::new(false);
        let preferred_attempts = Cell::new(0);
        let backup_attempts = Cell::new(0);
        let lock_id = RefCell::new(None);
        let failed_and_paused = Cell::new(false);
        let result = archive::observe_download_progress(
            true,
            |_| {},
            race_candidates(
                vec![
                    candidate("first"),
                    candidate("second"),
                    candidate("waiting"),
                ],
                || cancelled.get(),
                |candidate, reporter| {
                    let first = candidate.url.contains("first");
                    let retry = if first {
                        preferred_attempts.replace(preferred_attempts.get() + 1) > 0
                    } else {
                        backup_attempts.replace(backup_attempts.get() + 1) > 0
                    };
                    async move {
                        receive(&reporter, if first { 1 } else { 3 });
                        if first && !retry {
                            tokio::time::sleep(Duration::from_secs(1)).await;
                            Err("download.httpStatus?status=429".into())
                        } else if retry || candidate.url.contains("waiting") {
                            tokio::time::sleep(Duration::from_secs(2)).await;
                            Ok(vec![if first { 1 } else { 2 }])
                        } else {
                            pending().await
                        }
                    }
                },
                Some(context(None, None)),
                RacePolicy {
                    parallelism: 2,
                    ..RacePolicy::default()
                },
                |event| {
                    let race = event.race.as_ref().unwrap();
                    if lock_id.borrow().is_none()
                        && race.can_lock
                        && race.primary_id == Some(2)
                        && event.downloaded == 3
                    {
                        assert!(request(&race.id, RaceAction::Lock(1)));
                        lock_id.replace(Some(race.id.clone()));
                    }
                    if race.locked_id == Some(1) {
                        assert_eq!(race.primary_id, Some(1));
                        assert!(event.downloaded <= 1);
                        assert_eq!(race.sources.len(), 1);
                    }
                    if event.phase == DownloadPhase::LockedFailed
                        && !failed_and_paused.replace(true)
                    {
                        assert_eq!(backup_attempts.get(), 1);
                        assert_eq!(race.locked_id, Some(1));
                        assert_eq!(race.waiting, 2);
                        assert_eq!(
                            race.sources[0].error.as_deref(),
                            Some("download.httpStatus")
                        );
                        if let Some(action) = after_failure {
                            assert!(request(&race.id, action));
                        } else {
                            cancelled.set(true);
                        }
                    }
                },
            ),
        )
        .await;
        assert!(failed_and_paused.get());
        assert_eq!(
            result,
            match after_failure {
                Some(RaceAction::Retry) => Ok(vec![1]),
                Some(RaceAction::Unlock) => Ok(vec![2]),
                _ => Err("download.cancelled".to_owned()),
            }
        );
        assert!(!request(
            lock_id.into_inner().as_deref().unwrap(),
            RaceAction::Unlock
        ));
    }
}

#[tokio::test(start_paused = true)]
async fn immediate_unlock_waits_for_the_same_route_to_stop_but_not_other_routes() {
    for include_waiting in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let history = Arc::new(DownloadHistory::new(directory.path().to_path_buf()));
        let primary = candidate("primary");
        history_with_verified_preference(&history, &primary);
        let session = history.session();
        let mut candidates = vec![primary, candidate("backup")];
        if include_waiting {
            candidates.push(candidate("waiting"));
        }
        let first_backup_dropped = Cell::new(false);
        let backup_attempts = Cell::new(0);
        let waiting_started_before_backup_stopped = Cell::new(false);
        let lock_sent = Cell::new(false);
        let unlock_sent = Cell::new(false);
        let result = archive::observe_download_progress(
            true,
            |_| {},
            race_candidates(
                candidates,
                || false,
                |candidate, reporter| {
                    let first_backup_dropped = &first_backup_dropped;
                    let is_backup = candidate.url.contains("backup");
                    let first_backup =
                        is_backup && backup_attempts.replace(backup_attempts.get() + 1) == 0;
                    if candidate.url.contains("waiting") {
                        waiting_started_before_backup_stopped.set(!first_backup_dropped.get());
                    }
                    if is_backup && !first_backup {
                        assert!(
                            first_backup_dropped.get(),
                            "a route cannot restart before its aborted attempt is drained"
                        );
                    }
                    async move {
                        if first_backup {
                            let _guard = SetOnDrop(first_backup_dropped);
                            receive(&reporter, 1);
                            pending().await
                        } else if is_backup {
                            Ok(vec![42])
                        } else {
                            pending().await
                        }
                    }
                },
                Some(context(Some(&session), None)),
                RacePolicy::default(),
                |event| {
                    let race = event.race.unwrap();
                    if !lock_sent.get()
                        && race.can_lock
                        && race
                            .sources
                            .iter()
                            .any(|row| row.id == 2 && row.downloaded > 0)
                    {
                        assert!(request(&race.id, RaceAction::Lock(1)));
                        lock_sent.set(true);
                    } else if race.locked_id == Some(1) && !unlock_sent.replace(true) {
                        assert!(!first_backup_dropped.get());
                        assert_eq!(race.sources.len(), 1);
                        assert_eq!(race.waiting, if include_waiting { 2 } else { 1 });
                        assert!(request(&race.id, RaceAction::Unlock));
                    }
                },
            ),
        )
        .await
        .unwrap();
        assert_eq!(result, vec![42]);
        assert_eq!(backup_attempts.get(), 2);
        assert_eq!(waiting_started_before_backup_stopped.get(), include_waiting);
    }
}
