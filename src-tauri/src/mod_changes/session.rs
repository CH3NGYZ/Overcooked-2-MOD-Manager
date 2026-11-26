//! Backend-owned plan IDs bind approval to one inventory and prepared payload set.
use super::{install_notes::InstallNote, inventory::*, model::*, transaction::Fingerprint};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt::Arguments,
    path::PathBuf,
    sync::Arc,
    time::Instant,
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
    #[serde(rename = "manifestOverwrite")]
    pub manifest_overwrite: Option<ManifestOverwriteIntent>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ManifestOverwriteIntent {
    pub repository_id: String,
    pub fingerprint: String,
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
    pub download_id: Option<uuid::Uuid>,
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
pub(crate) struct UntrackedOverwrite {
    pub node_id: String,
    pub path: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub unverified_source: bool,
    /// Durable backup path relative to the manager's data root.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ChangePlan {
    pub plan_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replan_scope: Option<String>,
    #[serde(flatten)]
    pub result: SolveResult,
    pub roots: Vec<RootSummary>,
    pub nodes: Vec<NodeSummary>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub bundled_dlls: BTreeMap<String, Vec<super::bundled_display::BundledDll>>,
    /// Source titles for referents that may have no installed or evaluated provider.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub relation_labels: BTreeMap<String, RelationLabel>,
    /// Physical older copies consumed by a package replacement, grouped by its action's node.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub retired_mod_copies: BTreeMap<String, Vec<RetiredModCopy>>,
    /// Existing loose files covered by the prepared package replacement.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub untracked_overwrites: Vec<UntrackedOverwrite>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub provider_choices: Vec<super::evidence::ProviderEvidence>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub recovery_conflicts: Vec<super::recovery_scope::Conflict>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_approval: Option<String>,
    pub install_notes: Vec<InstallNote>,
    pub prepared: bool,
}

pub(super) const DETAIL_LIMIT: usize = 24;
pub(super) const VERIFICATION_SUMMARY_STAGE: &str = "verification_summary";

pub(super) fn log_detail(value: Arguments<'_>) -> String {
    let value = crate::http_diagnostics::redact_text(&value.to_string());
    let Some((end, _)) = value.char_indices().nth(1400) else {
        return value;
    };
    format!("{}…", &value[..end])
}

pub(super) struct OperationTrace<'a> {
    pub op: &'a str,
    pub stage: &'static str,
    started: Instant,
}

impl<'a> OperationTrace<'a> {
    pub(super) fn new(op: &'a str, stage: &'static str) -> Self {
        Self {
            op,
            stage,
            started: Instant::now(),
        }
    }

    pub(super) fn elapsed_ms(&self) -> u128 {
        self.started.elapsed().as_millis()
    }

    pub(super) fn failure(&self, step: &str, error: &str) {
        let op = self.op;
        let stage = self.stage;
        let elapsed_ms = self.elapsed_ms();
        let reason = crate::locale_tag::diagnostic_tag(error);
        if error == "download.cancelled" {
            log::info!(
                "整链取消 op={op} step={step} stage={stage} elapsed_ms={elapsed_ms} reason={reason}"
            );
        } else {
            log::warn!(
                "整链失败 op={op} step={step} stage={stage} elapsed_ms={elapsed_ms} reason={reason}"
            );
        }
    }
}

pub(super) fn log_plan(op: &str, stage: &str, plan: &ChangePlan, elapsed_ms: u128, detail: bool) {
    let result = &plan.result;
    log::info!(
        "整链方案 op={op} stage={stage} plan={} status={:?} prepared={} actions={} blockers={} notices={} impacts={} retire_groups={} overwrites={} install_notes={} elapsed_ms={elapsed_ms}",
        plan.plan_id,
        result.status,
        plan.prepared,
        result.actions.len(),
        result.blockers.len(),
        result.notices.len(),
        result.impacts.len(),
        plan.retired_mod_copies.len(),
        plan.untracked_overwrites.len(),
        plan.install_notes.len(),
    );
    if !detail || !log::log_enabled!(log::Level::Info) {
        return;
    }
    for action in result.actions.iter().take(DETAIL_LIMIT) {
        log::info!(
            "整链行动 op={op} stage={stage} {}",
            log_detail(format_args!(
                "node={:?} member={:?} action={:?} from={:?} to={:?} reasons={:?} omitted_reasons={}",
                action.node_id,
                action.member_key,
                action.kind,
                action.from_tag,
                action.to_tag,
                action.reason_details.iter().take(4).collect::<Vec<_>>(),
                action.reason_details.len().saturating_sub(4),
            ))
        );
    }
    for blocker in result.blockers.iter().take(DETAIL_LIMIT) {
        log::info!(
            "整链阻断 op={op} stage={stage} {}",
            log_detail(format_args!(
                "kind={} owner={:?} target={:?} incomplete={} reason={:?} traces={} traces_truncated={}",
                blocker.code,
                blocker.owner_id,
                blocker.target_guid,
                blocker.incomplete,
                crate::locale_tag::diagnostic_tag(&blocker.message),
                blocker.traces.len(),
                blocker.traces_truncated,
            ))
        );
    }
    for overwrite in plan.untracked_overwrites.iter().take(DETAIL_LIMIT) {
        log::info!(
            "整链覆盖确认 op={op} stage={stage} {}",
            log_detail(format_args!(
                "node={:?} path={:?}",
                overwrite.node_id, overwrite.path,
            ))
        );
    }
    let retirements = plan
        .retired_mod_copies
        .iter()
        .flat_map(|(id, copies)| copies.iter().map(move |copy| (id, copy)));
    for (id, copy) in retirements.clone().take(DETAIL_LIMIT) {
        log::info!(
            "整链旧副本处理 op={op} stage={stage} {}",
            log_detail(format_args!(
                "node={id:?} version={:?} path={:?}",
                copy.version, copy.dll_file,
            ))
        );
    }
    let omitted = [
        result.actions.len(),
        result.blockers.len(),
        plan.untracked_overwrites.len(),
        retirements.count(),
    ]
    .into_iter()
    .map(|count| count.saturating_sub(DETAIL_LIMIT))
    .sum::<usize>();
    if omitted != 0 {
        log::info!("整链方案明细已截断 op={op} stage={stage} omitted={omitted}");
    }
}

