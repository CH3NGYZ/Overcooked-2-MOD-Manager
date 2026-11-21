//! Activate or repair the installed dependency cycle declared for DIYLevel 0.10+.
use super::*;

const FRAMEWORK: &str = "DIYLevel";
const FRAMEWORK_NODE: &str = "mod:DIYLevel";
const FW_GUID: &str = mods::plugin_target::DIYLEVEL_GUID;
const CORE_DLL: &str = "BepInEx/plugins/OC2DIYLevel/OC2DIYLevel.dll";
const STUB_DLL: &str = "BepInEx/plugins/OC2DIYLevel/LevelEditorStub.dll";
const FAST: &str = "FastInit";
const FAST_GUID: &str = "oc2.diylevel.fastinit";
const FAST_NODE: &str = "mod:FastInit";
const MESSAGE_FIX: &str = "fixture.message-fix";
const EXPERIENCE: &str = "fixture.experience-optimization";

fn repair(id: &str) -> PlanRequest {
    PlanRequest {
        repair_dependencies_of: Some(id.into()),
        ..Default::default()
    }
}

fn repositories(fast_floor: &str) -> Vec<ModRepositoryConfig> {
    // Pin the reported rules in the fixture: the parent can embed older sources.
    let mut framework = repository(FRAMEWORK, FW_GUID, &["0.10", "0.9"], vec![]);
    framework.release_relations = vec![ReleaseRelationOverride {
        version: ">=0.10.0,<=0.11.1".into(),
        relations: vec![
            requires(MESSAGE_FIX, ">=0.2.3"),
            requires(EXPERIENCE, ">=1.14.2"),
            requires(FAST_GUID, fast_floor),
            RepositoryRelation {
                kind: RelationKind::Conflicts,
                ..requires("yang.oc2.diylevel.fastinit", "==1.3.0")
            },
        ],
    }];
    vec![
        framework,
        repository(
            FAST,
            FAST_GUID,
            &["1.9.6"],
            vec![requires(FW_GUID, ">=0.10.0")],
        ),
        repository("MessageFix", MESSAGE_FIX, &[], vec![requires(FW_GUID, "")]),
        repository("Experience", EXPERIENCE, &[], vec![requires(FW_GUID, "")]),
    ]
}

fn disabled_package(fast_enabled: bool, fast_floor: &str) -> (Fixture, [String; 5]) {
    let f = fixture(repositories(fast_floor));
    let disabled_core = mods::disabled_dll_file(CORE_DLL);
    let disabled_stub = mods::disabled_dll_file(STUB_DLL);
    write(
        &f.game.join(&disabled_core),
        &mods::plugin_blob(FW_GUID, "OC2DIYLevel", "0.10"),
    );
    // The 0.10 ZIP has a GUID-less library alongside the main DLL.
    write(&f.game.join(&disabled_stub), b"fixture LevelEditorStub");
    let config = f.state.config.load_or_default();
    let repository = &config.repositories[0];
    assert_eq!(repository.id, FRAMEWORK);
    let mut manifest = archive::new_manifest(
        FRAMEWORK,
        "v0.10",
        BTreeMap::new(),
        BTreeMap::new(),
        vec![disabled_core, disabled_stub],
        mods::effective_relations(repository, Some("v0.10")),
    );
    manifest.repository_binding = ModRepositoryBinding::from_release(repository, "v0.10");
    archive::write_manifest(&f.state.config.manifests_dir(), &manifest).unwrap();
    let fast = install(&f, FAST, FAST_GUID, "1.9.6", fast_enabled);
    let message = install(&f, "MessageFix", MESSAGE_FIX, "0.2.3", false);
    let experience = install(&f, "Experience", EXPERIENCE, "1.14.2", false);
    (
        f,
        [CORE_DLL.into(), STUB_DLL.into(), fast, message, experience],
    )
}

fn apply_fixture(f: &Fixture, plan: &ChangePlan) {
    let mut session = take(f, plan);
    verify_selected(f, &mut session);
    preparation::finish(&f.state, &mut session).unwrap();
    session.plan.prepared = true;
    super::super::execution::commit(&f.state, &mut session, |_| {}).unwrap();
}

