use crate::archive::{self, DownloadPhase, DownloadProgress, DownloadReporter, DownloadRoute};
use futures_util::StreamExt;
use futures_util::future::{AbortHandle, Abortable};
use futures_util::stream::FuturesUnordered;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::AppHandle;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadCandidate {
    pub route: DownloadRoute,
    pub url: String,
}

pub fn candidate_progress(
    label: &str,
    candidate: &DownloadCandidate,
    attempt: usize,
    attempt_total: usize,
) -> DownloadProgress {
    let source = url::Url::parse(&candidate.url).ok().and_then(|url| {
        url.host().map(|host| match url.port() {
            Some(port) => format!("{host}:{port}"),
            None => host.to_string(),
        })
    });
    DownloadProgress {
        label: label.into(),
        downloaded: 0,
        total: None,
        speed_bytes: 0,
        finished: false,
        phase: if attempt > 1 {
            DownloadPhase::Switching
        } else {
            DownloadPhase::Connecting
        },
        route: Some(candidate.route),
        source,
        attempt: Some(attempt),
        attempt_total: Some(attempt_total),
        parallel_sources: None,
    }
}

pub struct DownloadContext<'a> {
    pub app: Option<&'a AppHandle>,
    pub label: &'a str,
    pub expected_size: Option<u64>,
}

pub trait DownloadOutput {
    fn discard(&self);
}

impl DownloadOutput for PathBuf {
    fn discard(&self) {
        let _ = std::fs::remove_file(self);
    }
}

impl DownloadOutput for Vec<u8> {
    fn discard(&self) {}
}

// A ready loser may already have closed its temporary file. Dropping its
// result must clean it just as dropping an in-flight archive future does.
struct CompletedDownload<T: DownloadOutput>(Option<T>);
impl<T: DownloadOutput> Drop for CompletedDownload<T> {
    fn drop(&mut self) {
        if let Some(value) = &self.0 {
            value.discard();
        }
    }
}

#[derive(Clone, Copy)]
struct RacePolicy {
    parallelism: usize,
    window: Duration,
    minimum_speed: u64,
    tick: Duration,
}

impl Default for RacePolicy {
    fn default() -> Self {
        Self {
            parallelism: 3,
            window: Duration::from_secs(5),
            minimum_speed: 512 * 1024,
            tick: Duration::from_millis(250),
        }
    }
}

struct ActiveSource {
    candidate: DownloadCandidate,
    reporter: Arc<DownloadReporter>,
    abort: AbortHandle,
    started: Instant,
    samples: VecDeque<(Instant, u64)>,
    speed: u64,
}

impl ActiveSource {
    fn sample(&mut self, now: Instant, policy: RacePolicy) -> bool {
        let bytes = self.reporter.transferred();
        self.samples.push_back((now, bytes));
        // Retain the sample at the beginning of the rolling five-second window.
        while self.samples.len() > 1 && now.duration_since(self.samples[1].0) >= policy.window {
            self.samples.pop_front();
        }
        let (beginning, previous) = self.samples.front().copied().unwrap();
        let seconds = now.duration_since(beginning).as_secs_f64();
        self.speed = (bytes.saturating_sub(previous) as f64 / seconds.max(0.001)) as u64;
        now.duration_since(self.started) >= policy.window && self.speed < policy.minimum_speed
    }
}

fn leading_source(
    sources: &HashMap<usize, ActiveSource>,
    previous: Option<usize>,
) -> Option<usize> {
    let best = sources
        .iter()
        .max_by_key(|(id, source)| {
            (
                source.reporter.snapshot().downloaded,
                std::cmp::Reverse(**id),
            )
        })
        .map(|(id, _)| *id)?;
    // Keep the displayed file until another one has strictly more bytes.
    if let Some(id) = previous {
        if let Some(source) = sources.get(&id) {
            if source.reporter.snapshot().downloaded
                >= sources[&best].reporter.snapshot().downloaded
            {
                return Some(id);
            }
        }
    }
    Some(best)
}