#[derive(Clone)]
pub(crate) struct Payload {
    pub candidate: Candidate,
    pub stage_root: PathBuf,
    pub stage_fingerprint: Fingerprint,
    /// Exact files in a Release ZIP verified against its cached SHA-256.
    pub release_files: Option<Arc<HashMap<String, Fingerprint>>>,
    pub release_zip_sha256: Option<String>,
    pub downloaded: bool,
    pub files: Vec<String>,
    pub manifest: Option<crate::archive::InstallManifest>,
    pub notices: Vec<String>,
    /// DLL roles read alongside the verified candidate, keyed by its physical member keys.
    pub member_roles: BTreeMap<String, crate::models::DllRole>,
}

impl Payload {
    pub(super) fn verified<'a>(
        verified: &'a HashMap<(String, String), Self>,
        staged: &'a HashMap<String, Self>,
        node: &str,
        candidate: &str,
    ) -> Option<&'a Self> {
        verified
            .get(&(node.to_owned(), candidate.to_owned()))
            .or_else(|| {
                staged
                    .get(node)
                    .filter(|payload| payload.candidate.id == candidate)
            })
    }

    pub(super) fn member_file(&self, key: &str) -> Option<&str> {
        self.files
            .iter()
            .find(|path| super::inventory::key(&crate::mods::canonical_dll_name(path)) == key)
            .map(String::as_str)
    }
}

pub(crate) struct PlanSession {
    pub trusted: TrustedInventory,
    pub request: ChangeRequest,
    pub wire_request: PlanRequest,
    pub dependency_policy: super::solve::DependencyPolicy,
    pub plan: ChangePlan,
    pub(super) recovery_revisions: BTreeMap<String, String>,
    pub solved: Option<Inventory>,
    pub payloads: HashMap<String, Payload>,
    /// Retain every verified asset across failed preparation and graph revisions.
    pub verified_payloads: HashMap<(String, String), Payload>,
    pub map_payload: Option<super::map_payload::MapBatch>,
    pub staging: PathBuf,
    /// Exact staged bytes, checked again before applying anything to the game.
    pub payload_fingerprints: HashMap<PathBuf, super::transaction::Fingerprint>,
    /// Frozen authority for loose files and records of missing installations.
    pub(super) release_target_claims: super::mutations::ReleaseTargetClaims,
    /// Frozen under preparation; execute must not recapture changed live targets.
    pub(super) mutations: Option<Vec<super::application_progress::OwnedMutation>>,
    pub created: std::time::Instant,
}

impl PlanSession {
    pub(super) fn may_replan_inventory(&self) -> bool {
        let request = &self.wire_request;
        request.map_import.is_none()
            && request.toggle_scope.is_none()
            && !request.bulk_toggle
            && (request.repair_dependencies_of.is_some()
                || (!request.roots.is_empty()
                    && request.roots.iter().all(|root| {
                        root.intent.enabled
                            && root.intent.member_key.is_none()
                            && root
                                .intent
                                .candidate_id
                                .as_deref()
                                .is_some_and(|id| id.starts_with("release:"))
                    })))
    }

    pub(super) fn operation_id(&self) -> &str {
        self.staging
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&self.plan.plan_id)
    }

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
        let rejected = |reason: &str| {
            log::warn!(
                "整链方案未受理：plan={:?}，原因={reason}",
                uuid::Uuid::parse_str(id).ok()
            );
            reason.to_owned()
        };
        let mut sessions = state
            .mod_changes
            .lock()
            .map_err(|_| rejected("lock.poisoned"))?;
        if sessions.busy.is_some() {
            return Err(rejected("modChanges.busy"));
        }
        let session = sessions
            .plans
            .remove(id)
            .ok_or_else(|| rejected("modChanges.planExpired"))?;
        if session.created.elapsed() >= std::time::Duration::from_secs(3600) {
            drop(sessions);
            let trace = OperationTrace::new(session.operation_id(), "expire_plan");
            let step = match super::execution::cleanup(state, &session.staging) {
                Ok(()) => "staging_cleaned",
                Err(error) => {
                    trace.failure("cleanup_staging", &error);
                    "staging_cleanup_failed"
                }
            };
            trace.failure(step, "modChanges.planExpired");
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
        replan_scope: trusted.replan_scope.clone(),
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
        bundled_dlls: Default::default(),
        retired_mod_copies: BTreeMap::new(),
        untracked_overwrites: Vec::new(),
        provider_choices: Vec::new(),
        recovery_conflicts: Vec::new(),
        recovery_approval: None,
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
