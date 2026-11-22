use super::actions::ActionReasons;
use super::evaluate::{evaluate, matches, owned_targets, same_guid, scoped_relations, targets};
use super::inventory::TrustedInventory;
use super::model::*;
use super::mutations::LooseReleaseSearch;
use crate::models::{RelationKind, RepositoryRelation};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

const MAX_SEARCH_STATES: usize = 30_000;

#[derive(Clone)]
struct SearchState {
    inventory: Inventory,
    decided: BTreeSet<usize>,
    replaced_installed: usize,
    /// Explicit/conflict disablement preserves its immediate reverse cascade.
    disabled_guids: BTreeSet<String>,
    /// Dependency fallback cascades search compatible owner versions before disabling.
    fallback_disabled_guids: BTreeSet<String>,
    reasons: ActionReasons,
    projected_loose_providers: BTreeSet<usize>,
}

#[derive(Clone, Copy)]
enum DisableSource {
    Immediate,
    DependencyFallback,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SearchPass {
    LimitReplacements(usize),
    CompatibleVersions,
    AllowDependencyDisable,
}

struct Search<'a, 'session> {
    original: &'a Inventory,
    trusted: Option<&'a TrustedInventory>,
    loose_releases: Option<&'a mut LooseReleaseSearch<'session>>,
    /// The adapter proved a specific replacement of these groups, not switches
    /// on their old DLLs or permission to upgrade already satisfied edges.
    replacement_only: BTreeSet<usize>,
    fixed: BTreeSet<usize>,
    roots: BTreeSet<usize>,
    repair_owner: Option<usize>,
    visited: BTreeMap<String, bool>,
    baseline: BTreeSet<String>,
    failure: Option<Diagnostic>,
    failure_traces: diagnostics::FailureTraces,
    catalog_unknown: bool,
    fixed_members: BTreeSet<(usize, String)>,
    policy: DependencyPolicy,
    trace: trace::SolveTrace<'a>,
    pass: SearchPass,
    preference_remaining: usize,
    preference_pruned: bool,
}

mod diagnostics;
mod package_switches;
mod policy;
mod reasons;
mod trace;
mod visit;

pub(crate) use policy::DependencyPolicy;

fn relation_key(owner: &str, relation: &RepositoryRelation) -> String {
    format!(
        "{owner}|{}|{}|{}|{:?}",
        relation.kind.as_str(),
        relation.guid.to_lowercase(),
        relation.version,
        relation.prerelease
    )
}

fn changed(original: &Inventory, current: &Inventory, index: usize) -> bool {
    original.nodes[index].current != current.nodes[index].current
        || original.nodes[index].enabled != current.nodes[index].enabled
}

fn provides(candidate: &Candidate, guid: &str) -> bool {
    candidate
        .provides
        .iter()
        .any(|plugin| same_guid(&plugin.guid, guid))
}

fn required_provider(inventory: &Inventory, relation: &RepositoryRelation) -> Option<usize> {
    if relation.kind == RelationKind::Requires
        && !relation.optional
        && let [(index, plugin)] = targets(inventory, &relation.guid, true).as_slice()
        && matches(relation, plugin) == Some(true)
    {
        Some(*index)
    } else {
        None
    }
}

fn preserves_package_identity(current: &Candidate, candidate: &Candidate) -> bool {
    candidate.verification_pending
        // Historical versions can predate optional bundled plugins. The trusted
        // repository plus a surviving GUID identifies the same replacement unit;
        // required retired members are still checked by the whole-graph solver.
        || (current.source_id.as_ref().is_some_and(|source| {
            !source.trim().is_empty() && candidate.source_id.as_ref() == Some(source)
        }) && current
            .provides
            .iter()
            .any(|plugin| !plugin.guid.trim().is_empty() && provides(candidate, &plugin.guid)))
        || current
            .provides
            .iter()
            .filter(|plugin| !plugin.guid.trim().is_empty())
            .all(|plugin| provides(candidate, &plugin.guid))
}

fn ordered_candidates(
    trusted: Option<&TrustedInventory>,
    node: &ChangeNode,
    prefer_installed: bool,
) -> Vec<Candidate> {
    let mut candidates = node
        .candidates
        .iter()
        .filter(|candidate| candidate.installable && !candidate.raw)
        .map(|candidate| package_switches::replacement_candidate(trusted, node, candidate.clone()))
        .collect::<Vec<_>>();
    if let Some(current) = &node.current {
        if !candidates
            .iter()
            .any(|candidate| candidate.id == current.id)
        {
            candidates.push(current.clone());
        }
    }
    candidates.sort_by(|left, right| {
        let current = node.current.as_ref().map(|candidate| &candidate.id);
        let installed_order = (Some(&right.id) == current).cmp(&(Some(&left.id) == current));
        if prefer_installed && installed_order != Ordering::Equal {
            return installed_order;
        }
        crate::releases::compare_release_versions(
            right
                .tag
                .as_deref()
                .or(right.version.as_deref())
                .unwrap_or(""),
            left.tag
                .as_deref()
                .or(left.version.as_deref())
                .unwrap_or(""),
        )
        .unwrap_or(Ordering::Equal)
        .then(installed_order)
        .then(left.source_id.cmp(&right.source_id))
        .then(left.id.cmp(&right.id))
    });
    candidates.dedup_by(|left, right| left.id == right.id);
    candidates
}

impl Search<'_, '_> {
    fn readiness(&self, index: usize, node: &ChangeNode) -> bool {
        self.repair_owner == Some(index) && !node.enabled
    }

