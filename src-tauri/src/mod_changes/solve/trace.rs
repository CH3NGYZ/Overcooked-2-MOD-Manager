use super::{Search, SearchPass, SearchState, changed, visit::SearchFailure};
use crate::mod_changes::{evaluate::owned_targets, model::*};
use crate::{
    mod_changes::session::{DETAIL_LIMIT, OperationTrace, VERIFICATION_SUMMARY_STAGE, log_detail},
    models::RepositoryRelation,
};
use std::{cell::Cell, fmt::Arguments};

const MAX_SEARCH_EVENTS: usize = 160;

pub(super) struct SolveTrace<'a> {
    operation: Option<&'a OperationTrace<'a>>,
    events: Cell<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_diagnostics::capture_test_logs;

    #[test]
    fn solver_logs_one_budget_notice_and_keeps_terminal_summary() {
        let operation = "op=fixture-solver-log-budget";
        let capture = capture_test_logs("app_lib::mod_changes::solve::trace", operation);
        let operation_trace = OperationTrace::new("fixture-solver-log-budget", "initial");
        let trace = SolveTrace::new(Some(&operation_trace));
        for _ in 0..MAX_SEARCH_EVENTS * 2 {
            if trace.step() {
                trace.write(format_args!("candidate"));
            }
        }
        trace.write(format_args!("terminal"));
        let messages = capture.messages();
        assert_eq!(messages.len(), MAX_SEARCH_EVENTS + 2);
        assert_eq!(
            messages.iter().filter(|line| line.contains("已达")).count(),
            1
        );
        assert!(messages.last().unwrap().ends_with("terminal"));
    }
}

impl<'a> SolveTrace<'a> {
    pub(super) fn new(operation: Option<&'a OperationTrace<'a>>) -> Self {
        Self {
            operation,
            events: Cell::new(0),
        }
    }

    fn write(&self, event: Arguments<'_>) {
        if let Some(operation) = self.operation {
            log::info!(
                "依赖求解 op={} stage={} {}",
                operation.op,
                operation.stage,
                log_detail(event)
            );
        }
    }

    pub(super) fn observing(&self) -> bool {
        self.operation
            .is_some_and(|operation| operation.stage != VERIFICATION_SUMMARY_STAGE)
            && log::log_enabled!(log::Level::Info)
            && self.events.get() <= MAX_SEARCH_EVENTS
    }

