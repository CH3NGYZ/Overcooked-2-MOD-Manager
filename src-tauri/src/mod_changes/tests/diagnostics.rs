use super::*;

const WEB: &str = "oc2.oc2diylevelruntimewloader";
const DIY: &str = "dev.gua.overcooked.diylevel";
const FIX: &str = "fixture-multipart-fix";

fn web_runtime_upgrade(fix: Option<Candidate>) -> (Inventory, ChangeRequest) {
    let requested_web = candidate(
        WEB,
        "4.0.0",
        vec![relation(RelationKind::Requires, DIY, ">=0.11.0,<0.12.0")],
    );
    let current_web = candidate(
        WEB,
        "3.10.2",
        vec![relation(RelationKind::Requires, DIY, ">=0.10.0,<0.11.0")],
    );
    let requires_fix = || vec![relation(RelationKind::Requires, FIX, ">=0.2.3")];
    let request = intent(WEB, Some(requested_web.id.clone()), true);
    let inventory = Inventory {
        nodes: vec![
            node(WEB, Some(current_web), vec![requested_web]),
            node(
                DIY,
                Some(candidate(DIY, "0.10.0", requires_fix())),
                vec![
                    candidate(DIY, "0.11.0", requires_fix()),
                    candidate(DIY, "0.11.1", requires_fix()),
                ],
            ),
            node(FIX, fix, vec![]),
        ],
    };
    (inventory, request)
}

#[test]
fn web_runtime_upgrade_preserves_both_tried_framework_branches_to_missing_fix() {
    let (inventory, request) = web_runtime_upgrade(None);
    let (result, solved) = solve_with_inventory(&inventory, &request);
    assert_eq!(result.status, SolveStatus::Unsatisfiable);
    assert!(solved.is_none());
    assert!(result.actions.is_empty());
    // The installed graph and failed candidates are deliberately separate.
    assert_eq!(
        result.evaluations[0].runtime[0].state,
        EvaluationState::Satisfied
    );
    let blocker = &result.blockers[0];
    assert_eq!(blocker.target_guid.as_deref(), Some(DIY));
    assert!(!blocker.traces_truncated);
    assert_eq!(blocker.traces.len(), 2);
    for (path, version) in blocker.traces.iter().zip(["0.11.1", "0.11.0"]) {
        let [root_edge, failed_edge] = path.as_slice() else {
            panic!("expected Web Runtime → DIYLevel → missing fix: {path:?}");
        };
        assert_eq!(root_edge.owner_id, WEB);
        assert_eq!(root_edge.owner_version.as_deref(), Some("4.0.0"));
        assert_eq!(
            root_edge.relation.as_ref().unwrap().version,
            ">=0.11.0,<0.12.0"
        );
        assert_eq!(root_edge.state, EvaluationState::Satisfied);
        assert_eq!(
            root_edge.targets,
            [DiagnosticTarget {
                node_id: DIY.into(),
                version: Some(version.into()),
                enabled: true,
                candidate_id: Some(format!("{DIY}@{version}")),
                member_key: Some(format!("{DIY}.dll")),
                source: Some(DIY.into()),
            }]
        );
        assert_eq!(failed_edge.owner_id, DIY);
        assert_eq!(failed_edge.owner_version.as_deref(), Some(version));
        assert_eq!(failed_edge.relation.as_ref().unwrap().guid, FIX);
        assert_eq!(failed_edge.relation.as_ref().unwrap().version, ">=0.2.3");
        assert_eq!(failed_edge.state, EvaluationState::Missing);
        assert_eq!(failed_edge.reason, "required_plugin_missing");
        assert!(failed_edge.targets.is_empty());
    }
}

#[test]
fn web_runtime_failed_traces_distinguish_installed_version_mismatch_and_unknown() {
    for (actual_version, expected_status, expected_state, expected_reason) in [
        (
            Some("0.2.2"),
            SolveStatus::Unsatisfiable,
            EvaluationState::VersionMismatch,
            "required_version_mismatch",
        ),
        (
            None,
            SolveStatus::Incomplete,
            EvaluationState::Unknown,
            "relation_metadata_unknown",
        ),
    ] {
        let mut fix = candidate(FIX, "0.2.2", vec![]);
        // The DLL, not the Release tag, is the evidence the matcher used.
        fix.tag = Some("v0.2.3".into());
        fix.provides[0].version = actual_version.map(str::to_owned);
        let (inventory, request) = web_runtime_upgrade(Some(fix));
        let blocked = solve(&inventory, &request);
        assert_eq!(blocked.status, expected_status);
        assert_eq!(blocked.blockers[0].traces.len(), 2);
        for trace in &blocked.blockers[0].traces {
            let failed = trace.last().unwrap();
            assert_eq!(failed.state, expected_state);
            assert_eq!(failed.reason, expected_reason);
            assert_eq!(
                failed.targets,
                [DiagnosticTarget {
                    node_id: FIX.into(),
                    version: actual_version.map(str::to_owned),
                    enabled: true,
                    candidate_id: Some(format!("{FIX}@0.2.2")),
                    member_key: Some(format!("{FIX}.dll")),
                    source: Some(FIX.into()),
                }]
            );
        }
    }
}

