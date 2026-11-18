use super::super::*;

fn incompatible_web() -> (Inventory, ChangeRequest) {
    let fw10 = candidate("diy", "0.10.0", vec![]);
    let fw11 = candidate("diy", "0.11.0", vec![]);
    let web4 = candidate(
        "web",
        "4.0.0",
        vec![relation(RelationKind::Requires, "diy", ">=0.11.0")],
    );
    (
        Inventory {
            nodes: vec![
                node("diy", Some(fw11), vec![fw10.clone()]),
                node("web", Some(web4), vec![]),
            ],
        },
        intent("diy", Some(fw10.id), true),
    )
}

#[test]
fn complete_incompatible_history_disables_web_and_its_nonroot_mod_and_map_dependents() {
    let (mut inventory, request) = incompatible_web();
    inventory.nodes.push(node(
        "dependent",
        Some(candidate(
            "dependent",
            "1.0",
            vec![relation(RelationKind::Requires, "web", "*")],
        )),
        vec![],
    ));
    let mut map = node(
        "map",
        Some(candidate(
            "map",
            "1.0",
            vec![relation(RelationKind::Requires, "dependent", "*")],
        )),
        vec![],
    );
    map.kind = NodeKind::Map;
    inventory.nodes.push(map);
    inventory.nodes.push(node(
        "unrelated",
        Some(candidate(
            "unrelated",
            "1.0",
            vec![relation(RelationKind::Requires, "missing-baseline", "*")],
        )),
        vec![],
    ));
    let (result, solved) = solve_with_inventory(&inventory, &request);
    assert_eq!(result.status, SolveStatus::Ready, "{result:?}");
    let solved = solved.unwrap();
    assert_eq!(
        solved.nodes[0].current.as_ref().unwrap().tag.as_deref(),
        Some("v0.10.0")
    );
    assert!(solved.nodes[0].enabled);
    for id in ["web", "dependent", "map"] {
        assert!(
            !solved
                .nodes
                .iter()
                .find(|node| node.id == id)
                .unwrap()
                .enabled
        );
        action_for(&result, id, ActionKind::Disable);
    }
    assert!(
        solved
            .nodes
            .iter()
            .find(|node| node.id == "unrelated")
            .unwrap()
            .enabled
    );
    assert!(
        result
            .actions
            .iter()
            .all(|action| action.node_id != "unrelated")
    );
}

#[test]
fn compatible_historical_web_version_is_preferred_over_disabling_it() {
    let (mut inventory, request) = incompatible_web();
    inventory.nodes[1].candidates.push(candidate(
        "web",
        "3.10.2",
        vec![relation(RelationKind::Requires, "diy", "==0.10.0")],
    ));
    let (result, solved) = solve_with_inventory(&inventory, &request);
    assert_eq!(result.status, SolveStatus::Ready, "{result:?}");
    let solved = solved.unwrap();
    assert!(solved.nodes[1].enabled);
    assert_eq!(
        solved.nodes[1].current.as_ref().unwrap().tag.as_deref(),
        Some("v3.10.2")
    );
    assert!(
        result
            .actions
            .iter()
            .all(|action| action.kind != ActionKind::Disable)
    );
}

#[test]
fn fallback_reverse_dependents_prefer_compatible_history_before_disabling() {
    let (mut inventory, request) = incompatible_web();
    inventory.nodes.push(node(
        "dependent",
        Some(candidate(
            "dependent",
            "1.0",
            vec![relation(RelationKind::Requires, "web", "*")],
        )),
        vec![candidate("dependent", "2.0", vec![])],
    ));
    let (result, solved) = solve_with_inventory(&inventory, &request);
    assert_eq!(result.status, SolveStatus::Ready, "{result:?}");
    let solved = solved.unwrap();
    assert!(!solved.nodes[1].enabled);
    assert!(solved.nodes[2].enabled);
    assert_eq!(
        solved.nodes[2].current.as_ref().unwrap().tag.as_deref(),
        Some("v2.0")
    );
    action_for(&result, "web", ActionKind::Disable);
    action_for(&result, "dependent", ActionKind::Upgrade);
    assert!(
        result
            .actions
            .iter()
            .all(|action| action.node_id != "dependent" || action.kind != ActionKind::Disable)
    );
}

