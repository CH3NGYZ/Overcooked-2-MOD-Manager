use super::super::evaluate::dependent_after_member_removal;
use super::*;

#[test]
fn duplicate_cleanup_checks_active_required_guid_and_version_for_mods_and_maps() {
    for kind in [NodeKind::Mod, NodeKind::Map] {
        for (owner_enabled, peer_guid, peer_requirement, should_block) in [
            (true, "new-guid", ">=1", true),
            (true, "old-guid", "<2", true),
            (true, "old-guid", ">=1", false),
            (false, "new-guid", ">=1", false),
        ] {
            let owner = candidate(
                "owner",
                "1.0",
                vec![relation(
                    RelationKind::Requires,
                    "old-guid",
                    peer_requirement,
                )],
            );
            let old = candidate("old-guid", "1.0", vec![]);
            let mut newer = candidate(peer_guid, "2.0", vec![]);
            newer.provides[0].member_key = Some("new.dll".into());
            let mut owner = node("owner", Some(owner), vec![]);
            owner.kind = kind.clone();
            owner.enabled = owner_enabled;
            let mut inventory = Inventory {
                nodes: vec![
                    owner,
                    node("old", Some(old), vec![]),
                    node("new", Some(newer), vec![]),
                ],
            };
            assert_eq!(
                dependent_after_member_removal(&mut inventory, "old-guid.dll", "old-guid").unwrap(),
                should_block.then(|| "owner".into())
            );
        }
    }
}

#[test]
fn disabled_dependency_is_unsatisfied_and_disabled_conflict_is_safe() {
    let owner = candidate(
        "owner",
        "1.0",
        vec![relation(RelationKind::Requires, "dep", ">=1")],
    );
    let mut dep = node("dep", Some(candidate("dep", "1.0", vec![])), vec![]);
    dep.enabled = false;
    let mut inventory = Inventory {
        nodes: vec![node("owner", Some(owner), vec![]), dep],
    };
    assert_eq!(
        evaluate(&inventory)[0].runtime[0].state,
        EvaluationState::Disabled
    );
    inventory.nodes[0].current.as_mut().unwrap().relations[0]
        .relation
        .kind = RelationKind::Conflicts;
    assert_eq!(
        evaluate(&inventory)[0].runtime[0].state,
        EvaluationState::Satisfied
    );
    inventory.nodes[0].enabled = false;
    assert!(evaluate(&inventory)[0].runtime.is_empty());
    assert_eq!(evaluate(&inventory)[0].readiness.len(), 1);
}

#[test]
fn disabled_member_does_not_emit_runtime_constraints() {
    let mut package = candidate(
        "active",
        "1.0",
        vec![OwnedRelation {
            owner_member_key: Some("disabled.dll".into()),
            owner_guid: Some("disabled".into()),
            relation: relation(RelationKind::Requires, "missing", "*").relation,
        }],
    );
    package.provides.push(ProvidedPlugin {
        member_key: Some("disabled.dll".into()),
        guid: "disabled".into(),
        version: Some("1.0".into()),
        prerelease: None,
        enabled: false,
    });
    let inventory = Inventory {
        nodes: vec![node("package", Some(package), vec![])],
    };
    let result = evaluate(&inventory);
    assert!(result[0].runtime.is_empty());
    assert_eq!(result[0].readiness[0].state, EvaluationState::Missing);
}

#[test]
fn multiple_enabled_guid_providers_are_unknown_not_arbitrary_success() {
    let root = candidate(
        "root",
        "1.0",
        vec![relation(RelationKind::Requires, "dep", "*")],
    );
    let inventory = Inventory {
        nodes: vec![
            node("root", Some(root.clone()), vec![]),
            node("one", Some(candidate("dep", "1.0", vec![])), vec![]),
            node("two", Some(candidate("dep", "2.0", vec![])), vec![]),
        ],
    };
    assert_eq!(
        evaluate(&inventory)[0].runtime[0].state,
        EvaluationState::Unknown
    );
    assert_eq!(
        solve(&inventory, &intent("root", None, true)).status,
        SolveStatus::Incomplete
    );
}

#[test]
fn prerelease_filter_and_unknown_versions_remain_three_state() {
    let mut root = candidate(
        "root",
        "1.0",
        vec![relation(RelationKind::Compatible, "dep", "*")],
    );
    root.relations[0].relation.prerelease = Some(false);
    let mut dep = candidate("dep", "1.0-beta.1", vec![]);
    dep.provides[0].prerelease = Some(true);
    let mut inventory = Inventory {
        nodes: vec![
            node("root", Some(root), vec![]),
            node("dep", Some(dep), vec![]),
        ],
    };
    assert_eq!(
        evaluate(&inventory)[0].runtime[0].state,
        EvaluationState::Conflict
    );
    inventory.nodes[1].current.as_mut().unwrap().provides[0].prerelease = None;
    inventory.nodes[1].current.as_mut().unwrap().provides[0].version = None;
    assert_eq!(
        evaluate(&inventory)[0].runtime[0].state,
        EvaluationState::Unknown
    );
}

#[test]
fn exact_owner_member_self_conflict_is_ignored_but_sibling_guid_and_requires_are_not() {
    let mut owner = candidate(
        "same",
        "1.0",
        vec![relation(RelationKind::Conflicts, "same", "*")],
    );
    owner.relations[0].owner_member_key = Some("same.dll".into());
    let mut inventory = Inventory {
        nodes: vec![node("owner", Some(owner), vec![])],
    };
    assert_eq!(
        evaluate(&inventory)[0].runtime[0].state,
        EvaluationState::Satisfied
    );
    inventory.nodes[0]
        .current
        .as_mut()
        .unwrap()
        .provides
        .push(ProvidedPlugin {
            member_key: Some("duplicate.dll".into()),
            guid: "same".into(),
            version: Some("1.0".into()),
            prerelease: None,
            enabled: true,
        });
    assert_eq!(
        evaluate(&inventory)[0].runtime[0].state,
        EvaluationState::Conflict
    );
    inventory.nodes[0].current.as_mut().unwrap().relations[0]
        .relation
        .kind = RelationKind::Requires;
    assert_eq!(
        evaluate(&inventory)[0].runtime[0].state,
        EvaluationState::Unknown
    );
    inventory.nodes[0].current.as_mut().unwrap().provides.pop();
    assert_eq!(
        evaluate(&inventory)[0].runtime[0].state,
        EvaluationState::Satisfied
    );
}

#[test]
fn sole_guid_owner_self_conflict_is_ignored_without_hiding_a_second_provider() {
    let mut owner = candidate(
        "same",
        "1.0",
        vec![relation(RelationKind::Conflicts, "same", "*")],
    );
    owner.relations[0].owner_guid = Some("same".into());
    let mut inventory = Inventory {
        nodes: vec![node("owner", Some(owner), vec![])],
    };
    assert_eq!(
        evaluate(&inventory)[0].runtime[0].state,
        EvaluationState::Satisfied
    );
    inventory.nodes.push(node(
        "second",
        Some(candidate("same", "1.0", vec![])),
        vec![],
    ));
    assert_eq!(
        evaluate(&inventory)[0].runtime[0].state,
        EvaluationState::Conflict
    );
}
