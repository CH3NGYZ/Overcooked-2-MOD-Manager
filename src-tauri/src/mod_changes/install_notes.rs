//! First-install notices belong to the same disk and repository snapshot as the plan.
use super::{evaluate::same_guid, model::*, session::Payload};
use crate::{config::mod_repository_is_available, models::*};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Clone, Serialize)]
pub(crate) struct InstallNote {
    pub id: String,
    pub name: String,
    pub name_i18n: BTreeMap<String, String>,
    pub note: String,
    pub note_i18n: BTreeMap<String, String>,
}

#[derive(Clone, Default)]
pub(crate) struct ManagedMods {
    ids: HashSet<String>,
    bindings: Vec<ModRepositoryBinding>,
    guids: HashSet<String>,
}

impl ManagedMods {
    pub(super) fn from_scan(installed: &[InstalledMod]) -> Self {
        let mut managed = Self::default();
        for plugin in installed.iter().filter(|plugin| {
            plugin
                .package_id
                .as_deref()
                .is_some_and(|id| !id.trim().is_empty())
        }) {
            managed.ids.extend(plugin.repository_id.iter().cloned());
            managed.ids.extend(plugin.repository_ids.iter().cloned());
            managed
                .bindings
                .extend(plugin.repository_binding.iter().cloned());
            if let Some(guid) = plugin
                .guid
                .as_deref()
                .map(str::trim)
                .filter(|guid| !guid.is_empty())
            {
                managed.guids.insert(guid.to_ascii_lowercase());
            }
        }
        managed
    }

    fn contains(&self, repo: &ModRepositoryConfig) -> bool {
        self.ids.contains(&repo.id)
            || self
                .bindings
                .iter()
                .any(|binding| binding.matches_identity(repo))
            || self
                .guids
                .contains(&repo.plugin_guid.trim().to_ascii_lowercase())
    }
}

pub(super) fn for_plan(
    config: &AppConfig,
    managed: &ManagedMods,
    result: &SolveResult,
    solved: &Inventory,
    payloads: &HashMap<String, Payload>,
) -> Vec<InstallNote> {
    if result.status != SolveStatus::Ready || result.actions.is_empty() {
        return Vec::new();
    }
    let repositories: Vec<_> = config
        .repositories
        .iter()
        .filter(|repo| mod_repository_is_available(config, repo, &config.region))
        .collect();
    let nodes: HashMap<_, _> = solved
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let mut seen = HashSet::new();
    let mut notes = Vec::new();
    let mut add = |repo: &ModRepositoryConfig| {
        if (repo.note.trim().is_empty()
            && repo.note_i18n.values().all(|note| note.trim().is_empty()))
            || managed.contains(repo)
            || !seen.insert(repo.id.clone())
        {
            return;
        }
        notes.push(InstallNote {
            id: repo.id.clone(),
            name: repo.name.clone(),
            name_i18n: repo.name_i18n.clone(),
            note: repo.note.clone(),
            note_i18n: repo.note_i18n.clone(),
        });
    };
    for action in &result.actions {
        if !matches!(
            action.kind,
            ActionKind::Install | ActionKind::Upgrade | ActionKind::Downgrade | ActionKind::Replace
        ) {
            continue;
        }
        let Some(candidate) = nodes
            .get(action.node_id.as_str())
            .filter(|node| node.kind == NodeKind::Mod)
            .and_then(|node| node.current.as_ref())
            .filter(|candidate| action.to_candidate_id.as_deref() == Some(candidate.id.as_str()))
        else {
            continue;
        };
        if let Some(source) = &candidate.source_id {
            if let Some(repo) = repositories.iter().find(|repo| &repo.id == source) {
                add(repo);
            }
            continue;
        }
        // Only a real unbound preview may infer a source. Its old package binding,
        // filename and provisional metadata are not evidence of a source.
        let Some(payload) = payloads.get(&action.node_id).filter(|payload| {
            payload.candidate.id == candidate.id && !payload.candidate.verification_pending
        }) else {
            continue;
        };
        for member in &payload.candidate.provides {
            if member.guid.trim().is_empty() {
                continue;
            }
            let mut matching = repositories
                .iter()
                .filter(|repo| same_guid(&repo.plugin_guid, &member.guid));
            if let Some(repo) = matching.next()
                && matching.next().is_none()
            {
                add(repo);
            }
        }
    }
    notes
}
