use super::control::{RaceAction, RaceControl};
use super::history::RaceSession;
use super::race_speed::{SourceSpeed, fastest_backup, probe_should_stop, switching_candidate};
use super::{
    CompletedDownload, DownloadCandidate, DownloadContext, DownloadOutput, RacePolicy, source_host,
};
use crate::archive::{
    self, DownloadProgress, DownloadReporter, DownloadSourcePhase, DownloadSourceProgress,
};
use futures_util::StreamExt;
use futures_util::future::{AbortHandle, Abortable};
use futures_util::stream::FuturesUnordered;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::future::{Future, pending};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;

mod progress;
#[cfg(test)]
mod tests;

pub(super) struct ActiveSource {
    pub(super) candidate: DownloadCandidate,
    pub(super) reporter: Arc<DownloadReporter>,
    pub(super) abort: AbortHandle,
    pub(super) speed: SourceSpeed,
    pub(super) started: Instant,
}

impl ActiveSource {
    pub(super) fn verifying(&self) -> bool {
        self.reporter.transfer_end().is_some() || self.reporter.snapshot().finished
    }

    fn lockable(&self, expected: Option<u64>) -> bool {
        let progress = self.reporter.snapshot();
        !self.verifying()
            && !expected
                .or(progress.total)
                .is_some_and(|size| size > 0 && progress.downloaded >= size)
    }
}

pub(super) fn leading_source(
    sources: &HashMap<usize, ActiveSource>,
    previous: Option<usize>,
) -> Option<usize> {
    sources
        .iter()
        .max_by_key(|(id, source)| {
            (
                source.reporter.snapshot().downloaded,
                previous == Some(**id),
                std::cmp::Reverse(**id),
            )
        })
        .map(|(id, _)| *id)
}

struct RaceState {
    id: String,
    started: Instant,
    total: usize,
    expected: Option<u64>,
    pending: VecDeque<(usize, DownloadCandidate)>,
    parked: VecDeque<(usize, DownloadCandidate)>,
    sources: HashMap<usize, ActiveSource>,
    retired: BTreeMap<usize, DownloadSourceProgress>,
    retiring: HashSet<usize>,
    primary: Option<usize>,
    displayed: Option<usize>,
    preferred: Option<usize>,
    probe: Option<usize>,
    probe_running: Option<usize>,
    locked: Option<usize>,
    locked_failed: Option<DownloadCandidate>,
    limit: usize,
    reserve_open: bool,
    last_error: Option<String>,
    last_switch: Option<Instant>,
    switched: bool,
}

impl RaceState {
    fn new(
        id: String,
        expected: Option<u64>,
        candidates: Vec<(usize, DownloadCandidate)>,
        probe: Option<usize>,
        preferred: Option<usize>,
        policy: RacePolicy,
    ) -> Self {
        Self {
            id,
            started: Instant::now(),
            expected,
            total: candidates.len(),
            pending: candidates.into(),
            parked: VecDeque::new(),
            sources: HashMap::new(),
            retired: BTreeMap::new(),
            retiring: HashSet::new(),
            primary: None,
            displayed: None,
            preferred,
            probe,
            probe_running: None,
            locked: None,
            locked_failed: None,
            limit: if preferred.is_some() {
                1
            } else {
                policy.parallelism
            },
            reserve_open: preferred.is_none(),
            last_error: None,
            last_switch: None,
            switched: false,
        }
    }

    fn can_start(&self) -> bool {
        self.locked_failed.is_none() && self.sources.len() + self.retiring.len() < self.limit
    }

