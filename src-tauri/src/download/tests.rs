use super::*;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

fn candidate(id: usize) -> DownloadCandidate {
    DownloadCandidate {
        route: DownloadRoute::Proxy,
        url: format!("https://source{id}.example/file"),
    }
}

fn source(bytes: u64, now: Instant) -> ActiveSource {
    let reporter = Arc::new(DownloadReporter::new("test"));
    let mut progress = DownloadProgress::connecting("test");
    progress.downloaded = bytes;
    reporter.update(progress);
    let (abort, _) = AbortHandle::new_pair();
    ActiveSource {
        candidate: candidate(0),
        reporter,
        abort,
        started: now,
        samples: VecDeque::from([(now, 0)]),
        speed: 0,
    }
}

fn policy() -> RacePolicy {
    RacePolicy {
        parallelism: 3,
        window: Duration::from_millis(40),
        minimum_speed: 1024,
        tick: Duration::from_millis(2),
        terminal_error: None,
    }
}

#[test]
fn progress_keeps_the_leader_until_another_file_overtakes_it() {
    let now = Instant::now();
    let mut sources = HashMap::from([(0, source(100, now)), (1, source(90, now))]);
    assert_eq!(leading_source(&sources, None), Some(0));
    let mut progress = DownloadProgress::connecting("test");
    progress.downloaded = 100;
    sources[&1].reporter.update(progress.clone());
    assert_eq!(leading_source(&sources, Some(0)), Some(0));
    progress.downloaded = 101;
    sources[&1].reporter.update(progress);
    assert_eq!(leading_source(&sources, Some(0)), Some(1));
    sources.remove(&1);
    assert_eq!(leading_source(&sources, Some(1)), Some(0));
}

#[test]
fn speed_uses_the_recent_five_seconds_and_gives_new_sources_a_full_window() {
    let now = Instant::now();
    let mut source = source(0, now);
    let policy = RacePolicy::default();
    source.reporter.received(1024 * 1024);
    assert!(!source.sample(now + Duration::from_secs(1), policy));
    assert!(source.sample(now + Duration::from_secs(5), policy));
    source.reporter.received(10 * 1024 * 1024);
    assert!(!source.sample(now + Duration::from_secs(6), policy));
    assert!(source.sample(now + Duration::from_secs(11), policy));
    assert_eq!(source.speed, 0);
}

#[test]
fn default_policy_only_replaces_sources_below_256_kib_per_second() {
    let now = Instant::now();
    let policy = RacePolicy::default();
    for (speed, replace) in [
        (256 * 1024 - 1, true),
        (256 * 1024, false),
        (256 * 1024 + 1, false),
        (384 * 1024, false),
        (512 * 1024, false),
        (768 * 1024, false),
    ] {
        let mut source = source(0, now);
        source.reporter.received(speed * 5);
        assert_eq!(source.sample(now + Duration::from_secs(5), policy), replace);
        assert_eq!(source.speed, speed);
    }
}

#[test]
fn unknown_or_zero_size_stays_unknown_until_a_source_reports_a_positive_size() {
    let now = Instant::now();
    let first = source(50, now);
    let mut known_total = None;
    let mut snapshot = first.reporter.snapshot();
    snapshot.total = Some(0);
    first.reporter.update(snapshot.clone());
    let progress = source_progress("test", &first, 0, 3, &mut known_total);
    assert_eq!(progress.total, None);
    assert_eq!(progress.phase, DownloadPhase::Downloading);
    snapshot.total = Some(200);
    first.reporter.update(snapshot);
    assert_eq!(
        source_progress("test", &first, 0, 3, &mut known_total).total,
        Some(200)
    );
    let next = source(100, now);
    let progress = source_progress("test", &next, 1, 3, &mut known_total);
    assert_eq!(progress.total, Some(200));
    assert_eq!(progress.downloaded, 100);
    assert_eq!(progress.phase, DownloadPhase::Downloading);
    assert!(!progress.finished);
}

