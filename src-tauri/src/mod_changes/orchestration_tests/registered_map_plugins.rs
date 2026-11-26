//! An external DLL's recorded map ownership controls its lifecycle, not its filename.
use super::*;
use crate::level_sets;
use std::{path::PathBuf, sync::atomic::Ordering};

#[path = "registered_map_plugins/custody.rs"]
mod custody;
#[path = "registered_map_plugins/recovery.rs"]
mod recovery;

const MAP: &str = "Young";
const MAP_ID: &str = "map:Young";
const MAP_PATH: &str = "BepInEx/plugins/DIYLevel/levels/Young";
const BUNDLED: &str = "BepInEx/plugins/YoungLevelRuntimeLoader/YoungLevels.dll";
const STANDALONE: &str = "BepInEx/plugins/Standalone/YoungLevels.dll";
const GUID: &str = "fixture.young-levels";
const AUTHOR: &str = "Integration";
const MAP_CONTENT: &[u8] = b"preserve map content";

fn write_map(path: &Path, name: &str) {
    write(
        &path.join("info"),
        &live_game_info::bundle(name, AUTHOR, "0.9.2"),
    );
    write(&path.join("user-level"), MAP_CONTENT);
}

fn setup() -> Fixture {
    let framework = mods::plugin_target::DIYLEVEL_GUID;
    let fixture = fixture(vec![repository("DIYLevel", framework, &["0.11.1"], vec![])]);
    install(&fixture, "DIYLevel", framework, "0.11.1", true);
    write_map(&fixture.game.join(MAP_PATH), MAP);
    write(
        &fixture.game.join(BUNDLED),
        &mods::plugin_blob(GUID, "YoungLevelsRuntime", "1.7.6"),
    );
    write(
        &fixture.game.join(STANDALONE),
        &mods::plugin_blob("com.example.standalone", "YoungLevelsRuntime", "1.7.6"),
    );
    register_plugins(&fixture, &[(MAP, BUNDLED)]);
    fixture
}

fn register_plugins(fixture: &Fixture, plugins: &[(&str, &str)]) {
    let entries = plugins.iter().map(|(name, dll)| {
        serde_json::json!({"dll": dll, "owner": {"name": name, "author": AUTHOR}})
    }).collect::<Vec<_>>();
    write(
        &registry_path(fixture),
        &serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "entries": entries,
        }))
        .unwrap(),
    );
}

fn registry_path(fixture: &Fixture) -> PathBuf {
    fixture
        .state
        .config
        .manifests_dir()
        .join(level_sets::LEVEL_SET_PLUGIN_REGISTRY)
}

fn toggle(enabled: bool) -> PlanRequest {
    PlanRequest {
        roots: vec![RootRequest {
            intent: RootIntent {
                node_id: MAP_ID.into(),
                candidate_id: None,
                enabled,
                member_key: None,
            },
            asset_name: None,
        }],
        ..Default::default()
    }
}

fn bundled_id() -> String {
    format!("map-dll:{}", inventory::key(BUNDLED))
}

fn assert_bundle_actions(plan: &ChangePlan, enabled: bool) {
    assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    let kind = if enabled {
        ActionKind::Enable
    } else {
        ActionKind::Disable
    };
    for node in [MAP_ID.to_string(), bundled_id()] {
        assert!(
            plan.result
                .actions
                .iter()
                .any(|action| action.node_id == node && action.kind == kind),
            "missing {kind:?} action for {node}: {:?}",
            plan.result,
        );
    }
    assert_eq!(plan.result.actions.len(), 2, "{:?}", plan.result);
    let [dll] = plan.bundled_dlls[MAP_ID].as_slice() else {
        panic!(
            "expected one displayed bundled DLL: {:?}",
            plan.bundled_dlls
        );
    };
    assert_eq!(dll.parent_node_id, MAP_ID);
    assert_eq!(dll.node_id, bundled_id());
    assert_eq!(dll.dll_file, BUNDLED);
    assert_eq!(dll.action_kind, kind);
}

async fn prepare(fixture: &Fixture, plan: &ChangePlan) -> ChangePlan {
    let outcome = preparation::prepare(fixture.state.clone(), plan.plan_id.clone(), |_| {}).await;
    assert_eq!(outcome.error, None);
    let prepared = outcome.plan.unwrap();
    assert!(prepared.prepared);
    prepared
}

