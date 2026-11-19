//! Real loose DLLs and map directories exercise the same approval and write path as the UI.
use super::*;
use crate::{asset_bundle::LevelSetMetadata, level_sets};

const LOOSE: &str = "BepInEx/plugins/Loose/Loose.dll";
const LOOSE_GUID: &str = "test-fixture.loose.id";
const PEER_GUID: &str = "test-fixture.peer.id";
const EMBEDDED_GUID: &str = "test-fixture.embedded.id";
const FRAMEWORK_GUID: &str = mods::plugin_target::DIYLEVEL_GUID;
const MAP: &str = "map:Kitchen";

fn loose_id() -> String {
    format!("dll:{}", inventory::key(LOOSE))
}

fn loose(f: &Fixture, enabled: bool) {
    let path = if enabled {
        LOOSE.into()
    } else {
        mods::disabled_dll_file(LOOSE)
    };
    write(
        &f.game.join(path),
        &mods::plugin_blob(LOOSE_GUID, "Loose", "1.0.0"),
    );
}

fn toggle(f: &Fixture, node_id: &str, member_key: Option<String>, enabled: bool) -> PlanRequest {
    let config = f.state.config.load_or_default();
    let scanned =
        mods::scan_mods_with_manifests(&f.game, &config, Some(&f.state.config.manifests_dir()));
    let mut snapshots = f.state.local_mod_snapshot.lock().unwrap();
    let generation = snapshots.begin(Some(&f.game));
    snapshots
        .publish(generation, Some(&f.game), scanned)
        .unwrap();
    drop(snapshots);
    let evaluation = planning::evaluate(&f.state, vec![]).unwrap();
    assert!(evaluation.nodes.iter().any(|node| node.node_id == node_id));
    PlanRequest {
        roots: vec![RootRequest {
            intent: RootIntent {
                node_id: node_id.into(),
                candidate_id: None,
                member_key,
                enabled,
            },
            asset_name: None,
        }],
        toggle_scope: Some(ToggleScope {
            generation,
            revision: evaluation.revision,
        }),
        ..Default::default()
    }
}

fn plan_actions(plan: &ChangePlan) -> Vec<(&str, ActionKind)> {
    assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    plan.result
        .actions
        .iter()
        .map(|action| (action.node_id.as_str(), action.kind.clone()))
        .collect()
}

fn prepared(f: &Fixture, plan: &ChangePlan) -> PlanSession {
    let mut session = take(f, plan);
    let initial = preparation::approval_signature(&session).unwrap();
    planning::resolve(&f.state, &mut session).unwrap();
    preparation::finish(&f.state, &mut session).unwrap();
    assert_eq!(preparation::approval_signature(&session).unwrap(), initial);
    session.plan.prepared = true;
    session
}

fn commit(f: &Fixture, session: &mut PlanSession) {
    super::super::execution::commit(&f.state, session, |_| {}).unwrap();
}

fn map(f: &Fixture, enabled: bool, relations: Vec<RepositoryRelation>) -> std::path::PathBuf {
    let root = if enabled {
        f.game.join("BepInEx/plugins/DIYLevel/levels")
    } else {
        level_sets::disabled_root(&f.game)
    };
    let path = root.join("Kitchen");
    let info = path.join("info");
    write(&info, b"local map fixture");
    let metadata = LevelSetMetadata {
        name: "Kitchen".into(),
        author: "Fixture".into(),
        version: "1.0.0".into(),
        ..Default::default()
    };
    level_sets::seed_info_metadata(&info, metadata.clone());
    if !relations.is_empty() {
        archive::write_map_relation_snapshots(
            &f.state.config.manifests_dir(),
            &MapRelationSnapshots {
                schema_version: 1,
                entries: BTreeMap::from([(
                    "historical-map".into(),
                    MapRelationSnapshot {
                        repository_id: "historical-map".into(),
                        level_set_name: metadata.name,
                        level_set_name_zh: String::new(),
                        author: metadata.author,
                        group_i18n: BTreeMap::new(),
                        group: String::new(),
                        relations,
                        release_relations: vec![],
                        installed_mapset_version: metadata.version,
                    },
                )]),
            },
        )
        .unwrap();
    }
    path
}