fn source_progress(
    label: &str,
    source: &ActiveSource,
    id: usize,
    total_candidates: usize,
    known_total: &mut Option<u64>,
) -> DownloadProgress {
    let mut progress = candidate_progress(label, &source.candidate, id + 1, total_candidates);
    let snapshot = source.reporter.snapshot();
    // Release metadata is authoritative. Without it, retain the first known
    // file size when the next leading proxy uses a response without a length.
    if known_total.is_none() {
        *known_total = snapshot.total.filter(|total| *total > 0);
    }
    progress.downloaded = snapshot.downloaded;
    progress.total = *known_total;
    progress.speed_bytes = source.speed;
    // A source can still be undergoing validation after receiving its bytes.
    // Completion belongs to the coordinator once the entire callback succeeds.
    if snapshot.downloaded > 0 {
        progress.phase = DownloadPhase::Downloading;
    }
    progress
}

pub async fn try_download_candidates<T, F, Fut, C>(
    candidates: Vec<DownloadCandidate>,
    is_cancelled: C,
    download: F,
    context: Option<DownloadContext<'_>>,
) -> Result<T, String>
where
    T: DownloadOutput,
    F: FnMut(DownloadCandidate, Arc<DownloadReporter>) -> Fut,
    Fut: std::future::Future<Output = Result<T, String>>,
    C: FnMut() -> bool,
{
    let app = context.as_ref().and_then(|context| context.app);
    race_candidates(
        candidates,
        is_cancelled,
        download,
        context,
        RacePolicy::default(),
        |progress| archive::emit_progress_event(app, progress),
    )
    .await
}

async fn race_candidates<T, F, Fut, C>(
    candidates: Vec<DownloadCandidate>,
    mut is_cancelled: C,
    mut download: F,
    context: Option<DownloadContext<'_>>,
    policy: RacePolicy,
    mut report_progress: impl FnMut(DownloadProgress),
) -> Result<T, String>
where
    T: DownloadOutput,
    F: FnMut(DownloadCandidate, Arc<DownloadReporter>) -> Fut,
    Fut: std::future::Future<Output = Result<T, String>>,
    C: FnMut() -> bool,
{
    let mut seen = HashSet::new();
    let mut pending = candidates
        .into_iter()
        .filter(|candidate| seen.insert(candidate.url.clone()))
        .enumerate()
        .collect::<VecDeque<_>>();
    let total_candidates = pending.len();
    let mut active = FuturesUnordered::new();
    let mut sources = HashMap::new();
    let mut retiring = HashSet::new();
    let mut leader = None;
    let mut last_error = None;
    let mut known_total = context
        .as_ref()
        .and_then(|context| context.expected_size)
        .filter(|total| *total > 0);
    let mut ticker = tokio::time::interval(policy.tick);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        if is_cancelled() {
            return Err("下载已取消".into());
        }
        while sources.len() + retiring.len() < policy.parallelism {
            if is_cancelled() {
                return Err("下载已取消".into());
            }
            let Some((id, candidate)) = pending.pop_front() else {
                break;
            };
            let reporter = Arc::new(DownloadReporter::new(
                context.as_ref().map(|ctx| ctx.label).unwrap_or(""),
            ));
            let now = Instant::now();
            let (abort, registration) = AbortHandle::new_pair();
            if let Some(context) = &context {
                let progress =
                    candidate_progress(context.label, &candidate, id + 1, total_candidates);
                log::info!(
                    "启动下载候选：任务={}，渠道={:?}，地址={}，候选={}/{}",
                    context.label,
                    candidate.route,
                    progress.source.as_deref().unwrap_or("未知地址"),
                    id + 1,
                    total_candidates
                );
            }
            let operation = download(candidate.clone(), reporter.clone());
            sources.insert(
                id,
                ActiveSource {
                    candidate,
                    reporter,
                    abort,
                    started: now,
                    samples: VecDeque::from([(now, 0)]),
                    speed: 0,
                },
            );
            active.push(async move {
                (
                    id,
                    Abortable::new(
                        async move { operation.await.map(|value| CompletedDownload(Some(value))) },
                        registration,
                    )
                    .await,
                )
            });
        }
        if sources.is_empty() && active.is_empty() {
            break;
        }
        leader = leading_source(&sources, leader);
        if let (Some(context), Some(id)) = (&context, leader) {
            let mut progress = source_progress(
                context.label,
                &sources[&id],
                id,
                total_candidates,
                &mut known_total,
            );
            progress.parallel_sources = Some(sources.len());
            report_progress(progress);
        }
        tokio::select! {
            biased;
            result = active.next(), if !active.is_empty() => {
                let Some((id, result)) = result else { continue; };
                if retiring.remove(&id) { drop(result); continue; }
                let source = sources.remove(&id);
                match result {
                    Ok(Ok(mut completed)) => {
                        if is_cancelled() { return Err("下载已取消".into()); }
                        // Drop all other request futures (and their temp-file
                        // guards) before handing the winner to installation.
                        drop(active);
                        if let (Some(context), Some(source)) = (&context, source) {
                            let mut progress = source_progress(context.label, &source, id, total_candidates, &mut known_total);
                            progress.finished = true;
                            progress.phase = DownloadPhase::Finished;
                            progress.parallel_sources = Some(1);
                            report_progress(progress);
                            log::info!("下载竞速完成：任务={}，胜出地址={}", context.label,
                                source.candidate.url.parse::<url::Url>().ok()
                                    .and_then(|url| url.host_str().map(str::to_string)).unwrap_or_default());
                        }
                        return Ok(completed.0.take().unwrap());
                    }
                    Ok(Err(error)) if error == "下载已取消" || is_cancelled() => return Err("下载已取消".into()),
                    Ok(Err(error)) => {
                        log::warn!("下载候选失败，补充其他候选：候选序号={}", id + 1);
                        last_error = Some(error);
                    }
                    Err(_) => {} // A slow source was aborted; its slot is already free.
                }
            }
            _ = ticker.tick() => {
                if is_cancelled() { return Err("下载已取消".into()); }
                let now = Instant::now();
                let mut slow = sources.iter_mut().filter_map(|(id, source)| source.sample(now, policy).then_some(*id))
                    .collect::<Vec<_>>();
                // When every candidate is slow and no replacements remain,
                // retain the fastest active source so the file can still finish.
                let fastest = sources.iter().max_by_key(|(id, source)|
                    (source.speed, source.reporter.snapshot().downloaded, std::cmp::Reverse(**id)))
                    .map(|(id, _)| *id);
                slow.sort_unstable();
                for id in slow {
                    if pending.is_empty() && Some(id) == fastest { continue; }
                    if let Some(source) = sources.remove(&id) {
                        log::info!("下载源最近五秒速度过低，释放下载槽：候选序号={}，速度={}B/s", id + 1, source.speed);
                        retiring.insert(id);
                        source.abort.abort();
                    }
                }
            }
        }
    }
    Err(last_error.unwrap_or_else(|| "没有可用的下载地址".into()))
}