#[tokio::test]
async fn registered_external_dll_and_map_switch_together_without_touching_standalone_copy() {
    let fixture = setup();
    let active_map = fixture.game.join(MAP_PATH);
    let inactive_map = level_sets::disabled_root(&fixture.game).join(MAP);
    let active_dll = fixture.game.join(BUNDLED);
    let inactive_dll = fixture.game.join(mods::disabled_dll_file(BUNDLED));
    let original_dll = fs::read(&active_dll).unwrap();
    let original_registry = fs::read(registry_path(&fixture)).unwrap();
    let standalone_dll = fixture.game.join(STANDALONE);
    let standalone_bytes = fs::read(&standalone_dll).unwrap();

    for enabled in [false, true] {
        let plan = planning::create(&fixture.state, toggle(enabled)).unwrap();
        assert_bundle_actions(&plan, enabled);
        let (before_map, after_map, before_dll, after_dll) = if enabled {
            (&inactive_map, &active_map, &inactive_dll, &active_dll)
        } else {
            (&active_map, &inactive_map, &active_dll, &inactive_dll)
        };
        let prepared = prepare(&fixture, &plan).await;
        assert_bundle_actions(&prepared, enabled);
        assert!(before_map.is_dir() && !after_map.exists());
        assert_eq!(fs::read(before_dll).unwrap(), original_dll);
        assert!(!after_dll.exists());

        let mut session = take(&fixture, &prepared);
        super::super::execution::commit(&fixture.state, &mut session, |_| {}).unwrap();
        assert!(!before_map.exists() && !before_dll.exists());
        assert_eq!(fs::read(after_map.join("user-level")).unwrap(), MAP_CONTENT);
        assert_eq!(fs::read(after_dll).unwrap(), original_dll);
        assert_eq!(fs::read(&standalone_dll).unwrap(), standalone_bytes);
        assert_eq!(
            fs::read(registry_path(&fixture)).unwrap(),
            original_registry
        );
        let fresh = planning::evaluate(&fixture.state, vec![]).unwrap();
        for id in [MAP_ID, &bundled_id()] {
            assert_eq!(
                fresh
                    .nodes
                    .iter()
                    .find(|node| node.node_id == id)
                    .unwrap()
                    .enabled,
                enabled
            );
        }
    }
}

fn snapshot(fixture: &Fixture) -> [Fingerprint; 3] {
    [
        fixture.game.clone(),
        fixture.state.config.manifests_dir(),
        fixture.state.config.mod_change_transactions_dir(),
    ]
    .map(|path| Fingerprint::capture(&path).unwrap())
}

#[tokio::test]
async fn explicitly_disabling_an_already_disabled_map_only_corrects_its_own_dll() {
    const OTHER_MAP: &str = "Other";
    const OTHER_DLL: &str = "BepInEx/plugins/OtherLevelRuntimeLoader/OtherLevels.dll";
    let fixture = setup();
    let disabled_root = level_sets::disabled_root(&fixture.game);
    let disabled_map = disabled_root.join(MAP);
    fs::create_dir_all(&disabled_root).unwrap();
    fs::rename(fixture.game.join(MAP_PATH), &disabled_map).unwrap();
    write_map(&disabled_root.join(OTHER_MAP), OTHER_MAP);
    write(
        &fixture.game.join(OTHER_DLL),
        &mods::plugin_blob("fixture.other-levels", "OtherLevelsRuntime", "1.0.0"),
    );
    register_plugins(&fixture, &[(MAP, BUNDLED), (OTHER_MAP, OTHER_DLL)]);
    let maps_before = Fingerprint::capture(&disabled_root).unwrap();
    let original_dll = fs::read(fixture.game.join(BUNDLED)).unwrap();
    let other_dll = fs::read(fixture.game.join(OTHER_DLL)).unwrap();

    let plan = planning::create(&fixture.state, toggle(false)).unwrap();
    let prepared = prepare(&fixture, &plan).await;
    assert_eq!(prepared.result.actions.len(), 1, "{:?}", prepared.result);
    assert_eq!(prepared.result.actions[0].node_id, bundled_id());
    assert_eq!(prepared.result.actions[0].kind, ActionKind::Disable);
    super::super::execution::commit(&fixture.state, &mut take(&fixture, &prepared), |_| {})
        .unwrap();

    assert_eq!(Fingerprint::capture(&disabled_root).unwrap(), maps_before);
    assert!(!fixture.game.join(MAP_PATH).exists());
    assert!(!fixture.game.join(BUNDLED).exists());
    assert_eq!(
        fs::read(fixture.game.join(mods::disabled_dll_file(BUNDLED))).unwrap(),
        original_dll
    );
    assert_eq!(fs::read(fixture.game.join(OTHER_DLL)).unwrap(), other_dll);
    assert!(
        !fixture
            .game
            .join(mods::disabled_dll_file(OTHER_DLL))
            .exists()
    );
}

#[tokio::test]
async fn registered_external_dll_cannot_switch_while_the_game_is_running() {
    let fixture = setup();
    let before = snapshot(&fixture);
    fixture
        .state
        .game_running_for_test
        .store(true, Ordering::Relaxed);
    assert_eq!(
        planning::create(&fixture.state, toggle(false)).unwrap_err(),
        "game.running"
    );
    assert_eq!(snapshot(&fixture), before);

    fixture
        .state
        .game_running_for_test
        .store(false, Ordering::Relaxed);
    let plan = planning::create(&fixture.state, toggle(false)).unwrap();
    assert_bundle_actions(&plan, false);
    fixture
        .state
        .game_running_for_test
        .store(true, Ordering::Relaxed);
    let blocked = preparation::prepare(fixture.state.clone(), plan.plan_id, |_| {}).await;
    assert_eq!(blocked.error.as_deref(), Some("game.running"));
    assert!(blocked.plan.is_none());
    assert_eq!(snapshot(&fixture), before);

    fixture
        .state
        .game_running_for_test
        .store(false, Ordering::Relaxed);
    let plan = planning::create(&fixture.state, toggle(false)).unwrap();
    let prepared = prepare(&fixture, &plan).await;
    fixture
        .state
        .game_running_for_test
        .store(true, Ordering::Relaxed);
    let error =
        super::super::execution::commit(&fixture.state, &mut take(&fixture, &prepared), |_| {})
            .unwrap_err();
    assert_eq!(error, "game.running");
    assert_eq!(snapshot(&fixture), before);
}