#[test]
fn repairing_fastinit_upgrades_diylevel_and_does_not_change_fastinit() {
    let f = fixture(repositories(">=1.9.6"));
    let fw = install(&f, FRAMEWORK, FW_GUID, "0.9", true);
    let fast = install(&f, FAST, FAST_GUID, "1.9.6", true);
    install(&f, "MessageFix", MESSAGE_FIX, "0.2.3", true);
    install(&f, "Experience", EXPERIENCE, "1.14.2", true);
    let fast_dll = f.game.join(fast);
    let fast_before = Fingerprint::capture(&fast_dll).unwrap();
    let manifest_dir = f.state.config.manifests_dir();
    let fast_manifest = Fingerprint::capture(&manifest_dir.join(format!("{FAST}.toml"))).unwrap();
    let before = Fingerprint::capture(&f.game).unwrap();
    let plan = planning::create(&f.state, repair(FAST_NODE)).unwrap();
    assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    let [action] = plan.result.actions.as_slice() else {
        panic!(
            "expected just the DIYLevel upgrade: {:?}",
            plan.result.actions
        );
    };
    assert_eq!(action.node_id, FRAMEWORK_NODE);
    assert_eq!(action.kind, ActionKind::Upgrade);
    assert_eq!(action.from_tag.as_deref(), Some("v0.9"));
    assert_eq!(action.to_tag.as_deref(), Some("v0.10"));
    assert_eq!(Fingerprint::capture(&f.game).unwrap(), before);
    apply_fixture(&f, &plan);
    assert_eq!(Fingerprint::capture(&fast_dll).unwrap(), fast_before);
    assert_eq!(
        Fingerprint::capture(&manifest_dir.join(format!("{FAST}.toml"))).unwrap(),
        fast_manifest
    );
    assert_eq!(
        archive::read_manifest(&manifest_dir, FRAMEWORK)
            .unwrap()
            .release_tag,
        "v0.10"
    );
    assert_eq!(
        fs::read(f.game.join(fw)).unwrap(),
        mods::plugin_blob(FW_GUID, FRAMEWORK, "0.10")
    );
}

#[test]
fn disabled_diylevel_rejects_standalone_repair_but_enables_the_entire_cycle() {
    let (f, paths) = disabled_package(false, ">=1.9.6");
    let manifests = f.state.config.manifests_dir();
    let before = Fingerprint::capture(&f.game).unwrap();
    let before_manifests = Fingerprint::capture(&manifests).unwrap();
    let trusted = inventory::build(&f.state).unwrap();
    assert!(trusted.atomic_mod_package(FRAMEWORK_NODE));
    assert_eq!(trusted.files[FRAMEWORK_NODE].summary.members.len(), 2);
    let pure = super::super::solve::solve(
        &trusted.inventory,
        &ChangeRequest {
            repair_dependencies_of: Some(FRAMEWORK_NODE.into()),
            ..Default::default()
        },
    );
    assert_eq!(pure.status, SolveStatus::Incomplete);
    assert_eq!(pure.blockers[0].code, "root_missing");
    assert!(pure.actions.is_empty());
    let evaluated = pure
        .evaluations
        .iter()
        .find(|node| node.node_id == FRAMEWORK_NODE)
        .unwrap();
    assert!(!evaluated.enabled && evaluated.runtime.is_empty());
    assert_eq!(
        evaluated
            .readiness
            .iter()
            .filter(|edge| edge.state == EvaluationState::Disabled)
            .count(),
        3
    );
    assert_eq!(
        planning::create(&f.state, repair(FRAMEWORK_NODE)).unwrap_err(),
        "modChanges.targetChanged"
    );
    assert!(
        !manifests
            .parent()
            .unwrap()
            .join("mod-change-staging")
            .exists()
    );
    assert!(f.state.mod_changes.lock().unwrap().plans.is_empty());
    assert_eq!(Fingerprint::capture(&f.game).unwrap(), before);
    assert_eq!(Fingerprint::capture(&manifests).unwrap(), before_manifests);

    let toggle = super::unbound_toggle::toggle(&f, FRAMEWORK_NODE, None, true);
    let plan = planning::create(&f.state, toggle).unwrap();
    let actions = assert_actions(
        &plan,
        ActionKind::Enable,
        &[
            FRAMEWORK_NODE,
            "mod:Experience",
            FAST_NODE,
            "mod:MessageFix",
        ],
    );
    assert_eq!(actions[0].reason_details[0].kind, ReasonKind::Requested);
    for (action, requirement) in actions[1..].iter().zip([">=1.14.2", ">=1.9.6", ">=0.2.3"]) {
        let reason = &action.reason_details[0];
        assert_eq!(reason.kind, ReasonKind::RequiredBy);
        assert_eq!(reason.related_node_ids, [FRAMEWORK_NODE]);
        assert_eq!(reason.requirement.as_deref(), Some(requirement));
    }
    let summary = plan
        .nodes
        .iter()
        .find(|node| node.node_id == FRAMEWORK_NODE)
        .unwrap();
    assert_eq!(summary.members.len(), 2);
    assert!(summary.members.iter().all(|member| member.enabled));
    assert_eq!(Fingerprint::capture(&f.game).unwrap(), before);
    apply_fixture(&f, &plan);
    assert_switch_files(&f, &paths, true);
    assert_eq!(
        archive::read_manifest(&manifests, FRAMEWORK)
            .unwrap()
            .release_tag,
        "v0.10"
    );
}

