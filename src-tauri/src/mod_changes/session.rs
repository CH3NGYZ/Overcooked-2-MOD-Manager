//! Backend-owned plan IDs bind approval to one inventory and prepared payload set.
use super::{install_notes::InstallNote, inventory::*, model::*};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, sync::Arc};

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct PlanRequest {
    #[serde(default)]
    pub roots: Vec<RootRequest>,
    pub map_import: Option<MapImport>,
}
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RootRequest {
    #[serde(flatten)]
    pub intent: RootIntent,
    pub asset_name: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct MapImport {
    pub zip_path: String,
    pub selected: Vec<String>,
    pub origin: Option<crate::models::MapInstallOrigin>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RootSummary {
    pub node_id: String,
    pub label: String,
    pub tag: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ChangePlan {
    pub plan_id: String,
    #[serde(flatten)]
    pub result: SolveResult,
    pub roots: Vec<RootSummary>,
    pub nodes: Vec<NodeSummary>,
    pub install_notes: Vec<InstallNote>,
    pub prepared: bool,
}

#[derive(Clone)]
pub(crate) struct Payload {
    pub candidate: Candidate,
    pub stage_root: PathBuf,
    pub files: Vec<String>,
    pub manifest: Option<crate::archive::InstallManifest>,
}

pub(crate) struct PlanSession {
    pub trusted: TrustedInventory,
    pub request: ChangeRequest,
    pub wire_request: PlanRequest,
    pub plan: ChangePlan,
    pub solved: Option<Inventory>,
    pub payloads: HashMap<String, Payload>,
    /// Retain every verified asset across failed preparation and graph revisions.
    pub verified_payloads: HashMap<(String, String), Payload>,
    pub map_payload: Option<super::map_payload::MapBatch>,
    pub staging: PathBuf,
    /// Exact staged bytes, checked again before applying anything to the game.
    pub payload_fingerprints: HashMap<PathBuf, super::transaction::Fingerprint>,
    /// Frozen under preparation; execute must not recapture changed live targets.
    pub mutations: Option<Vec<super::transaction::Mutation>>,
    pub created: std::time::Instant,
}

#[derive(Default)]
pub(crate) struct ChangeSessions {
    pub plans: HashMap<String, PlanSession>,
    pub snapshot: Option<Arc<TrustedInventory>>,
    pub busy: Option<String>,
}

/// A short mutex protects ownership only. Network and filesystem work hold no session lock.
pub(crate) struct SessionLease {
    state: crate::app_state::AppState,
    pub session: Option<PlanSession>,
}
impl SessionLease {
    pub fn creating(state: &crate::app_state::AppState) -> Result<Self, String> {
        let expired = {
            let mut sessions = state.mod_changes.lock().map_err(|_| "lock.poisoned")?;
            if sessions.busy.is_some() {
                return Err("modChanges.busy".into());
            }
            let ids = sessions
                .plans
                .iter()
                .filter(|(_, session)| {
                    session.created.elapsed() >= std::time::Duration::from_secs(3600)
                })
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>();
            ids.into_iter()
                .filter_map(|id| sessions.plans.remove(&id))
                .collect::<Vec<_>>()
        };
        for session in expired {
            if let Err(error) = super::execution::cleanup(state, &session.staging) {
                log::warn!("过期整链暂存未清理：{error}");
            }
        }
        let mut sessions = state.mod_changes.lock().map_err(|_| "lock.poisoned")?;
        if sessions.busy.is_some() || sessions.plans.len() >= 16 {
            return Err("modChanges.busy".into());
        }
        sessions.busy = Some(uuid::Uuid::new_v4().to_string());
        Ok(Self {
            state: state.clone(),
            session: None,
        })
    }

    pub fn take(state: &crate::app_state::AppState, id: &str) -> Result<Self, String> {
        let mut sessions = state.mod_changes.lock().map_err(|_| "lock.poisoned")?;
        if sessions.busy.is_some() {
            return Err("modChanges.busy".into());
        }
        let session = sessions.plans.remove(id).ok_or("modChanges.planExpired")?;
        if session.created.elapsed() >= std::time::Duration::from_secs(3600) {
            drop(sessions);
            if let Err(error) = super::execution::cleanup(state, &session.staging) {
                log::warn!("过期整链暂存未清理：{error}");
            }
            return Err("modChanges.planExpired".into());
        }
        sessions.busy = Some(id.to_string());
        Ok(Self {
            state: state.clone(),
            session: Some(session),
        })
    }
}
impl Drop for SessionLease {
    fn drop(&mut self) {
        if let Ok(mut sessions) = self.state.mod_changes.lock() {
            sessions.busy = None;
            if let Some(session) = self.session.take() {
                sessions.insert(session);
            }
        }
    }
}

impl ChangeSessions {
    pub fn invalidate(&mut self) {
        self.snapshot = None;
    }
    pub fn insert(&mut self, session: PlanSession) {
        self.plans.insert(session.plan.plan_id.clone(), session);
    }
}

pub(crate) fn presentation(
    trusted: &TrustedInventory,
    request: &ChangeRequest,
    result: SolveResult,
    id: String,
    prepared: bool,
) -> ChangePlan {
    ChangePlan {
        plan_id: id,
        result,
        prepared,
        roots: request
            .roots
            .iter()
            .map(|r| RootSummary {
                node_id: r.node_id.clone(),
                label: trusted
                    .files
                    .get(&r.node_id)
                    .map(|f| f.summary.label.clone())
                    .unwrap_or_else(|| r.node_id.clone()),
                tag: r
                    .candidate_id
                    .as_ref()
                    .and_then(|id| {
                        trusted
                            .inventory
                            .nodes
                            .iter()
                            .flat_map(|n| &n.candidates)
                            .find(|c| &c.id == id)
                    })
                    .and_then(|c| c.tag.clone()),
            })
            .collect(),
        nodes: trusted.files.values().map(|f| f.summary.clone()).collect(),
        install_notes: Vec::new(),
    }
}