    fn next(&mut self, session: Option<&RaceSession>) -> Option<(usize, DownloadCandidate)> {
        let locked = self.locked;
        let take_ready = |queue: &mut VecDeque<(usize, DownloadCandidate)>| {
            let index = queue.iter().position(|(id, _)| {
                !self.retiring.contains(id) && locked.is_none_or(|locked| locked == *id)
            })?;
            queue.remove(index)
        };
        let (id, candidate) = take_ready(&mut self.pending).or_else(|| {
            if locked.is_none() && self.sources.is_empty() {
                take_ready(&mut self.parked)
            } else {
                None
            }
        })?;
        if self.probe == Some(id) && locked.is_none() {
            self.probe = None;
            if session
                .zip(candidate.identity.as_ref())
                .is_some_and(|(session, route)| session.started(route, true))
            {
                self.probe_running = Some(id);
            } else {
                self.pending.push_back((id, candidate));
                if !self.reserve_open && self.sources.len() == 1 {
                    self.limit = 1;
                    return None;
                }
                return self.next(session);
            }
        } else if let Some((session, route)) = session.zip(candidate.identity.as_ref()) {
            session.started(route, false);
        }
        self.retired.remove(&id);
        self.primary.get_or_insert(id);
        Some((id, candidate))
    }

    fn stop(&mut self, id: usize, allow_later: bool) {
        let source = self.sources.remove(&id).unwrap();
        let mut progress = source.progress(id, self.expected, self.preferred);
        progress.phase = DownloadSourcePhase::Stopped;
        progress.speed_bytes = None;
        self.retired.insert(id, progress);
        self.retiring.insert(id);
        source.abort.abort();
        if allow_later {
            self.parked.push_back((id, source.candidate));
        }
        if self.probe_running == Some(id) {
            self.probe_running = None;
        }
    }

    fn action(&mut self, action: RaceAction, policy: RacePolicy, adaptive: bool) {
        match action {
            RaceAction::Lock(index) if self.locked.is_none() => {
                let Some(id) = index.checked_sub(1) else {
                    return;
                };
                if !self.sources.contains_key(&id)
                    || self
                        .sources
                        .values()
                        .any(|source| !source.lockable(self.expected))
                {
                    return;
                }
                self.locked = Some(id);
                self.primary = Some(id);
                self.limit = 1;
                for other in self
                    .sources
                    .keys()
                    .copied()
                    .filter(|other| *other != id)
                    .collect::<Vec<_>>()
                {
                    self.stop(other, true);
                }
            }
            RaceAction::Retry if self.locked_failed.is_some() => {
                let id = self.locked.unwrap();
                self.pending
                    .push_front((id, self.locked_failed.take().unwrap()));
            }
            RaceAction::Unlock if self.locked.take().is_some() => {
                let failed = self.locked_failed.take().is_some();
                if !adaptive {
                    self.primary = leading_source(&self.sources, self.primary);
                } else if failed {
                    self.primary = fastest_backup(&self.sources);
                }
                self.pending.append(&mut self.parked);
                self.limit = policy.parallelism;
                self.reserve_open = true;
            }
            _ => {}
        }
    }