#[test]
fn incomplete_history_does_not_present_unconfirmed_absence_as_missing() {
    let (mut inventory, request) = web_runtime_upgrade(None);
    inventory.nodes[2].candidates_complete = false;
    let result = solve(&inventory, &request);
    assert_eq!(result.status, SolveStatus::Incomplete);
    assert_eq!(result.blockers[0].traces.len(), 2);
    for trace in &result.blockers[0].traces {
        let failed = trace.last().unwrap();
        assert_eq!(failed.state, EvaluationState::Unknown);
        assert_eq!(failed.reason, "relation_metadata_unknown");
        assert_eq!(failed.relation.as_ref().unwrap().guid, FIX);
        assert!(failed.targets.is_empty());
    }
}

#[test]
fn trace_uses_actual_disabled_provider_and_retains_intermediate_links() {
    let root = candidate(
        "root",
        "1",
        vec![relation(RelationKind::Requires, "a", "*")],
    );
    let a = candidate("a", "1", vec![relation(RelationKind::Requires, "b", "*")]);
    let b = candidate("b", "1", vec![relation(RelationKind::Requires, "c", "*")]);
    let mut c = node("c", Some(candidate("c", "1", vec![])), vec![]);
    c.enabled = false;
    c.mutable = false;
    let inventory = Inventory {
        nodes: vec![
            node("root", None, vec![root.clone()]),
            node("a", Some(a), vec![]),
            node("b", Some(b), vec![]),
            c,
        ],
    };
    let result = solve(&inventory, &intent("root", Some(root.id), true));
    assert_eq!(result.status, SolveStatus::Unsatisfiable);
    let traces = &result.blockers[0].traces;
    assert_eq!(traces.len(), 1);
    assert_eq!(
        traces[0]
            .iter()
            .map(|step| step.owner_id.as_str())
            .collect::<Vec<_>>(),
        ["root", "a", "b"]
    );
    let failed = traces[0].last().unwrap();
    assert_eq!(failed.state, EvaluationState::Disabled);
    assert_eq!(
        failed.targets,
        [DiagnosticTarget {
            node_id: "c".into(),
            version: Some("1".into()),
            enabled: false,
            candidate_id: Some("c@1".into()),
            member_key: Some("c.dll".into()),
            source: Some("c".into()),
        }]
    );
}

#[test]
fn alternative_branch_can_report_unknown_candidate_metadata_or_conflict() {
    let (mut inventory, request) = web_runtime_upgrade(Some(candidate(FIX, "0.2.3", vec![])));
    let diy = &mut inventory.nodes[1].candidates;
    diy[0].relations_known = false;
    let mut conflict = relation(RelationKind::Conflicts, WEB, ">=4");
    conflict.relation.prerelease = Some(false);
    diy[1].relations.push(conflict);
    let result = solve(&inventory, &request);
    assert_eq!(result.status, SolveStatus::Incomplete);
    assert_eq!(result.blockers[0].traces.len(), 2);
    let complete = result.blockers[0].traces[0].last().unwrap();
    assert_eq!(complete.owner_version.as_deref(), Some("0.11.1"));
    assert_eq!(complete.state, EvaluationState::Conflict);
    assert_eq!(complete.relation.as_ref().unwrap().prerelease, Some(false));
    assert_eq!(complete.targets[0].version.as_deref(), Some("4.0.0"));
    let unknown = result.blockers[0].traces[1].last().unwrap();
    assert_eq!(unknown.owner_version.as_deref(), Some("0.11.0"));
    assert_eq!(unknown.state, EvaluationState::Unknown);
    assert_eq!(unknown.reason, "candidate_relations_unknown");
    let wire = serde_json::to_value(unknown).unwrap();
    assert!(wire.get("relation").is_none());
    assert_eq!(wire["state"], "unknown");
}

#[test]
fn successful_alternative_does_not_publish_failed_branches() {
    let (mut inventory, request) = web_runtime_upgrade(None);
    inventory.nodes[1].candidates[0].relations.clear();
    let result = solve(&inventory, &request);
    assert_eq!(result.status, SolveStatus::Ready);
    assert_eq!(
        action_for(&result, DIY, ActionKind::Upgrade)
            .to_tag
            .as_deref(),
        Some("v0.11.0")
    );
    assert!(result.blockers.is_empty());
}

#[test]
fn failed_branch_limit_keeps_examples_and_reports_omissions() {
    let root = candidate(
        "root",
        "1",
        vec![relation(RelationKind::Requires, "dependency", "*")],
    );
    let alternatives = (1..=12)
        .map(|patch| {
            candidate(
                "dependency",
                &format!("1.0.{patch}"),
                vec![relation(RelationKind::Requires, "gone", "*")],
            )
        })
        .collect();
    let inventory = Inventory {
        nodes: vec![
            node("root", None, vec![root.clone()]),
            node("dependency", None, alternatives),
        ],
    };
    let result = solve(&inventory, &intent("root", Some(root.id), true));
    assert_eq!(result.status, SolveStatus::Unsatisfiable);
    let blocker = &result.blockers[0];
    let wire = serde_json::to_value(blocker).unwrap();
    assert!(blocker.traces_truncated);
    assert_eq!(blocker.traces.len(), 8);
    assert_eq!(wire["traces_truncated"], true);
    assert_eq!(wire["traces"][0][0]["targets"][0]["version"], "1.0.12");
    assert_eq!(wire["traces"][0][1]["state"], "missing");
}