    fn order(&self, state: &SearchState) -> BTreeMap<usize, usize> {
        let mut distances = self
            .roots
            .iter()
            .map(|index| (*index, 0))
            .collect::<BTreeMap<_, _>>();
        loop {
            let before = distances.len();
            for (index, distance) in distances.clone() {
                let own = state.inventory.nodes[index]
                    .current
                    .iter()
                    .flat_map(|candidate| &candidate.relations)
                    .map(|owned| owned.relation.guid.trim().to_lowercase())
                    .collect::<BTreeSet<_>>();
                for (target, node) in state.inventory.nodes.iter().enumerate() {
                    let forward = node
                        .current
                        .iter()
                        .chain(node.candidates.iter())
                        .flat_map(|candidate| &candidate.provides)
                        .any(|plugin| own.contains(&plugin.guid.trim().to_lowercase()));
                    let reverse =
                        node.current
                            .iter()
                            .flat_map(|candidate| &candidate.relations)
                            .any(|owned| {
                                state.inventory.nodes[index].current.as_ref().is_some_and(
                                    |candidate| provides(candidate, &owned.relation.guid),
                                )
                            });
                    if forward || reverse {
                        distances.entry(target).or_insert(distance + 1);
                    }
                }
            }
            if distances.len() == before {
                break;
            }
        }
        distances
    }

    fn node_order(
        &self,
        state: &SearchState,
        distances: &BTreeMap<usize, usize>,
        index: usize,
    ) -> (usize, String, String) {
        let node = &state.inventory.nodes[index];
        let guid = node
            .current
            .iter()
            .chain(node.candidates.iter())
            .flat_map(|candidate| &candidate.provides)
            .map(|plugin| plugin.guid.trim().to_lowercase())
            .min()
            .unwrap_or_default();
        (
            distances.get(&index).copied().unwrap_or(usize::MAX),
            guid,
            node.id.clone(),
        )
    }

    fn changed_guids(&self, state: &SearchState) -> BTreeSet<String> {
        // Runtime constraints observe enabled providers, their versions and
        // prerelease verdicts. Switching one sibling must not pull unrelated
        // baseline errors about every other member of its atomic package in.
        let providers = |inventory: &Inventory| {
            let mut values = BTreeMap::<String, Vec<(Option<String>, Option<bool>)>>::new();
            for node in inventory.nodes.iter().filter(|node| node.enabled) {
                for plugin in node
                    .current
                    .iter()
                    .flat_map(|candidate| &candidate.provides)
                    .filter(|plugin| plugin.enabled && !plugin.guid.trim().is_empty())
                {
                    values
                        .entry(plugin.guid.trim().to_lowercase())
                        .or_default()
                        .push((plugin.version.clone(), plugin.prerelease));
                }
            }
            for versions in values.values_mut() {
                versions.sort();
            }
            values
        };
        let before = providers(self.original);
        let after = providers(&state.inventory);
        before
            .keys()
            .chain(after.keys())
            .filter(|guid| before.get(*guid) != after.get(*guid))
            .cloned()
            .collect()
    }

