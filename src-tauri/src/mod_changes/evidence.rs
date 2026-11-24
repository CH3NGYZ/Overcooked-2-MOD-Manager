use super::{
    evaluate::same_guid,
    inventory::{self, TrustedInventory},
    model::{Candidate, ChangeNode, DiagnosticTarget, Inventory, ProvidedPlugin, SolveResult},
};
use crate::{models::InstalledMod, mods};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

const ROW_LIMIT: usize = 24;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(crate) struct ProviderFact {
    #[serde(skip_serializing_if = "Option::is_none")]
    node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    candidate_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    member_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    origin: Origin,
    enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    installed_enabled: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
enum Origin {
    Installed,
    Planned,
    Selected,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct ProviderEvidence {
    guid: String,
    rows: Vec<ProviderFact>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    truncated: bool,
}

impl ProviderEvidence {
    fn new(guid: &str, rows: impl IntoIterator<Item = ProviderFact>) -> Self {
        let mut rows: Vec<_> = rows.into_iter().collect();
        let truncated = rows.len() > ROW_LIMIT;
        rows.truncate(ROW_LIMIT);
        Self {
            guid: guid.into(),
            rows,
            truncated,
        }
    }

    fn error(&self, tag: &str) -> String {
        self.error_with_params(tag, &[])
    }

    fn error_with_params(&self, tag: &str, params: &[(&str, &str)]) -> String {
        match serde_json::to_string(self) {
            Ok(evidence) => {
                let mut fields = params.to_vec();
                fields.push(("evidence", &evidence));
                crate::locale_tag::with_params(tag, &fields)
            }
            Err(_) => crate::locale_tag::with_params(tag, params),
        }
    }
}

fn physical_path(path: &str, enabled: bool) -> String {
    let canonical = mods::canonical_dll_name(path);
    if enabled {
        canonical
    } else {
        mods::disabled_dll_file(&canonical)
    }
}

pub(crate) fn framework_ambiguity(mods: &[&InstalledMod]) -> String {
    ProviderEvidence::new(
        mods::plugin_target::DIYLEVEL_GUID,
        mods.iter().map(|item| ProviderFact {
            node_id: None,
            candidate_id: None,
            member_key: None,
            path: Some(physical_path(&item.dll_file, item.enabled)),
            version: item.version.clone(),
            source: item.repository_id.clone(),
            origin: Origin::Installed,
            enabled: item.enabled,
            installed_enabled: Some(item.enabled),
        }),
    )
    .error("framework.diyLevelAmbiguous")
}

pub(crate) fn planned_framework_ambiguity<'a>(
    members: impl IntoIterator<Item = (&'a ChangeNode, &'a ProvidedPlugin)>,
) -> String {
    ProviderEvidence::new(
        mods::plugin_target::DIYLEVEL_GUID,
        members.into_iter().map(|(node, member)| ProviderFact {
            node_id: Some(node.id.clone()),
            candidate_id: node.current.as_ref().map(|candidate| candidate.id.clone()),
            member_key: member.member_key.clone(),
            path: None,
            version: member.version.clone(),
            source: node
                .current
                .as_ref()
                .and_then(|candidate| candidate.source_id.clone()),
            origin: Origin::Planned,
            enabled: node.enabled && member.enabled,
            installed_enabled: None,
        }),
    )
    .error("framework.diyLevelAmbiguous")
}

pub(super) fn duplicate_replacement(trusted: &TrustedInventory, repo: &str, tag: &str) -> String {
    let guid = trusted
        .config
        .repositories
        .iter()
        .find(|item| item.id == repo)
        .map(|item| item.plugin_guid.as_str())
        .unwrap_or_default();
    let rows = trusted
        .inventory
        .nodes
        .iter()
        .filter(|node| {
            trusted
                .files
                .get(&node.id)
                .is_some_and(|files| files.summary.repository_id.as_deref() == Some(repo))
        })
        .filter_map(|node| node.current.as_ref().map(|candidate| (node, candidate)))
        .flat_map(|(node, candidate)| {
            candidate
                .provides
                .iter()
                .map(move |member| (node, candidate, member))
        })
        .filter(|(_, _, member)| same_guid(&member.guid, guid))
        .map(|(node, candidate, member)| {
            fact(
                trusted,
                &node.id,
                candidate,
                member,
                node.enabled && member.enabled,
                false,
            )
        })
        .collect::<Vec<_>>();
    if rows.len() < 2 {
        return tag.into();
    }
    ProviderEvidence::new(guid, rows).error(tag)
}

fn installed_member<'a>(
    trusted: &'a TrustedInventory,
    node: &str,
    candidate: &str,
    member: &str,
) -> Option<&'a inventory::MemberSummary> {
    let source = trusted
        .inventory
        .nodes
        .iter()
        .find(|item| item.id == node)?
        .current
        .as_ref()?;
    if source.id != candidate
        || !source
            .provides
            .iter()
            .any(|item| item.member_key.as_deref() == Some(member))
    {
        return None;
    }
    trusted
        .files
        .get(node)?
        .summary
        .members
        .iter()
        .find(|item| item.key == member)
}

