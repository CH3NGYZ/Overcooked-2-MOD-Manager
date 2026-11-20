use super::*;

#[test]
fn enabled_requirement_reuses_an_installed_disabled_member() {
    let root = candidate(
        "root",
        "1.0",
        vec![relation(RelationKind::Requires, "dep", ">=1")],
    );
    let mut dep = node("dep", Some(candidate("dep", "1.0", vec![])), vec![]);
    dep.current.as_mut().unwrap().provides[0].enabled = false;
    let inventory = Inventory {
        nodes: vec![node("root", None, vec![root.clone()]), dep],
    };
    let (result, solved) =
        super::solve_with_inventory(&inventory, &intent("root", Some(root.id), true));
    assert_eq!(result.status, SolveStatus::Ready, "{result:?}");
    assert!(solved.unwrap().nodes[1].current.as_ref().unwrap().provides[0].enabled);
    assert_eq!(
        action_for(&result, "dep", ActionKind::Enable)
            .member_key
            .as_deref(),
        None
    );
    assert!(
        !result
            .actions
            .iter()
            .any(|action| action.node_id == "dep" && action.kind == ActionKind::Install)
    );
}

#[test]
fn separate_requirements_enable_multiple_members_of_one_decided_package() {
    let root = candidate(
        "root",
        "1.0",
        vec![
            relation(RelationKind::Requires, "dep", ">=1"),
            relation(RelationKind::Requires, "extra", ">=1"),
        ],
    );
    let mut package = candidate("dep", "1.0", vec![]);
    package.provides[0].enabled = false;
    for guid in ["extra", "parked"] {
        package.provides.push(ProvidedPlugin {
            member_key: Some(format!("{guid}.dll")),
            guid: guid.into(),
            version: Some("1.0".into()),
            prerelease: None,
            enabled: false,
        });
    }
    let package_id = package.id.clone();
    let mut installed = node("package", Some(package), vec![]);
    installed.enabled = false;
    let inventory = Inventory {
        nodes: vec![node("root", None, vec![root.clone()]), installed],
    };
    let (result, solved) = solve_with_inventory(&inventory, &intent("root", Some(root.id), true));
    assert_eq!(result.status, SolveStatus::Ready, "{result:?}");
    let solved = solved.unwrap();
    assert!(solved.nodes[1].enabled);
    let actual = solved.nodes[1].current.as_ref().unwrap();
    assert_eq!(actual.id, package_id);
    assert!(actual.provides[0].enabled);
    assert!(actual.provides[1].enabled);
    assert!(!actual.provides[2].enabled);
    assert!(
        result.evaluations[0]
            .runtime
            .iter()
            .all(|relation| relation.state == EvaluationState::Satisfied)
    );
    assert!(
        result
            .actions
            .iter()
            .filter(|action| action.node_id == "package")
            .all(|action| action.kind == ActionKind::Enable)
    );
}

#[test]
fn multiple_member_requirements_cannot_override_an_explicit_disabled_member() {
    let root = candidate(
        "root",
        "1.0",
        vec![
            relation(RelationKind::Requires, "dep", "*"),
            relation(RelationKind::Requires, "extra", "*"),
        ],
    );
    let mut package = candidate("dep", "1.0", vec![]);
    package.provides[0].enabled = false;
    package.provides.push(ProvidedPlugin {
        member_key: Some("extra.dll".into()),
        guid: "extra".into(),
        version: Some("1.0".into()),
        prerelease: None,
        enabled: true,
    });
    let inventory = Inventory {
        nodes: vec![
            node("root", None, vec![root.clone()]),
            node("package", Some(package), vec![]),
        ],
    };
    let mut request = intent("root", Some(root.id), true);
    request.roots.push(RootIntent {
        node_id: "package".into(),
        candidate_id: None,
        enabled: false,
        member_key: Some("extra.dll".into()),
    });
    let (result, solved) = solve_with_inventory(&inventory, &request);
    assert_eq!(result.status, SolveStatus::Unsatisfiable, "{result:?}");
    assert!(solved.is_none());
}

#[test]
fn conflict_can_disable_unrelated_member_of_required_asset() {
    let root = candidate(
        "root",
        "1.0",
        vec![
            relation(RelationKind::Requires, "dep", "*"),
            relation(RelationKind::Conflicts, "extra", "*"),
        ],
    );
    let mut package = candidate("dep", "1.0", vec![]);
    package.provides.push(ProvidedPlugin {
        member_key: Some("extra.dll".into()),
        guid: "extra".into(),
        version: Some("1.0".into()),
        prerelease: None,
        enabled: true,
    });
    let inventory = Inventory {
        nodes: vec![
            node("root", None, vec![root.clone()]),
            node("package", Some(package), vec![]),
        ],
    };
    let result = solve(&inventory, &intent("root", Some(root.id), true));
    assert_eq!(result.status, SolveStatus::Ready, "{result:?}");
    assert_eq!(
        action_for(&result, "package", ActionKind::Disable)
            .member_key
            .as_deref(),
        Some("extra.dll")
    );
    assert_eq!(
        result.evaluations[0].runtime[0].state,
        EvaluationState::Satisfied
    );
}