    fn protected(&self, state: &SearchState) -> BTreeSet<usize> {
        let mut protected = self
            .roots
            .iter()
            .copied()
            .filter(|index| {
                state.inventory.nodes[*index].enabled || self.repair_owner == Some(*index)
            })
            .collect::<BTreeSet<_>>();
        loop {
            let previous = protected.len();
            for index in protected.clone() {
                let node = &state.inventory.nodes[index];
                for owned in scoped_relations(node, self.readiness(index, node)) {
                    // As in evaluation, a required edge is only satisfied by
                    // a unique enabled provider, regardless of version matches.
                    if let Some(target) = required_provider(&state.inventory, &owned.relation)
                        .or_else(|| self.unconfirmed_provider(state, &owned.relation))
                    {
                        protected.insert(target);
                    }
                }
            }
            if protected.len() == previous {
                break;
            }
        }
        protected
    }

    fn relevant(
        &self,
        state: &SearchState,
        index: usize,
        relation: &RepositoryRelation,
        protected: &BTreeSet<usize>,
        changed_guids: &BTreeSet<String>,
    ) -> bool {
        protected.contains(&index)
            || changed(self.original, &state.inventory, index)
            || changed_guids.contains(&relation.guid.trim().to_lowercase())
            || !self
                .baseline
                .contains(&relation_key(&state.inventory.nodes[index].id, relation))
    }

    fn mark_failure(
        &mut self,
        owner: &ChangeNode,
        relation: Option<&RepositoryRelation>,
        incomplete: bool,
        code: &str,
    ) {
        let mut blocker = diagnostic(code, Some(&owner.id), incomplete);
        blocker.target_guid = relation.map(|relation| relation.guid.clone());
        self.failure = Some(blocker);
    }

    fn with_candidate(
        &mut self,
        state: &SearchState,
        index: usize,
        mut candidate: Candidate,
        relation: Option<&RepositoryRelation>,
        reason: Option<&Arc<ChangeReason>>,
    ) -> Option<SearchState> {
        let mut replaced_installed = state.replaced_installed;
        if let SearchPass::LimitReplacements(limit) = self.pass {
            // In the bounded pass each candidate decision is final on its branch.
            // Switches and new installs do not replace an installed package.
            replaced_installed += usize::from(
                self.original.nodes[index]
                    .current
                    .as_ref()
                    .is_some_and(|installed| installed.id != candidate.id),
            );
            if replaced_installed > limit {
                self.preference_pruned = true;
                return None;
            }
        }
        let installed = state.inventory.nodes[index].current.as_ref();
        if self.replacement_only.contains(&index) {
            if installed.is_some_and(|current| current.id == candidate.id) {
                return None;
            }
            // The combined old group cannot supply switch defaults for its new
            // release. Payload verification preserves eligible real members.
        } else if let Some(current) = installed {
            for plugin in &mut candidate.provides {
                let existing = current
                    .provides
                    .iter()
                    .find(|previous| {
                        plugin.member_key.is_some() && previous.member_key == plugin.member_key
                    })
                    .or_else(|| {
                        current.provides.iter().find(|previous| {
                            !plugin.guid.is_empty() && same_guid(&previous.guid, &plugin.guid)
                        })
                    });
                if let Some(existing) = existing {
                    plugin.enabled = existing.enabled;
                }
            }
        }
        let node = &state.inventory.nodes[index];
        if let Some(relation) = relation {
            if relation.kind == RelationKind::Requires && self.whole_mod_package(node) {
                if !node.enabled {
                    // Re-activating a stopped package enables all its members,
                    // even when a candidate kept their pre-switch logical states.
                    for member in &mut candidate.provides {
                        member.enabled = true;
                    }
                } else {
                    self.enable_required_members(index, node, &mut candidate, relation);
                }
            } else {
                for plugin in &mut candidate.provides {
                    if same_guid(&plugin.guid, &relation.guid)
                        && matches(relation, plugin) == Some(true)
                    {
                        plugin.enabled = true;
                    }
                }
            }
        }
        let mut next = state.clone();
        next.replaced_installed = replaced_installed;
        next.inventory.nodes[index].current = Some(candidate);
        next.inventory.nodes[index].enabled = true;
        next.decided.insert(index);
        if let Some(reason) = reason {
            next.reasons.record(
                index,
                &state.inventory.nodes[index],
                &next.inventory.nodes[index],
                reason,
            );
        }
        Some(next)
    }