#[test]
fn unbound_mod_switch_commits_without_claiming_its_metadata_is_known() {
    let f = fixture(vec![]);
    loose(&f, false);
    let id = loose_id();
    let intent = toggle(&f, &id, Some(inventory::key(LOOSE)), true);
    let plan = planning::create(&f.state, intent).unwrap();
    assert_eq!(plan_actions(&plan), [(id.as_str(), ActionKind::Enable)]);
    assert!(
        !plan
            .result
            .evaluations
            .iter()
            .find(|evaluation| evaluation.node_id == id)
            .unwrap()
            .metadata_known
    );
    let mut session = prepared(&f, &plan);
    assert!(!f.game.join(LOOSE).is_file());
    commit(&f, &mut session);
    assert!(f.game.join(LOOSE).is_file());

    let disable = toggle(&f, &id, Some(inventory::key(LOOSE)), false);
    let plan = planning::create(&f.state, disable).unwrap();
    assert_eq!(plan_actions(&plan), [(id.as_str(), ActionKind::Disable)]);
    commit(&f, &mut prepared(&f, &plan));
    assert!(f.game.join(mods::disabled_dll_file(LOOSE)).is_file());
}

#[test]
fn known_reverse_conflict_disables_only_its_owner_after_approval() {
    let conflicts = RepositoryRelation {
        kind: RelationKind::Conflicts,
        ..requires(LOOSE_GUID, ">=1.0.0")
    };
    let f = fixture(vec![repository(
        "Peer",
        PEER_GUID,
        &["1.0.0"],
        vec![conflicts],
    )]);
    let peer = install(&f, "Peer", PEER_GUID, "1.0.0", true);
    loose(&f, false);
    let id = loose_id();
    let plan = planning::create(&f.state, toggle(&f, &id, None, true)).unwrap();
    assert_eq!(
        plan_actions(&plan),
        [
            ("mod:Peer", ActionKind::Disable),
            (id.as_str(), ActionKind::Enable)
        ]
    );
    let mut session = prepared(&f, &plan);
    assert!(f.game.join(&peer).is_file());
    assert!(!f.game.join(LOOSE).is_file());
    commit(&f, &mut session);
    assert!(f.game.join(LOOSE).is_file());
    assert!(f.game.join(mods::disabled_dll_file(&peer)).is_file());
}

#[test]
fn disabling_unbound_provider_includes_a_known_map_dependent() {
    let f = fixture(vec![repository(
        "DIYLevel",
        FRAMEWORK_GUID,
        &["0.10.0"],
        vec![],
    )]);
    install(&f, "DIYLevel", FRAMEWORK_GUID, "0.10.0", true);
    loose(&f, true);
    let active_map = map(&f, true, vec![requires(LOOSE_GUID, ">=1.0.0")]);
    let id = loose_id();
    let plan = planning::create(&f.state, toggle(&f, &id, None, false)).unwrap();
    assert_eq!(
        plan_actions(&plan),
        [
            (id.as_str(), ActionKind::Disable),
            (MAP, ActionKind::Disable)
        ]
    );
    let mut session = prepared(&f, &plan);
    assert!(f.game.join(LOOSE).is_file() && active_map.is_dir());
    commit(&f, &mut session);
    assert!(!active_map.exists());
    assert!(
        level_sets::disabled_root(&f.game)
            .join("Kitchen/info")
            .is_file()
    );
}

#[test]
fn unbound_local_map_preserves_framework_and_bound_map_can_reenable_known_provider() {
    let f = fixture(vec![repository(
        "DIYLevel",
        FRAMEWORK_GUID,
        &["0.10.0", "0.11.0"],
        vec![],
    )]);
    let framework = install(&f, "DIYLevel", FRAMEWORK_GUID, "0.10.0", true);
    let inactive_map = map(&f, false, vec![]);
    let plan = planning::create(&f.state, toggle(&f, MAP, None, true)).unwrap();
    assert_eq!(plan_actions(&plan), [(MAP, ActionKind::Enable)]);
    let mut session = prepared(&f, &plan);
    assert!(inactive_map.is_dir());
    commit(&f, &mut session);
    assert!(!inactive_map.exists());
    assert!(f.game.join(framework).is_file());

    let f = fixture(vec![
        repository("DIYLevel", FRAMEWORK_GUID, &["0.10.0", "0.11.0"], vec![]),
        repository("Loose", LOOSE_GUID, &["1.0.0"], vec![]),
    ]);
    install(&f, "DIYLevel", FRAMEWORK_GUID, "0.10.0", true);
    install(&f, "Loose", LOOSE_GUID, "1.0.0", false);
    map(&f, false, vec![requires(LOOSE_GUID, ">=1.0.0")]);
    let plan = planning::create(&f.state, toggle(&f, MAP, None, true)).unwrap();
    let mut actions = plan_actions(&plan);
    actions.sort_by_key(|action| action.0);
    assert_eq!(
        actions,
        [(MAP, ActionKind::Enable), ("mod:Loose", ActionKind::Enable)]
    );
    let mut session = prepared(&f, &plan);
    assert!(!f.game.join(LOOSE).is_file());
    commit(&f, &mut session);
    assert!(f.game.join(LOOSE).is_file());
}

