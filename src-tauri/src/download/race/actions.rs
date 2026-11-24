use super::{RacePolicy, RaceState, fastest_backup, leading_source};
use crate::download::control::RaceAction;

impl RaceState {
    pub(super) fn action(&mut self, action: RaceAction, policy: RacePolicy, adaptive: bool) {
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
                let mut others: Vec<_> = self
                    .sources
                    .keys()
                    .copied()
                    .filter(|other| *other != id)
                    .collect();
                others.sort_unstable();
                for other in others {
                    let source = self.abort_source(other);
                    self.pending.push_back((other, source.candidate));
                }
            }
            RaceAction::Retry if let Some(id) = self.locked_failed() => {
                self.retired
                    .get_mut(&id)
                    .unwrap()
                    .mark_queued(self.expected);
                self.pending
                    .push_front((id, self.retryable.remove(&id).unwrap()));
            }
            RaceAction::Requeue(index) => {
                let Some(id) = index.checked_sub(1) else {
                    return;
                };
                if self.locked == Some(id) {
                    return;
                }
                let candidate = self.retryable.remove(&id).or_else(|| {
                    let position = self.parked.iter().position(|(parked, _)| *parked == id)?;
                    self.parked.remove(position).map(|(_, candidate)| candidate)
                });
                if let Some(candidate) = candidate {
                    self.retired
                        .get_mut(&id)
                        .unwrap()
                        .mark_queued(self.expected);
                    self.pending.push_back((id, candidate));
                }
            }
            RaceAction::Unlock if self.locked.is_some() => {
                let failed = self.locked_failed().is_some();
                self.locked = None;
                if !adaptive {
                    self.primary = leading_source(&self.sources, self.primary);
                } else if failed {
                    self.primary = fastest_backup(&self.sources);
                }
                self.release_parked();
                self.limit = policy.parallelism;
                self.reserve_open = true;
            }
            _ => {}
        }
    }
}