#[test]
fn solved_inventory_preserves_exact_member_enablement_and_failed_solve_has_none() {
    let mut dep = candidate("dep", "1.0", vec![]);
    dep.provides.push(ProvidedPlugin {
        member_key: Some("other.dll".into()),
        guid: "other".into(),
        version: Some("1.0".into()),
        prerelease: None,
        enabled: false,
    });
    let inventory = Inventory {
        nodes: vec![node("dep", Some(dep), vec![])],
    };
    let request = ChangeRequest {
        roots: vec![RootIntent {
            node_id: "dep".into(),
            candidate_id: None,
            enabled: true,
            member_key: Some("other.dll".into()),
        }],
        ..Default::default()
    };
    let (result, solved) = super::solve_with_inventory(&inventory, &request);
    assert_eq!(result.status, SolveStatus::Ready);
    assert!(
        solved.unwrap().nodes[0]
            .current
            .as_ref()
            .unwrap()
            .provides
            .iter()
            .all(|plugin| plugin.enabled)
    );
    let (_, solved) = super::solve_with_inventory(&inventory, &intent("missing", None, true));
    assert!(solved.is_none());
}

#[test]
fn overlapping_member_and_package_roots_cannot_silently_override_each_other() {
    let inventory = Inventory {
        nodes: vec![node("dep", Some(candidate("dep", "1.0", vec![])), vec![])],
    };
    let mut request = intent("dep", None, false);
    request.roots.push(RootIntent {
        node_id: "dep".into(),
        candidate_id: None,
        enabled: true,
        member_key: Some("dep.dll".into()),
    });
    assert_eq!(
        solve(&inventory, &request).status,
        SolveStatus::Unsatisfiable
    );
    request.roots.reverse();
    assert_eq!(
        solve(&inventory, &request).status,
        SolveStatus::Unsatisfiable
    );
}

#[test]
fn existing_member_cannot_vanish_by_selecting_unrelated_package_version() {
    let fw1 = candidate("fw", "1.0", vec![]);
    let fw2 = candidate("fw", "2.0", vec![]);
    let web = candidate(
        "web",
        "1.0",
        vec![relation(RelationKind::Requires, "fw", "==1")],
    );
    let impostor = candidate("unrelated", "9.0", vec![]);
    let inventory = Inventory {
        nodes: vec![
            node("fw", Some(fw1), vec![fw2.clone()]),
            node("web", Some(web), vec![impostor]),
        ],
    };
    let (result, solved) = solve_with_inventory(&inventory, &intent("fw", Some(fw2.id), true));
    assert_eq!(result.status, SolveStatus::Ready, "{result:?}");
    let solved = solved.unwrap();
    assert!(!solved.nodes[1].enabled);
    assert_eq!(
        solved.nodes[1].current.as_ref().unwrap().provides[0].guid,
        "web"
    );
    action_for(&result, "web", ActionKind::Disable);
    assert!(
        result
            .actions
            .iter()
            .all(|action| action.node_id != "web" || action.kind == ActionKind::Disable)
    );
}

#[test]
fn required_package_newest_optimization_can_retire_unrequired_bundled_members() {
    let root = candidate(
        "root",
        "1.0",
        vec![relation(RelationKind::Requires, "dep", ">=1")],
    );
    let mut installed = candidate("dep", "1.0", vec![]);
    installed.provides.push(ProvidedPlugin {
        member_key: Some("helper.dll".into()),
        guid: "helper".into(),
        version: Some("1.0".into()),
        prerelease: None,
        enabled: false,
    });
    let newer = candidate("dep", "2.0", vec![]);
    let inventory = Inventory {
        nodes: vec![
            node("root", None, vec![root.clone()]),
            node("dep", Some(installed), vec![newer]),
        ],
    };
    let (result, solved) =
        super::solve_with_inventory(&inventory, &intent("root", Some(root.id), true));
    assert_eq!(result.status, SolveStatus::Ready, "{result:?}");
    let dep = solved.unwrap().nodes[1].current.clone().unwrap();
    assert_eq!(dep.version.as_deref(), Some("2.0"));
    assert_eq!(dep.provides.len(), 1);
}

#[test]
fn retiring_a_bundled_member_still_checks_all_enabled_reverse_dependents() {
    for explicit_dependent in [false, true] {
        let mut newer = candidate("framework", "0.11.0", vec![]);
        newer.provides.push(ProvidedPlugin {
            member_key: Some("splitter.dll".into()),
            guid: "splitter".into(),
            version: Some("0.1.0".into()),
            prerelease: None,
            enabled: true,
        });
        let older = candidate("framework", "0.10.0", vec![]);
        let web = candidate(
            "web",
            "3.10.2",
            vec![relation(
                RelationKind::Requires,
                "framework",
                ">=0.10.0,<0.11.0",
            )],
        );
        let dependent = candidate(
            "dependent",
            "1.0.0",
            vec![relation(RelationKind::Requires, "splitter", ">=0.1.0")],
        );
        let inventory = Inventory {
            nodes: vec![
                node("framework", Some(newer), vec![older]),
                node("web", Some(web), vec![]),
                node("dependent", Some(dependent), vec![]),
            ],
        };
        let mut request = intent("web", None, true);
        if explicit_dependent {
            request.roots.extend(intent("dependent", None, true).roots);
        }
        let (result, solved) = solve_with_inventory(&inventory, &request);
        if explicit_dependent {
            assert_eq!(result.status, SolveStatus::Unsatisfiable, "{result:?}");
            assert!(solved.is_none());
            assert!(result.actions.is_empty());
        } else {
            assert_eq!(result.status, SolveStatus::Ready, "{result:?}");
            action_for(&result, "framework", ActionKind::Downgrade);
            action_for(&result, "dependent", ActionKind::Disable);
            let solved = solved.unwrap();
            assert!(solved.nodes[1].enabled);
            assert!(!solved.nodes[2].enabled);
        }
    }
}

