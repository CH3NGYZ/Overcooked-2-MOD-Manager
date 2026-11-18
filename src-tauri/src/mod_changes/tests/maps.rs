use super::*;

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