#[tokio::test]
async fn repeated_proxy_failures_preserve_api_or_learned_size_until_validated_completion() {
    for expected_size in [Some(1000), None] {
        let observed = AtomicUsize::new(0);
        let complete_bytes_observed = AtomicBool::new(false);
        let mut events = Vec::new();
        let race = race_candidates(
            (0..7).map(candidate).collect(),
            || false,
            |candidate, reporter| {
                let observed = &observed;
                let complete_bytes_observed = &complete_bytes_observed;
                async move {
                    let id = candidate
                        .url
                        .parse::<url::Url>()
                        .unwrap()
                        .host_str()
                        .unwrap()
                        .trim_start_matches("source")
                        .trim_end_matches(".example")
                        .parse::<usize>()
                        .unwrap();
                    // Each replacement starts transferring only after the
                    // previous leader was actually shown to the user.
                    while id > 0 && observed.load(Ordering::SeqCst) & (1 << (id - 1)) == 0 {
                        tokio::time::sleep(Duration::from_millis(1)).await;
                    }
                    let mut progress = DownloadProgress::connecting("test");
                    progress.downloaded = (id as u64 + 1) * 100;
                    // Metadata must win even over an incorrect proxy size.
                    progress.total = if id == 0 {
                        Some(expected_size.map_or(1000, |_| 1))
                    } else {
                        None
                    };
                    reporter.received(progress.downloaded);
                    reporter.update(progress.clone());
                    while observed.load(Ordering::SeqCst) & (1 << id) == 0 {
                        tokio::time::sleep(Duration::from_millis(1)).await;
                    }
                    if id < 6 {
                        Err("代理下载失败".into())
                    } else {
                        progress.downloaded = 1000;
                        progress.finished = true;
                        progress.phase = DownloadPhase::Finished;
                        reporter.update(progress);
                        // All bytes have arrived, but the callback has not
                        // yet completed its integrity checks.
                        while !complete_bytes_observed.load(Ordering::SeqCst) {
                            tokio::time::sleep(Duration::from_millis(1)).await;
                        }
                        Ok(vec![42])
                    }
                }
            },
            Some(DownloadContext {
                execution: None,
                app: None,
                label: "test",
                expected_size,
            }),
            RacePolicy {
                window: Duration::from_secs(5),
                ..policy()
            },
            |progress| {
                if progress.downloaded > 0 {
                    observed.fetch_or(1 << (progress.attempt.unwrap() - 1), Ordering::SeqCst);
                }
                if progress.downloaded == 1000 && !progress.finished {
                    complete_bytes_observed.store(true, Ordering::SeqCst);
                }
                events.push(progress);
            },
        );
        let result = tokio::time::timeout(Duration::from_secs(2), race)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result, vec![42]);
        assert_eq!(observed.load(Ordering::SeqCst), 0b111_1111);
        let transferring = events
            .iter()
            .filter(|event| event.downloaded > 0 && !event.finished)
            .collect::<Vec<_>>();
        assert!(!transferring.is_empty());
        assert!(transferring.iter().all(
            |event| event.total == Some(1000) && event.phase == DownloadPhase::Downloading
        ));
        assert!(
            events
                .iter()
                .all(|event| event.parallel_sources.unwrap() <= 3)
        );
        assert_eq!(events.iter().filter(|event| event.finished).count(), 1);
        let finished = events.last().unwrap();
        assert_eq!(finished.phase, DownloadPhase::Finished);
        assert_eq!(finished.downloaded, 1000);
        assert_eq!(finished.total, Some(1000));
        assert_eq!(finished.attempt, Some(7));
    }
}

