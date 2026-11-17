use super::evaluate::{evaluate, matches, owned_targets, relation_active, same_guid, targets};
use super::model::*;
use crate::models::{RelationKind, RepositoryRelation};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

const MAX_SEARCH_STATES: usize = 30_000;

#[derive(Clone)]
struct SearchState {
    inventory: Inventory,
    decided: BTreeSet<usize>,
    /// Only explicit/conflict disablement may initiate a reverse-disable cascade.
    disabled_guids: BTreeSet<String>,
    causes: BTreeMap<usize, BTreeSet<String>>,
}

struct Search<'a> {
    original: &'a Inventory,
    fixed: BTreeSet<usize>,
    roots: BTreeSet<usize>,
    visited: BTreeSet<String>,
    baseline: BTreeSet<String>,
    incomplete: bool,
    failure: Option<Diagnostic>,
    catalog_unknown: bool,
    fixed_members: BTreeSet<(usize, String)>,
}

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

fn preserves_members(current: &Candidate, candidate: &Candidate) -> bool {
    candidate.verification_pending
        || current
            .provides
            .iter()
            .filter(|plugin| !plugin.guid.trim().is_empty())
            .all(|plugin| provides(candidate, &plugin.guid))
}

fn ordered_candidates(node: &ChangeNode) -> Vec<Candidate> {
    let mut candidates = node
        .candidates
        .iter()
        .filter(|candidate| candidate.installable && !candidate.raw)
        .cloned()
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
        .then_with(|| {
            let current = node.current.as_ref().map(|candidate| &candidate.id);
            (Some(&right.id) == current).cmp(&(Some(&left.id) == current))
        })
        .then(left.source_id.cmp(&right.source_id))
        .then(left.id.cmp(&right.id))
    });
    candidates.dedup_by(|left, right| left.id == right.id);
    candidates
}

