use crate::models::RepositoryRelation;
use serde::{Deserialize, Serialize};

pub(super) const MULTIPLE_INSTALLED_COPIES: &str = "modChanges.multipleInstalledCopies";

/// A node is one atomic replacement unit: an asset/package, a loose DLL, or a map.
/// Adapters must not offer unrelated package members as independently replaceable nodes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Mod,
    Map,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProvidedPlugin {
    /// Physical member identity for installed packages; unknown before actual package verification.
    pub member_key: Option<String>,
    pub guid: String,
    pub version: Option<String>,
    /// The exact Release/API verdict, not a display-version inference.
    pub prerelease: Option<bool>,
    /// Package-member switch state. The node must also be enabled to contribute.
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OwnedRelation {
    pub owner_member_key: Option<String>,
    /// None = package/map-level declaration; Some = only this enabled member emits it.
    pub owner_guid: Option<String>,
    pub relation: RepositoryRelation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Candidate {
    /// Stable exact identity, normally provider/repository/channel/full tag/asset.
    pub id: String,
    pub source_id: Option<String>,
    pub tag: Option<String>,
    pub version: Option<String>,
    pub provides: Vec<ProvidedPlugin>,
    /// Already selected by mods::effective_relations/effective_map_relations.
    pub relations: Vec<OwnedRelation>,
    /// Distinguishes an explicitly empty declaration from unavailable metadata.
    pub relations_known: bool,
    /// Remote candidates must have an actual installable matching asset.
    pub installable: bool,
    /// A repository-file candidate cannot be automatically selected as a versioned dependency.
    pub raw: bool,
    /// Planning may use declared primary provision; prepared assets must be re-evaluated.
    #[serde(default)]
    pub verification_pending: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChangeNode {
    pub id: String,
    pub kind: NodeKind,
    pub current: Option<Candidate>,
    pub enabled: bool,
    /// False for unknown/shared ownership or other adapter safety barriers.
    pub mutable: bool,
    /// Pre-filtered by availability, binding, channel, pin and asset rules.
    pub candidates: Vec<Candidate>,
    /// False means exhaustion proves insufficient data, not unsatisfiability.
    pub candidates_complete: bool,
}

impl ChangeNode {
    pub(super) fn is_registered_map_plugin(&self) -> bool {
        self.id.starts_with("map-dll:")
    }

    pub(super) fn can_repair_dependencies(&self) -> bool {
        !self.is_registered_map_plugin()
            && self.current.is_some()
            && (self.enabled || self.kind == NodeKind::Map)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Inventory {
    pub nodes: Vec<ChangeNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RootIntent {
    pub node_id: String,
    /// None retains the installed candidate. Some fixes this exact candidate.
    pub candidate_id: Option<String>,
    pub enabled: bool,
    /// None changes the whole unit; Some switches exactly one installed member.
    pub member_key: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChangeRequest {
    pub roots: Vec<RootIntent>,
    /// Check an enabled MOD or installed map without changing its candidate or switches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repair_dependencies_of: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RelationScope {
    Runtime,
    Readiness,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationState {
    Satisfied,
    Missing,
    Disabled,
    VersionMismatch,
    Conflict,
    Unknown,
    Optional,
}

impl EvaluationState {
    pub fn is_issue(&self) -> bool {
        !matches!(self, Self::Satisfied | Self::Optional)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RelationEvaluation {
    pub owner_id: String,
    pub relation: RepositoryRelation,
    pub scope: RelationScope,
    pub state: EvaluationState,
    pub targets: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeEvaluation {
    pub node_id: String,
    pub enabled: bool,
    pub runtime: Vec<RelationEvaluation>,
    pub readiness: Vec<RelationEvaluation>,
    pub metadata_known: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    Install,
    Upgrade,
    Downgrade,
    Replace,
    Enable,
    Disable,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReasonKind {
    Requested,
    RequiredBy,
    DependencyMissing,
    DependencyDisabled,
    DependencyVersion,
    DependencyUnconfirmed,
    CompatibilityUnconfirmed,
    ConflictsWith,
    IncompatibleWith,
    CompatibleUpgrade,
}

/// The edge that made the solver select this operation. If present, the
/// version and prerelease predicates describe the edge, not the chosen tag.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChangeReason {
    pub kind: ReasonKind,
    pub related_node_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_guid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requirement: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prerelease: Option<bool>,
    /// The version condition applies to the related item rather than this row.
    #[serde(default)]
    pub condition_on_related: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChangeAction {
    pub node_id: String,
    pub member_key: Option<String>,
    pub kind: ActionKind,
    pub from_candidate_id: Option<String>,
    pub to_candidate_id: Option<String>,
    pub from_tag: Option<String>,
    pub to_tag: Option<String>,
    pub reason: String,
    pub caused_by: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reason_details: Vec<ChangeReason>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: String,
    pub owner_id: Option<String>,
    pub target_guid: Option<String>,
    pub message: String,
    pub incomplete: bool,
    /// Examples from failed search states, not actions or original-inventory evaluations.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub traces: Vec<Vec<DiagnosticStep>>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub traces_truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiagnosticStep {
    pub owner_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation: Option<RepositoryRelation>,
    pub targets: Vec<DiagnosticTarget>,
    pub state: EvaluationState,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiagnosticTarget {
    pub node_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub enabled: bool,
    #[serde(skip)]
    pub(super) candidate_id: Option<String>,
    #[serde(skip)]
    pub(super) member_key: Option<String>,
    #[serde(skip)]
    pub(super) source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SolveStatus {
    Ready,
    Unsatisfiable,
    Incomplete,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SolveResult {
    pub status: SolveStatus,
    pub actions: Vec<ChangeAction>,
    pub evaluations: Vec<NodeEvaluation>,
    pub blockers: Vec<Diagnostic>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notices: Vec<String>,
    pub impacts: Vec<String>,
}
