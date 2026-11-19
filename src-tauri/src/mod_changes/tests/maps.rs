use super::*;

fn kitchen_inventory(require_web: &str) -> Inventory {
    let diy = node(
        "diy",
        Some(candidate("diy", "0.10", vec![])),
        vec![candidate("diy", "0.11.1", vec![])],
    );
    let web = node(
        "web",
        Some(candidate(
            "web",
            "3.10.2",
            vec![relation(RelationKind::Requires, "diy", ">=0.10,<0.11")],
        )),
        vec![candidate(
            "web",
            "4.0.0",
            vec![relation(RelationKind::Requires, "diy", ">=0.11")],
        )],
    );
    let mut kitchen = node(
        "dmm-kitchen",
        Some(candidate(
            "dmm-kitchen",
            "10.1.1",
            vec![
                relation(RelationKind::Requires, "web", require_web),
                relation(RelationKind::Requires, "diy", ""),
            ],
        )),
        vec![],
    );
    kitchen.kind = NodeKind::Map;
    kitchen.enabled = false;
    Inventory {
        nodes: vec![diy, web, kitchen],
    }
}

#[test]
fn enabling_map_with_satisfied_dependencies_only_enables_that_map() {
    let inventory = kitchen_inventory(">=3.10.2");
    let result = solve(&inventory, &intent("dmm-kitchen", None, true));
    assert_eq!(result.status, SolveStatus::Ready, "{result:?}");
    assert_eq!(result.actions.len(), 1, "{:?}", result.actions);
    let action = action_for(&result, "dmm-kitchen", ActionKind::Enable);
    assert_eq!(action.from_candidate_id, action.to_candidate_id);
    assert_eq!(result.impacts, ["dmm-kitchen"]);
}

#[test]
fn enabling_map_reuses_installed_dependency_that_only_needs_enabling() {
    let mut inventory = kitchen_inventory(">=3.10.2");
    inventory.nodes[1].enabled = false;
    let result = solve(&inventory, &intent("dmm-kitchen", None, true));
    assert_eq!(result.status, SolveStatus::Ready, "{result:?}");
    assert_eq!(result.actions.len(), 2, "{:?}", result.actions);
    let web = action_for(&result, "web", ActionKind::Enable);
    assert_eq!(web.from_candidate_id, web.to_candidate_id);
    action_for(&result, "dmm-kitchen", ActionKind::Enable);
}

#[test]
fn enabling_map_with_real_version_mismatch_still_resolves_the_required_chain() {
    let inventory = kitchen_inventory(">=4.0");
    let result = solve(&inventory, &intent("dmm-kitchen", None, true));
    assert_eq!(result.status, SolveStatus::Ready, "{result:?}");
    assert_eq!(result.actions.len(), 3, "{:?}", result.actions);
    assert_eq!(
        action_for(&result, "web", ActionKind::Upgrade)
            .to_tag
            .as_deref(),
        Some("v4.0.0")
    );
    assert_eq!(
        action_for(&result, "diy", ActionKind::Upgrade)
            .to_tag
            .as_deref(),
        Some("v0.11.1")
    );
    action_for(&result, "dmm-kitchen", ActionKind::Enable);
}

