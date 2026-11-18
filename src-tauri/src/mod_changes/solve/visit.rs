//! Depth-first search preserves local uncertainty before accepting disable fallbacks.
use super::*;

impl Search<'_> {
    pub(super) fn visit(&mut self, state: SearchState) -> Result<SearchState, bool> {
        let key = serde_json::to_string(&(
            &state.inventory,
            &state.decided,
            &state.disabled_guids,
            &state.fallback_disabled_guids,
        ))
        .map_err(|_| true)?;
        if let Some(incomplete) = self.visited.get(&key) {
            return Err(*incomplete);
        }
        if self.visited.len() >= MAX_SEARCH_STATES {
            return Err(true);
        }
        // A pending state closes cycles. Completed failures retain uncertainty so
        // reaching them through another branch cannot unlock a disable fallback.
        self.visited.insert(key.clone(), false);
        let result = self.visit_new(state);
        if let Err(incomplete) = &result {
            self.visited.insert(key, *incomplete);
        }
        result
    }

    fn visit_new(&mut self, state: SearchState) -> Result<SearchState, bool> {
        let protected = self.protected(&state);
        let changed_guids = self.changed_guids(&state);
        let evaluations = evaluate(&state.inventory);
        for (index, node) in state.inventory.nodes.iter().enumerate() {
            if node.enabled
                && (protected.contains(&index) || changed(self.original, &state.inventory, index))
                && node
                    .current
                    .as_ref()
                    .is_some_and(|candidate| !candidate.relations_known)
            {
                self.mark_failure(node, None, true, "candidate_relations_unknown");
                return Err(true);
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
            let mut incomplete =
                issue.state == EvaluationState::Unknown || missing_history || target_unknown;
            if metadata_unknown {
                self.mark_failure(
                    &state.inventory.nodes[*owner],
                    Some(&issue.relation),
                    true,
                    "candidate_relations_unknown",
                );
                return Err(true);
            }
            let (branches, fallback) = self.repair(&state, *owner, issue, &protected);
            // Missing history prevents discarding an owner, but a known candidate
            // can still produce a completely verified solution in either pass.
            for branch in branches {
                match self.visit(branch) {
                    Ok(solution) => return Ok(solution),
                    Err(unknown) => incomplete |= unknown,
                }
            }
            if !incomplete {
                if let Some(fallback) = fallback {
                    match self.visit(fallback) {
                        Ok(solution) => return Ok(solution),
                        Err(unknown) => incomplete |= unknown,
                    }
                }
            }
            self.mark_failure(
                &state.inventory.nodes[*owner],
                Some(&issue.relation),
                incomplete,
                "relation_has_no_solution",
            );
            return Err(incomplete);
        }
        let mut pending = protected
            .iter()
            .copied()
            .filter(|index| !state.decided.contains(index))
            .collect::<Vec<_>>();
        pending.sort_by_key(|index| self.node_order(&state, &distances, *index));
        if let Some(index) = pending.first().copied() {
            let node = &state.inventory.nodes[index];
            if node.kind != NodeKind::Mod || !node.mutable {
                let mut next = state;
                next.decided.insert(index);
                return self.visit(next);
            }
            let mut incomplete = false;
            for candidate in ordered_candidates(node) {
                if node
                    .current
                    .as_ref()
                    .is_some_and(|current| !preserves_members(current, &candidate))
                {
                    continue;
                }
                let next = self.with_candidate(&state, index, candidate, None, index);
                match self.visit(next) {
                    Ok(solution) => return Ok(solution),
                    Err(unknown) => incomplete |= unknown,
                }
            }
            return Err(incomplete);
        }
        Ok(state)
    }
}