    fn replacements(
        &mut self,
        state: &SearchState,
        index: usize,
        reason: &Arc<ChangeReason>,
    ) -> Vec<SearchState> {
        let node = &state.inventory.nodes[index];
        if self.fixed.contains(&index)
            || state.decided.contains(&index)
            || !node.mutable
            || node.kind == NodeKind::Map
        {
            return Vec::new();
        }
        let proven_upgrade = self.replacement_only.contains(&index);
        ordered_candidates(self.trusted, node, false)
            .into_iter()
            .filter(|candidate| {
                node.current.as_ref().is_none_or(|current| {
                    candidate.id != current.id
                        && (proven_upgrade || preserves_package_identity(current, candidate))
                })
            })
            .filter_map(|candidate| {
                self.with_candidate(state, index, candidate, None, Some(reason))
            })
            .collect()
    }

    fn disable(
        &self,
        state: &SearchState,
        index: usize,
        guid: Option<&str>,
        reason: &Arc<ChangeReason>,
        protected: &BTreeSet<usize>,
        source: DisableSource,
    ) -> Option<SearchState> {
        let node = &state.inventory.nodes[index];
        if self.fixed.contains(&index) || !node.mutable || self.replacement_only.contains(&index) {
            return None;
        }
        let guid = if self.whole_mod_package(node) {
            None
        } else {
            guid
        };
        let mut next = state.clone();
        let target = &mut next.inventory.nodes[index];
        let candidate = target.current.as_mut()?;
        let mut switched = false;
        for plugin in &mut candidate.provides {
            if plugin.enabled && guid.is_none_or(|guid| same_guid(&plugin.guid, guid)) {
                match source {
                    DisableSource::Immediate => &mut next.disabled_guids,
                    DisableSource::DependencyFallback => &mut next.fallback_disabled_guids,
                }
                .insert(plugin.guid.trim().to_lowercase());
                plugin.enabled = false;
                switched = true;
            }
        }
        if guid.is_none() || candidate.provides.iter().all(|plugin| !plugin.enabled) {
            switched |= target.enabled;
            target.enabled = false;
        }
        if !switched {
            return None;
        }
        if protected.contains(&index) {
            // An asset may contain an unrelated conflicting member. Preserve all
            // currently satisfied required edges, rather than protecting every DLL
            // merely because one member provides a required GUID.
            if !next.inventory.nodes[index].enabled {
                return None;
            }
            for owner_index in protected {
                let owner = &state.inventory.nodes[*owner_index];
                for owned in scoped_relations(owner, self.readiness(*owner_index, owner)) {
                    if owned.relation.kind != RelationKind::Requires || owned.relation.optional {
                        continue;
                    }
                    let before = targets(&state.inventory, &owned.relation.guid, true)
                        .iter()
                        .any(|(_, plugin)| matches(&owned.relation, plugin) == Some(true));
                    let after = targets(&next.inventory, &owned.relation.guid, true)
                        .iter()
                        .any(|(_, plugin)| matches(&owned.relation, plugin) == Some(true));
                    if before && !after {
                        return None;
                    }
                }
            }
        }
        next.reasons
            .record(index, node, &next.inventory.nodes[index], reason);
        Some(next)
    }