#[test]
fn a_repository_candidate_cannot_drop_every_installed_guid() {
    let fw1 = candidate("framework", "1.0", vec![]);
    let fw2 = candidate("framework", "2.0", vec![]);
    let web = candidate(
        "web",
        "1.0",
        vec![relation(RelationKind::Requires, "framework", "==1")],
    );
    let mut impostor = candidate("unrelated", "9.0", vec![]);
    impostor.source_id = web.source_id.clone();
    let inventory = Inventory {
        nodes: vec![
            node("framework", Some(fw1), vec![fw2.clone()]),
            node("web", Some(web), vec![impostor]),
        ],
    };
    let (result, solved) =
        solve_with_inventory(&inventory, &intent("framework", Some(fw2.id), true));
    assert_eq!(result.status, SolveStatus::Ready, "{result:?}");
    action_for(&result, "web", ActionKind::Disable);
    assert_eq!(
        solved.unwrap().nodes[1].current.as_ref().unwrap().provides[0].guid,
        "web"
    );
}

#[test]
fn retiring_members_requires_a_nonempty_shared_trusted_repository() {
    for (old_source, new_source) in [
        (None, None),
        (Some(""), Some("")),
        (Some(" "), Some(" ")),
        (Some("framework"), Some("other-source")),
        (None, Some("framework")),
    ] {
        let root = candidate(
            "root",
            "1.0",
            vec![relation(RelationKind::Requires, "dep", "==1")],
        );
        let mut installed = candidate("dep", "2.0", vec![]);
        installed.source_id = old_source.map(str::to_string);
        installed.provides.push(ProvidedPlugin {
            member_key: Some("helper.dll".into()),
            guid: "helper".into(),
            version: Some("1.0".into()),
            prerelease: None,
            enabled: false,
        });
        let mut older = candidate("dep", "1.0", vec![]);
        older.source_id = new_source.map(str::to_string);
        let inventory = Inventory {
            nodes: vec![
                node("root", Some(root), vec![]),
                node("dep", Some(installed), vec![older]),
            ],
        };
        let (result, solved) = solve_with_inventory(&inventory, &intent("root", None, true));
        assert_eq!(
            result.status,
            SolveStatus::Unsatisfiable,
            "{old_source:?}, {new_source:?}: {result:?}"
        );
        assert!(solved.is_none());
    }
}

#[test]
fn required_member_can_enable_inside_fixed_version_but_explicit_member_disable_is_fixed() {
    let mut package = candidate(
        "primary",
        "1.0",
        vec![relation(RelationKind::Requires, "sibling", "*")],
    );
    package.provides.push(ProvidedPlugin {
        member_key: Some("sibling.dll".into()),
        guid: "sibling".into(),
        version: Some("1.0".into()),
        prerelease: None,
        enabled: false,
    });
    let inventory = Inventory {
        nodes: vec![node("package", None, vec![package.clone()])],
    };
    let mut request = intent("package", Some(package.id.clone()), true);
    let (result, solved) = super::solve_with_inventory(&inventory, &request);
    assert_eq!(result.status, SolveStatus::Ready);
    assert!(solved.unwrap().nodes[0].current.as_ref().unwrap().provides[1].enabled);
    request.roots.push(RootIntent {
        node_id: "package".into(),
        candidate_id: Some(package.id),
        enabled: false,
        member_key: Some("sibling.dll".into()),
    });
    assert_eq!(
        solve(&inventory, &request).status,
        SolveStatus::Unsatisfiable
    );
}

#[test]
fn reverse_package_version_preserves_nonrequired_member_switches() {
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
    next.provides.push(ProvidedPlugin {
        member_key: Some("new/helper.dll".into()),
        guid: "helper".into(),
        version: Some("2.0".into()),
        prerelease: None,
        enabled: true,
    });
    let inventory = Inventory {
        nodes: vec![
            node("fw", Some(fw1), vec![fw2.clone()]),
            node("web", Some(old), vec![next]),
        ],
    };
    let (_, solved) = super::solve_with_inventory(&inventory, &intent("fw", Some(fw2.id), true));
    let solved = solved.unwrap();
    assert!(!solved.nodes[1].current.as_ref().unwrap().provides[1].enabled);
}