    fn tick(&mut self, policy: RacePolicy, session: Option<&RaceSession>) {
        let now = Instant::now();
        let window = session.map_or(policy.window, |_| Duration::from_secs(2));
        for source in self.sources.values_mut() {
            source
                .speed
                .sample(now, source.reporter.transferred(), window);
        }
        if self.locked.is_some() {
            return;
        }
        let Some(session) = session else {
            if policy.reference_speed > 0
                && !self.pending.is_empty()
                && self
                    .last_switch
                    .is_none_or(|last| now.duration_since(last) >= policy.window)
                && let Some((id, fastest)) = switching_candidate(
                    &self.sources,
                    self.primary,
                    self.expected,
                    policy.reference_speed,
                )
            {
                log::info!(
                    "下载线路明显落后，尝试备用线路：候选序号={}，速度={}B/s，最快={}B/s，换线参考速度={}B/s",
                    id + 1,
                    self.sources[&id].speed.bytes_per_second,
                    fastest,
                    policy.reference_speed
                );
                self.last_switch = Some(now);
                self.stop(id, false);
            }
            return;
        };

        let Some(primary) = self.primary.and_then(|id| self.sources.get(&id)) else {
            return;
        };
        let latest_start = self.sources.values().map(|source| source.started).max();
        if self.limit < policy.parallelism
            && self.sources.len() == self.limit
            && (!self.pending.is_empty() || !self.parked.is_empty())
            && !self.sources.values().any(ActiveSource::verifying)
            && (self.limit == 1
                || latest_start.is_some_and(|start| now.duration_since(start) >= policy.soft_start))
        {
            let (slow_first_byte, slow_body) = match primary.reporter.first_byte_at() {
                None => (
                    now.duration_since(primary.started) >= policy.soft_start,
                    false,
                ),
                Some(first_byte) => {
                    let verified_speed = primary
                        .candidate
                        .identity
                        .as_ref()
                        .and_then(|route| session.verified_speed(route));
                    let slow = primary
                        .speed
                        .comparable_speed()
                        .zip(verified_speed)
                        .is_some_and(|(recent, recorded)| {
                            recent > 0 && recent.saturating_mul(2) < recorded
                        });
                    (false, primary.speed.stalled(now, first_byte) || slow)
                }
            };
            if slow_first_byte || slow_body {
                self.pending.append(&mut self.parked);
                self.limit += 1;
                self.reserve_open = true;
            }
        }
        if let Some(id) = self.probe_running
            && self.primary != Some(id)
            && let Some(probe) = self.sources.get(&id)
            && probe.lockable(self.expected)
            && probe_should_stop(primary, probe, self.expected)
        {
            if !self.reserve_open {
                self.limit = 1;
            }
            self.stop(id, true);
        }
    }
}