struct Activity(Arc<AtomicUsize>);
impl Drop for Activity {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn failed_sources_are_replaced_without_exceeding_three_downloads() {
    let active = Arc::new(AtomicUsize::new(0));
    let maximum = Arc::new(AtomicUsize::new(0));
    let started = Arc::new(Mutex::new(Vec::new()));
    let result = race_candidates(
        (0..5).map(candidate).collect(),
        || false,
        |candidate, _| {
            let active = active.clone();
            let maximum = maximum.clone();
            let started = started.clone();
            async move {
                let count = active.fetch_add(1, Ordering::SeqCst) + 1;
                let _guard = Activity(active);
                maximum.fetch_max(count, Ordering::SeqCst);
                let id = candidate
                    .url
                    .chars()
                    .find(|c| c.is_ascii_digit())
                    .unwrap()
                    .to_digit(10)
                    .unwrap();
                started.lock().unwrap().push(id);
                if id == 0 {
                    tokio::time::sleep(Duration::from_millis(8)).await;
                    Err("下载失败".into())
                } else if id == 3 {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    Ok(vec![3])
                } else {
                    std::future::pending::<Result<Vec<u8>, String>>().await
                }
            }
        },
        None,
        policy(),
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(result, vec![3]);
    assert_eq!(maximum.load(Ordering::SeqCst), 3);
    assert_eq!(active.load(Ordering::SeqCst), 0);
    assert!(started.lock().unwrap().contains(&3));
    assert!(!started.lock().unwrap().contains(&4));
}

#[tokio::test]
async fn slow_sources_release_slots_and_exhausted_candidates_can_still_finish() {
    let result = race_candidates(
        (0..4).map(candidate).collect(),
        || false,
        |candidate, _| async move {
            if candidate.url.contains("source3") {
                tokio::time::sleep(Duration::from_millis(8)).await;
                Ok(vec![3])
            } else {
                std::future::pending::<Result<Vec<u8>, String>>().await
            }
        },
        None,
        policy(),
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(result, vec![3]);
    let result = race_candidates(
        vec![candidate(0)],
        || false,
        |_, _| async {
            tokio::time::sleep(Duration::from_millis(80)).await;
            Ok(vec![0])
        },
        None,
        policy(),
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(result, vec![0]);
}

#[tokio::test]
async fn slow_proxy_rotation_preserves_total_size_and_reports_the_actual_leading_file() {
    let mut events = Vec::new();
    let race = race_candidates(
        (0..7).map(candidate).collect(),
        || false,
        |candidate, reporter| {
            let id = candidate
                .url
                .parse::<url::Url>()
                .unwrap()
                .host_str()
                .unwrap()
                .trim_start_matches("source")
                .trim_end_matches(".example")
                .parse::<usize>()
                .unwrap();
            // Seed each stage before polling, independent of timer granularity.
            let mut progress = DownloadProgress::connecting("test");
            progress.downloaded = (id as u64 + 1) * 100;
            reporter.received(progress.downloaded);
            reporter.update(progress.clone());
            async move {
                if id < 6 {
                    std::future::pending::<Result<Vec<u8>, String>>().await
                } else {
                    progress.downloaded = 1000;
                    reporter.update(progress);
                    Ok(vec![42])
                }
            }
        },
        Some(DownloadContext {
            execution: None,
            app: None,
            label: "test",
            expected_size: Some(1000),
        }),
        RacePolicy {
            minimum_speed: RacePolicy::default().minimum_speed,
            ..policy()
        },
        |progress| {
            // Reproduce a scheduler stall longer than the test's 40 ms window.
            if events.is_empty() {
                std::thread::sleep(Duration::from_millis(80));
            }
            events.push(progress);
        },
    );
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), race)
            .await
            .unwrap()
            .unwrap(),
        vec![42]
    );
    let transferring = events
        .iter()
        .filter(|event| event.downloaded > 0 && !event.finished)
        .collect::<Vec<_>>();
    assert!(transferring.iter().any(|event| event.attempt == Some(3)));
    assert!(transferring.iter().any(|event| event.attempt == Some(6)));
    assert!(transferring.iter().any(|event| event.attempt == Some(7)));
    for event in transferring {
        assert_eq!(event.phase, DownloadPhase::Downloading);
        assert_eq!(event.total, Some(1000));
        assert_eq!(event.downloaded, event.attempt.unwrap() as u64 * 100);
        assert!(event.parallel_sources.unwrap() <= 3);
    }
    assert_eq!(events.iter().filter(|event| event.finished).count(), 1);
    assert_eq!(events.last().unwrap().downloaded, 1000);
}

#[derive(Clone, Copy)]
enum ResponseKind {
    Valid,
    Empty,
    Truncated,
    WrongDigest,
    Slow,
    Chunked,
}

struct TestServer {
    address: String,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl TestServer {
    fn new(routes: Vec<ResponseKind>, payload: Vec<u8>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let thread = std::thread::spawn(move || {
            let mut handlers = Vec::new();
            while !stopped.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        let payload = payload.clone();
                        let routes = routes.clone();
                        let stop = stopped.clone();
                        handlers.push(std::thread::spawn(move || {
                            // Winsock inherits the listener's nonblocking
                            // mode. Read the full request in blocking mode.
                            socket.set_nonblocking(false).unwrap();
                            socket.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
                            socket.set_nodelay(true).unwrap();
                            let mut request = Vec::new();
                            while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                                let mut buffer = [0; 2048];
                                let Ok(read) = socket.read(&mut buffer) else { return; };
                                if read == 0 || request.len() + read > 8192 { return; }
                                request.extend_from_slice(&buffer[..read]);
                            }
                            let request = String::from_utf8_lossy(&request);
                            let id: usize = request.split_whitespace().nth(1).unwrap().trim_start_matches('/').parse().unwrap();
                            let kind = routes[id];
                            let length = if matches!(kind, ResponseKind::Empty) { 0 } else { payload.len() };
                            let response = if matches!(kind, ResponseKind::Chunked) {
                                "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n".into()
                            } else {
                                format!("HTTP/1.1 200 OK\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n")
                            };
                            if socket.write_all(response.as_bytes()).is_err() { return; }
                            match kind {
                                ResponseKind::Empty => {}
                                ResponseKind::Truncated => { let _ = socket.write_all(&payload[..100]); }
                                ResponseKind::WrongDigest => { let _ = socket.write_all(&vec![0; payload.len()]); }
                                ResponseKind::Valid => { let _ = socket.write_all(&payload); }
                                ResponseKind::Chunked => {
                                    for chunk in payload.chunks(4096) {
                                        if stop.load(Ordering::Relaxed) { return; }
                                        let frame = format!("{:x}\r\n", chunk.len());
                                        if socket.write_all(frame.as_bytes())
                                            .and_then(|_| socket.write_all(chunk))
                                            .and_then(|_| socket.write_all(b"\r\n")).is_err() { return; }
                                        std::thread::sleep(Duration::from_millis(10));
                                    }
                                    let _ = socket.write_all(b"0\r\n\r\n");
                                }
                                ResponseKind::Slow => {
                                    for chunk in payload.chunks(1024) {
                                        if stop.load(Ordering::Relaxed) || socket.write_all(chunk).is_err() { break; }
                                        std::thread::sleep(Duration::from_millis(10));
                                    }
                                }
                            }
                            let _ = socket.shutdown(Shutdown::Write);
                            let mut discarded = [0; 1024];
                            while let Ok(read) = socket.read(&mut discarded) {
                                if read == 0 { break; }
                            }
                        }));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2))
                    }
                    Err(_) => break,
                }
            }
            for handler in handlers {
                handler.join().unwrap();
            }
        });
        Self {
            address,
            stop,
            thread: Some(thread),
        }
    }

    fn candidates(&self, count: usize) -> Vec<DownloadCandidate> {
        (0..count)
            .map(|id| DownloadCandidate {
                route: DownloadRoute::Proxy,
                url: format!("http://{}/{id}", self.address),
            })
            .collect()
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.take().unwrap().join().unwrap();
    }
}

