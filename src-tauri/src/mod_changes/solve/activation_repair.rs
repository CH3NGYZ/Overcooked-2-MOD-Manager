//! Keep a disabled repair target fixed even when its new dependencies need it back.
use super::*;

impl Search<'_> {
    pub(super) fn note_disabled_repair_provider(
        &mut self,
        state: &SearchState,
        issue: &RelationEvaluation,
    ) {
        if self.probing_activation
            || self.activation_probe_needed
            || issue.relation.kind != RelationKind::Requires
            || issue.state != EvaluationState::Disabled
        {
            return;
        }
        self.activation_probe_needed = self.repair_owner.is_some_and(|index| {
            let node = &state.inventory.nodes[index];
            node.kind == NodeKind::Mod
                && !node.enabled
                && node.mutable
                && node.current.iter().flat_map(|c| &c.provides).any(|p| {
                    same_guid(&p.guid, &issue.relation.guid)
                        && matches(&issue.relation, p) == Some(true)
                })
        });
    }

    pub(super) fn try_activation_repair(
        &mut self,
        result: Result<SearchState, bool>,
        state: &SearchState,
    ) -> Result<SearchState, bool> {
        let Some(root) = self.repair_owner else {
            return result;
        };
        if result.is_ok() || !self.activation_probe_needed {
            return result;
        }
        // Use the same search and checks as an explicit package switch. This
        // branch alone may assume the target is active; it cannot be committed.
        let mut probe = state.clone();
        let node = &mut probe.inventory.nodes[root];
        node.enabled = true;
        for member in &mut node.current.as_mut().unwrap().provides {
            member.enabled = true;
        }
        let failure = self.failure.take();
        let traces = std::mem::take(&mut self.failure_traces);
        self.probing_activation = true;
        let projected = self.find_active_solution(&probe);
        self.probing_activation = false;
        match projected {
            Ok(solution) => Ok(solution),
            Err(incomplete) => {
                self.failure = failure;
                self.failure_traces = traces;
                result.map_err(|prior| prior || incomplete)
            }
        }
    }

    pub(super) fn finish_activation_repair(
        &self,
        mut state: SearchState,
    ) -> Result<SearchState, bool> {
        if !self.probing_activation {
            return Ok(state);
        }
        let root = self.repair_owner.unwrap();
        let closure = self.protected(&state);
        let mut deferred = BTreeSet::from([root]);
        loop {
            let before = deferred.len();
            for &index in &closure {
                if scoped_relations(&state.inventory.nodes[index], false).any(|owned| {
                    required_provider(&state.inventory, &owned.relation)
                        .is_some_and(|provider| deferred.contains(&provider))
                }) {
                    deferred.insert(index);
                }
            }
            if deferred.len() == before {
                break;
            }
        }
        if deferred.len() == 1 {
            // A self-declaration alone cannot justify a readiness-only repair.
            return Err(false);
        }

        state.inventory.nodes[root] = self.original.nodes[root].clone();
        deferred.remove(&root);
        for &index in &deferred {
            let before = &self.original.nodes[index];
            let node = &mut state.inventory.nodes[index];
            let same_version =
                before.current.as_ref().map(|c| &c.id) == node.current.as_ref().map(|c| &c.id);
            if node.kind != NodeKind::Mod
                || self.replacement_only.contains(&index)
                || !node.mutable
                || (same_version && before.enabled)
            {
                return Err(false);
            }
            if same_version {
                *node = before.clone();
            } else {
                node.enabled = false;
                for member in &mut node.current.as_mut().unwrap().provides {
                    member.enabled = false;
                }
            }
        }
        // Re-check the real runtime. A live map or MOD outside the cycle must
        // not lose an active provider as a side effect of preparing the update.
        let changed_guids = self.changed_guids(&state);
        let protected = self.protected(&state);
        let evaluations = evaluate(&state.inventory);
        if evaluations.iter().enumerate().any(|(index, evaluation)| {
            evaluation.runtime.iter().any(|issue| {
                issue.state.is_issue()
                    && self.relevant(&state, index, &issue.relation, &protected, &changed_guids)
            })
        }) {
            return Err(false);
        }
        Ok(state)
    }
}