#[tokio::test]
async fn registered_external_dll_and_map_roll_back_together_after_live_changes() {
    let fixture = setup();
    let plan = planning::create(&fixture.state, toggle(false)).unwrap();
    let prepared = prepare(&fixture, &plan).await;
    let before = snapshot(&fixture);
    let changes = take(&fixture, &prepared)
        .mutations
        .unwrap()
        .into_iter()
        .map(|owned| owned.mutation)
        .collect();
    let journal = fixture.state.config.mod_change_transactions_dir();
    let roots = TransactionRoots {
        game_root: fixture.game.clone(),
        data_root: fixture
            .state
            .config
            .manifests_dir()
            .parent()
            .unwrap()
            .to_path_buf(),
    };
    let inactive_map = level_sets::disabled_root(&fixture.game).join(MAP);
    let inactive_dll = fixture.game.join(mods::disabled_dll_file(BUNDLED));
    let original_bytes = fs::read(fixture.game.join(BUNDLED)).unwrap();
    let error = super::prepare(&journal, &roots, changes)
        .unwrap()
        .apply_checked(
            || false,
            || Ok(()),
            || {
                assert!(!fixture.game.join(MAP_PATH).exists());
                assert!(!fixture.game.join(BUNDLED).exists());
                assert_eq!(
                    fs::read(inactive_map.join("user-level")).unwrap(),
                    MAP_CONTENT
                );
                assert_eq!(fs::read(&inactive_dll).unwrap(), original_bytes);
                Err("injected pre-commit failure".into())
            },
            |_| {},
        )
        .unwrap_err();
    assert_eq!(error, "injected pre-commit failure");
    assert_eq!(Fingerprint::capture(&fixture.game).unwrap(), before[0]);
    assert_eq!(
        Fingerprint::capture(&fixture.state.config.manifests_dir()).unwrap(),
        before[1]
    );
    assert!(!has_pending(&journal).unwrap());
}

#[tokio::test]
async fn registered_external_dll_with_both_filenames_blocks_the_whole_toggle() {
    let fixture = setup();
    write(
        &fixture.game.join(mods::disabled_dll_file(BUNDLED)),
        b"occupied by another copy",
    );
    assert_toggle_refused(&fixture).await;
}

#[tokio::test]
async fn registered_external_dll_with_multiple_matching_maps_blocks_the_toggle() {
    let fixture = setup();
    write_map(
        &fixture
            .game
            .join("BepInEx/plugins/DIYLevel/levels/YoungDuplicate"),
        MAP,
    );
    assert_toggle_refused(&fixture).await;
}

#[tokio::test]
async fn registered_external_dll_with_a_mod_manifest_claim_blocks_the_toggle() {
    let fixture = setup();
    let manifest = archive::new_manifest(
        "Standalone-claim",
        "v1.7.6",
        BTreeMap::new(),
        BTreeMap::new(),
        vec![BUNDLED.into()],
        vec![],
    );
    archive::write_manifest(&fixture.state.config.manifests_dir(), &manifest).unwrap();
    assert_toggle_refused(&fixture).await;
}

#[tokio::test]
async fn registered_external_dll_cannot_be_switched_as_an_independent_root() {
    let fixture = setup();
    let mut request = toggle(false);
    request.roots[0].intent.node_id = bundled_id();
    request.roots[0].intent.candidate_id = Some(format!("installed:{}", bundled_id()));
    assert_request_refused(&fixture, request).await;
}

async fn assert_toggle_refused(fixture: &Fixture) {
    assert_request_refused(fixture, toggle(false)).await;
}

async fn assert_request_refused(fixture: &Fixture, request: PlanRequest) {
    let before = snapshot(fixture);
    let plan = planning::create(&fixture.state, request);
    if let Ok(plan) = plan {
        if plan.result.status == SolveStatus::Ready {
            let outcome = preparation::prepare(fixture.state.clone(), plan.plan_id, |_| {}).await;
            assert!(
                outcome.error.is_some(),
                "unsafe toggle obtained approval: {:?}",
                outcome.plan
            );
            assert!(outcome.plan.is_none());
        } else {
            assert!(!plan.result.blockers.is_empty(), "{:?}", plan.result);
        }
    }
    assert_eq!(snapshot(fixture), before);
}