#[tokio::test]
async fn real_downloads_reject_empty_truncated_and_bad_hash_files_and_clean_losers() {
    let payload = vec![42; 64 * 1024];
    let digest = format!("sha256:{:x}", Sha256::digest(&payload));
    let server = TestServer::new(
        vec![
            ResponseKind::Empty,
            ResponseKind::Truncated,
            ResponseKind::WrongDigest,
            ResponseKind::Slow,
            ResponseKind::Valid,
        ],
        payload.clone(),
    );
    let cache = std::env::temp_dir().join(format!("oc2-race-test-{}", uuid::Uuid::new_v4()));
    let client = reqwest::Client::builder()
        .no_proxy()
        .pool_max_idle_per_host(0)
        .build()
        .unwrap();
    let winner = race_candidates(
        server.candidates(5),
        || false,
        |candidate, reporter| {
            let client = &client;
            let cache = &cache;
            let digest = &digest;
            async move {
                let path = archive::download_to_temp_with_threads_and_first_byte_timeout(
                    client,
                    &candidate.url,
                    cache,
                    "test",
                    &reporter,
                    1,
                    Some(Duration::from_secs(1)),
                )
                .await?;
                if let Err(error) =
                    archive::validate_download_file(&path, Some(64 * 1024), Some(digest))
                {
                    let _ = std::fs::remove_file(&path);
                    return Err(error);
                }
                Ok(path)
            }
        },
        None,
        RacePolicy::default(),
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read(&winner).unwrap(), payload);
    assert_eq!(std::fs::read_dir(&cache).unwrap().count(), 1);
    std::fs::remove_file(winner).unwrap();
    std::fs::remove_dir(cache).unwrap();
}