    fn repair(
        &mut self,
        state: &SearchState,
        owner: usize,
        issue: &RelationEvaluation,
        protected: &BTreeSet<usize>,
    ) -> (Vec<SearchState>, Option<SearchState>) {
        let relation = &issue.relation;
        let mut branches = Vec::new();
        let is_conflict_edge = matches!(
            relation.kind,
            RelationKind::Conflicts | RelationKind::Compatible
        );
        let mut incompatible_nodes = Vec::new();
        if is_conflict_edge {
            let owner_node = &state.inventory.nodes[owner];
            let active_targets = scoped_relations(owner_node, self.readiness(owner, owner_node))
                .filter(|owned| owned.relation == *relation)
                .flat_map(|owned| owned_targets(&state.inventory, owner_node, owned, true));
            for (index, plugin) in active_targets {
                let incompatible = match relation.kind {
                    RelationKind::Conflicts => matches(relation, plugin) == Some(true),
                    RelationKind::Compatible => matches(relation, plugin) == Some(false),
                    _ => false,
                };
                if incompatible {
                    incompatible_nodes.push(state.inventory.nodes[index].id.clone());
                    if let Some(next) = self.disable(
                        state,
                        index,
                        Some(&relation.guid),
                        &reasons::provider_change(&state.inventory, index, issue),
                        protected,
                        DisableSource::Immediate,
                    ) {
                        branches.push(next);
                    }
                }
            }
        }
        let owner_reason = reasons::owner_change(
            self.original,
            &state.inventory,
            owner,
            issue,
            &incompatible_nodes,
        );
        // A fixed target can force its conflicting owner off; an explicit
        // disable of a provider can also cascade to its nonrequired owners.
        if (is_conflict_edge
            || relation.kind == RelationKind::Requires
                && state
                    .disabled_guids
                    .contains(&relation.guid.trim().to_lowercase()))
            && let Some(next) = self.disable(
                state,
                owner,
                None,
                &owner_reason,
                protected,
                DisableSource::Immediate,
            )
        {
            branches.push(next);
        }
        // A reverse dependent first explores its own newer compatible historical versions
        // when the changed dependency is fixed by a root choice.
        let fixed_target = self.fixed.iter().any(|index| {
            state.inventory.nodes[*index]
                .current
                .as_ref()
                .is_some_and(|candidate| provides(candidate, &relation.guid))
        });
        if fixed_target {
            branches.extend(self.replacements(state, owner, &owner_reason))
        }
        let mut indices = (0..state.inventory.nodes.len()).collect::<Vec<_>>();
        let distances = self.order(state);
        indices.sort_by_key(|index| {
            let installed_match = !self.policy.prefers_newest()
                && relation.kind == RelationKind::Requires
                && self.original.nodes[*index]
                    .current
                    .iter()
                    .flat_map(|candidate| &candidate.provides)
                    .any(|plugin| {
                        same_guid(&plugin.guid, &relation.guid)
                            && matches(relation, plugin) == Some(true)
                    });
            (!installed_match, self.node_order(state, &distances, *index))
        });
        for index in indices {
            let node = &state.inventory.nodes[index];
            // Deciding a package fixes its version, not every sibling's switch.
            // Later requirements may still enable members of that exact candidate.
            if self.repair_owner != Some(index)
                && !self.replacement_only.contains(&index)
                && (self.fixed.contains(&index) || (state.decided.contains(&index) && node.mutable))
                && relation.kind == RelationKind::Requires
                && node.enabled
            {
                if node.current.is_some() {
                    let mut next = state.clone();
                    let actual = next.inventory.nodes[index].current.as_mut().unwrap();
                    if self.enable_required_members(index, node, actual, relation) {
                        next.reasons.record(
                            index,
                            node,
                            &next.inventory.nodes[index],
                            &reasons::provider_change(&state.inventory, index, issue),
                        );
                        branches.push(next);
                    }
                }
                continue;
            }
            if self.fixed.contains(&index)
                || state.decided.contains(&index)
                || !node.mutable
                || node.kind == NodeKind::Map
            {
                continue;
            }
            // A switch or installed repair should re-enable a suitable installed
            // provider before downloading a replacement. Mismatches still try
            // compatible releases from newest to oldest.
            let mut reason = None;
            for candidate in ordered_candidates(self.trusted, node, !self.policy.prefers_newest()) {
                // Consolidation supplies one candidate, already authorized by
                // physical Release evidence. Its verified core can have a wholly
                // different GUID; the solver still checks actual providers.
                let proven_upgrade = self.replacement_only.contains(&index)
                    && node
                        .current
                        .as_ref()
                        .is_some_and(|old| old.id != candidate.id);
                if node.current.as_ref().is_some_and(|current| {
                    !proven_upgrade && !preserves_package_identity(current, &candidate)
                }) {
                    continue;
                }
                let retires_conflicting_provider = proven_upgrade
                    && relation.kind == RelationKind::Conflicts
                    && incompatible_nodes.contains(&node.id)
                    && !provides(&candidate, &relation.guid);
                if !retires_conflicting_provider
                    && !candidate.provides.iter().any(|plugin| {
                        same_guid(&plugin.guid, &relation.guid)
                            && match relation.kind {
                                RelationKind::Conflicts => matches(relation, plugin) == Some(false),
                                _ => matches(relation, plugin) == Some(true),
                            }
                    })
                {
                    if provides(&candidate, &relation.guid)
                        || node
                            .current
                            .as_ref()
                            .is_some_and(|old| provides(old, &relation.guid))
                    {
                        self.trace
                            .candidate(node, &candidate, Some(relation), "provider_mismatch");
                    }
                    continue;
                }
                self.trace
                    .candidate(node, &candidate, Some(relation), "try_provider");
                branches.extend(self.with_candidate(
                    state,
                    index,
                    candidate,
                    (relation.kind == RelationKind::Requires).then_some(relation),
                    Some(reason.get_or_insert_with(|| {
                        reasons::provider_change(&state.inventory, index, issue)
                    })),
                ));
            }
        }
        if !fixed_target {
            branches.extend(self.replacements(state, owner, &owner_reason))
        }
        let mut fallback = None;
        // Only a complete version-only search may unlock this second-pass fallback.
        // Never discard a newly installed requirement to make its own install succeed.
        if self.pass == SearchPass::AllowDependencyDisable
            && relation.kind == RelationKind::Requires
            && issue.state != EvaluationState::Unknown
            && self.original.nodes[owner].enabled
            && self.original.nodes[owner].current.is_some()
        {
            let mut installed = state.clone();
            installed.inventory.nodes[owner].current = self.original.nodes[owner].current.clone();
            if let Some(next) = self.disable(
                &installed,
                owner,
                None,
                &owner_reason,
                protected,
                DisableSource::DependencyFallback,
            ) {
                fallback = Some(next);
            }
        }
        (branches, fallback)
    }
}

fn diagnostic(code: &str, owner: Option<&str>, incomplete: bool) -> Diagnostic {
    Diagnostic {
        code: code.into(),
        owner_id: owner.map(str::to_string),
        target_guid: None,
        message: code.into(),
        incomplete,
        traces: Vec::new(),
        traces_truncated: false,
    }
}

/// Pure planner; adapters own snapshots, repository policy and actual-asset verification.
#[cfg(test)]
pub(crate) fn solve(inventory: &Inventory, request: &ChangeRequest) -> SolveResult {
    solve_with_inventory(inventory, request).0
}

/// Execution adapters consume the exact solved package/member states, never replay
/// presentation actions to reconstruct a subtly different installed inventory.
#[cfg(test)]
pub(crate) fn solve_with_inventory(
    inventory: &Inventory,
    request: &ChangeRequest,
) -> (SolveResult, Option<Inventory>) {
    let policy = DependencyPolicy::for_plan(inventory, request, false);
    solve_with_policy(inventory, request, None, None, policy, None)
        .expect("an inventory-only solve cannot check disk ownership")
}

pub(super) fn solve_with_policy(
    inventory: &Inventory,
    request: &ChangeRequest,
    trusted: Option<&TrustedInventory>,
    loose_releases: Option<&mut LooseReleaseSearch<'_>>,
    policy: DependencyPolicy,
    operation_id: Option<&str>,
) -> Result<(SolveResult, Option<Inventory>), String> {
    let failure = |blocker: Diagnostic| SolveResult {
        status: if blocker.incomplete {
            SolveStatus::Incomplete
        } else {
            SolveStatus::Unsatisfiable
        },
        actions: Vec::new(),
        evaluations: evaluate(inventory),
        blockers: vec![blocker],
        notices: Vec::new(),
        impacts: Vec::new(),
    };
    match solve_inner(
        inventory,
        request,
        trusted,
        loose_releases,
        policy,
        operation_id,
    ) {
        Ok((result, solved)) => Ok((result, Some(solved))),
        Err(SolveError::Graph(blocker)) => Ok((failure(*blocker), None)),
        Err(SolveError::Ownership(message)) => Err(message),
    }
}

enum SolveError {
    Graph(Box<Diagnostic>),
    Ownership(String),
}

impl From<Diagnostic> for SolveError {
    fn from(blocker: Diagnostic) -> Self {
        Self::Graph(Box::new(blocker))
    }
}

fn solve_inner<'a>(
    inventory: &'a Inventory,
    request: &ChangeRequest,
    trusted: Option<&'a TrustedInventory>,
    loose_releases: Option<&'a mut LooseReleaseSearch<'_>>,
    policy: DependencyPolicy,
    operation_id: Option<&'a str>,
) -> Result<(SolveResult, Inventory), SolveError> {
    let replacement_only = |id: &str| {
        trusted.is_some_and(|trusted| trusted.duplicate_releases.upgrades.contains_key(id))
    };
    let mut state = SearchState {
        inventory: inventory.clone(),
        decided: BTreeSet::new(),
        replaced_installed: 0,
        disabled_guids: BTreeSet::new(),
        fallback_disabled_guids: BTreeSet::new(),
        reasons: ActionReasons::default(),
        projected_loose_providers: BTreeSet::new(),
    };
    let mut roots = BTreeSet::new();
    // Repair keeps the owner's version and switches fixed. Only maps can be
    // repaired while disabled; a disabled MOD is checked when it is enabled.
    let repair_owner = match request.repair_dependencies_of.as_deref() {
        Some(id) => {
            if !request.roots.is_empty() {
                return Err(diagnostic("contradictory_roots", Some(id), false).into());
            }
            let index = inventory
                .nodes
                .iter()
                .position(|node| node.id == id && node.can_repair_dependencies())
                .ok_or_else(|| diagnostic("root_missing", Some(id), true))?;
            roots.insert(index);
            state.decided.insert(index);
            Some(index)
        }
        None => None,
    };
    let mut seen_roots = BTreeMap::new();
    let requested = reasons::requested();
    for root in &request.roots {
        let Some(index) = state
            .inventory
            .nodes
            .iter()
            .position(|node| node.id == root.node_id)
        else {
            return Err(diagnostic("root_missing", Some(&root.node_id), true).into());
        };
        let root_key = (&root.node_id, &root.member_key);
        if seen_roots
            .insert(root_key, root)
            .is_some_and(|previous| previous != root)
        {
            return Err(diagnostic("contradictory_roots", Some(&root.node_id), false).into());
        }
        let node = &mut state.inventory.nodes[index];
        let before = node.clone();
        if (root.member_key.is_some()
            && trusted.is_some_and(|trusted| trusted.atomic_mod_package(&node.id)))
            || (replacement_only(&node.id)
                && (!root.enabled
                    || root.member_key.is_some()
                    || !node
                        .candidates
                        .iter()
                        .any(|candidate| Some(&candidate.id) == root.candidate_id.as_ref())))
            || !node.mutable
                && (root.candidate_id.is_some()
                    || node.enabled != root.enabled
                    || root.member_key.is_some())
        {
            return Err(diagnostic("root_ownership_unsafe", Some(&node.id), true).into());
        }
        if let Some(id) = &root.candidate_id {
            let Some(candidate) = node
                .current
                .iter()
                .chain(node.candidates.iter())
                .find(|candidate| &candidate.id == id)
                .cloned()
            else {
                return Err(diagnostic("root_candidate_missing", Some(&node.id), true).into());
            };
            if node
                .current
                .as_ref()
                .is_none_or(|current| current.id != candidate.id)
                && !candidate.installable
            {
                return Err(
                    diagnostic("root_candidate_not_installable", Some(&node.id), true).into(),
                );
            }
            node.current = Some(package_switches::replacement_candidate(
                trusted, node, candidate,
            ));
        }
        let Some(candidate) = node.current.as_mut() else {
            return Err(diagnostic("root_candidate_missing", Some(&node.id), true).into());
        };
        if let Some(member) = &root.member_key {
            let Some(plugin) = candidate
                .provides
                .iter_mut()
                .find(|plugin| plugin.member_key.as_ref() == Some(member))
            else {
                return Err(diagnostic("root_member_missing", Some(&node.id), true).into());
            };
            plugin.enabled = root.enabled;
            if root.enabled {
                node.enabled = true
            }
        } else {
            node.enabled = root.enabled;
            if root.enabled && root.candidate_id.is_none() {
                for plugin in &mut candidate.provides {
                    plugin.enabled = true
                }
            }
        }
        if !root.enabled {
            for plugin in &candidate.provides {
                if root.member_key.is_none() || root.member_key == plugin.member_key {
                    state
                        .disabled_guids
                        .insert(plugin.guid.trim().to_lowercase());
                }
            }
        }
        state.decided.insert(index);
        roots.insert(index);
        state.reasons.record(index, &before, node, &requested);
    }
    // Whole-package and member intents can overlap. Check their combined state
    // instead of letting the order of the user's batch silently decide the root.
    for root in &request.roots {
        let node = state
            .inventory
            .nodes
            .iter()
            .find(|node| node.id == root.node_id)
            .unwrap();
        let candidate = node.current.as_ref().unwrap();
        let identity_matches = root
            .candidate_id
            .as_ref()
            .is_none_or(|id| candidate.id == *id);
        let enabled_matches =
            root.member_key
                .as_ref()
                .map_or(node.enabled == root.enabled, |member| {
                    candidate
                        .provides
                        .iter()
                        .find(|plugin| plugin.member_key.as_ref() == Some(member))
                        .is_some_and(|plugin| {
                            plugin.enabled == root.enabled && (!root.enabled || node.enabled)
                        })
                });
        if !identity_matches || !enabled_matches {
            return Err(diagnostic("contradictory_roots", Some(&node.id), false).into());
        }
    }
    let baseline = evaluate(inventory)
        .iter()
        .flat_map(|evaluation| {
            evaluation
                .runtime
                .iter()
                .filter(|issue| issue.state.is_issue())
                .map(|issue| relation_key(&issue.owner_id, &issue.relation))
        })
        .collect();
    let fixed_members = request
        .roots
        .iter()
        .filter_map(|root| {
            Some((
                inventory
                    .nodes
                    .iter()
                    .position(|node| node.id == root.node_id)?,
                root.member_key.clone()?,
            ))
        })
        .collect();
    let mut search = Search {
        original: inventory,
        trusted,
        loose_releases,
        replacement_only: inventory
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(index, node)| replacement_only(&node.id).then_some(index))
            .collect(),
        fixed: roots.clone(),
        roots,
        repair_owner,
        fixed_members,
        visited: BTreeMap::new(),
        baseline,
        failure: None,
        failure_traces: diagnostics::FailureTraces::default(),
        catalog_unknown: inventory.nodes.iter().any(|node| !node.candidates_complete),
        policy,
        trace: trace::SolveTrace::new(operation_id),
        pass: SearchPass::CompatibleVersions,
        preference_remaining: MAX_SEARCH_STATES,
        preference_pruned: false,
    };
    let solution = match search.find_solution(&state) {
        Ok(solution) => solution,
        Err(visit::SearchFailure::Ownership(message)) => {
            return Err(SolveError::Ownership(message));
        }
        Err(visit::SearchFailure::Blocked(incomplete)) => {
            let mut blocker = search
                .failure
                .unwrap_or_else(|| diagnostic("search_exhausted", None, incomplete));
            blocker.incomplete |= incomplete;
            search.failure_traces.attach(&mut blocker);
            return Err(blocker.into());
        }
    };
    let actions = super::actions::build_actions(
        inventory,
        &solution.inventory,
        &solution.reasons,
        &solution.projected_loose_providers,
    );
    let mut impacts = solution
        .inventory
        .nodes
        .iter()
        .enumerate()
        .filter(|(index, _)| {
            !solution.projected_loose_providers.contains(index)
                && changed(inventory, &solution.inventory, *index)
        })
        .map(|(_, node)| node.id.clone())
        .collect::<Vec<_>>();
    impacts.extend(
        state
            .inventory
            .nodes
            .iter()
            .enumerate()
            .filter(|(index, _)| search.roots.contains(index))
            .map(|(_, node)| node.id.clone()),
    );
    impacts.sort();
    impacts.dedup();
    let evaluations = evaluate(&solution.inventory);
    let notices = search.unconfirmed_notices(&solution, &evaluations);
    search.trace.solution(&search, &solution, &evaluations);
    Ok((
        SolveResult {
            status: SolveStatus::Ready,
            actions,
            evaluations,
            blockers: Vec::new(),
            notices,
            impacts,
        },
        solution.inventory,
    ))
}