#[cfg(test)]
mod tests {
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
        source.reporter.received(2 * 1024 * 1024);
        assert!(!source.sample(now + Duration::from_secs(1), policy));
        assert!(source.sample(now + Duration::from_secs(5), policy));
        source.reporter.received(10 * 1024 * 1024);
        assert!(!source.sample(now + Duration::from_secs(6), policy));
        assert!(source.sample(now + Duration::from_secs(11), policy));
        assert_eq!(source.speed, 0);
    }

    #[test]
    fn default_policy_only_replaces_sources_below_512_kib_per_second() {
        let now = Instant::now();
        let policy = RacePolicy::default();
        for (speed, replace) in [
            (512 * 1024 - 1, true),
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
            |candidate, reporter| async move {
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
                let mut progress = DownloadProgress::connecting("test");
                progress.downloaded = (id as u64 + 1) * 100;
                reporter.received(progress.downloaded);
                reporter.update(progress.clone());
                if id < 6 {
                    std::future::pending::<Result<Vec<u8>, String>>().await
                } else {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    progress.downloaded = 1000;
                    reporter.update(progress);
                    Ok(vec![42])
                }
            },
            Some(DownloadContext {
                app: None,
                label: "test",
                expected_size: Some(1000),
            }),
            RacePolicy {
                minimum_speed: RacePolicy::default().minimum_speed,
                ..policy()
            },
            |progress| events.push(progress),
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
        assert_eq!(result.unwrap_err(), "下载已取消");
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
}
