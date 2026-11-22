//! Depth-first search preserves local uncertainty before accepting disable fallbacks.
use super::super::evaluate::{DUPLICATE_ENABLED_PROVIDERS, scoped_evaluations};
use super::*;

pub(super) enum SearchFailure {
    Blocked(bool),
    Ownership(String),
}

type Branch = Result<SearchState, SearchFailure>;

impl Search<'_, '_> {
    pub(super) fn find_solution(&mut self, state: &SearchState) -> Branch {
        let mut limit = 0;
        while !self.policy.prefers_newest() && self.preference_remaining > 0 {
            let result = self.run_pass(SearchPass::LimitReplacements(limit), state.clone());
            if !matches!(&result, Err(SearchFailure::Blocked(_))) {
                return result;
            }
            if self.preference_remaining == 0 {
                break;
            }
            if !self.preference_pruned {
                // This pass visited every branch of the unbounded search.
                return self.try_dependency_disable(result, state);
            }
            limit += 1;
        }
        // A preference limit cannot prove a graph unsatisfiable. Fall back to
        // the complete compatibility search with its original admission budget.
        let result = self.run_pass(SearchPass::CompatibleVersions, state.clone());
        self.try_dependency_disable(result, state)
    }

    fn try_dependency_disable(&mut self, result: Branch, state: &SearchState) -> Branch {
        // Unknown declarations, incomplete history and search limits block this.
        if matches!(&result, Err(SearchFailure::Blocked(false))) {
            self.run_pass(SearchPass::AllowDependencyDisable, state.clone())
        } else {
            result
        }
    }

    fn run_pass(&mut self, pass: SearchPass, state: SearchState) -> Branch {
        self.pass = pass;
        self.preference_pruned = false;
        self.visited.clear();
        self.failure = None;
        self.failure_traces = Default::default();
        let result = self.visit(state);
        self.trace.pass(pass, self.visited.len(), &result);
        result
    }

    fn visit(&mut self, mut state: SearchState) -> Branch {
        let preference = matches!(self.pass, SearchPass::LimitReplacements(_));
        if preference && self.preference_remaining == 0 {
            return Err(SearchFailure::Blocked(true));
        }
        if let Some(releases) = self.loose_releases.as_deref() {
            releases.project(
                &mut state.inventory,
                self.original,
                &mut state.projected_loose_providers,
            );
        }
        let key = serde_json::to_string(&(
            &state.inventory,
            &state.decided,
            &state.disabled_guids,
            &state.fallback_disabled_guids,
        ))
        .map_err(|_| SearchFailure::Blocked(true))?;
        if let Some(incomplete) = self.visited.get(&key) {
            return Err(SearchFailure::Blocked(*incomplete));
        }
        if self.visited.len() >= MAX_SEARCH_STATES {
            self.failure_traces.mark_truncated();
            return Err(SearchFailure::Blocked(true));
        }
        if preference {
            self.preference_remaining -= 1;
        }
        // A pending state closes cycles. Completed failures retain uncertainty so
        // reaching them through another branch cannot unlock a disable fallback.
        self.visited.insert(key.clone(), false);
        let result = self.visit_new(state);
        if let Err(SearchFailure::Blocked(incomplete)) = &result {
            self.visited.insert(key, *incomplete);
        }
        result
    }

