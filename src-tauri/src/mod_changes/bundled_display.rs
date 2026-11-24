use super::{inventory::*, model::*};
use serde::Serialize;
use std::cmp::Ordering;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct MapName {
    pub label: String,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub name_i18n: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_zh: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct BundledDll {
    pub parent_node_id: String,
    pub node_id: String,
    pub member_key: String,
    pub dll_file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_member_key: Option<String>,
    pub action_kind: ActionKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<crate::models::DllRole>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<ActionKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_name: Option<MapName>,
}

pub(crate) fn collect(
    inventory: &TrustedInventory,
    summaries: &[NodeSummary],
    actions: &[ChangeAction],
    solved: Option<&Inventory>,
) -> BTreeMap<String, Vec<BundledDll>> {
    let summaries = summaries
        .iter()
        .map(|summary| (summary.node_id.as_str(), summary))
        .collect::<BTreeMap<_, _>>();
    let Some(solved) = solved else {
        return BTreeMap::new();
    };
    let solved = solved
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    let installed = inventory
        .inventory
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    let mut rows = BTreeMap::<String, Vec<BundledDll>>::new();
    for action in actions {
        let Some(summary) = summaries.get(action.node_id.as_str()).copied() else {
            continue;
        };
        let is_group = summary.kind == NodeKind::Map
            || matches!(
                summary.presentation_kind,
                Some(PresentationKind::ModPackage)
            );
        if !is_group && summary.bundle_parent_node_id.is_none() {
            continue;
        }
        let parent_id = summary
            .bundle_parent_node_id
            .as_deref()
            .unwrap_or(&summary.node_id);
        let Some(parent) = summaries.get(parent_id).copied() else {
            continue;
        };
        let map_metadata = inventory
            .files
            .get(parent_id)
            .and_then(|files| files.map_metadata.as_ref());
        let Some(final_node) = solved.get(action.node_id.as_str()) else {
            continue;
        };
        let Some(current) = final_node
            .current
            .as_ref()
            .filter(|candidate| !candidate.verification_pending)
        else {
            continue;
        };
        if action.to_candidate_id.as_deref() != Some(current.id.as_str()) {
            continue;
        }
        let old_node = installed.get(action.node_id.as_str());
        let previous = old_node.and_then(|node| node.current.as_ref());
        let is_package_action = !matches!(action.kind, ActionKind::Enable | ActionKind::Disable);
        for plugin in &current.provides {
            let Some(member_key) = plugin.member_key.as_deref() else {
                continue;
            };
            if action
                .member_key
                .as_deref()
                .is_some_and(|key| key != member_key)
            {
                continue;
            }
            let Some(member_summary) = summary
                .members
                .iter()
                .find(|member| member.key == member_key)
            else {
                continue;
            };
            let (previous_member, ambiguous) = previous_member(previous, plugin, member_key);
            let was_enabled = old_node.is_some_and(|node| node.enabled)
                && previous_member.is_some_and(|member| member.enabled);
            let is_enabled = final_node.enabled && plugin.enabled;
            if !is_package_action && is_enabled == was_enabled && action.member_key.is_none() {
                continue;
            }
            let from_version = previous_member.and_then(|member| member.version.clone());
            let to_version = plugin.version.clone();
            let kind = if is_package_action {
                if ambiguous {
                    None
                } else if previous_member.is_some() {
                    Some(
                        match from_version.as_deref().zip(to_version.as_deref()).and_then(
                            |(from, to)| crate::releases::compare_release_versions(to, from),
                        ) {
                            Some(Ordering::Greater) => ActionKind::Upgrade,
                            Some(Ordering::Less) => ActionKind::Downgrade,
                            _ => ActionKind::Replace,
                        },
                    )
                } else {
                    Some(ActionKind::Install)
                }
            } else {
                Some(action.kind.clone())
            };
            rows.entry(parent_id.to_owned())
                .or_default()
                .push(BundledDll {
                    parent_node_id: parent_id.to_owned(),
                    node_id: action.node_id.clone(),
                    member_key: member_key.to_owned(),
                    dll_file: member_summary.dll_file.clone(),
                    status_member_key: action.member_key.clone(),
                    action_kind: action.kind.clone(),
                    candidate_id: action.to_candidate_id.clone(),
                    role: member_summary.role,
                    kind,
                    from_version: if is_package_action {
                        from_version
                    } else {
                        None
                    },
                    to_version: if is_package_action { to_version } else { None },
                    map_name: (parent.kind == NodeKind::Map).then(|| MapName {
                        label: map_metadata
                            .map(|metadata| metadata.name.clone())
                            .filter(|name| !name.trim().is_empty())
                            .unwrap_or_else(|| parent.label.clone()),
                        name_i18n: parent.name_i18n.clone(),
                        name_zh: map_metadata
                            .and_then(|metadata| metadata.name_zh.clone())
                            .or_else(|| parent.map_name_zh.clone()),
                    }),
                });
        }
    }
    rows
}

fn previous_member<'a>(
    previous: Option<&'a Candidate>,
    plugin: &ProvidedPlugin,
    member_key: &str,
) -> (Option<&'a ProvidedPlugin>, bool) {
    let Some(previous) = previous else {
        return (None, false);
    };
    let mut matches = previous
        .provides
        .iter()
        .filter(|old| old.member_key.as_deref() == Some(member_key));
    if let Some(first) = matches.next() {
        return if matches.next().is_some() {
            (None, true)
        } else {
            (Some(first), false)
        };
    }
    if plugin.guid.is_empty() {
        return (None, false);
    }
    let mut matches = previous
        .provides
        .iter()
        .filter(|old| old.guid.eq_ignore_ascii_case(&plugin.guid));
    let first = matches.next();
    if matches.next().is_some() {
        (None, true)
    } else {
        (first, false)
    }
}
