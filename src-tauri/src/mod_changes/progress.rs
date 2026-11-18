//! Per-invocation preparation progress, backed by the existing download snapshots.
use super::{
    model::ChangeAction,
    session::{ChangePlan, PlanSession},
};
use crate::archive::DownloadProgress;
use serde::Serialize;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PreparationStage {
    Resolving,
    Downloading,
    Verifying,
    Finalizing,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ItemStage {
    Queued,
    Downloading,
    Verifying,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PreparationItem {
    pub node_id: String,
    pub candidate_id: String,
    pub label: String,
    pub tag: Option<String>,
    pub stage: ItemStage,
    pub download: Option<DownloadProgress>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ChangePreparationProgress {
    /// Always the ID supplied to this prepare call, including a revised result.
    pub plan_id: String,
    pub stage: PreparationStage,
    pub active_node_id: Option<String>,
    pub actions: Vec<ChangeAction>,
    /// The current solution, not an append-only list of abandoned candidates.
    pub items: Vec<PreparationItem>,
    pub error: Option<String>,
}

/// The final snapshot is returned with the IPC result as well as streamed. A
/// large Channel message can arrive after the enclosing invoke has settled.
#[derive(Serialize)]
pub(crate) struct PreparationOutcome {
    pub plan: Option<ChangePlan>,
    pub error: Option<String>,
    pub progress: ChangePreparationProgress,
}

#[derive(Clone)]
pub(super) struct PreparationReporter {
    snapshot: Arc<Mutex<ChangePreparationProgress>>,
    report: Arc<dyn Fn(ChangePreparationProgress) + Send + Sync>,
}

impl PreparationReporter {
    pub(super) fn new(
        id: String,
        report: impl Fn(ChangePreparationProgress) + Send + Sync + 'static,
    ) -> Self {
        Self {
            snapshot: Arc::new(Mutex::new(ChangePreparationProgress {
                plan_id: id,
                stage: PreparationStage::Resolving,
                active_node_id: None,
                actions: vec![],
                items: vec![],
                error: None,
            })),
            report: Arc::new(report),
        }
    }

    fn update(&self, update: impl FnOnce(&mut ChangePreparationProgress)) {
        let snapshot = {
            let mut snapshot = self.snapshot.lock().unwrap_or_else(|e| e.into_inner());
            update(&mut snapshot);
            snapshot.clone()
        };
        (self.report)(snapshot);
    }

    pub(super) fn synchronize(&self, session: &PlanSession) {
        self.update(|snapshot| {
            snapshot.actions = session.plan.result.actions.clone();
            let previous = std::mem::take(&mut snapshot.items);
            if let Some(solved) = &session.solved {
                for node in &solved.nodes {
                    let Some(candidate) = node.current.as_ref() else {
                        continue;
                    };
                    if !session
                        .plan
                        .result
                        .actions
                        .iter()
                        .any(|a| a.node_id == node.id)
                    {
                        continue;
                    }
                    let mut item = previous
                        .iter()
                        .find(|item| item.node_id == node.id && item.candidate_id == candidate.id)
                        .cloned()
                        .unwrap_or_else(|| PreparationItem {
                            node_id: node.id.clone(),
                            candidate_id: candidate.id.clone(),
                            label: session
                                .trusted
                                .files
                                .get(&node.id)
                                .map(|files| files.summary.label.clone())
                                .unwrap_or_default(),
                            tag: candidate.tag.clone(),
                            stage: ItemStage::Queued,
                            download: None,
                        });
                    if !candidate.verification_pending {
                        item.stage = ItemStage::Ready;
                    }
                    snapshot.items.push(item);
                }
            }
            snapshot.stage = PreparationStage::Resolving;
            snapshot.active_node_id = None;
        });
    }

    pub(super) fn stage(&self, stage: PreparationStage, node_id: Option<&str>) {
        self.update(|snapshot| {
            snapshot.stage = stage;
            snapshot.active_node_id = node_id.map(str::to_string);
            if let Some(item) = snapshot
                .items
                .iter_mut()
                .find(|item| Some(item.node_id.as_str()) == node_id)
            {
                item.stage = match stage {
                    PreparationStage::Downloading => ItemStage::Downloading,
                    PreparationStage::Verifying => ItemStage::Verifying,
                    _ => item.stage,
                };
            }
        });
    }

    pub(super) fn download(&self, node_id: &str, candidate_id: &str, download: DownloadProgress) {
        self.update(|snapshot| {
            let Some(item) = snapshot
                .items
                .iter_mut()
                .find(|item| item.node_id == node_id && item.candidate_id == candidate_id)
            else {
                return;
            };
            // Transfer completion includes archive integrity, not actual member
            // inspection. Keep its bytes while the package is being verified.
            snapshot.stage = if download.finished {
                PreparationStage::Verifying
            } else {
                PreparationStage::Downloading
            };
            snapshot.active_node_id = Some(node_id.to_string());
            item.stage = if download.finished {
                ItemStage::Verifying
            } else {
                ItemStage::Downloading
            };
            item.download = Some(download);
        });
    }

    pub(super) fn verified(&self, node_id: &str, candidate_id: &str) {
        self.update(|snapshot| {
            if let Some(item) = snapshot
                .items
                .iter_mut()
                .find(|item| item.node_id == node_id && item.candidate_id == candidate_id)
            {
                item.stage = ItemStage::Ready;
            }
            snapshot.active_node_id = None;
            snapshot.stage = PreparationStage::Resolving;
        });
    }

    pub(super) fn failed(&self, error: &str) {
        self.update(|snapshot| {
            if let Some(item) = snapshot
                .items
                .iter_mut()
                .find(|item| snapshot.active_node_id.as_deref() == Some(&item.node_id))
            {
                item.stage = ItemStage::Failed;
            }
            snapshot.stage = PreparationStage::Failed;
            snapshot.error = Some(error.to_string());
        });
    }

    pub(super) fn finish(&self, outcome: Result<ChangePlan, String>) -> PreparationOutcome {
        let (plan, error) = match outcome {
            Ok(plan) => {
                if plan.prepared {
                    self.stage(PreparationStage::Ready, None);
                }
                (Some(plan), None)
            }
            Err(error) => {
                self.failed(&error);
                (None, Some(error))
            }
        };
        PreparationOutcome {
            plan,
            error,
            progress: self
                .snapshot
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
        }
    }
}

#[cfg(test)]
mod tests;