#[test]
fn active_fastinit_can_repair_disabled_diylevel_and_follows_its_disable() {
    let (f, paths) = disabled_package(true, ">=1.9.6");
    let before = planning::evaluate(&f.state, vec![]).unwrap();
    let owner = before
        .evaluations
        .iter()
        .find(|node| node.node_id == FAST_NODE)
        .unwrap();
    assert!(owner.enabled);
    assert!(
        owner.runtime.iter().any(|edge| {
            edge.relation.guid == FW_GUID && edge.state == EvaluationState::Disabled
        })
    );
    let plan = planning::create(&f.state, repair(FAST_NODE)).unwrap();
    assert_eq!(plan.roots[0].node_id, FAST_NODE);
    let actions = assert_actions(
        &plan,
        ActionKind::Enable,
        &[FRAMEWORK_NODE, "mod:Experience", "mod:MessageFix"],
    );
    let reason = &actions[0].reason_details[0];
    assert_eq!(reason.kind, ReasonKind::RequiredBy);
    assert_eq!(reason.related_node_ids, [FAST_NODE]);
    assert_eq!(reason.requirement.as_deref(), Some(">=0.10.0"));
    assert!(!reason.condition_on_related);
    let fast_path = f.game.join(&paths[2]);
    let original_fast = Fingerprint::capture(&fast_path).unwrap();
    apply_fixture(&f, &plan);
    assert_eq!(Fingerprint::capture(&fast_path).unwrap(), original_fast);
    assert_switch_files(&f, &paths, true);

    let toggle = super::unbound_toggle::toggle(&f, FRAMEWORK_NODE, None, false);
    let disable = planning::create(&f.state, toggle).unwrap();
    let actions = assert_actions(
        &disable,
        ActionKind::Disable,
        &[
            FRAMEWORK_NODE,
            "mod:Experience",
            FAST_NODE,
            "mod:MessageFix",
        ],
    );
    let reason = &actions[2].reason_details[0];
    assert_eq!(reason.kind, ReasonKind::DependencyDisabled);
    assert_eq!(reason.related_node_ids, [FRAMEWORK_NODE]);
    apply_fixture(&f, &disable);
    assert_switch_files(&f, &paths, false);
}

#[test]
fn explicit_cyclic_activation_reports_a_real_version_mismatch_without_writing() {
    let (f, _) = disabled_package(false, ">=1.9.7");
    let toggle = super::unbound_toggle::toggle(&f, FRAMEWORK_NODE, None, true);
    let before = Fingerprint::capture(&f.game).unwrap();
    let plan = planning::create(&f.state, toggle).unwrap();
    assert_eq!(
        plan.result.status,
        SolveStatus::Unsatisfiable,
        "{:?}",
        plan.result
    );
    assert!(plan.result.actions.is_empty());
    assert!(
        plan.result
            .blockers
            .iter()
            .flat_map(|blocker| &blocker.traces)
            .flatten()
            .any(|step| {
                step.owner_id == FRAMEWORK_NODE
                    && step.state == EvaluationState::VersionMismatch
                    && step.relation.as_ref().is_some_and(|relation| {
                        relation.guid == FAST_GUID && relation.version == ">=1.9.7"
                    })
            }),
        "{:?}",
        plan.result.blockers
    );
    assert_eq!(Fingerprint::capture(&f.game).unwrap(), before);
}

fn assert_actions<'a>(plan: &'a ChangePlan, kind: ActionKind, ids: &[&str]) -> &'a [ChangeAction] {
    assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    let actions = &plan.result.actions;
    assert_eq!(
        actions
            .iter()
            .map(|action| action.node_id.as_str())
            .collect::<Vec<_>>(),
        ids
    );
    assert!(
        actions
            .iter()
            .all(|action| action.kind == kind && action.member_key.is_none())
    );
    actions
}

fn assert_switch_files(f: &Fixture, paths: &[String], enabled: bool) {
    for path in paths {
        assert_eq!(f.game.join(path).is_file(), enabled, "{path}");
        assert_ne!(
            f.game.join(mods::disabled_dll_file(path)).is_file(),
            enabled,
            "{path}"
        );
    }
}

#[test]
fn an_enabled_mod_install_is_only_one_action() {
    let f = fixture(vec![]);
    let id = "NewDependency";
    let guid = "fixture.new-dependency";
    let mut config = f.state.config.load_or_default();
    config
        .repositories
        .push(repository(id, guid, &["1.0"], vec![]));
    f.state.config.save(&config).unwrap();
    let plan = planning::create(&f.state, request(id, Some("v1.0"), true)).unwrap();
    assert_eq!(plan.result.status, SolveStatus::Ready);
    assert_eq!(plan.result.actions.len(), 1, "{:?}", plan.result.actions);
    assert_eq!(plan.result.actions[0].kind, ActionKind::Install);
    apply_fixture(&f, &plan);
    assert!(
        f.game
            .join(format!("BepInEx/plugins/{id}/{id}.dll"))
            .is_file()
    );
}
