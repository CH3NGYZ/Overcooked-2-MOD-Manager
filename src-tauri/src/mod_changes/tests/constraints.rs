use super::*;

#[test]
fn conflict_disables_target_and_reverse_dependents_without_uninstall() {
    let blocker = candidate(
        "blocker",
        "1.0",
        vec![relation(RelationKind::Conflicts, "target", "*")],
    );
    let inventory = Inventory {
        nodes: vec![
            node("blocker", None, vec![blocker.clone()]),
            node("target", Some(candidate("target", "1.0", vec![])), vec![]),
            node(
                "dependent",
                Some(candidate(
                    "dependent",
                    "1.0",
                    vec![relation(RelationKind::Requires, "target", "*")],
                )),
                vec![],
            ),
        ],
    };
    let result = solve(&inventory, &intent("blocker", Some(blocker.id), true));
    assert_eq!(result.status, SolveStatus::Ready, "{result:?}");
    let target = action_for(&result, "target", ActionKind::Disable);
    assert_eq!(target.reason_details[0].kind, ReasonKind::ConflictsWith);
    assert_eq!(target.reason_details[0].related_node_ids, ["blocker"]);
    assert!(!target.reason_details[0].condition_on_related);
    let dependent = action_for(&result, "dependent", ActionKind::Disable);
    assert_eq!(
        dependent.reason_details[0].kind,
        ReasonKind::DependencyDisabled
    );
    assert_eq!(dependent.reason_details[0].related_node_ids, ["target"]);
}

#[test]
fn required_root_cannot_be_disabled_to_hide_a_conflict() {
    let a = candidate(
        "a",
        "1.0",
        vec![
            relation(RelationKind::Requires, "b", "*"),
            relation(RelationKind::Conflicts, "b", "*"),
        ],
    );
    let inventory = Inventory {
        nodes: vec![
            node("a", None, vec![a.clone()]),
            node("b", Some(candidate("b", "1.0", vec![])), vec![]),
        ],
    };
    assert_eq!(
        solve(&inventory, &intent("a", Some(a.id), true)).status,
        SolveStatus::Unsatisfiable
    );
}

#[test]
fn unrelated_baseline_failure_does_not_trigger_a_repair() {
    let root = candidate("root", "1.0", vec![]);
    let bad = candidate(
        "bad",
        "1.0",
        vec![relation(RelationKind::Requires, "missing", ">=1")],
    );
    let inventory = Inventory {
        nodes: vec![
            node("root", None, vec![root.clone()]),
            node("bad", Some(bad), vec![]),
        ],
    };
    let result = solve(&inventory, &intent("root", Some(root.id), true));
    assert_eq!(result.status, SolveStatus::Ready);
    assert!(result.actions.iter().all(|action| action.node_id == "root"));
    assert_eq!(
        result.evaluations[1].runtime[0].state,
        EvaluationState::Missing
    );
}

#[test]
fn disabled_reverse_owner_only_gets_readiness_diagnostics() {
    let fw1 = candidate("fw", "1.0", vec![]);
    let fw2 = candidate("fw", "2.0", vec![]);
    let mut disabled = node(
        "disabled",
        Some(candidate(
            "disabled",
            "1.0",
            vec![relation(RelationKind::Requires, "fw", "==1")],
        )),
        vec![],
    );
    disabled.enabled = false;
    let inventory = Inventory {
        nodes: vec![node("fw", Some(fw1), vec![fw2.clone()]), disabled],
    };
    let result = solve(&inventory, &intent("fw", Some(fw2.id), true));
    assert_eq!(result.status, SolveStatus::Ready);
    assert!(result.actions.iter().all(|action| action.node_id == "fw"));
    assert!(result.evaluations[1].runtime.is_empty());
    assert_eq!(
        result.evaluations[1].readiness[0].state,
        EvaluationState::VersionMismatch
    );
}

#[test]
fn explicit_multi_root_disable_cascades_but_never_disables_an_enabled_root() {
    let dep = candidate("dep", "1.0", vec![]);
    let owner = candidate(
        "owner",
        "1.0",
        vec![relation(RelationKind::Requires, "dep", "*")],
    );
    let inventory = Inventory {
        nodes: vec![
            node("dep", Some(dep), vec![]),
            node("owner", Some(owner), vec![]),
        ],
    };
    let result = solve(&inventory, &intent("dep", None, false));
    assert_eq!(result.status, SolveStatus::Ready);
    action_for(&result, "owner", ActionKind::Disable);
    let mut request = intent("dep", None, false);
    request.roots.push(RootIntent {
        node_id: "owner".into(),
        candidate_id: None,
        enabled: true,
        member_key: None,
    });
    assert_eq!(
        solve(&inventory, &request).status,
        SolveStatus::Unsatisfiable
    );
}

