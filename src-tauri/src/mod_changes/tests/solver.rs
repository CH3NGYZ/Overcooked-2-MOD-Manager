use super::*;

mod disable_fallback;

#[test]
fn user_save_fix_downgrades_framework_and_reverse_web_dependency() {
    let fw10 = candidate("diy", "0.10.0", vec![]);
    let fw11 = candidate("diy", "0.11.0", vec![]);
    let web3 = candidate(
        "web",
        "3.10.2",
        vec![relation(RelationKind::Requires, "diy", ">=0.10.0")],
    );
    let web4 = candidate(
        "web",
        "4.0.0",
        vec![relation(RelationKind::Requires, "diy", ">=0.11.0")],
    );
    let fix = candidate(
        "fix",
        "0.4.0",
        vec![relation(RelationKind::Requires, "diy", "==0.10.0")],
    );
    let inventory = Inventory {
        nodes: vec![
            node("fix", None, vec![fix.clone()]),
            node("diy", Some(fw11.clone()), vec![fw10, fw11]),
            node("web", Some(web4.clone()), vec![web3, web4]),
        ],
    };
    let result = solve(&inventory, &intent("fix", Some(fix.id), true));
    assert_eq!(result.status, SolveStatus::Ready, "{result:?}");
    let framework = action_for(&result, "diy", ActionKind::Downgrade);
    assert_eq!(framework.to_tag.as_deref(), Some("v0.10.0"));
    let cause = &framework.reason_details[0];
    assert_eq!(cause.kind, ReasonKind::RequiredBy);
    assert_eq!(cause.related_node_ids, ["fix"]);
    assert_eq!(cause.requirement.as_deref(), Some("==0.10.0"));
    assert!(!cause.condition_on_related);
    assert_eq!(
        action_for(&result, "web", ActionKind::Downgrade)
            .to_tag
            .as_deref(),
        Some("v3.10.2")
    );
}

#[test]
fn manual_framework_root_remains_fixed_while_reverse_dependent_changes() {
    let fw10 = candidate("diy", "0.10.0", vec![]);
    let fw11 = candidate("diy", "0.11.0", vec![]);
    let web3 = candidate(
        "web",
        "3.10.2",
        vec![relation(RelationKind::Requires, "diy", "==0.10.0")],
    );
    let web4 = candidate(
        "web",
        "4.0",
        vec![relation(RelationKind::Requires, "diy", ">=0.11.0")],
    );
    let inventory = Inventory {
        nodes: vec![
            node("diy", Some(fw11.clone()), vec![fw10.clone(), fw11]),
            node("web", Some(web4.clone()), vec![web3, web4]),
        ],
    };
    let result = solve(&inventory, &intent("diy", Some(fw10.id), true));
    assert_eq!(result.status, SolveStatus::Ready, "{result:?}");
    action_for(&result, "web", ActionKind::Downgrade);
}

#[test]
fn satisfiable_cycle_is_not_rejected() {
    let a = candidate("a", "1.0", vec![relation(RelationKind::Requires, "b", "*")]);
    let b = candidate("b", "1.0", vec![relation(RelationKind::Requires, "a", "*")]);
    let inventory = Inventory {
        nodes: vec![node("a", None, vec![a.clone()]), node("b", None, vec![b])],
    };
    assert_eq!(
        solve(&inventory, &intent("a", Some(a.id), true)).status,
        SolveStatus::Ready
    );
}

#[test]
fn newer_candidate_backtracks_to_older_complete_solution() {
    let a = candidate(
        "a",
        "1.0",
        vec![relation(RelationKind::Requires, "b", ">=1")],
    );
    let b1 = candidate("b", "1.0", vec![]);
    let b2 = candidate(
        "b",
        "2.0",
        vec![relation(RelationKind::Requires, "absent", ">=1")],
    );
    let inventory = Inventory {
        nodes: vec![
            node("a", None, vec![a.clone()]),
            node("b", None, vec![b1, b2]),
        ],
    };
    let result = solve(&inventory, &intent("a", Some(a.id), true));
    assert_eq!(result.status, SolveStatus::Ready, "{result:?}");
    assert_eq!(
        action_for(&result, "b", ActionKind::Install)
            .to_tag
            .as_deref(),
        Some("v1.0")
    );
}

#[test]
fn unknown_candidate_metadata_and_incomplete_catalog_differ_from_unsatisfiable() {
    let mut root = candidate("root", "1.0", vec![]);
    root.relations_known = false;
    let inventory = Inventory {
        nodes: vec![node("root", None, vec![root.clone()])],
    };
    assert_eq!(
        solve(&inventory, &intent("root", Some(root.id), true)).status,
        SolveStatus::Incomplete
    );
    let root = candidate(
        "root",
        "1.0",
        vec![relation(RelationKind::Requires, "dep", ">=2")],
    );
    let mut dep = node("dep", Some(candidate("dep", "1.0", vec![])), vec![]);
    dep.candidates_complete = false;
    let inventory = Inventory {
        nodes: vec![node("root", None, vec![root.clone()]), dep],
    };
    assert_eq!(
        solve(&inventory, &intent("root", Some(root.id), true)).status,
        SolveStatus::Incomplete
    );
}

