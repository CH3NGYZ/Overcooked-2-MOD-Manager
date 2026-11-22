use super::{Search, SearchState, changed, matches, provides, targets};
use crate::mod_changes::model::*;
use crate::models::{RelationKind, RepositoryRelation};
use crate::mods::plugin_target::{DIYLEVEL_GUID, RUNTIME_WLOADER_GUID};
use std::collections::BTreeSet;

pub(super) fn installed_runtime_nodes(
    inventory: &Inventory,
    roots: &BTreeSet<usize>,
) -> BTreeSet<usize> {
    inventory
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(index, node)| {
            (node.kind == NodeKind::Mod
                && !roots.contains(&index)
                && node.current.as_ref().is_some_and(|installed| {
                    [DIYLEVEL_GUID, RUNTIME_WLOADER_GUID]
                        .into_iter()
                        .any(|guid| provides(installed, guid))
                }))
            .then_some(index)
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DependencyPolicy {
    #[default]
    KeepInstalled,
    MapInstall,
    PreferNewest,
}

impl DependencyPolicy {
    pub(crate) fn for_plan(
        inventory: &Inventory,
        request: &ChangeRequest,
        map_import: bool,
    ) -> Self {
        if map_import {
            return Self::MapInstall;
        }
        let mut installs_map = false;
        for root in request
            .roots
            .iter()
            .filter(|root| root.candidate_id.is_some())
        {
            let kind = inventory
                .nodes
                .iter()
                .find(|node| node.id == root.node_id)
                .map(|node| &node.kind);
            match kind {
                Some(NodeKind::Mod) if root.enabled => return Self::PreferNewest,
                Some(NodeKind::Map) => installs_map = true,
                _ => {}
            }
        }
        if installs_map {
            Self::MapInstall
        } else {
            Self::KeepInstalled
        }
    }

    pub(super) fn prefers_newest(self) -> bool {
        self == Self::PreferNewest
    }
}

impl Search<'_, '_> {
    pub(super) fn accepts_partial_declarations(&self, state: &SearchState, index: usize) -> bool {
        let node = &state.inventory.nodes[index];
        self.can_keep_unconfirmed(state, index)
            || (self.trusted.is_some()
                && self.roots.contains(&index)
                && node.current.as_ref().is_some_and(|candidate| {
                    !candidate.verification_pending
                        && ((candidate.id == node.id && node.id.starts_with("preview:"))
                            || !self.manual_declaration_notices(node).is_empty())
                }))
    }

    fn manual_declaration_notices<'a>(&'a self, node: &ChangeNode) -> &'a [String] {
        self.trusted
            .and_then(|trusted| {
                trusted
                    .manual_declaration_notices
                    .get(&node.current.as_ref()?.id)
            })
            .map_or(&[], Vec::as_slice)
    }

    pub(super) fn can_keep_unconfirmed(&self, state: &SearchState, index: usize) -> bool {
        let node = &state.inventory.nodes[index];
        (self.policy == DependencyPolicy::MapInstall || self.installed_runtimes.contains(&index))
            && node.kind == NodeKind::Mod
            && node.mutable
            && !self.replacement_only.contains(&index)
            && !changed(self.original, &state.inventory, index)
            && node
                .current
                .as_ref()
                .is_some_and(|candidate| !candidate.verification_pending)
    }

    pub(super) fn unconfirmed_provider(
        &self,
        state: &SearchState,
        relation: &RepositoryRelation,
    ) -> Option<usize> {
        if relation.kind != RelationKind::Requires
            || relation.optional
            || relation.guid.trim().is_empty()
        {
            return None;
        }
        let providers = targets(&state.inventory, &relation.guid, false);
        let [(index, plugin)] = providers.as_slice() else {
            return None;
        };
        let node = &state.inventory.nodes[*index];
        if !self.can_keep_unconfirmed(state, *index) {
            return None;
        }
        let verdict = matches(relation, plugin);
        let valid_requirement = || {
            crate::version_range::VersionRange::parse(&relation.version).is_ok_and(|range| {
                range.clauses().iter().all(|clause| {
                    crate::releases::compare_release_versions(&clause.version, &clause.version)
                        .is_some()
                })
            })
        };
        ((verdict.is_none() && valid_requirement())
            || (verdict == Some(true)
                && !(node.enabled && plugin.enabled)
                && node
                    .current
                    .as_ref()
                    .is_some_and(|candidate| !candidate.relations_known)))
        .then_some(*index)
    }

    pub(super) fn unconfirmed_notices(
        &self,
        state: &SearchState,
        evaluations: &[NodeEvaluation],
    ) -> Vec<String> {
        let mut notices = std::collections::BTreeSet::new();
        for &index in &self.roots {
            let node = &state.inventory.nodes[index];
            if node.enabled {
                notices.extend(self.manual_declaration_notices(node).iter().cloned());
            }
        }
        let label = |index: usize| {
            let id = state.inventory.nodes[index].id.as_str();
            self.trusted
                .and_then(|trusted| trusted.files.get(id))
                .map(|files| files.summary.label.as_str())
                .filter(|name| !name.trim().is_empty())
                .unwrap_or(id)
        };
        let subjects = self
            .roots
            .iter()
            .copied()
            .map(label)
            .collect::<Vec<_>>()
            .join(" · ");
        for index in self.protected(state) {
            if self.can_keep_unconfirmed(state, index) && !evaluations[index].metadata_known {
                notices.insert(crate::locale_tag::with_params(
                    "modChanges.dependencyDeclarationsUnconfirmed",
                    &[("subject", &subjects), ("dependency", label(index))],
                ));
            }
            for issue in &evaluations[index].runtime {
                if matches!(
                    issue.state,
                    EvaluationState::Unknown | EvaluationState::Disabled
                ) && let Some(provider) = self.unconfirmed_provider(state, &issue.relation)
                {
                    let requirement = if issue.relation.version.trim().is_empty() {
                        "*"
                    } else {
                        &issue.relation.version
                    };
                    let disabled = targets(&state.inventory, &issue.relation.guid, true).is_empty();
                    let tag = if disabled {
                        "modChanges.dependencyDisabledUnconfirmed"
                    } else {
                        "modChanges.dependencyVersionUnconfirmed"
                    };
                    notices.insert(crate::locale_tag::with_params(
                        tag,
                        &[
                            ("subject", label(index)),
                            ("dependency", label(provider)),
                            ("requirement", requirement),
                        ],
                    ));
                }
            }
        }
        notices.into_iter().collect()
    }
}