#[test]
fn explicit_framework_update_explains_each_link_of_an_upgrade_and_map_disable_plan() {
    let mut inventory = kitchen_inventory(">=3.10.2,<4.0");
    // An older local install may only have the info/DLL version, not a Release tag.
    inventory.nodes[0].current.as_mut().unwrap().tag = None;
    inventory.nodes[2].enabled = true;
    let mut second_map = node(
        "other-kitchen",
        Some(candidate(
            "other-kitchen",
            "1.0",
            vec![relation(RelationKind::Requires, "diy", ">=0.10,<0.11")],
        )),
        vec![],
    );
    second_map.kind = NodeKind::Map;
    inventory.nodes.push(second_map);

    let new_framework = inventory.nodes[0].candidates[0].id.clone();
    let result = solve(&inventory, &intent("diy", Some(new_framework), true));
    assert_eq!(result.status, SolveStatus::Ready, "{result:?}");
    assert_eq!(result.actions.len(), 4);
    let framework = action_for(&result, "diy", ActionKind::Upgrade);
    assert_eq!(framework.from_tag.as_deref(), Some("0.10"));
    assert_eq!(framework.to_tag.as_deref(), Some("v0.11.1"));
    assert_eq!(framework.reason_details[0].kind, ReasonKind::Requested);
    for (item, action, dependency, old_requirement) in [
        ("web", ActionKind::Upgrade, "diy", ">=0.10,<0.11"),
        ("dmm-kitchen", ActionKind::Disable, "web", ">=3.10.2,<4.0"),
        ("other-kitchen", ActionKind::Disable, "diy", ">=0.10,<0.11"),
    ] {
        let action = action_for(&result, item, action);
        let [cause] = action.reason_details.as_slice() else {
            panic!("expected one cause for {action:?}");
        };
        assert_eq!(cause.kind, ReasonKind::DependencyVersion);
        assert_eq!(cause.related_node_ids, [dependency]);
        assert_eq!(cause.requirement.as_deref(), Some(old_requirement));
        assert!(cause.condition_on_related);
    }
}

#[test]
fn enabling_map_can_choose_older_missing_dependency_to_keep_compatible_installed_versions() {
    let mut inventory = kitchen_inventory(">=3.10.2");
    let helper1 = candidate(
        "helper",
        "1.0",
        vec![relation(RelationKind::Requires, "web", ">=3.10.2")],
    );
    let helper2 = candidate(
        "helper",
        "2.0",
        vec![relation(RelationKind::Requires, "web", ">=4.0")],
    );
    let map = inventory.nodes[2].current.as_mut().unwrap();
    let map_id = map.id.clone();
    map.relations
        .push(relation(RelationKind::Requires, "helper", ">=1"));
    inventory
        .nodes
        .push(node("helper", None, vec![helper1, helper2]));

    let result = solve(&inventory, &intent("dmm-kitchen", None, true));
    assert_eq!(result.status, SolveStatus::Ready, "{result:?}");
    assert_eq!(result.actions.len(), 3, "{:?}", result.actions);
    assert_eq!(
        action_for(&result, "helper", ActionKind::Install)
            .to_tag
            .as_deref(),
        Some("v1.0")
    );
    action_for(&result, "helper", ActionKind::Enable);
    action_for(&result, "dmm-kitchen", ActionKind::Enable);

    // User-selected versions still choose the newest compatible dependency chain.
    let exact = solve(&inventory, &intent("dmm-kitchen", Some(map_id), true));
    assert_eq!(exact.status, SolveStatus::Ready, "{exact:?}");
    assert_eq!(
        action_for(&exact, "helper", ActionKind::Install)
            .to_tag
            .as_deref(),
        Some("v2.0")
    );
    action_for(&exact, "web", ActionKind::Upgrade);
    action_for(&exact, "diy", ActionKind::Upgrade);
}

#[test]
fn enabling_map_reuses_compatible_installed_provider_before_another_repository() {
    let mut inventory = kitchen_inventory(">=3.10.2");
    inventory.nodes[1].enabled = false;
    inventory.nodes.push(node(
        "alternate-repository",
        None,
        vec![candidate("web", "4.0.0", vec![])],
    ));
    let result = solve(&inventory, &intent("dmm-kitchen", None, true));
    assert_eq!(result.status, SolveStatus::Ready, "{result:?}");
    assert_eq!(result.actions.len(), 2, "{:?}", result.actions);
    action_for(&result, "web", ActionKind::Enable);
    action_for(&result, "dmm-kitchen", ActionKind::Enable);
}