#[test]
fn self_guid_conflict_uses_release_line_without_rejecting_itself() {
    let mut stable = candidate(
        "same",
        "1.0",
        vec![relation(RelationKind::Conflicts, "same", "*")],
    );
    stable.relations[0].relation.prerelease = Some(true);
    stable.provides[0].prerelease = Some(false);
    let mut beta = candidate("same", "1.0-beta.1", vec![]);
    beta.provides[0].prerelease = Some(true);
    let inventory = Inventory {
        nodes: vec![
            node("stable", None, vec![stable.clone()]),
            node("beta", Some(beta), vec![]),
        ],
    };
    let result = solve(&inventory, &intent("stable", Some(stable.id), true));
    assert_eq!(result.status, SolveStatus::Ready);
    action_for(&result, "beta", ActionKind::Disable);
    assert!(
        !result
            .actions
            .iter()
            .any(|action| action.node_id == "stable" && action.kind == ActionKind::Disable)
    );
}

#[test]
fn sibling_member_switch_does_not_pull_unrelated_baseline_version_errors() {
    let mut package = candidate("first", "1.0", vec![]);
    package.provides[0].enabled = false;
    package.provides.push(ProvidedPlugin {
        member_key: Some("second.dll".into()),
        guid: "second".into(),
        version: Some("1.0".into()),
        prerelease: None,
        enabled: true,
    });
    let dependent = candidate(
        "dependent",
        "1.0",
        vec![relation(RelationKind::Requires, "second", ">=2")],
    );
    let inventory = Inventory {
        nodes: vec![
            node("package", Some(package), vec![]),
            node("dependent", Some(dependent), vec![]),
        ],
    };
    let request = ChangeRequest {
        roots: vec![RootIntent {
            node_id: "package".into(),
            candidate_id: None,
            enabled: true,
            member_key: Some("first.dll".into()),
        }],
    };
    let (result, solved) = super::solve_with_inventory(&inventory, &request);
    assert_eq!(result.status, SolveStatus::Ready, "{result:?}");
    assert!(solved.unwrap().nodes[1].enabled);
    assert!(
        !result
            .actions
            .iter()
            .any(|action| action.node_id == "dependent")
    );
    assert_eq!(
        result.evaluations[1].runtime[0].state,
        EvaluationState::VersionMismatch
    );
}

#[test]
fn compatible_mismatch_can_disable_optional_target_and_its_reverse_dependents() {
    let root = candidate(
        "root",
        "1.0",
        vec![relation(RelationKind::Compatible, "dep", ">=2")],
    );
    let dependent = candidate(
        "dependent",
        "1.0",
        vec![relation(RelationKind::Requires, "dep", "*")],
    );
    let inventory = Inventory {
        nodes: vec![
            node("root", None, vec![root.clone()]),
            node("dep", Some(candidate("dep", "1.0", vec![])), vec![]),
            node("dependent", Some(dependent), vec![]),
        ],
    };
    let (result, solved) =
        super::solve_with_inventory(&inventory, &intent("root", Some(root.id), true));
    assert_eq!(result.status, SolveStatus::Ready, "{result:?}");
    let solved = solved.unwrap();
    assert!(solved.nodes[0].enabled);
    assert!(!solved.nodes[1].enabled);
    assert!(!solved.nodes[2].enabled);
}

#[test]
fn compatible_mismatch_cannot_disable_an_explicit_root_or_required_closure() {
    let root = candidate(
        "root",
        "1.0",
        vec![relation(RelationKind::Compatible, "dep", ">=2")],
    );
    let mut inventory = Inventory {
        nodes: vec![
            node("root", None, vec![root.clone()]),
            node("dep", Some(candidate("dep", "1.0", vec![])), vec![]),
        ],
    };
    let mut request = intent("root", Some(root.id.clone()), true);
    request.roots.push(RootIntent {
        node_id: "dep".into(),
        candidate_id: None,
        enabled: true,
        member_key: None,
    });
    assert_eq!(
        solve(&inventory, &request).status,
        SolveStatus::Unsatisfiable
    );
    inventory.nodes[0].candidates[0]
        .relations
        .push(relation(RelationKind::Requires, "dep", "*"));
    assert_eq!(
        solve(&inventory, &intent("root", Some(root.id), true)).status,
        SolveStatus::Unsatisfiable
    );
}