fn fact(
    trusted: &TrustedInventory,
    node: &str,
    candidate: &Candidate,
    member: &ProvidedPlugin,
    enabled: bool,
    selected: bool,
) -> ProviderFact {
    fact_for_target(
        trusted,
        &DiagnosticTarget {
            node_id: node.into(),
            version: member.version.clone(),
            enabled,
            candidate_id: Some(candidate.id.clone()),
            member_key: member.member_key.clone(),
            source: candidate.source_id.clone(),
        },
        selected,
    )
}

pub(crate) fn duplicate_runtime<'a>(
    trusted: &TrustedInventory,
    guid: &str,
    members: impl IntoIterator<Item = (&'a ChangeNode, &'a ProvidedPlugin)>,
) -> Option<String> {
    let rows: Vec<_> = members
        .into_iter()
        .filter_map(|(node, plugin)| {
            let candidate = node.current.as_ref()?;
            Some(fact(trusted, &node.id, candidate, plugin, true, false))
        })
        .collect();
    if rows.len() < 2 {
        return None;
    }
    let evidence = ProviderEvidence::new(guid, rows);
    let paths = evidence.rows.iter().filter_map(|row| row.path.as_deref());
    let locations = paths.collect::<Vec<_>>().join(" · ");
    Some(evidence.error_with_params(
        "modChanges.duplicatePluginRuntime",
        &[("guid", guid), ("paths", &locations)],
    ))
}

fn fact_for_target(
    trusted: &TrustedInventory,
    target: &DiagnosticTarget,
    selected: bool,
) -> ProviderFact {
    let installed = target
        .member_key
        .as_deref()
        .zip(target.candidate_id.as_deref())
        .and_then(|(member, candidate)| {
            installed_member(trusted, &target.node_id, candidate, member)
        });
    ProviderFact {
        node_id: Some(target.node_id.clone()),
        candidate_id: target.candidate_id.clone(),
        member_key: target.member_key.clone(),
        path: installed.map(|item| physical_path(&item.dll_file, item.enabled)),
        version: target.version.clone(),
        source: target.source.clone(),
        origin: if selected {
            Origin::Selected
        } else if installed.is_some() {
            Origin::Installed
        } else {
            Origin::Planned
        },
        enabled: target.enabled,
        installed_enabled: installed.map(|item| item.enabled),
    }
}

fn enrich_group(
    groups: &mut BTreeMap<String, BTreeSet<ProviderFact>>,
    guid: &str,
    facts: impl IntoIterator<Item = ProviderFact>,
) {
    let rows = groups.entry(guid.trim().to_ascii_lowercase()).or_default();
    rows.extend(facts);
}