#[test]
fn known_reverse_version_can_solve_even_when_its_history_is_incomplete() {
    let (mut inventory, request) = incompatible_web();
    let mut dependent = node(
        "dependent",
        Some(candidate(
            "dependent",
            "1.0",
            vec![relation(RelationKind::Requires, "web", "*")],
        )),
        vec![candidate("dependent", "2.0", vec![])],
    );
    dependent.candidates_complete = false;
    inventory.nodes.push(dependent);
    let (result, solved) = solve_with_inventory(&inventory, &request);
    assert_eq!(result.status, SolveStatus::Ready, "{result:?}");
    let solved = solved.unwrap();
    assert!(!solved.nodes[1].enabled);
    assert!(solved.nodes[2].enabled);
    assert_eq!(
        solved.nodes[2].current.as_ref().unwrap().tag.as_deref(),
        Some("v2.0")
    );
    action_for(&result, "web", ActionKind::Disable);
    action_for(&result, "dependent", ActionKind::Upgrade);
}

#[test]
fn unknown_reverse_candidate_cannot_be_hidden_by_a_later_disable_fallback() {
    let (mut inventory, request) = incompatible_web();
    let mut unknown = candidate("dependent", "2.0", vec![]);
    unknown.relations_known = false;
    inventory.nodes.push(node(
        "dependent",
        Some(candidate(
            "dependent",
            "1.0",
            vec![relation(RelationKind::Requires, "web", "*")],
        )),
        vec![unknown],
    ));
    let (result, solved) = solve_with_inventory(&inventory, &request);
    assert_eq!(result.status, SolveStatus::Incomplete, "{result:?}");
    assert!(result.actions.is_empty());
    assert!(solved.is_none());
}

#[test]
fn known_complete_reverse_version_can_succeed_after_an_unknown_newer_candidate() {
    let (mut inventory, request) = incompatible_web();
    let mut unknown = candidate("dependent", "3.0", vec![]);
    unknown.relations_known = false;
    inventory.nodes.push(node(
        "dependent",
        Some(candidate(
            "dependent",
            "1.0",
            vec![relation(RelationKind::Requires, "web", "*")],
        )),
        vec![unknown, candidate("dependent", "2.0", vec![])],
    ));
    let (result, solved) = solve_with_inventory(&inventory, &request);
    assert_eq!(result.status, SolveStatus::Ready, "{result:?}");
    let solved = solved.unwrap();
    assert!(!solved.nodes[1].enabled);
    assert!(solved.nodes[2].enabled);
    assert_eq!(
        solved.nodes[2].current.as_ref().unwrap().tag.as_deref(),
        Some("v2.0")
    );
}

#[test]
fn unknown_transitive_target_cannot_be_hidden_by_disabling_its_reverse_owner() {
    let (mut inventory, request) = incompatible_web();
    inventory.nodes.push(node(
        "dependent",
        Some(candidate(
            "dependent",
            "1.0",
            vec![relation(RelationKind::Requires, "web", "*")],
        )),
        vec![candidate(
            "dependent",
            "2.0",
            vec![relation(RelationKind::Requires, "helper", ">=2")],
        )],
    ));
    let mut helper = node("helper", Some(candidate("helper", "1.0", vec![])), vec![]);
    helper.candidates_complete = false;
    inventory.nodes.push(helper);
    let (result, solved) = solve_with_inventory(&inventory, &request);
    assert_eq!(result.status, SolveStatus::Incomplete, "{result:?}");
    assert!(result.actions.is_empty());
    assert!(solved.is_none());
}

