//! DIYChef editor wire contracts; paths stay in backend sessions.
use crate::models::InstalledMod;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocalModSnapshot {
    pub generation: u64,
    pub mods: Vec<InstalledMod>,
    #[serde(default)]
    pub diy_level_target: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChefTarget {
    pub id: String,
    pub guid: String,
    pub version: Option<String>,
    pub display_path: String,
    pub enabled: bool,
    pub generation: u64,
    pub compatibility: String,
    #[serde(default)]
    pub supports_custom_default: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChefSource {
    pub state: String,
    pub filename: String,
    pub revision: String,
    pub encoding: String,
    pub newline: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChefItem {
    pub id: String,
    pub source_kind: String,
    pub display_name: String,
    pub config_name: String,
    pub head_name: String,
    pub custom_id: Option<u8>,
    pub resource_state: String,
    pub preview_state: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChefHat {
    pub id: String,
    pub kind: String,
    pub config_value: String,
    pub display_name: String,
    pub resource_state: String,
    pub preview_state: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChefReference {
    Known { chef_id: String },
    Unresolved { raw_key: String, row_id: String },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HatReference {
    Known { hat_id: String },
    Unresolved { raw_value: String, row_id: String },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChefEntry {
    pub entry_id: String,
    pub row_id: Option<String>,
    pub chef_ref: ChefReference,
    pub hat_ref: HatReference,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChefSelection {
    pub entries: Vec<ChefEntry>,
    #[serde(default)]
    pub hat_bindings: BTreeMap<String, HatReference>,
    pub lobby_switch: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChefDocumentRow {
    pub row_id: String,
    pub category: String,
    pub raw_display: String,
    pub duplicate_group_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChefDiagnostic {
    pub id: String,
    pub severity: String,
    pub code: String,
    pub chef_id: Option<String>,
    pub hat_id: Option<String>,
    pub row_id: Option<String>,
    pub save_blocking: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChefChange {
    pub id: String,
    pub before: String,
    pub after: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChefValidation {
    pub diagnostics: Vec<ChefDiagnostic>,
    pub changes: Vec<ChefChange>,
    pub save_blocking: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChefDraft {
    pub base_revision: String,
    pub catalog_revision: String,
    pub target_generation: u64,
    pub entries: Vec<ChefEntry>,
    #[serde(default)]
    pub hat_bindings: BTreeMap<String, HatReference>,
    pub lobby_switch: bool,
    #[serde(default)]
    pub removed_problem_row_ids: Vec<String>,
    #[serde(default)]
    pub acknowledged_change_ids: Vec<String>,
    pub reset_intent: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChefEditorSnapshot {
    pub session_id: String,
    pub target: ChefTarget,
    pub source: ChefSource,
    pub catalog_revision: String,
    pub chefs: Vec<ChefItem>,
    pub hats: Vec<ChefHat>,
    pub document_rows: Vec<ChefDocumentRow>,
    pub selection: ChefSelection,
    pub diagnostics: Vec<ChefDiagnostic>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChefEditorOpen {
    pub status: String,
    pub candidates: Vec<ChefTarget>,
    pub snapshot: Option<ChefEditorSnapshot>,
}
