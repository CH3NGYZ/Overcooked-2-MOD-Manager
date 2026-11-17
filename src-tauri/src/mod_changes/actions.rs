use super::model::*;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn build_actions(
    original: &Inventory,
    solved: &Inventory,
    causes: &BTreeMap<usize, BTreeSet<String>>,
) -> Vec<ChangeAction> {
    let mut actions = Vec::new();
    for (index, (before, after)) in original.nodes.iter().zip(&solved.nodes).enumerate() {
        let from = before.current.as_ref();
        let to = after.current.as_ref();
        let mut push = |kind, member_key, reason: &str| {
            actions.push(ChangeAction {
                node_id: after.id.clone(),
                member_key,
                kind,
                from_candidate_id: from.map(|candidate| candidate.id.clone()),
                to_candidate_id: to.map(|candidate| candidate.id.clone()),
                from_tag: from.and_then(|candidate| candidate.tag.clone()),
                to_tag: to.and_then(|candidate| candidate.tag.clone()),
                reason: reason.into(),
                caused_by: causes
                    .get(&index)
                    .map(|causes| causes.iter().cloned().collect())
                    .unwrap_or_default(),
            });
        };
        if from.map(|candidate| &candidate.id) != to.map(|candidate| &candidate.id) {
            let kind = if from.is_none() {
                ActionKind::Install
            } else {
                match from.zip(to).and_then(|(from, to)| {
                    crate::releases::compare_release_versions(
                        to.tag.as_deref().or(to.version.as_deref()).unwrap_or(""),
                        from.tag
                            .as_deref()
                            .or(from.version.as_deref())
                            .unwrap_or(""),
                    )
                }) {
                    Some(Ordering::Greater) => ActionKind::Upgrade,
                    Some(Ordering::Less) => ActionKind::Downgrade,
                    _ => ActionKind::Replace,
                }
            };
            push(kind, None, "version_required_by_plan");
        }
        let singleton = before.kind == NodeKind::Mod
            && from.is_some_and(|c| c.provides.len() == 1)
            && to.is_some_and(|c| c.provides.len() == 1);
        let before_enabled = if singleton {
            before.enabled && from.unwrap().provides[0].enabled
        } else {
            before.enabled
        };
        let after_enabled = if singleton {
            after.enabled && to.unwrap().provides[0].enabled
        } else {
            after.enabled
        };
        if before_enabled != after_enabled {
            push(
                if after_enabled {
                    ActionKind::Enable
                } else {
                    ActionKind::Disable
                },
                None,
                if after_enabled {
                    "enable_required_plugin"
                } else {
                    "disable_conflict_or_dependent"
                },
            );
        } else if let (Some(from), Some(to)) = (from, to) {
            if from.id == to.id {
                for member in &to.provides {
                    if let Some(previous) = from.provides.iter().find(|plugin| {
                        if member.member_key.is_some() {
                            plugin.member_key == member.member_key
                        } else {
                            plugin.guid.eq_ignore_ascii_case(&member.guid)
                        }
                    }) {
                        if !singleton && previous.enabled != member.enabled {
                            push(
                                if member.enabled {
                                    ActionKind::Enable
                                } else {
                                    ActionKind::Disable
                                },
                                member.member_key.clone(),
                                "member_state_required_by_plan",
                            );
                        }
                    }
                }
            }
        }
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