#[test]
fn second_pass_keeps_explicit_and_conflict_disable_cascades_immediate() {
    for explicit in [true, false] {
        let (mut inventory, mut request) = incompatible_web();
        inventory.nodes.push(node(
            "target",
            Some(candidate("target", "1.0", vec![])),
            vec![],
        ));
        inventory.nodes.push(node(
            "target-dependent",
            Some(candidate(
                "target-dependent",
                "1.0",
                vec![relation(RelationKind::Requires, "target", "*")],
            )),
            vec![candidate("target-dependent", "2.0", vec![])],
        ));
        if explicit {
            request.roots.push(RootIntent {
                node_id: "target".into(),
                candidate_id: None,
                enabled: false,
                member_key: None,
            });
        } else {
            inventory.nodes[0].candidates[0].relations.push(relation(
                RelationKind::Conflicts,
                "target",
                "*",
            ));
        }
        let (result, solved) = solve_with_inventory(&inventory, &request);
        assert_eq!(
            result.status,
            SolveStatus::Ready,
            "explicit={explicit}: {result:?}"
        );
        let solved = solved.unwrap();
        assert!(!solved.nodes[1].enabled);
        assert!(!solved.nodes[2].enabled);
        assert!(!solved.nodes[3].enabled);
        assert_eq!(
            solved.nodes[3].current.as_ref().unwrap().tag.as_deref(),
            Some("v1.0")
        );
        action_for(&result, "target-dependent", ActionKind::Disable);
    }
}

#[test]
fn all_version_only_branches_finish_before_a_deep_owner_can_be_disabled() {
    let (mut inventory, request) = incompatible_web();
    inventory.nodes[1].candidates = vec![
        candidate(
            "web",
            "3.10.2",
            vec![
                relation(RelationKind::Requires, "diy", "==0.10.0"),
                relation(RelationKind::Requires, "bridge", ">=2"),
            ],
        ),
        candidate(
            "web",
            "3.10.1",
            vec![
                relation(RelationKind::Requires, "diy", "==0.10.0"),
                relation(RelationKind::Requires, "bridge", "==1"),
            ],
        ),
    ];
    inventory.nodes.push(node(
        "bridge",
        Some(candidate("bridge", "1.0", vec![])),
        vec![candidate("bridge", "2.0", vec![])],
    ));
    inventory.nodes.push(node(
        "bridge-dependent",
        Some(candidate(
            "bridge-dependent",
            "1.0",
            vec![relation(RelationKind::Requires, "bridge", "==1")],
        )),
        vec![],
    ));
    let (result, solved) = solve_with_inventory(&inventory, &request);
    assert_eq!(result.status, SolveStatus::Ready, "{result:?}");
    let solved = solved.unwrap();
    assert_eq!(
        solved.nodes[1].current.as_ref().unwrap().tag.as_deref(),
        Some("v3.10.1")
    );
    assert!(solved.nodes.iter().all(|node| node.enabled));
    assert!(
        result
            .actions
            .iter()
            .all(|action| action.kind != ActionKind::Disable)
    );
}

#[test]
fn fallback_never_disables_a_whole_root_fixed_enabled_member_or_required_closure() {
    let (inventory, request) = incompatible_web();
    for member_key in [None, Some("web.dll".to_string())] {
        let mut request = request.clone();
        request.roots.push(RootIntent {
            node_id: "web".into(),
            candidate_id: None,
            enabled: true,
            member_key,
        });
        assert_eq!(
            solve(&inventory, &request).status,
            SolveStatus::Unsatisfiable
        );
    }
    let mut inventory = inventory;
    let app = candidate(
        "app",
        "1.0",
        vec![relation(RelationKind::Requires, "web", "*")],
    );
    inventory.nodes.push(node("app", None, vec![app.clone()]));
    let mut request = request;
    request.roots.push(RootIntent {
        node_id: "app".into(),
        candidate_id: Some(app.id),
        enabled: true,
        member_key: None,
    });
    assert_eq!(
        solve(&inventory, &request).status,
        SolveStatus::Unsatisfiable
    );
}

