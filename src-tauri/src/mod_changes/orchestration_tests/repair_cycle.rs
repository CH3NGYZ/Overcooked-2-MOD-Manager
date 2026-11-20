//! Repair FastInit's dependencies with the relations declared for DIYLevel 0.10+.
use super::*;

const FRAMEWORK: &str = "DIYLevel";
const FW_GUID: &str = mods::plugin_target::DIYLEVEL_GUID;
const FAST: &str = "FastInit";
const FAST_GUID: &str = "oc2.diylevel.fastinit";
const MESSAGE_FIX: &str = "com.example.messagefix";
const EXPERIENCE: &str = "com.example.diylevelexperience";
const CONSUMER_GUID: &str = "com.example.consumer";

fn repair() -> PlanRequest {
    PlanRequest {
        repair_dependencies_of: Some(format!("mod:{FAST}")),
        ..Default::default()
    }
}

fn scenario(
    fast_on: bool,
    framework_on: bool,
    addons_on: bool,
    fast_floor: &str,
) -> (Fixture, String, String) {
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
    let f = fixture(vec![
        framework,
        repository(
            FAST,
            FAST_GUID,
            &["1.9.6"],
            vec![requires(FW_GUID, ">=0.10.0")],
        ),
        repository("MessageFix", MESSAGE_FIX, &[], vec![requires(FW_GUID, "")]),
        repository("Experience", EXPERIENCE, &[], vec![requires(FW_GUID, "")]),
    ]);
    let fw = install(&f, FRAMEWORK, FW_GUID, "0.9", framework_on);
    let fast = install(&f, FAST, FAST_GUID, "1.9.6", fast_on);
    install(&f, "MessageFix", MESSAGE_FIX, "0.2.3", addons_on);
    install(&f, "Experience", EXPERIENCE, "1.14.2", addons_on);
    (f, fw, fast)
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
    for (fast_enabled, fw_enabled) in [(true, true), (false, false), (false, true)] {
        let (f, fw, fast) = scenario(fast_enabled, fw_enabled, fast_enabled, ">=1.9.6");
        let fast_dll = f.game.join(if fast_enabled {
            fast.clone()
        } else {
            mods::disabled_dll_file(&fast)
        });
        let fast_before = Fingerprint::capture(&fast_dll).unwrap();
        let manifest_dir = f.state.config.manifests_dir();
        let fast_manifest =
            Fingerprint::capture(&manifest_dir.join(format!("{FAST}.toml"))).unwrap();
        let before = Fingerprint::capture(&f.game).unwrap();
        let plan = planning::create(&f.state, repair()).unwrap();
        assert_eq!(
            plan.result.status,
            SolveStatus::Ready,
            "{fast_enabled}/{fw_enabled}: {:?}",
            plan.result.blockers
        );
        assert!(
            plan.result
                .actions
                .iter()
                .all(|action| action.node_id == "mod:DIYLevel")
        );
        assert!(plan.result.actions.iter().any(|action| {
            action.kind == ActionKind::Upgrade
                && action.from_tag.as_deref() == Some("v0.9")
                && action.to_tag.as_deref() == Some("v0.10")
        }));
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
        let saved = f.game.join(if fast_enabled {
            fw.clone()
        } else {
            mods::disabled_dll_file(&fw)
        });
        assert_eq!(
            fs::read(saved).unwrap(),
            mods::plugin_blob(FW_GUID, FRAMEWORK, "0.10")
        );
        assert_eq!(f.game.join(&fw).exists(), fast_enabled);
        if !fast_enabled {
            let plan = planning::create(&f.state, request(FAST, None, true)).unwrap();
            assert_eq!(
                plan.result.status,
                SolveStatus::Ready,
                "{:?}",
                plan.result.blockers
            );
            assert_eq!(plan.result.actions.len(), 4);
            assert!(
                plan.result
                    .actions
                    .iter()
                    .all(|action| action.kind == ActionKind::Enable)
            );
            apply_fixture(&f, &plan);
            assert!(f.game.join(fw).is_file() && f.game.join(fast).is_file());
            assert!(
                f.game
                    .join("BepInEx/plugins/MessageFix/MessageFix.dll")
                    .is_file()
            );
            assert!(
                f.game
                    .join("BepInEx/plugins/Experience/Experience.dll")
                    .is_file()
            );
        }
    }
}

#[test]
fn cyclic_repair_does_not_ignore_a_real_version_mismatch_or_break_an_active_consumer() {
    for (floor, addons_on, live_consumer) in [
        (">=1.9.7", false, false),
        (">=1.9.6", true, false),
        (">=1.9.6", false, true),
    ] {
        let (f, ..) = scenario(false, true, addons_on, floor);
        if live_consumer {
            let mut config = f.state.config.load_or_default();
            config.repositories.push(repository(
                "LiveConsumer",
                CONSUMER_GUID,
                &[],
                vec![requires(FW_GUID, ">=0.9.0")],
            ));
            f.state.config.save(&config).unwrap();
            install(&f, "LiveConsumer", CONSUMER_GUID, "1.0", true);
            let trusted = inventory::build(&f.state).unwrap();
            assert!(
                trusted.inventory.nodes.iter().any(|node| {
                    node.id == "mod:LiveConsumer"
                        && node.enabled
                        && node.current.as_ref().is_some_and(|candidate| {
                            candidate
                                .relations
                                .iter()
                                .any(|owned| owned.relation.guid == FW_GUID)
                        })
                }),
                "the active consumer must be recognized by the real DLL scanner"
            );
        }
        let before = Fingerprint::capture(&f.game).unwrap();
        let plan = planning::create(&f.state, repair()).unwrap();
        assert!(
            !matches!(plan.result.status, SolveStatus::Ready),
            "{floor}/{addons_on}/{live_consumer}: {:?}",
            plan.result.actions
        );
        assert!(plan.result.actions.is_empty());
        assert_eq!(Fingerprint::capture(&f.game).unwrap(), before);
    }
}

#[test]
fn an_enabled_mod_install_is_only_one_action() {
    let (f, ..) = scenario(false, false, false, ">=1.9.6");
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
