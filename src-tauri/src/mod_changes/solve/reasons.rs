//! Trace the exact edge selected by the search. These reasons describe the
//! winning branch only, and must not affect admission or candidate ordering.
use super::*;
use std::sync::Arc;

pub(super) fn requested() -> Arc<ChangeReason> {
    Arc::new(ChangeReason {
        kind: ReasonKind::Requested,
        related_node_ids: Vec::new(),
        target_guid: None,
        requirement: None,
        prerelease: None,
        condition_on_related: false,
    })
}

fn edge(
    affected: &ChangeNode,
    kind: ReasonKind,
    related: impl IntoIterator<Item = String>,
    relation: &RepositoryRelation,
    points_at_dependency: bool,
) -> Arc<ChangeReason> {
    let related_node_ids = related
        .into_iter()
        .filter(|id| *id != affected.id)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    // The GUID can identify an otherwise unnamed dependency. On a forward
    // change it belongs to the affected node itself, not the requesting owner.
    let guid = relation.guid.trim();
    let target_guid = (points_at_dependency
        && !guid.is_empty()
        && (!related_node_ids.is_empty()
            || affected.current.as_ref().is_none_or(|c| !provides(c, guid))))
    .then(|| guid.to_owned());
    let unconfirmed_conflict = kind == ReasonKind::CompatibilityUnconfirmed;
    let range = relation.version.trim();
    Arc::new(ChangeReason {
        kind,
        related_node_ids,
        target_guid,
        requirement: (!unconfirmed_conflict && !range.is_empty() && range != "*")
            .then(|| range.to_owned()),
        prerelease: if unconfirmed_conflict {
            None
        } else {
            relation.prerelease
        },
        condition_on_related: points_at_dependency,
    })
}

fn kind_for_issue(issue: &RelationEvaluation) -> ReasonKind {
    if issue.state == EvaluationState::Unknown {
        return if issue.relation.kind == RelationKind::Requires {
            ReasonKind::DependencyUnconfirmed
        } else {
            ReasonKind::CompatibilityUnconfirmed
        };
    }
    match issue.relation.kind {
        RelationKind::Conflicts => ReasonKind::ConflictsWith,
        RelationKind::Compatible => ReasonKind::IncompatibleWith,
        RelationKind::Requires => match issue.state {
            EvaluationState::Missing => ReasonKind::DependencyMissing,
            EvaluationState::Disabled => ReasonKind::DependencyDisabled,
            _ => ReasonKind::DependencyVersion,
        },
    }
}

pub(super) fn provider_change(
    inventory: &Inventory,
    index: usize,
    issue: &RelationEvaluation,
) -> Arc<ChangeReason> {
    edge(
        &inventory.nodes[index],
        if issue.relation.kind == RelationKind::Requires {
            ReasonKind::RequiredBy
        } else {
            kind_for_issue(issue)
        },
        [issue.owner_id.clone()],
        &issue.relation,
        false,
    )
}

pub(super) fn owner_change(
    original: &Inventory,
    inventory: &Inventory,
    owner: usize,
    issue: &RelationEvaluation,
    incompatible: &[String],
) -> Arc<ChangeReason> {
    let relation = &issue.relation;
    let kind = kind_for_issue(issue);
    let mut ids = if incompatible.is_empty() {
        issue.targets.clone()
    } else {
        incompatible.to_vec()
    };
    if ids.is_empty() {
        // An upgrade can remove the DLL, so the failed edge has no provider in
        // the branch. Name the original provider instead of exposing its GUID.
        ids.extend(
            targets(original, &relation.guid, true)
                .into_iter()
                .map(|(index, _)| original.nodes[index].id.clone()),
        );
    }
    edge(&inventory.nodes[owner], kind, ids, relation, true)
}

pub(super) fn compatible_upgrade(
    inventory: &Inventory,
    index: usize,
    protected: &BTreeSet<usize>,
) -> Option<Arc<ChangeReason>> {
    let target = &inventory.nodes[index];
    let candidate = target.current.as_ref()?;
    // Membership in `protected` is proved by a satisfied Requires edge. Use
    // the same unique enabled provider and matcher as the search for its label.
    for owner in protected.iter().copied().filter(|&owner| owner != index) {
        let owner = &inventory.nodes[owner];
        let Some(current) = &owner.current else {
            continue;
        };
        for owned in &current.relations {
            let relation = &owned.relation;
            if relation.kind != RelationKind::Requires
                || relation.optional
                || !relation_active(current, owned, false)
                || !provides(candidate, &relation.guid)
            {
                continue;
            }
            if let [(actual_index, plugin)] = targets(inventory, &relation.guid, true).as_slice()
                && *actual_index == index
                && matches(relation, plugin) == Some(true)
            {
                return Some(edge(
                    target,
                    ReasonKind::CompatibleUpgrade,
                    [owner.id.clone()],
                    relation,
                    false,
                ));
            }
        }
    }
    None
}