fn conflicting_member_catalogs() -> Fixture {
    use crate::level_sets::runtime_relation;
    let runtime = runtime_relation("3.10.2").unwrap();
    let f = fixture(vec![
        repository("DIYLevel", FRAMEWORK_GUID, &["0.10.0", "0.11.0"], vec![]),
        repository("WebRuntime", &runtime.guid, &["3.10.2", "4.0.0"], vec![]),
        repository("EmbeddedA", EMBEDDED_GUID, &[], vec![]),
        repository(
            "EmbeddedB",
            EMBEDDED_GUID,
            &[],
            vec![requires(PEER_GUID, "")],
        ),
    ]);
    install(&f, "DIYLevel", FRAMEWORK_GUID, "0.10.0", true);
    f
}

fn write_embedded_plugin(dir: &Path) {
    write(
        &dir.join("Embedded.dll"),
        &mods::plugin_blob(EMBEDDED_GUID, "Embedded", "1.0.0"),
    );
}

#[test]
fn unbound_map_switch_ignores_own_runtime_requirement_but_still_displays_it() {
    let f = conflicting_member_catalogs();
    let runtime = level_sets::runtime_relation("3.10.2").unwrap();
    let runtime_file = install(&f, "WebRuntime", &runtime.guid, "3.10.2", false);
    let inactive = map(&f, false, vec![]);
    write_embedded_plugin(&inactive);
    write(
        &inactive.join("AlreadyOff.dll.disabled"),
        &mods::plugin_blob("test-fixture.already-off", "AlreadyOff", "1.0.0"),
    );
    write(&inactive.join("requires.txt"), b"3.10.2\n");

    let request = toggle(&f, MAP, None, true);
    let mut repair = request.clone();
    repair.toggle_scope = None;
    let strict = planning::create(&f.state, repair).unwrap();
    assert_eq!(strict.result.status, SolveStatus::Incomplete);
    assert_eq!(
        strict.result.blockers[0].code,
        "candidate_relations_unknown"
    );

    let plan = planning::create(&f.state, request).unwrap();
    assert_eq!(plan_actions(&plan), [(MAP, ActionKind::Enable)]);
    let wire = serde_json::to_value(&plan).unwrap();
    assert!(wire.get("previous_enabled_members").is_none());
    let members = &plan
        .nodes
        .iter()
        .find(|node| node.node_id == MAP)
        .unwrap()
        .members;
    assert_eq!(members.len(), 2);
    assert!(members.iter().all(|member| member.enabled));
    let evaluation = plan
        .result
        .evaluations
        .iter()
        .find(|e| e.node_id == MAP)
        .unwrap();
    assert!(!evaluation.metadata_known);
    assert!(evaluation.runtime.iter().any(|edge| {
        edge.relation.guid == runtime.guid && edge.state == EvaluationState::Disabled
    }));
    let mut session = prepared(&f, &plan);
    assert!(!f.game.join(&runtime_file).exists() && inactive.is_dir());
    commit(&f, &mut session);
    assert!(
        f.game
            .join(mods::disabled_dll_file(&runtime_file))
            .is_file()
    );
    assert!(
        f.game
            .join("BepInEx/plugins/DIYLevel/levels/Kitchen/Embedded.dll")
            .is_file()
    );
    assert!(
        f.game
            .join("BepInEx/plugins/DIYLevel/levels/Kitchen/AlreadyOff.dll")
            .is_file()
    );
    assert!(!inactive.exists());
}

