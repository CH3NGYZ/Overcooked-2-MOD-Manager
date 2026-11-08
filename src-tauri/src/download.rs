use crate::archive::{self, DownloadPhase, DownloadProgress, DownloadReporter, DownloadRoute};
use futures_util::StreamExt;
use futures_util::future::{AbortHandle, Abortable};
use futures_util::stream::FuturesUnordered;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::AppHandle;

pub(crate) mod planning;

#[cfg(test)]
mod race_tests;

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

#[derive(Debug, Clone, Copy)]
pub struct DownloadExecution {
    pub max_parallelism: u32,
    pub race: bool,
}

pub struct DownloadContext<'a> {
    pub execution: Option<DownloadExecution>,
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
    terminal_error: Option<&'static str>,
}

impl Default for RacePolicy {
    fn default() -> Self {
        Self {
            parallelism: 3,
            window: Duration::from_secs(5),
            minimum_speed: 256 * 1024,
            tick: Duration::from_millis(250),
            terminal_error: None,
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
    let mut policy = RacePolicy::default();
    if let Some(execution) = context.as_ref().and_then(|value| value.execution) {
        if execution.max_parallelism == 0
            || execution.max_parallelism > 3
            || (!execution.race && execution.max_parallelism != 1)
        {
            return Err("proxy.validationFailed".into());
        }
        policy.parallelism = execution.max_parallelism as usize;
        if !execution.race {
            policy.minimum_speed = 0;
        }
    }
    race_candidates(
        candidates,
        is_cancelled,
        download,
        context,
        policy,
        |progress| archive::emit_progress_event(app, progress),
    )
    .await
}

pub async fn try_download_candidates_until_error<T, F, Fut, C>(
    candidates: Vec<DownloadCandidate>,
    is_cancelled: C,
    download: F,
    terminal_error: &'static str,
) -> Result<T, String>
where
    T: DownloadOutput,
    F: FnMut(DownloadCandidate, Arc<DownloadReporter>) -> Fut,
    Fut: std::future::Future<Output = Result<T, String>>,
    C: FnMut() -> bool,
{
    race_candidates(
        candidates,
        is_cancelled,
        download,
        None,
        RacePolicy {
            terminal_error: Some(terminal_error),
            ..RacePolicy::default()
        },
        |_| {},
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
            return Err("download.cancelled".into());
        }
        while sources.len() + retiring.len() < policy.parallelism {
            if is_cancelled() {
                return Err("download.cancelled".into());
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
                        if is_cancelled() { return Err("download.cancelled".into()); }
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
                    Ok(Err(error)) if error == "download.cancelled" || is_cancelled() => return Err("download.cancelled".into()),
                    Ok(Err(error)) if policy.terminal_error == Some(error.as_str()) => return Err(error),
                    Ok(Err(error)) => {
                        log::warn!("下载候选失败，补充其他候选：候选序号={}", id + 1);
                        last_error = Some(error);
                    }
                    Err(_) => {} // A slow source was aborted; its slot is already free.
                }
            }
            _ = ticker.tick() => {
                if is_cancelled() { return Err("download.cancelled".into()); }
                let now = Instant::now();
                let mut slow = sources.iter_mut().filter_map(|(id, source)| source.sample(now, policy).then_some(*id))
                    .collect::<Vec<_>>();
                // A slow connection is only worth dropping when an unstarted
                // candidate can replace it. Reserve slots already being retired
                // so repeated ticks cannot spend the same replacement twice.
                slow.sort_unstable_by_key(|id| {
                    let source = &sources[id];
                    (source.speed, source.reporter.snapshot().downloaded, *id)
                });
                let replacements = pending.len().saturating_sub(retiring.len());
                for id in slow.into_iter().take(replacements) {
                    if let Some(source) = sources.remove(&id) {
                        log::info!("下载源最近五秒速度过低，释放下载槽：候选序号={}，速度={}B/s", id + 1, source.speed);
                        retiring.insert(id);
                        source.abort.abort();
                    }
                }
            }
        }
    }
    Err(last_error.unwrap_or_else(|| "download.noCandidate".into()))
}

#[cfg(test)]
#[path = "download_retention_tests.rs"]
mod retention_tests;

#[cfg(test)]
mod tests;