impl Search<'_> {
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
            .filter(|index| state.inventory.nodes[*index].enabled)
            .collect::<BTreeSet<_>>();
        loop {
            let previous = protected.len();
            for index in protected.clone() {
                let node = &state.inventory.nodes[index];
                let Some(candidate) = &node.current else {
                    continue;
                };
                for owned in &candidate.relations {
                    if owned.relation.kind != RelationKind::Requires
                        || owned.relation.optional
                        || !relation_active(candidate, owned, false)
                    {
                        continue;
                    }
                    if let Some((target, _)) = targets(&state.inventory, &owned.relation.guid, true)
                        .into_iter()
                        .find(|(_, plugin)| matches(&owned.relation, plugin) == Some(true))
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
        self.incomplete |= incomplete;
        self.failure = Some(Diagnostic {
            code: code.into(),
            owner_id: Some(owner.id.clone()),
            target_guid: relation.map(|relation| relation.guid.clone()),
            message: code.into(),
            incomplete,
        });
    }

    fn with_candidate(
        &self,
        state: &SearchState,
        index: usize,
        mut candidate: Candidate,
        relation: Option<&RepositoryRelation>,
        cause: usize,
    ) -> SearchState {
        if let Some(current) = &state.inventory.nodes[index].current {
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
        if let Some(relation) = relation {
            for plugin in &mut candidate.provides {
                if same_guid(&plugin.guid, &relation.guid)
                    && matches(relation, plugin) == Some(true)
                {
                    plugin.enabled = true;
                }
            }
        }
        let mut next = state.clone();
        next.inventory.nodes[index].current = Some(candidate);
        next.inventory.nodes[index].enabled = true;
        next.decided.insert(index);
        next.causes
            .entry(index)
            .or_default()
            .insert(state.inventory.nodes[cause].id.clone());
        next
    }

    fn replacements(&self, state: &SearchState, index: usize, cause: usize) -> Vec<SearchState> {
        let node = &state.inventory.nodes[index];
        if self.fixed.contains(&index)
            || state.decided.contains(&index)
            || !node.mutable
            || node.kind == NodeKind::Map
        {
            return Vec::new();
        }
        ordered_candidates(node)
            .into_iter()
            .filter(|candidate| {
                node.current.as_ref().is_none_or(|current| {
                    candidate.id != current.id && preserves_members(current, candidate)
                })
            })
            .map(|candidate| self.with_candidate(state, index, candidate, None, cause))
            .collect()
    }

    fn disable(
        &self,
        state: &SearchState,
        index: usize,
        guid: Option<&str>,
        cause: usize,
        protected: &BTreeSet<usize>,
    ) -> Option<SearchState> {
        let node = &state.inventory.nodes[index];
        if self.fixed.contains(&index) || !node.mutable {
            return None;
        }
        let mut next = state.clone();
        let target = &mut next.inventory.nodes[index];
        let candidate = target.current.as_mut()?;
        let mut switched = false;
        for plugin in &mut candidate.provides {
            if plugin.enabled && guid.is_none_or(|guid| same_guid(&plugin.guid, guid)) {
                next.disabled_guids
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
                let Some(candidate) = &owner.current else {
                    continue;
                };
                for owned in &candidate.relations {
                    if owned.relation.kind != RelationKind::Requires
                        || owned.relation.optional
                        || !relation_active(candidate, owned, false)
                    {
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
        next.causes
            .entry(index)
            .or_default()
            .insert(state.inventory.nodes[cause].id.clone());
        Some(next)
    }

    fn repair(
        &mut self,
        state: &SearchState,
        owner: usize,
        issue: &RelationEvaluation,
        protected: &BTreeSet<usize>,
    ) -> Vec<SearchState> {
        let relation = &issue.relation;
        let mut branches = Vec::new();
        if matches!(
            relation.kind,
            RelationKind::Conflicts | RelationKind::Compatible
        ) {
            let owner_node = &state.inventory.nodes[owner];
            let active_targets = owner_node
                .current
                .iter()
                .flat_map(|candidate| {
                    candidate.relations.iter().filter(move |owned| {
                        owned.relation == *relation && relation_active(candidate, owned, false)
                    })
                })
                .flat_map(|owned| owned_targets(&state.inventory, owner_node, owned, true));
            for (index, plugin) in active_targets {
                let incompatible = match relation.kind {
                    RelationKind::Conflicts => matches(relation, plugin) == Some(true),
                    RelationKind::Compatible => matches(relation, plugin) == Some(false),
                    _ => false,
                };
                if incompatible {
                    if let Some(next) =
                        self.disable(state, index, Some(&relation.guid), owner, protected)
                    {
                        branches.push(next);
                    }
                }
            }
            // A fixed target can instead force its conflicting, nonrequired owner off.
            if let Some(next) = self.disable(state, owner, None, owner, protected) {
                branches.push(next)
            }
        }
        if relation.kind == RelationKind::Requires
            && state
                .disabled_guids
                .contains(&relation.guid.trim().to_lowercase())
        {
            if let Some(next) = self.disable(state, owner, None, owner, protected) {
                branches.push(next)
            }
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
            branches.extend(self.replacements(state, owner, owner))
        }
        let mut indices = (0..state.inventory.nodes.len()).collect::<Vec<_>>();
        let distances = self.order(state);
        indices.sort_by_key(|index| self.node_order(state, &distances, *index));
        for index in indices {
            let node = &state.inventory.nodes[index];
            // Deciding a package fixes its version, not every sibling's switch.
            // Later requirements may still enable members of that exact candidate.
            if (self.fixed.contains(&index) || (state.decided.contains(&index) && node.mutable))
                && relation.kind == RelationKind::Requires
                && node.enabled
            {
                if let Some(current) = &node.current {
                    let mut next = state.clone();
                    let actual = next.inventory.nodes[index].current.as_mut().unwrap();
                    let mut switched = false;
                    for (member_index, member) in current.provides.iter().enumerate() {
                        if !member.enabled
                            && same_guid(&member.guid, &relation.guid)
                            && matches(relation, member) == Some(true)
                            && member.member_key.as_ref().is_none_or(|key| {
                                !self.fixed_members.contains(&(index, key.clone()))
                            })
                        {
                            actual.provides[member_index].enabled = true;
                            switched = true;
                        }
                    }
                    if switched {
                        next.causes
                            .entry(index)
                            .or_default()
                            .insert(state.inventory.nodes[owner].id.clone());
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
            for candidate in ordered_candidates(node) {
                if node
                    .current
                    .as_ref()
                    .is_some_and(|current| !preserves_members(current, &candidate))
                {
                    continue;
                }
                if !candidate.provides.iter().any(|plugin| {
                    same_guid(&plugin.guid, &relation.guid)
                        && match relation.kind {
                            RelationKind::Conflicts => matches(relation, plugin) == Some(false),
                            _ => matches(relation, plugin) == Some(true),
                        }
                }) {
                    continue;
                }
                branches.push(self.with_candidate(
                    state,
                    index,
                    candidate,
                    (relation.kind == RelationKind::Requires).then_some(relation),
                    owner,
                ));
            }
        }
        if !fixed_target {
            branches.extend(self.replacements(state, owner, owner))
        }
        branches
    }

    fn visit(&mut self, state: SearchState) -> Option<SearchState> {
        if self.visited.len() >= MAX_SEARCH_STATES {
            self.incomplete = true;
            return None;
        }
        let key = serde_json::to_string(&(&state.inventory, &state.decided, &state.disabled_guids))
            .ok()?;
        if !self.visited.insert(key) {
            return None;
        }
        let protected = self.protected(&state);
        let changed_guids = self.changed_guids(&state);
        let evaluations = evaluate(&state.inventory);
        // Roots and their required closure cannot be accepted with unknown declarations.
        for (index, node) in state.inventory.nodes.iter().enumerate() {
            if node.enabled
                && (protected.contains(&index) || changed(self.original, &state.inventory, index))
                && node
                    .current
                    .as_ref()
                    .is_some_and(|candidate| !candidate.relations_known)
            {
                self.mark_failure(node, None, true, "candidate_relations_unknown");
                return None;
            }
        }
        let mut issues = Vec::new();
        for (index, evaluation) in evaluations.iter().enumerate() {
            for issue in &evaluation.runtime {
                if issue.state.is_issue()
                    && self.relevant(&state, index, &issue.relation, &protected, &changed_guids)
                {
                    issues.push((index, issue.clone()));
                }
            }
        }
        let distances = self.order(&state);
        issues.sort_by_key(|(index, issue)| {
            (
                self.node_order(&state, &distances, *index),
                issue.relation.guid.to_lowercase(),
            )
        });
        if let Some((owner, issue)) = issues.first() {
            let branches = self.repair(&state, *owner, issue, &protected);
            for branch in branches {
                if let Some(solution) = self.visit(branch) {
                    return Some(solution);
                }
            }
            let missing_data = issue.state == EvaluationState::Unknown
                || state.inventory.nodes.iter().any(|node| {
                    !node.candidates_complete
                        && (node
                            .current
                            .as_ref()
                            .is_some_and(|candidate| provides(candidate, &issue.relation.guid))
                            || node
                                .candidates
                                .iter()
                                .any(|candidate| provides(candidate, &issue.relation.guid))
                            || node.id == state.inventory.nodes[*owner].id)
                });
            self.mark_failure(
                &state.inventory.nodes[*owner],
                Some(&issue.relation),
                missing_data || self.catalog_unknown,
                "relation_has_no_solution",
            );
            return None;
        }
        // Select the newest compatible version for each required node, then backtrack
        // over its entire downstream closure. Cycles terminate through decided nodes.
        let mut pending = protected
            .iter()
            .copied()
            .filter(|index| !state.decided.contains(index))
            .collect::<Vec<_>>();
        pending.sort_by_key(|index| self.node_order(&state, &distances, *index));
        if let Some(index) = pending.first().copied() {
            let node = &state.inventory.nodes[index];
            if node.kind == NodeKind::Mod && node.mutable {
                for candidate in ordered_candidates(node) {
                    if node
                        .current
                        .as_ref()
                        .is_some_and(|current| !preserves_members(current, &candidate))
                    {
                        continue;
                    }
                    let next = self.with_candidate(&state, index, candidate, None, index);
                    if let Some(solution) = self.visit(next) {
                        return Some(solution);
                    }
                }
            } else {
                let mut next = state;
                next.decided.insert(index);
                return self.visit(next);
            }
            return None;
        }
        Some(state)
    }
}

fn diagnostic(code: &str, owner: Option<&str>, incomplete: bool) -> Diagnostic {
    Diagnostic {
        code: code.into(),
        owner_id: owner.map(str::to_string),
        target_guid: None,
        message: code.into(),
        incomplete,
    }
}

/// Pure planner; adapters own snapshots, repository policy and actual-asset verification.
#[cfg(test)]
pub(crate) fn solve(inventory: &Inventory, request: &ChangeRequest) -> SolveResult {
    solve_with_inventory(inventory, request).0
}

/// Execution adapters consume the exact solved package/member states, never replay
/// presentation actions to reconstruct a subtly different installed inventory.
pub(crate) fn solve_with_inventory(
    inventory: &Inventory,
    request: &ChangeRequest,
) -> (SolveResult, Option<Inventory>) {
    let failure = |blocker: Diagnostic| SolveResult {
        status: if blocker.incomplete {
            SolveStatus::Incomplete
        } else {
            SolveStatus::Unsatisfiable
        },
        actions: Vec::new(),
        evaluations: evaluate(inventory),
        blockers: vec![blocker],
        impacts: Vec::new(),
    };
    match solve_inner(inventory, request) {
        Ok((result, solved)) => (result, Some(solved)),
        Err(blocker) => (failure(blocker), None),
    }
}

fn solve_inner(
    inventory: &Inventory,
    request: &ChangeRequest,
) -> Result<(SolveResult, Inventory), Diagnostic> {
    let mut state = SearchState {
        inventory: inventory.clone(),
        decided: BTreeSet::new(),
        disabled_guids: BTreeSet::new(),
        causes: BTreeMap::new(),
    };
    let mut roots = BTreeSet::new();
    let mut seen_roots = BTreeMap::new();
    for root in &request.roots {
        let Some(index) = state
            .inventory
            .nodes
            .iter()
            .position(|node| node.id == root.node_id)
        else {
            return Err(diagnostic("root_missing", Some(&root.node_id), true));
        };
        let root_key = (&root.node_id, &root.member_key);
        if seen_roots
            .insert(root_key, root)
            .is_some_and(|previous| previous != root)
        {
            return Err(diagnostic(
                "contradictory_roots",
                Some(&root.node_id),
                false,
            ));
        }
        let node = &mut state.inventory.nodes[index];
        if !node.mutable
            && (root.candidate_id.is_some()
                || node.enabled != root.enabled
                || root.member_key.is_some())
        {
            return Err(diagnostic("root_ownership_unsafe", Some(&node.id), true));
        }
        if let Some(id) = &root.candidate_id {
            let Some(candidate) = node
                .current
                .iter()
                .chain(node.candidates.iter())
                .find(|candidate| &candidate.id == id)
                .cloned()
            else {
                return Err(diagnostic("root_candidate_missing", Some(&node.id), true));
            };
            if node
                .current
                .as_ref()
                .is_none_or(|current| current.id != candidate.id)
                && !candidate.installable
            {
                return Err(diagnostic(
                    "root_candidate_not_installable",
                    Some(&node.id),
                    true,
                ));
            }
            node.current = Some(candidate);
        }
        let Some(candidate) = node.current.as_mut() else {
            return Err(diagnostic("root_candidate_missing", Some(&node.id), true));
        };
        if let Some(member) = &root.member_key {
            let Some(plugin) = candidate
                .provides
                .iter_mut()
                .find(|plugin| plugin.member_key.as_ref() == Some(member))
            else {
                return Err(diagnostic("root_member_missing", Some(&node.id), true));
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
            return Err(diagnostic("contradictory_roots", Some(&node.id), false));
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
        fixed: roots.clone(),
        roots,
        fixed_members,
        visited: BTreeSet::new(),
        baseline,
        incomplete: false,
        failure: None,
        catalog_unknown: inventory.nodes.iter().any(|node| !node.candidates_complete),
    };
    let initial = state.clone();
    let Some(solution) = search.visit(state) else {
        let mut blocker = search
            .failure
            .unwrap_or_else(|| diagnostic("search_exhausted", None, search.incomplete));
        blocker.incomplete |= search.incomplete;
        return Err(blocker);
    };
    let actions = super::actions::build_actions(inventory, &solution.inventory, &solution.causes);
    let mut impacts = solution
        .inventory
        .nodes
        .iter()
        .enumerate()
        .filter(|(index, _)| changed(inventory, &solution.inventory, *index))
        .map(|(_, node)| node.id.clone())
        .collect::<Vec<_>>();
    impacts.extend(
        initial
            .inventory
            .nodes
            .iter()
            .enumerate()
            .filter(|(index, _)| search.roots.contains(index))
            .map(|(_, node)| node.id.clone()),
    );
    impacts.sort();
    impacts.dedup();
    Ok((
        SolveResult {
            status: SolveStatus::Ready,
            actions,
            evaluations: evaluate(&solution.inventory),
            blockers: Vec::new(),
            impacts,
        },
        solution.inventory,
    ))
}
