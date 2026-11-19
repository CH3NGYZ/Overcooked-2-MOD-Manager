use super::model::*;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum ChangeKey {
    Package,
    Switch(bool, Option<String>),
}

/// Record each branch's actual changes, not a reason shared by every action on
/// a package. In particular two DLL switches can have different causes.
#[derive(Clone, Default)]
pub(super) struct ActionReasons(BTreeMap<usize, BTreeMap<ChangeKey, Arc<ChangeReason>>>);

impl ActionReasons {
    pub(super) fn record(
        &mut self,
        index: usize,
        before: &ChangeNode,
        after: &ChangeNode,
        reason: &Arc<ChangeReason>,
    ) {
        for_each_change(before, after, |change| {
            self.0
                .entry(index)
                .or_default()
                .insert(change, Arc::clone(reason));
        });
    }
}

fn for_each_change(before: &ChangeNode, after: &ChangeNode, mut visit: impl FnMut(ChangeKey)) {
    let from = before.current.as_ref();
    let to = after.current.as_ref();
    let package_changed = from.map(|candidate| &candidate.id) != to.map(|candidate| &candidate.id);
    if package_changed {
        visit(ChangeKey::Package);
    }
    let singleton = before.kind == NodeKind::Mod
        && from.is_some_and(|c| c.provides.len() == 1)
        && to.is_some_and(|c| c.provides.len() == 1);
    let enabled = |node: &ChangeNode, candidate: Option<&Candidate>| {
        node.enabled && (!singleton || candidate.unwrap().provides[0].enabled)
    };
    let after_enabled = enabled(after, to);
    if enabled(before, from) != after_enabled {
        visit(ChangeKey::Switch(after_enabled, None));
    } else if !singleton
        && !package_changed
        && let (Some(from), Some(to)) = (from, to)
    {
        for member in &to.provides {
            let previous = from.provides.iter().find(|plugin| {
                if member.member_key.is_some() {
                    plugin.member_key == member.member_key
                } else {
                    plugin.guid.eq_ignore_ascii_case(&member.guid)
                }
            });
            if previous.is_some_and(|previous| previous.enabled != member.enabled) {
                visit(ChangeKey::Switch(member.enabled, member.member_key.clone()));
            }
        }
    }
}

fn display_version(candidate: &Candidate) -> Option<&str> {
    candidate
        .tag
        .as_deref()
        .filter(|tag| !tag.trim().is_empty())
        .or(candidate.version.as_deref())
}

fn version_action(from: Option<&Candidate>, to: Option<&Candidate>) -> ActionKind {
    if from.is_none() {
        return ActionKind::Install;
    }
    match from.zip(to).and_then(|(from, to)| {
        crate::releases::compare_release_versions(
            display_version(to).unwrap_or(""),
            display_version(from).unwrap_or(""),
        )
    }) {
        Some(Ordering::Greater) => ActionKind::Upgrade,
        Some(Ordering::Less) => ActionKind::Downgrade,
        _ => ActionKind::Replace,
    }
}

pub(super) fn build_actions(
    original: &Inventory,
    solved: &Inventory,
    reasons: &ActionReasons,
) -> Vec<ChangeAction> {
    let mut actions = Vec::new();
    for (index, (before, after)) in original.nodes.iter().zip(&solved.nodes).enumerate() {
        let from = before.current.as_ref();
        let to = after.current.as_ref();
        for_each_change(before, after, |change| {
            let traced = reasons
                .0
                .get(&index)
                .and_then(|entries| entries.get(&change))
                .map(Arc::as_ref);
            let (kind, member_key, reason) = match change {
                ChangeKey::Package => (version_action(from, to), None, "version_required_by_plan"),
                ChangeKey::Switch(enabled, member) => {
                    let kind = if enabled {
                        ActionKind::Enable
                    } else {
                        ActionKind::Disable
                    };
                    let reason = if member.is_some() {
                        "member_state_required_by_plan"
                    } else if enabled {
                        "enable_required_plugin"
                    } else {
                        "disable_conflict_or_dependent"
                    };
                    (kind, member, reason)
                }
            };
            actions.push(ChangeAction {
                node_id: after.id.clone(),
                member_key,
                kind,
                from_candidate_id: from.map(|candidate| candidate.id.clone()),
                to_candidate_id: to.map(|candidate| candidate.id.clone()),
                from_tag: from.and_then(display_version).map(str::to_owned),
                to_tag: to.and_then(display_version).map(str::to_owned),
                reason: reason.into(),
                caused_by: traced
                    .map(|reason| reason.related_node_ids.clone())
                    .unwrap_or_default(),
                reason_details: traced.cloned().into_iter().collect(),
            });
        });
    }
    // Disablements are explicit and must be reviewed before version/new-file actions.
    actions.sort_by_key(|action| {
        (
            match action.kind {
                ActionKind::Disable => 0,
                ActionKind::Enable => 2,
                _ => 1,
            },
            action.node_id.clone(),
            action.member_key.clone(),
        )
    });
    actions
}