fn members<'a>(
    nodes: &'a [ChangeNode],
    guid: &str,
) -> Vec<(&'a ChangeNode, &'a Candidate, &'a ProvidedPlugin)> {
    nodes
        .iter()
        .filter_map(|node| node.current.as_ref().map(|candidate| (node, candidate)))
        .flat_map(|(node, candidate)| {
            candidate
                .provides
                .iter()
                .map(move |plugin| (node, candidate, plugin))
        })
        .filter(|(_, _, plugin)| same_guid(&plugin.guid, guid))
        .collect()
}

pub(super) fn choices(
    trusted: &TrustedInventory,
    result: &SolveResult,
    solved: Option<&Inventory>,
) -> Vec<ProviderEvidence> {
    let mut groups = BTreeMap::new();
    for step in result
        .blockers
        .iter()
        .flat_map(|blocker| blocker.traces.iter().flatten())
    {
        let Some(relation) = &step.relation else {
            continue;
        };
        if step.targets.len() > 1 {
            enrich_group(
                &mut groups,
                &relation.guid,
                step.targets
                    .iter()
                    .map(|target| fact_for_target(trusted, target, false)),
            );
        }
    }
    if let Some(solved) = solved {
        let changed: BTreeSet<&str> = result
            .actions
            .iter()
            .map(|action| action.node_id.as_str())
            .collect();
        let affected: BTreeSet<_> = result
            .impacts
            .iter()
            .map(String::as_str)
            .chain(changed.iter().copied())
            .collect();
        let mut relevant: BTreeSet<_> = solved
            .nodes
            .iter()
            .chain(&trusted.inventory.nodes)
            .filter(|node| changed.contains(node.id.as_str()))
            .flat_map(|node| {
                node.current
                    .iter()
                    .flat_map(|candidate| &candidate.provides)
            })
            .map(|member| member.guid.trim().to_ascii_lowercase())
            .collect();
        relevant.extend(
            result
                .evaluations
                .iter()
                .filter(|evaluation| affected.contains(evaluation.node_id.as_str()))
                .flat_map(|evaluation| &evaluation.runtime)
                .filter(|edge| edge.relation.kind == crate::models::RelationKind::Requires)
                .map(|edge| edge.relation.guid.trim().to_ascii_lowercase()),
        );
        for guid in relevant.into_iter().filter(|guid| !guid.is_empty()) {
            let installed = members(&trusted.inventory.nodes, &guid);
            let final_members = members(&solved.nodes, &guid);
            if installed
                .iter()
                .chain(&final_members)
                .filter_map(|(_, _, member)| member.member_key.as_deref())
                .collect::<BTreeSet<_>>()
                .len()
                < 2
            {
                continue;
            }
            let selected: Vec<_> = final_members
                .into_iter()
                .filter(|(node, _, member)| node.enabled && member.enabled)
                .collect();
            if selected.len() != 1 {
                continue;
            }
            let alternatives = installed.into_iter().filter(|(node, candidate, member)| {
                !selected.iter().any(|(chosen, value, plugin)| {
                    node.id == chosen.id
                        && candidate.id == value.id
                        && member.member_key == plugin.member_key
                })
            });
            enrich_group(
                &mut groups,
                &guid,
                alternatives.map(|(node, candidate, member)| {
                    fact(
                        trusted,
                        &node.id,
                        candidate,
                        member,
                        node.enabled && member.enabled,
                        false,
                    )
                }),
            );
            enrich_group(
                &mut groups,
                &guid,
                selected.into_iter().map(|(node, candidate, member)| {
                    fact(trusted, &node.id, candidate, member, true, true)
                }),
            );
        }
    }
    groups
        .into_iter()
        .filter(|(_, rows)| rows.len() > 1)
        .map(|(guid, rows)| ProviderEvidence::new(&guid, rows))
        .collect()
}
