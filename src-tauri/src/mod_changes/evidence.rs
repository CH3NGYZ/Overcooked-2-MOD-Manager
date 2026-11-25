use super::{
    evaluate::same_guid,
    inventory::{self, TrustedInventory},
    model::{
        Candidate, ChangeNode, DiagnosticTarget, Inventory, NodeKind, ProvidedPlugin, SolveResult,
    },
    session::{Payload, PlanSession},
};
use crate::{
    models::{InstalledMod, Provider},
    mods,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

const ROW_LIMIT: usize = 24;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
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
    projected_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    name_i18n: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    repository: Option<RepositoryFact>,
    origin: Origin,
    enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    installed_enabled: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct RepositoryFact {
    provider: Provider,
    owner: String,
    repository: String,
}

impl RepositoryFact {
    fn from_coordinates(provider: &Provider, owner: &str, repository: &str) -> Option<Self> {
        let (owner, repository) =
            crate::providers::repository_coordinates(owner, repository).ok()?;
        Some(Self {
            provider: provider.clone(),
            owner,
            repository,
        })
    }

    fn from_installed(item: &InstalledMod) -> Option<Self> {
        let binding = item
            .repository_binding
            .as_ref()
            .filter(|binding| binding.is_valid())?;
        Self::from_coordinates(&binding.provider, &binding.owner, &binding.repository)
    }
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
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    runtime_conflict: bool,
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
            runtime_conflict: false,
        }
    }

    fn error(&self, tag: &str) -> String {
        match serde_json::to_string(self) {
            Ok(evidence) => crate::locale_tag::with_params(tag, &[("evidence", &evidence)]),
            Err(_) => tag.into(),
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
            projected_path: None,
            version: item.version.clone(),
            label: item.plugin_name.clone(),
            name_i18n: item.package_name_i18n.clone(),
            repository: RepositoryFact::from_installed(item),
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
            projected_path: None,
            version: member.version.clone(),
            label: None,
            name_i18n: BTreeMap::new(),
            repository: None,
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
    session: &PlanSession,
    guid: &str,
    members: impl IntoIterator<Item = (&'a ChangeNode, &'a ProvidedPlugin)>,
) -> Option<String> {
    let rows: Vec<_> = members
        .into_iter()
        .filter_map(|(node, plugin)| {
            let candidate = node.current.as_ref()?;
            let mut row = fact(&session.trusted, &node.id, candidate, plugin, true, false);
            if row.path.is_none()
                && !candidate.verification_pending
                && let Some(key) = plugin.member_key.as_deref()
            {
                row.projected_path = verified_path(session, &node.id, &candidate.id, key)
                    .map(|path| physical_path(path, true));
            }
            Some(row)
        })
        .collect();
    if rows.len() < 2 {
        return None;
    }
    let mut evidence = ProviderEvidence::new(guid, rows);
    evidence.runtime_conflict = true;
    Some(evidence.error("modChanges.duplicatePluginRuntime"))
}

fn verified_path<'a>(
    session: &'a PlanSession,
    node: &str,
    candidate: &str,
    key: &str,
) -> Option<&'a str> {
    if let Some(path) = session
        .map_payload
        .as_ref()
        .and_then(|batch| batch.member_files.get(candidate))
        .and_then(|files| files.get(key))
    {
        return Some(path);
    }
    Payload::verified(
        &session.verified_payloads,
        &session.payloads,
        node,
        candidate,
    )?
    .member_file(key)
}

fn repository_for_target(
    trusted: &TrustedInventory,
    target: &DiagnosticTarget,
    installed: Option<&inventory::MemberSummary>,
    summary: Option<&inventory::NodeSummary>,
) -> Option<RepositoryFact> {
    let map = summary.is_some_and(|item| {
        item.kind == NodeKind::Map || item.bundle_parent_node_id.is_some() && item.map_id.is_some()
    });
    if map {
        let source = match installed {
            Some(_) => summary?.map_id.as_ref(),
            None => target.source.as_ref(),
        }?;
        let repo = trusted
            .config
            .map_repositories
            .iter()
            .find(|repo| repo.id == *source)?;
        return RepositoryFact::from_coordinates(&repo.provider, &repo.username, &repo.repository);
    }
    if let Some(member) = installed {
        let mut matches = trusted
            .installed_mods
            .iter()
            .filter(|item| item.dll_file == member.dll_file && item.enabled == member.enabled);
        let item = matches.next()?;
        if matches.next().is_some() {
            return None;
        }
        return RepositoryFact::from_installed(item);
    }
    let source = target.source.as_ref()?;
    let repo = trusted
        .config
        .repositories
        .iter()
        .find(|repo| repo.id == *source)?;
    RepositoryFact::from_coordinates(&repo.provider, &repo.username, &repo.repository)
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
    let summary = trusted
        .files
        .get(&target.node_id)
        .map(|files| &files.summary);
    ProviderFact {
        node_id: Some(target.node_id.clone()),
        candidate_id: target.candidate_id.clone(),
        member_key: target.member_key.clone(),
        path: installed.map(|item| physical_path(&item.dll_file, item.enabled)),
        projected_path: None,
        version: target.version.clone(),
        label: summary
            .map(|item| item.label.clone())
            .filter(|label| !label.trim().is_empty()),
        name_i18n: summary
            .map(|item| item.name_i18n.clone())
            .unwrap_or_default(),
        repository: repository_for_target(trusted, target, installed, summary),
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
    groups: &mut BTreeMap<String, Vec<ProviderFact>>,
    guid: &str,
    facts: impl IntoIterator<Item = ProviderFact>,
) {
    let rows = groups.entry(guid.trim().to_ascii_lowercase()).or_default();
    for fact in facts {
        if !rows.contains(&fact) {
            rows.push(fact);
        }
    }
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
