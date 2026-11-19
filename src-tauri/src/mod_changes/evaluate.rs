use super::model::*;
use crate::models::RelationKind;

pub(super) const DUPLICATE_ENABLED_PROVIDERS: &str = "duplicate_enabled_providers";

pub(crate) fn same_guid(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

/// None is unknown metadata/syntax; never turn it into a known incompatibility.
pub(crate) fn matches(
    relation: &crate::models::RepositoryRelation,
    plugin: &ProvidedPlugin,
) -> Option<bool> {
    if relation.prerelease.is_some() && plugin.prerelease.is_none() && plugin.version.is_none() {
        return None;
    }
    if relation.prerelease.is_some_and(|wanted| {
        crate::releases::resolve_prerelease(plugin.prerelease, false, plugin.version.as_deref())
            != wanted
    }) {
        return Some(false);
    }
    let range = crate::version_range::VersionRange::parse(&relation.version).ok()?;
    if range.clauses().is_empty() {
        return Some(true);
    }
    let version = plugin.version.as_deref()?;
    if crate::releases::compare_release_versions(version, version).is_none() {
        return None;
    }
    if range
        .clauses()
        .iter()
        .any(|clause| crate::releases::compare_release_versions(version, &clause.version).is_none())
    {
        return None;
    }
    Some(range.matches(version, crate::releases::compare_release_versions))
}

pub(crate) fn relation_active(
    candidate: &Candidate,
    owned: &OwnedRelation,
    readiness: bool,
) -> bool {
    if readiness {
        return true;
    }
    if let Some(member) = &owned.owner_member_key {
        return candidate
            .provides
            .iter()
            .any(|plugin| plugin.member_key.as_ref() == Some(member) && plugin.enabled);
    }
    owned.owner_guid.as_deref().is_none_or(|guid| {
        candidate
            .provides
            .iter()
            .any(|plugin| same_guid(&plugin.guid, guid) && plugin.enabled)
    })
}

pub(crate) fn targets<'a>(
    inventory: &'a Inventory,
    guid: &str,
    enabled_only: bool,
) -> Vec<(usize, &'a ProvidedPlugin)> {
    inventory
        .nodes
        .iter()
        .enumerate()
        .flat_map(|(index, node)| {
            node.current.iter().flat_map(move |candidate| {
                candidate.provides.iter().filter_map(move |plugin| {
                    (same_guid(&plugin.guid, guid)
                        && (!enabled_only || (node.enabled && plugin.enabled)))
                        .then_some((index, plugin))
                })
            })
        })
        .collect()
}

pub(crate) fn owned_targets<'a>(
    inventory: &'a Inventory,
    owner: &ChangeNode,
    owned: &OwnedRelation,
    enabled_only: bool,
) -> Vec<(usize, &'a ProvidedPlugin)> {
    let relation = &owned.relation;
    let conflicts_with_self = |index: usize, plugin: &ProvidedPlugin| {
        if relation.kind != RelationKind::Conflicts || !std::ptr::eq(&inventory.nodes[index], owner)
        {
            return false;
        }
        let Some(candidate) = owner.current.as_ref() else {
            return false;
        };
        if let Some(member) = &owned.owner_member_key {
            let emitters = candidate
                .provides
                .iter()
                .filter(|plugin| plugin.member_key.as_ref() == Some(member))
                .collect::<Vec<_>>();
            return emitters.len() == 1 && std::ptr::eq(emitters[0], plugin);
        }
        let Some(guid) = &owned.owner_guid else {
            return false;
        };
        let emitters = candidate
            .provides
            .iter()
            .filter(|member| same_guid(&member.guid, guid))
            .collect::<Vec<_>>();
        emitters.len() == 1 && std::ptr::eq(emitters[0], plugin)
    };
    targets(inventory, &relation.guid, enabled_only)
        .into_iter()
        .filter(|(index, plugin)| !conflicts_with_self(*index, plugin))
        .collect()
}

/// See whether deactivating one physical DLL would remove a required provider.
/// The normal evaluator reports multiple enabled providers as ambiguous. For
/// cleanup, a remaining compatible provider is enough to show no edge was lost.
pub(crate) fn dependent_after_member_removal(
    inventory: &mut Inventory,
    member_key: &str,
    guid: &str,
) -> Result<Option<String>, String> {
    let removed = inventory
        .nodes
        .iter_mut()
        .find_map(|node| {
            if !node.enabled {
                return None;
            }
            let current = node.current.as_mut()?;
            let plugin = current.provides.iter_mut().find(|plugin| {
                plugin.enabled
                    && plugin.member_key.as_deref() == Some(member_key)
                    && same_guid(&plugin.guid, guid)
            })?;
            plugin.enabled = false;
            Some(plugin.clone())
        })
        .ok_or("modChanges.targetChanged")?;
    let remaining = targets(inventory, guid, true);
    Ok(inventory
        .nodes
        .iter()
        .filter(|node| node.enabled)
        .find_map(|node| {
            let current = node.current.as_ref()?;
            current
                .relations
                .iter()
                .any(|owned| {
                    let edge = &owned.relation;
                    edge.kind == RelationKind::Requires
                        && !edge.optional
                        && same_guid(&edge.guid, guid)
                        && relation_active(current, owned, false)
                        && matches(edge, &removed) != Some(false)
                        && !remaining
                            .iter()
                            .any(|(_, peer)| matches(edge, peer) == Some(true))
                })
                .then(|| node.id.clone())
        }))
}

