use super::{Search, SearchPass, SearchState, changed, scoped_relations, visit::SearchFailure};
use crate::mod_changes::{evaluate::owned_targets, model::*};
use crate::{http_diagnostics::redact_text, models::RepositoryRelation};
use std::{cell::Cell, fmt::Arguments};

const MAX_SEARCH_EVENTS: usize = 160;

pub(super) struct SolveTrace<'a> {
    operation: Option<&'a str>,
    events: Cell<usize>,
}

impl<'a> SolveTrace<'a> {
    pub(super) fn new(operation: Option<&'a str>) -> Self {
        Self {
            operation,
            events: Cell::new(0),
        }
    }

    fn write(&self, event: Arguments<'_>) {
        if let Some(id) = self.operation {
            log::info!("依赖求解 op={id} {}", redact_text(&event.to_string()));
        }
    }

    fn step(&self) -> bool {
        if self.operation.is_none() || !log::log_enabled!(log::Level::Info) {
            return false;
        }
        let count = self.events.get();
        if count == MAX_SEARCH_EVENTS {
            self.write(format_args!(
                "搜索过程已达 {MAX_SEARCH_EVENTS} 条日志上限，仍记录选定方案"
            ));
        }
        self.events.set(count.saturating_add(1));
        count < MAX_SEARCH_EVENTS
    }

    pub(super) fn issue(&self, node: &ChangeNode, issue: &RelationEvaluation) {
        if self.step() {
            self.write(format_args!(
                "待解决 owner={:?} version={:?} relation={:?} targets={:?} state={:?} reason={}",
                node.id,
                node.current
                    .as_ref()
                    .and_then(|selected| selected.version.as_deref()),
                issue.relation,
                issue.targets,
                issue.state,
                issue.reason,
            ));
        }
    }

    pub(super) fn candidate(
        &self,
        node: &ChangeNode,
        candidate: &Candidate,
        relation: Option<&RepositoryRelation>,
        reason: &str,
    ) {
        if self.step() {
            self.write(format_args!(
                "候选 node={:?} id={:?} source={:?} version={:?} verification_pending={} provides={:?} relation={relation:?} reason={reason}",
                node.id, candidate.id, candidate.source_id, candidate.version, candidate.verification_pending,
                candidate.provides.iter().map(|plugin| (&plugin.guid, &plugin.version, plugin.enabled)).collect::<Vec<_>>(),
            ));
        }
    }

    pub(super) fn pass(
        &self,
        pass: SearchPass,
        runtime_replacement_limit: usize,
        visited: usize,
        result: &Result<SearchState, SearchFailure>,
    ) {
        let (name, limit) = match pass {
            SearchPass::LimitReplacements(limit) => ("最少换包", Some(limit)),
            SearchPass::CompatibleVersions => ("兼容版本", None),
            SearchPass::AllowDependencyDisable => ("停用不兼容项", None),
        };
        let outcome = match result {
            Ok(_) => "ready",
            Err(SearchFailure::Blocked(true)) => "incomplete",
            Err(SearchFailure::Blocked(false)) => "unsatisfiable",
            Err(SearchFailure::Ownership(_)) => "blocked_ownership",
        };
        self.write(format_args!(
            "搜索轮次 pass={name} runtime_replacement_limit={runtime_replacement_limit} replacement_limit={limit:?} visited={visited} outcome={outcome}"
        ));
    }

    pub(super) fn solution(
        &self,
        search: &Search<'_, '_>,
        state: &SearchState,
        evaluations: &[NodeEvaluation],
    ) {
        if self.operation.is_none() || !log::log_enabled!(log::Level::Info) {
            return;
        }
        let protected = search.protected(state);
        for (index, node) in state.inventory.nodes.iter().enumerate() {
            if !protected.contains(&index) && !changed(search.original, &state.inventory, index) {
                continue;
            }
            let evaluation = &evaluations[index];
            let before = &search.original.nodes[index];
            let old = before.current.as_ref();
            let new = node.current.as_ref();
            let version = |candidate: Option<&Candidate>| {
                candidate.and_then(|candidate| {
                    candidate.tag.clone().or_else(|| candidate.version.clone())
                })
            };
            let decision = match (old, new) {
                (None, Some(_)) => "install_missing",
                (Some(old), Some(new)) if old.id != new.id => "replace",
                _ if !before.enabled && node.enabled => "enable_installed",
                _ if before.enabled && !node.enabled => "disable",
                _ if changed(search.original, &state.inventory, index) => "switch_members",
                _ => "keep_installed",
            };
            self.write(format_args!(
                "结果 node={:?} kind={:?} previous={:?} selected={:?} source={:?} enabled={} metadata_known={} decision={decision}",
                node.id, node.kind, version(old), version(new), new.and_then(|candidate| candidate.source_id.as_deref()),
                node.enabled, evaluation.metadata_known,
            ));
            let readiness = search.readiness(index, node);
            let relations = if readiness {
                &evaluation.readiness
            } else {
                &evaluation.runtime
            };
            for (owned, outcome) in scoped_relations(node, readiness).zip(relations) {
                let providers = owned_targets(&state.inventory, node, owned, false)
                    .into_iter()
                    .map(|(provider, plugin)| {
                        let provider = &state.inventory.nodes[provider];
                        (
                            &provider.id,
                            &plugin.version,
                            plugin.prerelease,
                            provider.enabled && plugin.enabled,
                        )
                    })
                    .collect::<Vec<_>>();
                self.write(format_args!(
                    "关系 owner={:?} relation={:?} state={:?} reason={} providers={providers:?}",
                    node.id, owned.relation, outcome.state, outcome.reason,
                ));
            }
        }
    }
}