pub(super) async fn race_candidates<T, F, Fut, C>(
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
    Fut: Future<Output = Result<T, String>>,
    C: FnMut() -> bool,
{
    if is_cancelled() {
        return Err("download.cancelled".into());
    }
    let mut seen = HashSet::new();
    let mut candidates = candidates
        .into_iter()
        .filter(|candidate| seen.insert(candidate.url.clone()))
        .enumerate()
        .collect::<Vec<_>>();
    let real_race = policy.race && candidates.len() > 1;
    let session = (real_race
        && policy.parallelism > 1
        && candidates
            .iter()
            .all(|(_, candidate)| candidate.identity.is_some()))
    .then(|| context.as_ref().and_then(|context| context.race_session))
    .flatten();
    let expected = context
        .as_ref()
        .and_then(|context| context.expected_size)
        .filter(|size| *size > 0);
    let probe = session.and_then(|session| session.order(&mut candidates, expected));
    let preferred = candidates.first().and_then(|(id, candidate)| {
        session
            .zip(candidate.identity.as_ref())
            .filter(|(session, key)| session.history_preferred(key))
            .map(|_| *id)
    });
    let control = (real_race && context.is_some() && archive::interactive_download_progress())
        .then(RaceControl::register);
    let id = control.as_ref().map_or_else(
        || uuid::Uuid::new_v4().to_string(),
        |control| control.id().to_owned(),
    );
    let mut state = RaceState::new(id, expected, candidates, probe, preferred, policy);
    let label = context.as_ref().map_or("unknown", |context| context.label);
    let mut active = FuturesUnordered::new();
    let mut ticker = tokio::time::interval_at(Instant::now() + policy.tick, policy.tick);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        if is_cancelled() {
            return Err("download.cancelled".into());
        }
        if let Some(action) = control.as_ref().and_then(RaceControl::take) {
            state.action(action, policy, session.is_some());
        }
        while state.can_start() {
            if is_cancelled() {
                return Err("download.cancelled".into());
            }
            let Some((id, candidate)) = state.next(session) else {
                break;
            };
            let reporter = Arc::new(DownloadReporter::new(label));
            let (abort, registration) = AbortHandle::new_pair();
            log::info!(target: "app_lib::download", "启动下载候选：任务={}，渠道={:?}，地址={:?}，候选={}/{}", label, candidate.route, source_host(&candidate.url), id + 1, state.total);
            let operation = download(candidate.clone(), reporter.clone());
            state.sources.insert(
                id,
                ActiveSource {
                    candidate,
                    reporter,
                    abort,
                    speed: SourceSpeed::default(),
                    started: Instant::now(),
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
            if state.probe.is_some() && state.sources.len() == 1 && state.preferred.is_some() {
                state.limit = state.limit.max(2);
            }
        }
        if let Some(context) = &context {
            report_progress(state.progress(context, control.as_ref(), None));
        }
        if active.is_empty() && state.locked_failed.is_none() {
            if state.pending.is_empty() && state.parked.is_empty() {
                return Err(state
                    .last_error
                    .unwrap_or_else(|| "download.noCandidate".into()));
            }
            continue;
        }
        tokio::select! {
            biased;
            _ = async { match &control { Some(control) => control.changed().await, None => pending().await } } => {}
            result = active.next(), if !active.is_empty() => {
                let Some((id, result)) = result else { continue; };
                if state.retiring.remove(&id) { drop(result); continue; }
                let source = state.sources.remove(&id).unwrap();
                if state.probe_running == Some(id) {
                    state.probe_running = None;
                }
                match result {
                    Ok(Ok(mut completed)) => {
                        if is_cancelled() { return Err("download.cancelled".into()); }
                        for other in state.sources.keys().copied().collect::<Vec<_>>() { state.stop(other, false); }
                        drop(active);
                        if let Some((session, route)) = session.zip(source.candidate.identity.as_ref()) {
                            let first = source.reporter.first_byte_at();
                            let end = source.reporter.transfer_end().unwrap_or_else(Instant::now);
                            session.succeeded(route, source.reporter.snapshot().downloaded, first.map(|first| first.saturating_duration_since(source.started)), end.saturating_duration_since(first.unwrap_or(source.started)));
                        }
                        let mut row = source.progress(id, state.expected, state.preferred);
                        row.phase = DownloadSourcePhase::Finished;
                        row.speed_bytes = None;
                        log::info!(target: "app_lib::download", "下载竞速完成：任务={}，胜出地址={:?}，已收字节={}，elapsed_ms={}", label, row.source, row.downloaded, source.started.elapsed().as_millis());
                        state.retired.insert(id, row);
                        if let Some(context) = &context { report_progress(state.progress(context, control.as_ref(), Some(id))); }
                        return Ok(completed.0.take().unwrap());
                    }
                    Ok(Err(error)) if error == "download.cancelled" || is_cancelled() => return Err("download.cancelled".into()),
                    Ok(Err(error)) if policy.terminal_error == Some(error.as_str()) => return Err(error),
                    Ok(Err(error)) => {
                        let mut row = source.progress(id, state.expected, state.preferred);
                        let tag = crate::locale_tag::diagnostic_tag(&error);
                        log::warn!(target: "app_lib::download", "下载候选失败，补充其他候选：任务={}，候选={}/{}，渠道={:?}，地址={:?}，已收字节={}，预期字节={:?}，elapsed_ms={}，原因={}",
                            label, id + 1, state.total, row.route, row.source, source.reporter.transferred(), row.total, source.started.elapsed().as_millis(), tag);
                        row.phase = DownloadSourcePhase::Failed;
                        row.speed_bytes = None;
                        row.error = (tag != "unclassified").then(|| tag.to_owned());
                        state.retired.insert(id, row);
                        state.last_error = Some(error);
                        if state.locked == Some(id) {
                            state.locked_failed = Some(source.candidate);
                        } else if state.primary == Some(id) {
                            state.primary = if session.is_some() {
                                fastest_backup(&state.sources)
                            } else {
                                leading_source(&state.sources, None)
                            };
                            state.switched = true;
                            state.reserve_open = true;
                            state.pending.append(&mut state.parked);
                        }
                    }
                    Err(_) => unreachable!("retiring work is removed before consuming its result"),
                }
            }
            _ = ticker.tick() => {
                if session.is_none() && state.locked.is_none() { state.primary = leading_source(&state.sources, state.primary); }
                state.tick(policy, session);
            }
        }
    }
}