fn evaluate_relation(
    inventory: &Inventory,
    owner: &ChangeNode,
    owned: &OwnedRelation,
    scope: RelationScope,
) -> RelationEvaluation {
    let relation = &owned.relation;
    let installed = owned_targets(inventory, owner, owned, false);
    let enabled = owned_targets(inventory, owner, owned, true);
    let mut target_ids = installed
        .iter()
        .map(|(index, _)| inventory.nodes[*index].id.clone())
        .collect::<Vec<_>>();
    target_ids.sort();
    target_ids.dedup();
    let states = enabled
        .iter()
        .map(|(_, plugin)| matches(relation, plugin))
        .collect::<Vec<_>>();
    let duplicate_provider = enabled.len() > 1;
    let state = if relation.kind == RelationKind::Requires && relation.optional {
        EvaluationState::Optional
    } else if relation.kind == RelationKind::Requires && duplicate_provider {
        EvaluationState::Unknown
    } else {
        match relation.kind {
            RelationKind::Requires if states.contains(&Some(true)) => EvaluationState::Satisfied,
            RelationKind::Requires if states.contains(&None) => EvaluationState::Unknown,
            RelationKind::Requires => {
                if installed
                    .iter()
                    .any(|(_, plugin)| matches(relation, plugin) == Some(true))
                {
                    EvaluationState::Disabled
                } else if installed.is_empty() {
                    EvaluationState::Missing
                } else if installed
                    .iter()
                    .any(|(_, plugin)| matches(relation, plugin).is_none())
                {
                    EvaluationState::Unknown
                } else {
                    EvaluationState::VersionMismatch
                }
            }
            RelationKind::Conflicts if states.contains(&Some(true)) => EvaluationState::Conflict,
            RelationKind::Conflicts if states.contains(&None) => EvaluationState::Unknown,
            RelationKind::Conflicts => EvaluationState::Satisfied,
            RelationKind::Compatible if states.contains(&Some(false)) => EvaluationState::Conflict,
            RelationKind::Compatible if states.contains(&None) => EvaluationState::Unknown,
            RelationKind::Compatible => EvaluationState::Satisfied,
        }
    };
    let reason =
        if duplicate_provider && relation.kind == RelationKind::Requires && !relation.optional {
            DUPLICATE_ENABLED_PROVIDERS
        } else {
            match state {
                EvaluationState::Satisfied => "relation_satisfied",
                EvaluationState::Missing => "required_plugin_missing",
                EvaluationState::Disabled => "required_plugin_disabled",
                EvaluationState::VersionMismatch => "required_version_mismatch",
                EvaluationState::Conflict => "enabled_plugins_incompatible",
                EvaluationState::Unknown => "relation_metadata_unknown",
                EvaluationState::Optional => "optional_requirement",
            }
        };
    RelationEvaluation {
        owner_id: owner.id.clone(),
        relation: relation.clone(),
        scope,
        state,
        targets: target_ids,
        reason: reason.into(),
    }
}

pub(super) fn runtime_relations(node: &ChangeNode) -> impl Iterator<Item = &OwnedRelation> {
    node.current.iter().flat_map(move |candidate| {
        candidate
            .relations
            .iter()
            .filter(move |owned| node.enabled && relation_active(candidate, owned, false))
    })
}

/// Authoritative batch evaluation of installed facts and hypothetical re-enablement.
/// Disabled owners emit readiness diagnostics, never current-runtime constraints.
pub(crate) fn evaluate(inventory: &Inventory) -> Vec<NodeEvaluation> {
    inventory
        .nodes
        .iter()
        .map(|node| {
            let runtime = runtime_relations(node)
                .map(|owned| evaluate_relation(inventory, node, owned, RelationScope::Runtime))
                .collect();
            let mut readiness = Vec::new();
            if let Some(candidate) = &node.current {
                for owned in &candidate.relations {
                    readiness.push(evaluate_relation(
                        inventory,
                        node,
                        owned,
                        RelationScope::Readiness,
                    ));
                }
            }
            NodeEvaluation {
                node_id: node.id.clone(),
                enabled: node.enabled,
                runtime,
                readiness,
                metadata_known: node
                    .current
                    .as_ref()
                    .is_none_or(|candidate| candidate.relations_known),
            }
        })
        .collect()
}