    fn step(&self) -> bool {
        if !self.observing() {
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
                "待解决 state={:?} reason={} owner={:?} version={:?} relation={:?} targets={:?}",
                issue.state,
                issue.reason,
                node.id,
                node.current
                    .as_ref()
                    .and_then(|selected| selected.version.as_deref()),
                issue.relation,
                issue.targets.iter().take(8).collect::<Vec<_>>(),
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
                "候选 reason={reason} node={:?} id={:?} source={:?} version={:?} verification_pending={} relation={relation:?} provides={:?} omitted_members={}",
                node.id, candidate.id, candidate.source_id, candidate.version, candidate.verification_pending,
                candidate.provides.iter().take(8).map(|plugin| (&plugin.guid, &plugin.version, plugin.enabled)).collect::<Vec<_>>(),
                candidate.provides.len().saturating_sub(8),
            ));
        }
    }

    pub(super) fn root(&self, node: &ChangeNode, requested: &RootIntent) {
        if self.step() {
            self.write(format_args!(
                "目标 node={:?} kind={:?} installed={:?} tag={:?} version={:?} source={:?} enabled={} mutable={} requested_candidate={:?} requested_enabled={} requested_member={:?} candidates={} complete={}",
                node.id, node.kind,
                node.current.as_ref().map(|current| current.id.as_str()),
                node.current.as_ref().and_then(|current| current.tag.as_deref()),
                node.current.as_ref().and_then(|current| current.version.as_deref()),
                node.current.as_ref().and_then(|current| current.source_id.as_deref()),
                node.enabled, node.mutable, requested.candidate_id, requested.enabled,
                requested.member_key, node.candidates.len(), node.candidates_complete,
            ));
        }
    }

    pub(super) fn branch(
        &self,
        node: &ChangeNode,
        reason: &str,
        relation: Option<&RepositoryRelation>,
    ) {
        if self.step() {
            self.write(format_args!(
                "分支 node={:?} enabled={} reason={reason} relation={relation:?}",
                node.id, node.enabled
            ));
        }
    }

    pub(super) fn pass(
        &self,
        pass: SearchPass,
        runtime_replacement_limit: usize,
        visited: usize,
        elapsed_ms: u128,
        result: &Result<SearchState, SearchFailure>,
    ) {
        if !self.step() {
            return;
        }
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
            "搜索轮次 pass={name} runtime_replacement_limit={runtime_replacement_limit} replacement_limit={limit:?} visited={visited} outcome={outcome} elapsed_ms={elapsed_ms}"
        ));
    }

    pub(super) fn solution(
        &self,
        search: &Search<'_, '_>,
        state: &SearchState,
        evaluations: &[NodeEvaluation],
    ) {
        let Some(operation) = self.operation else {
            return;
        };
        if !log::log_enabled!(log::Level::Info)
            || (operation.stage == VERIFICATION_SUMMARY_STAGE
                && state.inventory.nodes.iter().any(|node| {
                    node.current
                        .as_ref()
                        .is_some_and(|candidate| candidate.verification_pending)
                }))
        {
            return;
        }
        let protected = search.protected(state);
        let mut results = 0;
        for (index, node) in state.inventory.nodes.iter().enumerate() {
            if !search.roots.contains(&index)
                && !protected.contains(&index)
                && !changed(search.original, &state.inventory, index)
            {
                continue;
            }
            if results >= DETAIL_LIMIT {
                self.write(format_args!("结果明细已达 {DETAIL_LIMIT} 节点上限"));
                break;
            }
            results += 1;
            let evaluation = &evaluations[index];
            let before = &search.original.nodes[index];
            let old = before.current.as_ref();
            let new = node.current.as_ref();
            let decision = match (old, new) {
                (None, Some(_)) => "install_missing",
                (Some(old), Some(new)) if old.id != new.id => "replace",
                _ if !before.enabled && node.enabled => "enable_installed",
                _ if before.enabled && !node.enabled => "disable",
                _ if changed(search.original, &state.inventory, index) => "switch_members",
                _ => "keep_installed",
            };
            self.write(format_args!(
                "结果 node={:?} kind={:?} decision={decision} previous_enabled={} enabled={} metadata_known={} previous_id={:?} selected_id={:?} previous_tag={:?} selected_tag={:?} previous_version={:?} selected_version={:?} previous_source={:?} source={:?}",
                node.id, node.kind, before.enabled, node.enabled, evaluation.metadata_known,
                old.map(|candidate| &candidate.id), new.map(|candidate| &candidate.id),
                old.and_then(|candidate| candidate.tag.as_deref()), new.and_then(|candidate| candidate.tag.as_deref()),
                old.and_then(|candidate| candidate.version.as_deref()), new.and_then(|candidate| candidate.version.as_deref()),
                old.and_then(|candidate| candidate.source_id.as_deref()), new.and_then(|candidate| candidate.source_id.as_deref()),
            ));
            if node.kind == NodeKind::Mod
                && let Some(source) = new.and_then(|candidate| candidate.source_id.as_deref())
                && let Some(repo) = search.trusted.and_then(|trusted| {
                    trusted
                        .config
                        .repositories
                        .iter()
                        .find(|repo| repo.id == source)
                })
            {
                self.write(format_args!(
                    "选中MOD来源 node={:?} source={source:?} catalog={:?} provider={:?} repo={:?}/{:?} channel={:?} pin={:?} release_cache_unix_ms={:?} cached_releases_complete={}",
                    node.id, repo.source_id, repo.provider, repo.username, repo.repository, repo.channel,
                    repo.pinned_version, repo.releases_cached_at, repo.cached_releases_complete,
                ));
            }
            let readiness = search.readiness(index, node);
            let relations = if readiness {
                &evaluation.readiness
            } else {
                &evaluation.runtime
            };
            for (owned, outcome) in
                super::super::evaluate::scoped_evaluations(node, evaluation, readiness).take(8)
            {
                let providers = owned_targets(&state.inventory, node, owned, false)
                    .into_iter()
                    .take(8)
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
            if relations.len() > 8 {
                self.write(format_args!(
                    "关系明细已截断 node={:?} omitted={}",
                    node.id,
                    relations.len() - 8
                ));
            }
        }
    }
}