#[test]
fn incomplete_history_or_unknown_candidate_metadata_cannot_be_bypassed_by_disabling() {
    let (mut inventory, request) = incompatible_web();
    inventory.nodes[1].candidates_complete = false;
    let result = solve(&inventory, &request);
    assert_eq!(result.status, SolveStatus::Incomplete, "{result:?}");
    assert!(result.actions.is_empty());
    inventory.nodes[1].candidates_complete = true;
    inventory.nodes[1].current.as_mut().unwrap().relations_known = false;
    let result = solve(&inventory, &request);
    assert_eq!(result.status, SolveStatus::Incomplete, "{result:?}");
    assert!(result.actions.is_empty());
    inventory.nodes[1].current.as_mut().unwrap().relations_known = true;
    let mut unknown = candidate("web", "3.10.2", vec![]);
    unknown.relations_known = false;
    inventory.nodes[1].candidates.push(unknown);
    let result = solve(&inventory, &request);
    assert_eq!(result.status, SolveStatus::Incomplete, "{result:?}");
    assert!(result.actions.is_empty());
}

#[test]
fn incompatible_owner_without_a_safe_switch_still_blocks() {
    let (mut inventory, request) = incompatible_web();
    inventory.nodes[1].mutable = false;
    assert_eq!(
        solve(&inventory, &request).status,
        SolveStatus::Unsatisfiable
    );
}

#[test]
fn unrelated_unknown_baseline_and_empty_catalog_do_not_block_proven_disable_fallback() {
    let (mut inventory, request) = incompatible_web();
    let mut unrelated = candidate(
        "unrelated",
        "1.0",
        vec![relation(RelationKind::Requires, "missing-baseline", "*")],
    );
    unrelated.relations_known = false;
    inventory
        .nodes
        .push(node("unrelated", Some(unrelated), vec![]));
    let mut unloaded_catalog = node("unloaded", None, vec![]);
    unloaded_catalog.candidates_complete = false;
    inventory.nodes.push(unloaded_catalog);
    let (result, solved) = solve_with_inventory(&inventory, &request);
    assert_eq!(result.status, SolveStatus::Ready, "{result:?}");
    let solved = solved.unwrap();
    assert!(!solved.nodes[1].enabled);
    assert!(solved.nodes[2].enabled);
    assert!(
        result
            .actions
            .iter()
            .all(|action| action.node_id != "unrelated" && action.node_id != "unloaded")
    );
}

#[test]
fn new_disable_cascade_cannot_hide_incomplete_dependent_history() {
    let (mut inventory, request) = incompatible_web();
    let mut dependent = node(
        "dependent",
        Some(candidate(
            "dependent",
            "1.0",
            vec![relation(RelationKind::Requires, "web", "*")],
        )),
        vec![],
    );
    dependent.candidates_complete = false;
    inventory.nodes.push(dependent);
    let result = solve(&inventory, &request);
    assert_eq!(result.status, SolveStatus::Incomplete, "{result:?}");
    assert!(result.actions.is_empty());
}

#[test]
fn missing_target_with_unloaded_catalog_remains_unknown() {
    let (mut inventory, request) = incompatible_web();
    inventory.nodes[1].candidates.push(candidate(
        "web",
        "3.10.2",
        vec![relation(RelationKind::Requires, "unresolved-guid", "*")],
    ));
    let mut unloaded = node("unloaded", None, vec![]);
    unloaded.candidates_complete = false;
    inventory.nodes.push(unloaded);
    let result = solve(&inventory, &request);
    assert_eq!(result.status, SolveStatus::Incomplete, "{result:?}");
    assert!(result.actions.is_empty());
}

#[test]
fn fallback_keeps_original_files_instead_of_installing_a_version_it_will_disable() {
    let (mut inventory, request) = incompatible_web();
    inventory.nodes[1].candidates.push(candidate(
        "web",
        "3.10.2",
        vec![relation(RelationKind::Requires, "diy", ">=0.11.0")],
    ));
    let original = inventory.nodes[1].current.as_ref().unwrap().id.clone();
    let (result, solved) = solve_with_inventory(&inventory, &request);
    assert_eq!(result.status, SolveStatus::Ready, "{result:?}");
    let solved = solved.unwrap();
    assert!(!solved.nodes[1].enabled);
    assert_eq!(solved.nodes[1].current.as_ref().unwrap().id, original);
    assert!(
        result
            .actions
            .iter()
            .filter(|action| action.node_id == "web")
            .all(|action| action.kind == ActionKind::Disable)
    );
}