#[test]
fn map_disable_preserves_only_its_previously_enabled_members_in_the_plan() {
    let f = fixture(vec![repository(
        "DIYLevel",
        FRAMEWORK_GUID,
        &["0.10.0"],
        vec![],
    )]);
    install(&f, "DIYLevel", FRAMEWORK_GUID, "0.10.0", true);
    let active = map(&f, true, vec![]);
    write_embedded_plugin(&active);
    write(
        &active.join("AlreadyOff.dll.disabled"),
        &mods::plugin_blob("test-fixture.already-off", "AlreadyOff", "1.0.0"),
    );
    let request = toggle(&f, MAP, None, false);
    let plan = planning::create(&f.state, request).unwrap();
    assert_eq!(plan_actions(&plan), [(MAP, ActionKind::Disable)]);
    let map_members = &plan
        .nodes
        .iter()
        .find(|node| node.node_id == MAP)
        .unwrap()
        .members;
    assert_eq!(map_members.len(), 2);
    assert!(map_members.iter().all(|member| !member.enabled));
    let embedded = inventory::key("BepInEx/plugins/DIYLevel/levels/Kitchen/Embedded.dll");
    let wire = serde_json::to_value(&plan).unwrap();
    assert_eq!(
        wire["previous_enabled_members"],
        serde_json::json!({ (MAP): [embedded] })
    );
    let mut session = prepared(&f, &plan);
    assert_eq!(
        plan.previous_enabled_members,
        session.plan.previous_enabled_members
    );
    commit(&f, &mut session);
    let inactive = level_sets::disabled_root(&f.game).join("Kitchen");
    assert!(!active.exists());
    assert!(inactive.join("Embedded.dll").is_file());
    assert!(inactive.join("AlreadyOff.dll.disabled").is_file());
}

#[test]
fn other_unbound_map_still_declares_its_runtime_requirement() {
    let runtime = level_sets::runtime_relation("3.10.2").unwrap();
    let f = fixture(vec![repository(
        "DIYLevel",
        FRAMEWORK_GUID,
        &["0.10.0"],
        vec![],
    )]);
    install(&f, "DIYLevel", FRAMEWORK_GUID, "0.10.0", true);
    let runtime_file = "BepInEx/plugins/WebRuntime.dll";
    write(
        &f.game.join(runtime_file),
        &mods::plugin_blob(&runtime.guid, "WebRuntime", "3.10.2"),
    );
    let active = map(&f, true, vec![]);
    write(&active.join("requires.txt"), b"3.10.2\n");
    let id = format!("dll:{}", inventory::key(runtime_file));
    let plan = planning::create(&f.state, toggle(&f, &id, None, false)).unwrap();
    assert_eq!(
        plan_actions(&plan),
        [
            (id.as_str(), ActionKind::Disable),
            (MAP, ActionKind::Disable)
        ]
    );
    assert!(active.is_dir() && f.game.join(runtime_file).is_file());
    let mut session = prepared(&f, &plan);
    commit(&f, &mut session);
    assert!(f.game.join(mods::disabled_dll_file(runtime_file)).is_file());
    assert!(!active.exists());
}

#[test]
fn unbound_map_without_diy_level_destination_cannot_write() {
    let f = fixture(vec![]);
    let inactive = map(&f, false, vec![]);
    let plan = planning::create(&f.state, toggle(&f, MAP, None, true)).unwrap();
    let mut session = take(&f, &plan);
    assert_eq!(
        preparation::finish(&f.state, &mut session).unwrap_err(),
        "framework.diyLevelMissing"
    );
    assert!(inactive.join("info").is_file());
}

#[test]
fn a_matching_historical_map_snapshot_does_not_receive_unbound_metadata_policy() {
    let f = conflicting_member_catalogs();
    let inactive = map(&f, false, vec![requires(FRAMEWORK_GUID, ">=0.10")]);
    write_embedded_plugin(&inactive);
    let plan = planning::create(&f.state, toggle(&f, MAP, None, true)).unwrap();
    assert_eq!(plan.result.status, SolveStatus::Incomplete);
    assert_eq!(plan.result.blockers[0].code, "candidate_relations_unknown");
    assert!(inactive.join("Embedded.dll").is_file());
}

#[test]
fn unbound_forgiveness_is_not_granted_to_repair_or_filtered_repository_identity() {
    let f = fixture(vec![]);
    loose(&f, false);
    let mut request = toggle(&f, &loose_id(), None, true);
    request.toggle_scope = None;
    let repair = planning::create(&f.state, request.clone()).unwrap();
    assert_eq!(repair.result.status, SolveStatus::Incomplete);
    assert_eq!(
        repair.result.blockers[0].code,
        "candidate_relations_unknown"
    );
    request.bulk_toggle = true;
    let bulk = planning::create(&f.state, request).unwrap();
    assert_eq!(
        plan_actions(&bulk),
        [(loose_id().as_str(), ActionKind::Enable)]
    );

    let mut unavailable = repository("Unavailable", LOOSE_GUID, &["1.0.0"], vec![]);
    unavailable.enabled = false;
    let f = fixture(vec![unavailable]);
    loose(&f, false);
    let blocked = planning::create(&f.state, toggle(&f, &loose_id(), None, true)).unwrap();
    assert_eq!(blocked.result.status, SolveStatus::Incomplete);
    assert_eq!(
        blocked.result.blockers[0].code,
        "candidate_relations_unknown"
    );
}
