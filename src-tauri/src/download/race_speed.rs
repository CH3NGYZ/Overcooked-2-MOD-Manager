use super::ActiveSource;
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
}

impl SourceSpeed {
    pub(super) fn sample(&mut self, now: Instant, transferred: u64, window: Duration) {
        // The first tick with real data is the baseline. Connection and first-byte
        // waits are handled by the existing deadlines, not by a low speed estimate.
        if self.samples.is_empty() && transferred == 0 {
            return;
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
