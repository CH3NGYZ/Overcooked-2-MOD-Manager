use super::*;

#[tokio::test(start_paused = true)]
async fn progress_follows_the_backup_while_the_preferred_route_still_waits_for_bytes() {
    const TOTAL: u64 = 256_167_117;
    const LEAD: u64 = 168 * 1024 * 1024 + 512 * 1024;
    const SAMPLE: u64 = 11 * 1024 * 1024 + 720 * 1024;
    let directory = tempfile::tempdir().unwrap();
    let history = Arc::new(DownloadHistory::new(directory.path().to_path_buf()));
    let primary = candidate("primary");
    let backup = DownloadCandidate {
        route: DownloadRoute::Proxy,
        ..candidate("second")
    };
    history_with_verified_preference(&history, &primary);
    let session = history.session();
    let started = Instant::now();
    let mut events = Vec::new();
    let mut starts = Vec::new();
    let result = race_candidates(
        vec![primary, backup.clone()],
        || false,
        |candidate, reporter| {
            starts.push((candidate.url.clone(), started.elapsed()));
            async move {
                if candidate.url.contains("primary") {
                    tokio::time::sleep(Duration::from_secs(12)).await;
                    Err("download.firstByteTimeout".to_owned())
                } else {
                    receive(&reporter, LEAD - SAMPLE);
                    tokio::time::sleep(Duration::from_millis(375)).await;
                    receive(&reporter, SAMPLE);
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    receive(&reporter, TOTAL - LEAD);
                    reporter.verifying();
                    tokio::time::sleep(Duration::from_millis(300)).await;
                    Ok(vec![8])
                }
            }
        },
        Some(DownloadContext {
            file_name: Some("OC2DIYLevelRuntimeWLoader_v4.0.0_20261115.zip"),
            ..context(Some(&session), Some(TOTAL))
        }),
        RacePolicy::default(),
        |event| events.push(event),
    )
    .await
    .unwrap();
    assert_eq!(result, vec![8]);
    assert_eq!(starts[0].1, Duration::ZERO);
    assert_eq!(starts[1], (backup.url, Duration::from_secs(10)));
    assert_eq!(events[0].phase, DownloadPhase::Connecting);
    let lead = events
        .iter()
        .find(|event| {
            let rows = &event.race.as_ref().unwrap().sources;
            rows.len() == 2
                && rows[0].phase == DownloadSourcePhase::Connecting
                && rows[1].downloaded == LEAD
                && rows[1].speed_bytes.is_some()
        })
        .unwrap();
    println!("race-lead {}", serde_json::to_string(lead).unwrap());
    let race = lead.race.as_ref().unwrap();
    assert_eq!(race.sources[0].downloaded, 0);
    assert!(race.sources[0].history_preferred);
    assert!(!race.sources[1].history_preferred);
    assert_eq!(race.primary_id, Some(2));
    assert_eq!(lead.downloaded, LEAD);
    assert_eq!(lead.total, Some(TOTAL));
    assert_eq!(lead.route, Some(DownloadRoute::Proxy));
    assert_eq!(lead.source.as_deref(), Some("second.example"));
    assert_eq!(lead.phase, DownloadPhase::Downloading);
    assert_eq!(lead.speed_bytes, SAMPLE * 4);
    assert_eq!(race.sources[1].speed_bytes, Some(lead.speed_bytes));
    let checking = events
        .iter()
        .find(|event| event.phase == DownloadPhase::Verifying)
        .unwrap();
    assert!(!checking.finished);
    assert_eq!(checking.downloaded, TOTAL);
    assert_eq!(checking.speed_bytes, 0);
    assert_eq!(checking.race.as_ref().unwrap().primary_id, Some(2));
    assert!(events.last().unwrap().finished);
    assert_eq!(events.last().unwrap().downloaded, TOTAL);
}

#[tokio::test(start_paused = true)]
async fn preference_switches_only_past_ten_percent_and_never_borrows_another_routes_size() {
    for common_size in [Some(1000), None] {
        let (initial, behind_margin, at_margin, beyond_margin) = if common_size.is_some() {
            (440, 500, 540, 550)
        } else {
            (100, 106, 110, 111)
        };
        let mut events = Vec::new();
        let result = race_candidates(
            vec![candidate("primary"), candidate("backup")],
            || false,
            |candidate, reporter| async move {
                if candidate.url.contains("primary") {
                    receive(&reporter, initial);
                    let mut snapshot = reporter.snapshot();
                    snapshot.total = Some(1000);
                    reporter.update(snapshot);
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    receive(&reporter, 1000 - initial);
                    Ok(vec![1])
                } else {
                    let mut previous = 0;
                    for downloaded in [behind_margin, at_margin, beyond_margin] {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        receive(&reporter, downloaded - previous);
                        previous = downloaded;
                    }
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    Err("download.checksumMismatch".to_owned())
                }
            },
            Some(context(None, common_size)),
            RacePolicy::default(),
            |event| events.push(event),
        )
        .await
        .unwrap();
        assert_eq!(result, vec![1]);
        for (bytes, id, shown, total) in [
            (behind_margin, 1, initial, Some(1000)),
            (at_margin, 1, initial, Some(1000)),
            (beyond_margin, 2, beyond_margin, common_size),
        ] {
            let event = events.iter().find(|event| {
                let backup = &event.race.as_ref().unwrap().sources[1];
                backup.downloaded == bytes && backup.phase == DownloadSourcePhase::Downloading
            });
            let event = event.unwrap();
            assert_eq!(event.race.as_ref().unwrap().primary_id, Some(id));
            assert_eq!(event.downloaded, shown);
            assert_eq!(event.total, total);
        }
        let fallback = events.iter().find(|event| {
            event.race.as_ref().unwrap().sources[1].phase == DownloadSourcePhase::Failed
        });
        let fallback = fallback.unwrap();
        assert!(!fallback.finished);
        assert_eq!(fallback.race.as_ref().unwrap().primary_id, Some(1));
        assert_eq!(fallback.downloaded, initial);
        assert_eq!(fallback.total, Some(1000));
    }
}

#[tokio::test(start_paused = true)]
async fn after_validation_fails_the_new_progress_source_uses_its_own_known_size() {
    let directory = tempfile::tempdir().unwrap();
    let history = Arc::new(DownloadHistory::new(directory.path().to_path_buf()));
    let primary = candidate("primary");
    history_with_verified_preference(&history, &primary);
    let session = history.session();
    let mut events = Vec::new();
    let result = race_candidates(
        vec![primary, candidate("backup")],
        || false,
        |candidate, reporter| async move {
            let primary = candidate.url.contains("primary");
            let (downloaded, total) = if primary { (24, 24) } else { (4096, 8192) };
            receive(&reporter, downloaded);
            let mut progress = reporter.snapshot();
            progress.total = Some(total);
            reporter.update(progress);
            if primary {
                reporter.verifying();
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
            if primary {
                Err("download.checksumMismatch".to_owned())
            } else {
                receive(&reporter, downloaded);
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
    assert!(events.iter().any(|event| event.total == Some(24)));
    let resumed = events
        .iter()
        .find(|event| event.downloaded == 4096)
        .unwrap();
    assert_eq!(resumed.race.as_ref().unwrap().primary_id, Some(2));
    assert_eq!(resumed.total, Some(8192));
    assert_eq!(events.last().unwrap().total, Some(8192));
}
