//! Failure witnesses are projected from evaluated branch states. They never
//! participate in candidate selection, memoization, or transaction approval.
use super::super::evaluate::runtime_relations;
use super::*;
use std::collections::VecDeque;

const MAX_TRACES: usize = 8;
const MAX_STEPS: usize = 24;
const MAX_TARGETS: usize = 12;
const MAX_WITNESSES_INSPECTED: usize = 128;

#[derive(Default)]
pub(super) struct FailureTraces {
    traces: Vec<Vec<DiagnosticStep>>,
    truncated: bool,
    inspected: usize,
}

impl FailureTraces {
    pub(super) fn mark_truncated(&mut self) {
        self.truncated = true;
    }

    pub(super) fn record(
        &mut self,
        inventory: &Inventory,
        evaluations: &[NodeEvaluation],
        roots: &BTreeSet<usize>,
        owner: usize,
        edge: Option<(&OwnedRelation, &RelationEvaluation)>,
        unknown_reason: Option<&str>,
    ) {
        if self.inspected >= MAX_WITNESSES_INSPECTED {
            self.mark_truncated();
            return;
        }
        self.inspected += 1;
        let mut trace = self.path_to_owner(inventory, evaluations, roots, owner);
        let mut terminal = self.step(inventory, owner, edge, false);
        if let Some(reason) = unknown_reason {
            terminal.state = EvaluationState::Unknown;
            terminal.reason = reason.into();
        }
        trace.push(terminal);
        if self.traces.contains(&trace) {
            return;
        }
        if self.traces.len() < MAX_TRACES {
            self.traces.push(trace);
            return;
        }
        self.mark_truncated();
        // Prefer explanations that can show more of the affected chain.
        if let Some((index, _)) = self
            .traces
            .iter()
            .enumerate()
            .rev()
            .min_by_key(|(_, retained)| retained.len())
            .filter(|(_, retained)| retained.len() < trace.len())
        {
            self.traces[index] = trace;
        }
    }

    fn path_to_owner(
        &mut self,
        inventory: &Inventory,
        evaluations: &[NodeEvaluation],
        roots: &BTreeSet<usize>,
        owner: usize,
    ) -> Vec<DiagnosticStep> {
        if roots.contains(&owner) {
            return Vec::new();
        }
        let mut queue = VecDeque::new();
        let mut seen = vec![false; inventory.nodes.len()];
        let mut parents = vec![None; inventory.nodes.len()];
        for &root in roots {
            seen[root] = true;
            if inventory.nodes[root].enabled {
                queue.push_back(root);
            }
        }
        'walk: while let Some(index) = queue.pop_front() {
            for (owned, edge) in
                runtime_relations(&inventory.nodes[index]).zip(&evaluations[index].runtime)
            {
                if edge.relation.kind != RelationKind::Requires
                    || edge.state != EvaluationState::Satisfied
                {
                    continue;
                }
                // The evaluator already proved there is one matching active provider.
                if let [(target, _)] =
                    owned_targets(inventory, &inventory.nodes[index], owned, true).as_slice()
                    && !seen[*target]
                {
                    seen[*target] = true;
                    parents[*target] = Some((index, owned, edge));
                    if *target == owner {
                        break 'walk;
                    }
                    queue.push_back(*target);
                }
            }
        }
        // Reverse constraints may originate outside the requested dependencies.
        // In that case the real edge is sufficient, without inventing a link.
        let mut trace = Vec::new();
        let mut current = owner;
        while let Some((parent, owned, edge)) = parents[current] {
            if trace.len() + 1 == MAX_STEPS {
                self.mark_truncated();
                break;
            }
            trace.push(self.step(inventory, parent, Some((owned, edge)), true));
            current = parent;
        }
        trace.reverse();
        trace
    }

    fn step(
        &mut self,
        inventory: &Inventory,
        owner: usize,
        edge: Option<(&OwnedRelation, &RelationEvaluation)>,
        enabled_only: bool,
    ) -> DiagnosticStep {
        let node = &inventory.nodes[owner];
        let mut targets = edge.map_or_else(Vec::new, |(owned, _)| {
            owned_targets(inventory, node, owned, enabled_only)
        });
        // Show active providers first if unusually large duplicate sets are capped.
        targets.sort_by(|(left_index, left), (right_index, right)| {
            let left_node = &inventory.nodes[*left_index];
            let right_node = &inventory.nodes[*right_index];
            (right_node.enabled && right.enabled)
                .cmp(&(left_node.enabled && left.enabled))
                .then_with(|| left_node.id.cmp(&right_node.id))
                .then_with(|| left.version.cmp(&right.version))
        });
        if targets.len() > MAX_TARGETS {
            self.mark_truncated();
        }
        DiagnosticStep {
            owner_id: node.id.clone(),
            owner_version: node
                .current
                .as_ref()
                .and_then(|candidate| candidate.version.clone().or_else(|| candidate.tag.clone())),
            relation: edge.map(|(_, issue)| issue.relation.clone()),
            targets: targets
                .into_iter()
                .take(MAX_TARGETS)
                .map(|(index, plugin)| DiagnosticTarget {
                    node_id: inventory.nodes[index].id.clone(),
                    version: plugin.version.clone(),
                    enabled: inventory.nodes[index].enabled && plugin.enabled,
                })
                .collect(),
            state: edge.map_or(EvaluationState::Unknown, |(_, issue)| issue.state.clone()),
            reason: edge.map_or_else(String::new, |(_, issue)| issue.reason.clone()),
        }
    }

    pub(super) fn attach(self, blocker: &mut Diagnostic) {
        blocker.traces = self.traces;
        blocker.traces_truncated = self.truncated;
    }
}
