use crate::archive::{self, DownloadPhase, DownloadProgress, DownloadReporter, DownloadRoute};
use crate::models::NetworkConfig;
use futures_util::StreamExt;
use futures_util::future::{AbortHandle, Abortable};
use futures_util::stream::FuturesUnordered;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tauri::AppHandle;
use tokio::time::Instant;

pub(crate) mod planning;
mod race_speed;

use race_speed::{SourceSpeed, switching_candidate};

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
        file_name: None,
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
        volumes: Vec::new(),
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
    pub file_name: Option<&'a str>,
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
    reference_speed: u64,
    tick: Duration,
    terminal_error: Option<&'static str>,
}

impl RacePolicy {
    fn from_network(network: &NetworkConfig) -> Self {
        Self {
            parallelism: 3,
            window: network.race_window(),
            reference_speed: network.race_minimum_speed(),
            tick: Duration::from_millis(250),
            terminal_error: None,
        }
    }
}

impl Default for RacePolicy {
    fn default() -> Self {
        Self::from_network(&NetworkConfig::default())
    }
}

struct ActiveSource {
    candidate: DownloadCandidate,
    reporter: Arc<DownloadReporter>,
    abort: AbortHandle,
    speed: SourceSpeed,
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
    context: &DownloadContext<'_>,
    source: &ActiveSource,
    id: usize,
    total_candidates: usize,
    known_total: &mut Option<u64>,
) -> DownloadProgress {
    let mut progress =
        candidate_progress(context.label, &source.candidate, id + 1, total_candidates);
    progress.set_file_name(context.file_name);
    let snapshot = source.reporter.snapshot();
    // Release metadata is authoritative. Without it, retain the first known
    // file size when the next leading proxy uses a response without a length.
    if known_total.is_none() {
        *known_total = snapshot.total.filter(|total| *total > 0);
    }
    progress.downloaded = snapshot.downloaded;
    progress.total = *known_total;
    progress.speed_bytes = source.speed.bytes_per_second;
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
    network: &NetworkConfig,
    context: Option<DownloadContext<'_>>,
) -> Result<T, String>
where
    T: DownloadOutput,
    F: FnMut(DownloadCandidate, Arc<DownloadReporter>) -> Fut,
    Fut: std::future::Future<Output = Result<T, String>>,
    C: FnMut() -> bool,
{
    let app = context.as_ref().and_then(|context| context.app);
    try_download_candidates_with_progress(
        candidates,
        is_cancelled,
        download,
        network,
        context,
        |progress| archive::emit_progress_event(app, progress),
    )
    .await
}

/// Uses the same race state machine with a caller-owned progress destination.
/// Multipart downloads aggregate one winning candidate per volume here.
pub(crate) async fn try_download_candidates_with_progress<T, F, Fut, C>(
    candidates: Vec<DownloadCandidate>,
    is_cancelled: C,
    download: F,
    network: &NetworkConfig,
    context: Option<DownloadContext<'_>>,
    report_progress: impl FnMut(DownloadProgress),
) -> Result<T, String>
where
    T: DownloadOutput,
    F: FnMut(DownloadCandidate, Arc<DownloadReporter>) -> Fut,
    Fut: std::future::Future<Output = Result<T, String>>,
    C: FnMut() -> bool,
{
    let mut policy = RacePolicy::from_network(network);
    if let Some(execution) = context.as_ref().and_then(|value| value.execution) {
        if execution.max_parallelism == 0
            || execution.max_parallelism > 3
            || (!execution.race && execution.max_parallelism != 1)
        {
            return Err("proxy.validationFailed".into());
        }
        policy.parallelism = execution.max_parallelism as usize;
        if !execution.race {
            policy.reference_speed = 0;
        }
    }
    race_candidates(
        candidates,
        is_cancelled,
        download,
        context,
        policy,
        report_progress,
    )
    .await
}

pub async fn try_download_candidates_until_error<T, F, Fut, C>(
    candidates: Vec<DownloadCandidate>,
    is_cancelled: C,
    download: F,
    terminal_error: &'static str,
    network: &NetworkConfig,
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
            ..RacePolicy::from_network(network)
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
    let mut last_switch = None;
    let expected_size = context
        .as_ref()
        .and_then(|context| context.expected_size)
        .filter(|total| *total > 0);
    let mut known_total = expected_size;
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
                    speed: SourceSpeed::default(),
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
                context,
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
                            let mut progress = source_progress(context, &source, id, total_candidates, &mut known_total);
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
                for source in sources.values_mut() {
                    source.speed.sample(now, source.reporter.transferred(), policy.window);
                }
                // Switch at most once per window, only with a spare route.
                // Failed requests free their slots immediately, outside this cooldown.
                if policy.reference_speed > 0
                    && pending.len() > retiring.len()
                    && last_switch.is_none_or(|last| now.duration_since(last) >= policy.window)
                    && let Some((id, fastest)) = switching_candidate(
                        &sources, leading_source(&sources, leader), expected_size, policy.reference_speed,
                    )
                {
                    let source = sources.remove(&id).unwrap();
                    log::info!("下载线路明显落后，尝试备用线路：候选序号={}，速度={}B/s，最快={}B/s，换线参考速度={}B/s",
                        id + 1, source.speed.bytes_per_second, fastest, policy.reference_speed);
                    last_switch = Some(now);
                    retiring.insert(id);
                    source.abort.abort();
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