#[test]
fn enabling_map_only_replaces_the_installed_dependency_that_is_too_old() {
    let mut inventory = kitchen_inventory(">=3.10.2");
    let web = &mut inventory.nodes[1];
    web.candidates.push(web.current.take().unwrap());
    web.current = Some(candidate(
        "web",
        "3.0.0",
        vec![relation(RelationKind::Requires, "diy", ">=0.10,<0.11")],
    ));

    let result = solve(&inventory, &intent("dmm-kitchen", None, true));
    assert_eq!(result.status, SolveStatus::Ready, "{result:?}");
    assert_eq!(result.actions.len(), 2, "{:?}", result.actions);
    assert_eq!(
        action_for(&result, "web", ActionKind::Upgrade)
            .to_tag
            .as_deref(),
        Some("v3.10.2")
    );
    action_for(&result, "dmm-kitchen", ActionKind::Enable);
}

#[test]
fn explicit_map_root_can_update_but_auto_reverse_map_version_is_fixed() {
    let fw1 = candidate("fw", "1.0", vec![]);
    let fw2 = candidate("fw", "2.0", vec![]);
    let map1 = candidate(
        "map",
        "1.0",
        vec![relation(RelationKind::Requires, "fw", "==1")],
    );
    let map2 = candidate(
        "map",
        "2.0",
        vec![relation(RelationKind::Requires, "fw", "==2")],
    );
    let mut map = node("map", Some(map1), vec![map2.clone()]);
    map.kind = NodeKind::Map;
    let inventory = Inventory {
        nodes: vec![node("fw", Some(fw1), vec![fw2.clone()]), map],
    };
    let (result, solved) = solve_with_inventory(&inventory, &intent("fw", Some(fw2.id), true));
    assert_eq!(result.status, SolveStatus::Ready, "{result:?}");
    let solved = solved.unwrap();
    assert!(!solved.nodes[1].enabled);
    assert_eq!(
        solved.nodes[1].current,
        inventory.nodes[1].current.as_ref().map(|candidate| {
            let mut candidate = candidate.clone();
            for member in &mut candidate.provides {
                member.enabled = false;
            }
            candidate
        })
    );
    action_for(&result, "map", ActionKind::Disable);
    assert_eq!(
        solve(&inventory, &intent("map", Some(map2.id), true)).status,
        SolveStatus::Ready
    );
}

#[test]
fn disabled_map_only_reports_readiness_and_incompatible_enabled_nonroot_map_can_disable() {
    let fw1 = candidate("fw", "1.0", vec![]);
    let fw2 = candidate("fw", "2.0", vec![]);
    let mut map = node(
        "map",
        Some(candidate(
            "map",
            "1.0",
            vec![relation(RelationKind::Requires, "fw", "==1")],
        )),
        vec![],
    );
    map.kind = NodeKind::Map;
    map.enabled = false;
    let mut inventory = Inventory {
        nodes: vec![node("fw", Some(fw1), vec![fw2.clone()]), map],
    };
    assert_eq!(
        solve(&inventory, &intent("fw", Some(fw2.id.clone()), true)).status,
        SolveStatus::Ready
    );
    inventory.nodes[1].enabled = true;
    let result = solve(&inventory, &intent("fw", Some(fw2.id), true));
    assert_eq!(result.status, SolveStatus::Ready, "{result:?}");
    action_for(&result, "map", ActionKind::Disable);
}

#[test]
fn map_foundation_is_hard_requires_and_keeps_explicit_version_range() {
    let mut relations = vec![
        relation(
            RelationKind::Requires,
            crate::mods::plugin_target::DIYLEVEL_GUID,
            "==0.10",
        )
        .relation,
    ];
    super::inventory_maps::require_framework(&mut relations);
    assert_eq!(relations.len(), 1);
    assert_eq!(relations[0].version, "==0.10");
    let mut relations = vec![];
    super::inventory_maps::require_framework(&mut relations);
    assert_eq!(relations.len(), 1);
    assert!(!relations[0].optional);
}
