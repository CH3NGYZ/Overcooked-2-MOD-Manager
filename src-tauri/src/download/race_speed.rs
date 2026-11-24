use super::race::ActiveSource;
use std::cmp::Reverse;
use std::collections::{HashMap, VecDeque};
use std::time::Duration;
use tokio::time::Instant;

#[cfg(test)]
mod tests;

#[derive(Default)]
pub(super) struct SourceSpeed {
    samples: VecDeque<(Instant, u64)>,
    has_full_window: bool,
    pub(super) bytes_per_second: u64,
    last_advance: Option<Instant>,
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

impl SourceSpeed {
    pub(super) fn sample(&mut self, now: Instant, transferred: u64, window: Duration) {
        // The first tick with real data is the baseline. Connection and first-byte
        // waits are handled by the existing deadlines, not by a low speed estimate.
        if self.samples.is_empty() && transferred == 0 {
            return;
        }
        if self
            .samples
            .back()
            .is_none_or(|(_, bytes)| transferred > *bytes)
        {
            self.last_advance = Some(now);
        }
        self.samples.push_back((now, transferred));
        while self.samples.len() > 1 && now.duration_since(self.samples[1].0) >= window {
            self.samples.pop_front();
        }
        let (beginning, previous) = self.samples[0];
        let elapsed = now.duration_since(beginning);
        self.has_full_window = elapsed >= window;
        self.bytes_per_second =
            (transferred.saturating_sub(previous) as f64 / elapsed.as_secs_f64().max(0.001)) as u64;
    }

    pub(super) fn comparable_speed(&self) -> Option<u64> {
        self.has_full_window.then_some(self.bytes_per_second)
    }

    pub(super) fn display_speed(&self) -> Option<u64> {
        (self.samples.len() >= 2).then_some(self.bytes_per_second)
    }

    pub(super) fn stalled(&self, now: Instant, first_byte: Instant) -> bool {
        now.duration_since(self.last_advance.unwrap_or(first_byte)) >= Duration::from_secs(3)
    }
}

pub(super) fn fastest_backup(sources: &HashMap<usize, ActiveSource>) -> Option<usize> {
    sources
        .iter()
        .max_by_key(|(id, source)| {
            (
                source.verifying(),
                source.speed.display_speed().unwrap_or_default(),
                source.reporter.snapshot().downloaded,
                Reverse(**id),
            )
        })
        .map(|(id, _)| *id)
}

pub(super) fn probe_should_stop(
    primary: &ActiveSource,
    probe: &ActiveSource,
    expected: Option<u64>,
) -> bool {
    if primary.verifying() {
        return false;
    }
    let primary_progress = primary.reporter.snapshot();
    let probe_progress = probe.reporter.snapshot();
    let primary_total = expected.or(primary_progress.total).filter(|size| *size > 0);
    let probe_total = expected.or(probe_progress.total).filter(|size| *size > 0);
    let total = primary_total.filter(|size| Some(*size) == probe_total);
    let sample_bytes = total.map_or(1024 * 1024, |size| {
        size.min((size / 10).clamp(256 * 1024, 2 * 1024 * 1024))
    });
    if probe_progress.downloaded < sample_bytes {
        return false;
    }
    let (Some(primary_speed), Some(probe_speed)) =
        (primary.speed.display_speed(), probe.speed.display_speed())
    else {
        return false;
    };
    if primary_speed == 0 {
        return false;
    }
    if let Some(size) = total {
        let primary_remaining = u128::from(size.saturating_sub(primary_progress.downloaded));
        let probe_remaining = u128::from(size.saturating_sub(probe_progress.downloaded));
        return probe_remaining * u128::from(primary_speed)
            > primary_remaining * u128::from(probe_speed);
    }
    probe_speed.saturating_mul(4) < primary_speed.saturating_mul(3)
}

pub(super) fn switching_candidate(
    sources: &HashMap<usize, ActiveSource>,
    progress_leader: Option<usize>,
    expected_size: Option<u64>,
    reference_speed: u64,
) -> Option<(usize, u64)> {
    let mut fastest = 0;
    let mut laggard = None;
    for (&id, source) in sources {
        let Some(speed) = source.speed.comparable_speed() else {
            continue;
        };
        let progress = source.reporter.snapshot();
        let size = expected_size.or(progress.total.filter(|size| *size > 0));
        // A source that already has the file may still be verifying its contents.
        if progress.finished || size.is_some_and(|size| progress.downloaded >= size) {
            continue;
        }
        fastest = fastest.max(speed);
        if Some(id) != progress_leader && speed < reference_speed {
            let rank = (speed, progress.downloaded, Reverse(id));
            if laggard.is_none_or(|previous| rank < previous) {
                laggard = Some(rank);
            }
        }
    }
    let (speed, _, Reverse(id)) = laggard?;
    // Keep similarly paced routes, even when all are well below the setting.
    // Strict comparison also keeps every source if there is no positive sample.
    (speed.saturating_mul(2) < fastest).then_some((id, fastest))
}