#[test]
fn raw_candidate_only_accepts_explicit_root_and_pending_verification_can_plan() {
    let mut raw = candidate("raw", "1.0", vec![]);
    raw.raw = true;
    let root = candidate(
        "root",
        "1.0",
        vec![relation(RelationKind::Requires, "raw", ">=1")],
    );
    let inventory = Inventory {
        nodes: vec![
            node("root", None, vec![root.clone()]),
            node("raw", None, vec![raw.clone()]),
        ],
    };
    assert_eq!(
        solve(&inventory, &intent("root", Some(root.id), true)).status,
        SolveStatus::Unsatisfiable
    );
    assert_eq!(
        solve(&inventory, &intent("raw", Some(raw.id), true)).status,
        SolveStatus::Ready
    );
    let mut pending = candidate("pending", "1.0", vec![]);
    pending.verification_pending = true;
    let inventory = Inventory {
        nodes: vec![node("pending", None, vec![pending.clone()])],
    };
    assert_eq!(
        solve(&inventory, &intent("pending", Some(pending.id), true)).status,
        SolveStatus::Ready
    );
}

#[test]
fn exact_beta_tag_drives_order_even_when_display_versions_are_identical() {
    let root = candidate(
        "root",
        "1.0",
        vec![relation(RelationKind::Requires, "dep", ">=1,<2")],
    );
    let mut older = candidate("dep", "1.9.8-beta.2", vec![]);
    older.version = Some("1.9.8".into());
    let mut newer = candidate("dep", "1.9.8-beta.12", vec![]);
    newer.version = Some("1.9.8".into());
    let inventory = Inventory {
        nodes: vec![
            node("root", None, vec![root.clone()]),
            node("dep", None, vec![older, newer]),
        ],
    };
    let result = solve(&inventory, &intent("root", Some(root.id), true));
    assert_eq!(result.status, SolveStatus::Ready);
    assert_eq!(
        action_for(&result, "dep", ActionKind::Install)
            .to_tag
            .as_deref(),
        Some("v1.9.8-beta.12")
    );
}

#[test]
fn filtered_pin_candidates_cannot_be_bypassed_but_explicit_root_is_exact() {
    let fw1 = candidate("fw", "1.0", vec![]);
    let fw2 = candidate("fw", "2.0", vec![]);
    let root = candidate(
        "root",
        "1.0",
        vec![relation(RelationKind::Requires, "fw", ">=2")],
    );
    let inventory = Inventory {
        nodes: vec![
            node("root", None, vec![root.clone()]),
            node("fw", Some(fw1.clone()), vec![fw1]),
        ],
    };
    assert_eq!(
        solve(&inventory, &intent("root", Some(root.id), true)).status,
        SolveStatus::Unsatisfiable
    );
    let mut explicit = inventory.clone();
    explicit.nodes[1].candidates.push(fw2.clone());
    assert_eq!(
        solve(&explicit, &intent("fw", Some(fw2.id), true)).status,
        SolveStatus::Ready
    );
}

#[test]
fn unknown_uninstalled_source_is_not_an_implicit_runtime_constraint() {
    let mut empty = node("offline-source", None, vec![]);
    empty.candidates_complete = false;
    let root = candidate("root", "1.0", vec![]);
    let inventory = Inventory {
        nodes: vec![node("root", None, vec![root.clone()]), empty],
    };
    assert_eq!(
        solve(&inventory, &intent("root", Some(root.id), true)).status,
        SolveStatus::Ready
    );
}

#[test]
fn provisional_remote_package_can_be_planned_before_member_verification() {
    let fw1 = candidate("fw", "1.0", vec![]);
    let fw2 = candidate("fw", "2.0", vec![]);
    let mut old = candidate(
        "web",
        "1.0",
        vec![relation(RelationKind::Requires, "fw", "==1")],
    );
    old.provides.push(ProvidedPlugin {
        member_key: Some("helper.dll".into()),
        guid: "helper".into(),
        version: Some("1.0".into()),
        prerelease: None,
        enabled: false,
    });
    let mut next = candidate(
        "web",
        "2.0",
        vec![relation(RelationKind::Requires, "fw", "==2")],
    );
    next.verification_pending = true;
    let inventory = Inventory {
        nodes: vec![
            node("fw", Some(fw1), vec![fw2.clone()]),
            node("web", Some(old), vec![next]),
        ],
    };
    assert_eq!(
        solve(&inventory, &intent("fw", Some(fw2.id), true)).status,
        SolveStatus::Ready
    );
}
