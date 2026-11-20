//! Backend-owned plan IDs bind approval to one inventory and prepared payload set.
use super::{install_notes::InstallNote, inventory::*, model::*, transaction::Fingerprint};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::PathBuf,
    sync::Arc,
};

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct PlanRequest {
    #[serde(default)]
    pub roots: Vec<RootRequest>,
    pub repair_dependencies_of: Option<String>,
    pub map_import: Option<MapImport>,
    pub toggle_scope: Option<ToggleScope>,
    #[serde(default)]
    pub bulk_toggle: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ToggleScope {
    pub generation: u64,
    pub revision: String,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RelationLabel {
    pub label: String,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub name_i18n: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ChangePlan {
    pub plan_id: String,
    #[serde(flatten)]
    pub result: SolveResult,
    pub roots: Vec<RootSummary>,
    pub nodes: Vec<NodeSummary>,
    /// Source titles for referents that may have no installed or evaluated provider.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub relation_labels: BTreeMap<String, RelationLabel>,
    /// Member keys that were effectively enabled before a whole-node disable.
    /// The solved node summaries describe the final state, so cannot show these.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub previous_enabled_members: BTreeMap<String, Vec<String>>,
    /// Physical older copies consumed by a package replacement, grouped by its action's node.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub retired_mod_copies: BTreeMap<String, Vec<RetiredModCopy>>,
    pub install_notes: Vec<InstallNote>,
    pub prepared: bool,
}

#[derive(Clone)]
pub(crate) struct Payload {
    pub candidate: Candidate,
    pub stage_root: PathBuf,
    pub stage_fingerprint: Fingerprint,
    /// Only a Release ZIP verified against its cached SHA-256 supplies proof
    /// that a loose file is identical to the package's original staged bytes.
    pub release_files: Option<Arc<HashMap<String, Fingerprint>>>,
    pub files: Vec<String>,
    pub manifest: Option<crate::archive::InstallManifest>,
    /// DLL roles read alongside the verified candidate, keyed by its physical member keys.
    pub member_roles: BTreeMap<String, crate::models::DllRole>,
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
    /// Exact selected Release claims over previously unowned live files.
    pub(super) residue_adoptions: super::mutations::ResidueAdoptions,
    /// Frozen under preparation; execute must not recapture changed live targets.
    pub(super) mutations: Option<Vec<super::application_progress::OwnedMutation>>,
    pub created: std::time::Instant,
}

impl PlanSession {
    /// Only the requested installed switches can omit their own declarations.
    /// Roots from a repair or an explicit release request keep full validation.
    pub(super) fn unbound_toggle_roots(&self) -> BTreeSet<&str> {
        let wire = &self.wire_request;
        if (wire.toggle_scope.is_none() && !wire.bulk_toggle)
            || wire.map_import.is_some()
            || wire.roots.iter().any(|root| root.asset_name.is_some())
            || self
                .request
                .roots
                .iter()
                .any(|root| root.candidate_id.is_some())
        {
            return BTreeSet::new();
        }
        self.request
            .roots
            .iter()
            .filter_map(|root| {
                self.trusted
                    .toggle_unbound_nodes
                    .contains(&root.node_id)
                    .then_some(root.node_id.as_str())
            })
            .collect()
    }
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
    let disabled_nodes: BTreeSet<_> = result
        .actions
        .iter()
        .filter(|action| action.kind == ActionKind::Disable && action.member_key.is_none())
        .map(|action| action.node_id.as_str())
        .collect();
    let previous_enabled_members = trusted
        .inventory
        .nodes
        .iter()
        .filter(|node| node.enabled && disabled_nodes.contains(node.id.as_str()))
        .map(|node| {
            let enabled_keys = node
                .current
                .iter()
                .flat_map(|candidate| &candidate.provides)
                .filter(|member| member.enabled)
                .filter_map(|member| member.member_key.clone())
                .collect();
            (node.id.clone(), enabled_keys)
        })
        .collect();
    ChangePlan {
        plan_id: id,
        relation_labels: relation_labels(trusted, &result),
        result,
        prepared,
        roots: request
            .roots
            .iter()
            .map(|root| (&root.node_id, root.candidate_id.as_ref()))
            .chain(request.repair_dependencies_of.iter().map(|id| (id, None)))
            .map(|(node_id, candidate_id)| RootSummary {
                node_id: node_id.clone(),
                label: trusted
                    .files
                    .get(node_id)
                    .map(|f| f.summary.label.clone())
                    .unwrap_or_else(|| node_id.clone()),
                tag: candidate_id
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
        previous_enabled_members,
        retired_mod_copies: BTreeMap::new(),
        install_notes: Vec::new(),
    }
}

fn relation_labels(
    trusted: &TrustedInventory,
    result: &SolveResult,
) -> BTreeMap<String, RelationLabel> {
    let mut needed = BTreeSet::new();
    let mut add_guid = |guid: &str| {
        let guid = guid.trim().to_ascii_lowercase();
        if !guid.is_empty() {
            needed.insert(guid);
        }
    };
    for blocker in &result.blockers {
        if let Some(guid) = &blocker.target_guid {
            add_guid(guid);
        }
        for relation in blocker
            .traces
            .iter()
            .flatten()
            .filter_map(|step| step.relation.as_ref())
        {
            add_guid(&relation.guid);
        }
    }
    for guid in result
        .actions
        .iter()
        .flat_map(|a| &a.reason_details)
        .filter_map(|reason| reason.target_guid.as_deref())
    {
        add_guid(guid);
    }
    let config = &trusted.config;
    let mut labels: BTreeMap<_, Option<RelationLabel>> = BTreeMap::new();
    for repo in &config.repositories {
        let guid = repo.plugin_guid.trim().to_ascii_lowercase();
        if !needed.contains(&guid)
            || !crate::config::mod_repository_is_available(config, repo, &config.region)
        {
            continue;
        }
        let proposed = RelationLabel {
            label: repo.name.clone(),
            name_i18n: repo.name_i18n.clone(),
        };
        labels
            .entry(guid)
            .and_modify(|existing| {
                // Channels with different titles cannot safely name an unknown provider.
                if existing.as_ref() != Some(&proposed) {
                    *existing = None;
                }
            })
            .or_insert(Some(proposed));
    }
    labels
        .into_iter()
        .filter_map(|(guid, label)| Some((guid, label?)))
        .collect()
}
