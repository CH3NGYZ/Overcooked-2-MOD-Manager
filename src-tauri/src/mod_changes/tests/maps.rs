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
    assert_eq!(
        solve(&inventory, &intent("fw", Some(fw2.id), true)).status,
        SolveStatus::Unsatisfiable
    );
    assert_eq!(
        solve(&inventory, &intent("map", Some(map2.id), true)).status,
        SolveStatus::Ready
    );
}

#[test]
fn disabled_map_cannot_force_a_framework_change_but_enabled_map_blocks_it() {
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
    assert_eq!(
        solve(&inventory, &intent("fw", Some(fw2.id), true)).status,
        SolveStatus::Unsatisfiable
    );
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