#[tokio::test]
async fn chunked_proxy_without_content_length_publishes_real_percentages_from_metadata() {
    let payload = vec![42; 64 * 1024];
    let size = payload.len() as u64;
    let server = TestServer::new(vec![ResponseKind::Chunked], payload.clone());
    let cache = std::env::temp_dir().join(format!("oc2-chunked-test-{}", uuid::Uuid::new_v4()));
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let mut events = Vec::new();
    let winner = race_candidates(
        server.candidates(1),
        || false,
        |candidate, reporter| {
            let client = &client;
            let cache = &cache;
            async move {
                let path = archive::download_to_temp_with_threads_and_first_byte_timeout(
                    client,
                    &candidate.url,
                    cache,
                    "test",
                    &reporter,
                    1,
                    Some(Duration::from_secs(1)),
                )
                .await?;
                // Confirm the real HTTP response never supplied a size.
                assert_eq!(reporter.snapshot().total, None);
                archive::validate_download_file(&path, Some(size), None)?;
                Ok(path)
            }
        },
        Some(DownloadContext {
            execution: None,
            app: None,
            label: "test",
            expected_size: Some(size),
        }),
        RacePolicy {
            tick: Duration::from_millis(2),
            ..RacePolicy::default()
        },
        |progress| events.push(progress),
    )
    .await
    .unwrap();
    let bytes = std::fs::read(&winner).unwrap();
    std::fs::remove_file(winner).unwrap();
    std::fs::remove_dir(cache).unwrap();
    assert_eq!(bytes, payload);
    assert!(
        events
            .iter()
            .any(|event| event.downloaded > 0 && event.downloaded < size && !event.finished)
    );
    assert!(events.iter().all(|event| event.total == Some(size)));
    assert!(
        events
            .iter()
            .filter(|event| event.downloaded > 0 && !event.finished)
            .all(|event| event.phase == DownloadPhase::Downloading)
    );
    assert_eq!(events.iter().filter(|event| event.finished).count(), 1);
    assert_eq!(events.last().unwrap().downloaded, size);
}