    fn visit_new(&mut self, state: SearchState) -> Branch {
        let protected = self.protected(&state);
        let changed_guids = self.changed_guids(&state);
        let evaluations = evaluate(&state.inventory);
        for (index, node) in state.inventory.nodes.iter().enumerate() {
            if (node.enabled || self.readiness(index, node))
                && (protected.contains(&index) || changed(self.original, &state.inventory, index))
                && node
                    .current
                    .as_ref()
                    .is_some_and(|candidate| !candidate.relations_known)
                && !(protected.contains(&index) && self.accepts_partial_declarations(&state, index))
            {
                self.failure_traces.record(
                    &state.inventory,
                    &evaluations,
                    &self.roots,
                    self.repair_owner,
                    index,
                    None,
                    Some("candidate_relations_unknown"),
                );
                self.mark_failure(node, None, true, "candidate_relations_unknown");
                return Err(SearchFailure::Blocked(true));
            }
        }
        let mut issues = Vec::new();
        for (index, evaluation) in evaluations.iter().enumerate() {
            let node = &state.inventory.nodes[index];
            for (owned, issue) in scoped_evaluations(node, evaluation, self.readiness(index, node))
            {
                if issue.state.is_issue()
                    && self.relevant(&state, index, &issue.relation, &protected, &changed_guids)
                    && !(protected.contains(&index)
                        && matches!(
                            issue.state,
                            EvaluationState::Unknown | EvaluationState::Disabled
                        )
                        && self.unconfirmed_provider(&state, &issue.relation).is_some())
                {
                    issues.push((index, owned, issue));
                }
            }
        }
        if issues.is_empty() && !self.policy.prefers_newest() {
            return self.complete(state);
        }
        let distances = self.order(&state);
        issues.sort_by_key(|(index, _, issue)| {
            (
                self.node_order(&state, &distances, *index),
                issue.relation.guid.to_lowercase(),
            )
        });
        let next_issue = issues.first().map(|first| {
            // Resolve a known conflict that can remove the ambiguous provider,
            // then let the next visit check the requirement against the survivor.
            if first.2.reason == DUPLICATE_ENABLED_PROVIDERS {
                issues
                    .iter()
                    .find(|(_, _, issue)| {
                        issue.state == EvaluationState::Conflict
                            && issue.relation.kind == RelationKind::Conflicts
                            && same_guid(&issue.relation.guid, &first.2.relation.guid)
                    })
                    .unwrap_or(first)
            } else {
                first
            }
        });
        if let Some((owner, owned, issue)) = next_issue {
            self.trace.issue(&state.inventory.nodes[*owner], issue);
            let missing_history = state.inventory.nodes.iter().any(|node| {
                !node.candidates_complete
                    && (node
                        .current
                        .iter()
                        .chain(&node.candidates)
                        .any(|candidate| provides(candidate, &issue.relation.guid))
                        || node.id == state.inventory.nodes[*owner].id)
            });
            let target_unknown = self.catalog_unknown
                && !state.inventory.nodes.iter().any(|node| {
                    node.current
                        .iter()
                        .chain(&node.candidates)
                        .any(|candidate| provides(candidate, &issue.relation.guid))
                });
            let metadata_unknown = state.inventory.nodes[*owner]
                .current
                .as_ref()
                .is_some_and(|candidate| !candidate.relations_known);
            let mut incomplete = issue.state == EvaluationState::Unknown
                || missing_history
                || target_unknown
                || metadata_unknown;
            if metadata_unknown
                && !(protected.contains(owner) && self.accepts_partial_declarations(&state, *owner))
            {
                self.failure_traces.record(
                    &state.inventory,
                    &evaluations,
                    &self.roots,
                    self.repair_owner,
                    *owner,
                    Some((owned, issue)),
                    Some("candidate_relations_unknown"),
                );
                self.mark_failure(
                    &state.inventory.nodes[*owner],
                    Some(&issue.relation),
                    true,
                    "candidate_relations_unknown",
                );
                return Err(SearchFailure::Blocked(true));
            }
            let (branches, fallback) = self.repair(&state, *owner, issue, &protected);
            let mut tried_repair = !branches.is_empty();
            // Missing history prevents discarding an owner, but a known candidate
            // can still produce a completely verified solution in either pass.
            for branch in branches {
                match self.visit(branch) {
                    Err(SearchFailure::Blocked(unknown)) => incomplete |= unknown,
                    result => return result,
                }
            }
            if !incomplete {
                if let Some(fallback) = fallback {
                    tried_repair = true;
                    match self.visit(fallback) {
                        Err(SearchFailure::Blocked(unknown)) => incomplete |= unknown,
                        result => return result,
                    }
                }
            }
            // A parent mismatch was only the reason to try its children. Keep
            // those children's real failures, not their pre-repair ancestor.
            if (!tried_repair && !self.preference_pruned) || issue.state == EvaluationState::Unknown
            {
                self.failure_traces.record(
                    &state.inventory,
                    &evaluations,
                    &self.roots,
                    self.repair_owner,
                    *owner,
                    Some((owned, issue)),
                    (issue.state == EvaluationState::Missing
                        && (missing_history || target_unknown))
                        .then_some("relation_metadata_unknown"),
                );
            }
            self.mark_failure(
                &state.inventory.nodes[*owner],
                Some(&issue.relation),
                incomplete,
                "relation_has_no_solution",
            );
            return Err(SearchFailure::Blocked(incomplete));
        }
        let mut pending = protected
            .iter()
            .copied()
            .filter(|index| !state.decided.contains(index))
            .collect::<Vec<_>>();
        pending.sort_by_key(|index| self.node_order(&state, &distances, *index));
        if let Some(index) = pending.first().copied() {
            let node = &state.inventory.nodes[index];
            if node.kind != NodeKind::Mod || !node.mutable || self.replacement_only.contains(&index)
            {
                let mut next = state;
                next.decided.insert(index);
                return self.visit(next);
            }
            let mut incomplete = false;
            let reason = reasons::compatible_upgrade(&state.inventory, index, &protected);
            for candidate in ordered_candidates(self.trusted, node, false) {
                if node
                    .current
                    .as_ref()
                    .is_some_and(|current| !preserves_package_identity(current, &candidate))
                {
                    continue;
                }
                self.trace
                    .candidate(node, &candidate, None, "try_compatible_upgrade");
                if let Some(next) =
                    self.with_candidate(&state, index, candidate, None, reason.as_ref())
                {
                    match self.visit(next) {
                        Err(SearchFailure::Blocked(unknown)) => incomplete |= unknown,
                        result => return result,
                    }
                }
            }
            return Err(SearchFailure::Blocked(incomplete));
        }
        self.complete(state)
    }

    fn complete(&mut self, state: SearchState) -> Branch {
        if !state.projected_loose_providers.is_empty()
            && let Some(releases) = self.loose_releases.as_deref_mut()
            && !releases
                .accept(&state.inventory)
                .map_err(SearchFailure::Ownership)?
        {
            // Try sibling versions, but a file conflict cannot justify disabling
            // an installed dependent to make this branch appear compatible.
            return Err(SearchFailure::Blocked(true));
        }
        Ok(state)
    }
}
