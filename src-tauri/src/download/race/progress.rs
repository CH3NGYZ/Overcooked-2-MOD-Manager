use super::{
    ActiveSource, DownloadContext, DownloadProgress, DownloadSourcePhase, RaceControl, RaceState,
    source_host,
};
use crate::archive::{DownloadPhase, DownloadRaceProgress, DownloadSourceProgress};
use std::cmp::Reverse;
use std::collections::BTreeMap;

impl ActiveSource {
    pub(super) fn progress(
        &self,
        id: usize,
        expected: Option<u64>,
        preferred: Option<usize>,
    ) -> DownloadSourceProgress {
        let snapshot = self.reporter.snapshot();
        let phase = if self.verifying() {
            DownloadSourcePhase::Verifying
        } else if snapshot.downloaded > 0 {
            DownloadSourcePhase::Downloading
        } else {
            DownloadSourcePhase::Connecting
        };
        DownloadSourceProgress {
            id: id + 1,
            route: self.candidate.route,
            source: source_host(&self.candidate.url),
            downloaded: snapshot.downloaded,
            total: expected.or(snapshot.total.filter(|size| *size > 0)),
            speed_bytes: if phase == DownloadSourcePhase::Downloading {
                self.speed.display_speed()
            } else {
                None
            },
            phase,
            history_preferred: preferred == Some(id),
            can_requeue: false,
            error: None,
        }
    }
}

impl DownloadSourceProgress {
    pub(super) fn mark_queued(&mut self, expected: Option<u64>) {
        self.phase = DownloadSourcePhase::Queued;
        self.downloaded = 0;
        self.total = expected;
        self.speed_bytes = None;
        self.error = None;
    }
}

fn leads_by_more_than_ten_percent(
    challenger: &DownloadSourceProgress,
    incumbent: &DownloadSourceProgress,
) -> bool {
    let (lead, basis) = match incumbent
        .total
        .filter(|total| Some(*total) == challenger.total)
    {
        Some(total) => (
            challenger
                .downloaded
                .min(total)
                .saturating_sub(incumbent.downloaded.min(total)),
            total,
        ),
        None => (
            challenger.downloaded.saturating_sub(incumbent.downloaded),
            incumbent.downloaded,
        ),
    };
    u128::from(lead) * 10 > u128::from(basis)
}

impl RaceState {
    pub(super) fn progress(
        &mut self,
        context: &DownloadContext<'_>,
        control: Option<&RaceControl>,
        winner: Option<usize>,
    ) -> DownloadProgress {
        let mut progress = DownloadProgress::connecting(context.label);
        progress.set_file_name(context.file_name);
        let mut rows = self.retired.clone();
        let may_requeue = control.is_some()
            && winner.is_none()
            && (!self.sources.is_empty()
                || !self.pending.is_empty()
                || !self.parked.is_empty()
                || self.locked_failed().is_some());
        let mut requeueable = Vec::new();
        if winner.is_some() {
            rows.retain(|_, row| row.phase != DownloadSourcePhase::Queued);
        } else if may_requeue {
            for id in self
                .retryable
                .keys()
                .copied()
                .chain(self.parked.iter().map(|(id, _)| *id))
            {
                if self.locked != Some(id)
                    && let Some(row) = rows.get_mut(&id)
                {
                    row.can_requeue = true;
                    requeueable.push(row.id);
                }
            }
        }
        rows.extend(
            self.sources
                .iter()
                .map(|(&id, source)| (id, source.progress(id, self.expected, self.preferred))),
        );
        let displayed = winner
            .or(self.locked)
            .or_else(|| self.choose_progress_source(&rows));
        self.displayed = displayed;
        progress.total = self.expected;
        if let Some(row) = displayed.and_then(|id| rows.get(&id)) {
            progress.total = row.total;
            progress.downloaded = row.downloaded;
            progress.speed_bytes = row.speed_bytes.unwrap_or_default();
            progress.route = Some(row.route);
            progress.source = row.source.clone();
            progress.phase = match row.phase {
                DownloadSourcePhase::Connecting if self.switched => DownloadPhase::Switching,
                DownloadSourcePhase::Downloading => DownloadPhase::Downloading,
                DownloadSourcePhase::Verifying => DownloadPhase::Verifying,
                DownloadSourcePhase::Failed if self.locked_failed().is_some() => {
                    DownloadPhase::LockedFailed
                }
                DownloadSourcePhase::Finished => DownloadPhase::Finished,
                _ => DownloadPhase::Connecting,
            };
        }
        progress.finished = winner.is_some();
        if self.total > 1 {
            let lockable = if control.is_some()
                && winner.is_none()
                && self.locked.is_none()
                && self
                    .sources
                    .values()
                    .all(|source| source.lockable(self.expected))
            {
                self.sources.keys().map(|id| id + 1).collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            let can_lock = !lockable.is_empty();
            if let Some(control) = control {
                control.update(
                    lockable,
                    requeueable,
                    winner.is_none() && self.locked_failed().is_some(),
                    winner.is_none() && self.locked.is_some(),
                );
            }
            progress.race = Some(DownloadRaceProgress {
                id: self.id.clone(),
                primary_id: displayed.map(|id| id + 1),
                locked_id: self.locked.map(|id| id + 1),
                can_lock,
                waiting: if winner.is_some() {
                    0
                } else {
                    self.pending.len()
                },
                elapsed_ms: self.started.elapsed().as_millis() as u64,
                sources: rows.into_values().collect(),
            });
        }
        progress
    }

    fn choose_progress_source(
        &self,
        rows: &BTreeMap<usize, DownloadSourceProgress>,
    ) -> Option<usize> {
        let previous = self.displayed.filter(|id| self.sources.contains_key(id));
        let incumbent = previous.and_then(|id| rows.get(&id));
        rows.iter()
            .filter(|(id, row)| {
                self.sources.contains_key(id)
                    && incumbent
                        .is_none_or(|incumbent| leads_by_more_than_ten_percent(row, incumbent))
            })
            .max_by_key(|(id, row)| (row.downloaded, self.primary == Some(**id), Reverse(**id)))
            .map(|(&id, _)| id)
            .or(previous)
    }
}