#[tokio::test]
async fn cancellation_interrupts_pending_streams_and_removes_all_partial_files() {
    let server = TestServer::new(vec![ResponseKind::Slow; 3], vec![42; 64 * 1024]);
    let cache = std::env::temp_dir().join(format!("oc2-cancel-test-{}", uuid::Uuid::new_v4()));
    let client = reqwest::Client::builder()
        .no_proxy()
        .pool_max_idle_per_host(0)
        .build()
        .unwrap();
    let cancelled = AtomicBool::new(false);
    let reporters = Mutex::new(Vec::new());
    let race = race_candidates(
        server.candidates(3),
        || cancelled.load(Ordering::Acquire),
        |candidate, reporter| {
            reporters.lock().unwrap().push(reporter.clone());
            let client = &client;
            let cache = &cache;
            async move {
                archive::download_to_temp_with_threads_and_first_byte_timeout(
                    client,
                    &candidate.url,
                    cache,
                    "test",
                    &reporter,
                    1,
                    Some(Duration::from_secs(1)),
                )
                .await
            }
        },
        None,
        RacePolicy {
            tick: Duration::from_millis(5),
            ..RacePolicy::default()
        },
        |_| {},
    );
    let cancel = async {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let started = {
                    let reporters = reporters.lock().unwrap();
                    reporters.len() == 3
                        && reporters.iter().all(|reporter| reporter.transferred() > 0)
                };
                if started {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .expect("all three local response bodies must start before cancellation");
        cancelled.store(true, Ordering::Release);
    };
    let (result, _) = tokio::join!(race, cancel);
    assert_eq!(result.unwrap_err(), "download.cancelled");
    assert_eq!(std::fs::read_dir(&cache).unwrap().count(), 0);
    std::fs::remove_dir(cache).unwrap();
}

#[test]
fn ready_loser_outputs_are_discarded() {
    let path = std::env::temp_dir().join(format!("oc2-loser-test-{}", uuid::Uuid::new_v4()));
    std::fs::write(&path, b"ready loser").unwrap();
    drop(CompletedDownload(Some(path.clone())));
    assert!(!path.exists());
}

#[tokio::test]
async fn configured_sequence_attempts_only_one_candidate_and_stops_after_success() {
    let started = Arc::new(Mutex::new(Vec::new()));
    let observed = started.clone();
    let bytes: Vec<u8> = try_download_candidates(
        vec![candidate(0), candidate(1), candidate(2)],
        || false,
        move |candidate, _| {
            observed.lock().unwrap().push(candidate.url.clone());
            async move {
                if candidate.url.contains("source0") {
                    Err("first failed".into())
                } else {
                    Ok(vec![1, 2, 3])
                }
            }
        },
        Some(DownloadContext {
            app: None,
            label: "sequence",
            expected_size: None,
            execution: Some(DownloadExecution {
                max_parallelism: 1,
                race: false,
            }),
        }),
    )
    .await
    .unwrap();
    assert_eq!(bytes, [1, 2, 3]);
    assert_eq!(
        *started.lock().unwrap(),
        [candidate(0).url, candidate(1).url]
    );
}

#[tokio::test]
async fn configured_race_honors_two_candidate_limit() {
    let active = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let measured = active.clone();
    let maximum = peak.clone();
    let result: Result<Vec<u8>, String> = try_download_candidates(
        vec![candidate(0), candidate(1), candidate(2), candidate(3)],
        || false,
        move |_, _| {
            let active = measured.clone();
            let peak = maximum.clone();
            async move {
                let count = active.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(count, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(25)).await;
                active.fetch_sub(1, Ordering::SeqCst);
                Err("try next".into())
            }
        },
        Some(DownloadContext {
            app: None,
            label: "race",
            expected_size: None,
            execution: Some(DownloadExecution {
                max_parallelism: 2,
                race: true,
            }),
        }),
    )
    .await;
    assert!(result.is_err());
    assert_eq!(peak.load(Ordering::SeqCst), 2);
    assert_eq!(active.load(Ordering::SeqCst), 0);
}
