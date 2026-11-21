//! Exercise live-game rules with actual ZIP parsing, solving and transaction writes.
use super::super::{execution, progress::PreparationOutcome};
use super::*;
use std::{io::Write, path::PathBuf, sync::atomic::Ordering};

#[path = "live_game_info.rs"]
mod live_game_info;

const MAP_NAME: &str = "LiveFixture";
const MAP_ID: &str = "map:LiveFixture";
const AUTHOR: &str = "Integration Fixture";
const VERSION: &str = "1.0.0";
const MAP_REPO: &str = "live-map-fixture";
const FRAMEWORK: &str = "DIYLevel";
const EXTRA: &str = "Extra";
const EXTRA_GUID: &str = "fixture.live-extra";
const CONFLICT: &str = "Conflict";
const CONFLICT_GUID: &str = "fixture.live-conflict";
const EXTERNAL_DLL: &str = "BepInEx/plugins/External/External.dll";
const SCENE: &[u8] = b"fixture map data";

fn setup(additional: Vec<ModRepositoryConfig>, map_relations: Vec<RepositoryRelation>) -> Fixture {
    let framework_guid = mods::plugin_target::DIYLEVEL_GUID;
    let f = fixture(
        [repository(FRAMEWORK, framework_guid, &["0.11.1"], vec![])]
            .into_iter()
            .chain(additional)
            .collect(),
    );
    install(&f, FRAMEWORK, framework_guid, "0.11.1", true);
    if !map_relations.is_empty() {
        let mut config = f.state.config.load_or_default();
        let mut source: MapRepositoryConfig = serde_json::from_value(serde_json::json!({
            "id": MAP_REPO,
            "username": "Fixture",
            "repository": MAP_NAME,
            "level_set_name": MAP_NAME,
            "author": AUTHOR,
        }))
        .unwrap();
        source.relations = map_relations;
        config.map_repositories.push(source);
        f.state.config.save(&config).unwrap();
    }
    f
}

fn import(f: &Fixture, extra_entries: &[(&str, &[u8])]) -> PlanRequest {
    let zip_path = f.game.parent().unwrap().join("map.zip");
    let directory = format!("{}/levels/{MAP_NAME}", crate::framework::DIYLEVEL_DIR);
    let mut zip = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
    let info = live_game_info::bundle(MAP_NAME, AUTHOR, VERSION);
    let info_path = format!("{directory}/info");
    let scene_path = format!("{directory}/scene");
    let entries = [(info_path.as_str(), info.as_slice()), (&scene_path, SCENE)];
    for (name, bytes) in entries.into_iter().chain(extra_entries.iter().copied()) {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
    PlanRequest {
        map_import: Some(MapImport {
            zip_path: zip_path.to_string_lossy().into_owned(),
            selected: vec![MAP_NAME.into()],
            origin: None,
        }),
        ..Default::default()
    }
}

fn installed_map(f: &Fixture) -> PathBuf {
    f.game
        .join(format!("BepInEx/plugins/{FRAMEWORK}/levels/{MAP_NAME}"))
}

fn journal_root(f: &Fixture) -> PathBuf {
    f.state
        .config
        .config_path()
        .parent()
        .unwrap()
        .join("mod-change-transactions")
}

fn snapshot(f: &Fixture) -> [Fingerprint; 3] {
    [
        f.game.clone(),
        f.state.config.manifests_dir(),
        journal_root(f),
    ]
    .map(|path| Fingerprint::capture(&path).unwrap())
}

fn running(f: &Fixture, value: bool) {
    f.state
        .game_running_for_test
        .store(value, Ordering::Relaxed);
}

async fn prepare(f: &Fixture, plan: &ChangePlan) -> PreparationOutcome {
    preparation::prepare(f.state.clone(), plan.plan_id.clone(), |_| {}).await
}

fn ready(outcome: PreparationOutcome) -> ChangePlan {
    assert_eq!(outcome.error, None);
    let plan = outcome.plan.unwrap();
    assert!(plan.prepared);
    assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    plan
}

fn assert_prepare_blocked(outcome: PreparationOutcome) {
    assert_eq!(outcome.error.as_deref(), Some("game.running"));
    assert!(outcome.plan.is_none());
}

fn assert_map_only(plan: &ChangePlan) {
    assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    let [action] = plan.result.actions.as_slice() else {
        panic!("expected a single map action: {:?}", plan.result.actions);
    };
    assert_eq!(action.node_id, MAP_ID);
}

#[tokio::test]
async fn running_game_commits_map_data_and_relation_snapshot() {
    let relations = vec![requires(mods::plugin_target::DIYLEVEL_GUID, ">=0.11.1")];
    let f = setup(vec![], relations.clone());
    let request = import(&f, &[]);
    let before = snapshot(&f);
    running(&f, true);

    let plan = planning::create(&f.state, request).unwrap();
    assert_map_only(&plan);
    let prepared = ready(prepare(&f, &plan).await);
    assert_eq!(snapshot(&f), before);
    let mut session = take(&f, &prepared);
    execution::commit(&f.state, &mut session, |_| {}).unwrap();

    let destination = installed_map(&f);
    assert_eq!(fs::read(destination.join("scene")).unwrap(), SCENE);
    let info = crate::level_sets::read_info_metadata(&destination.join("info")).unwrap();
    assert_eq!(
        (
            info.name.as_str(),
            info.author.as_str(),
            info.version.as_str()
        ),
        (MAP_NAME, AUTHOR, VERSION)
    );
    let manifests = f.state.config.manifests_dir();
    let snapshots = archive::read_map_relation_snapshots(&manifests);
    assert_eq!(snapshots.entries[MAP_REPO].relations, relations);
    assert!(crate::level_sets::read_registered_plugins(&manifests).is_empty());
    assert!(!has_pending(&journal_root(&f)).unwrap());
}

#[test]
fn running_game_rejects_standalone_mod_and_map_induced_mod_actions() {
    let f = setup(
        vec![
            repository(EXTRA, EXTRA_GUID, &[VERSION], vec![]),
            repository(CONFLICT, CONFLICT_GUID, &[VERSION], vec![]),
        ],
        vec![
            requires(EXTRA_GUID, ">=1.0.0"),
            RepositoryRelation {
                kind: RelationKind::Conflicts,
                ..requires(CONFLICT_GUID, "")
            },
        ],
    );
    install(&f, EXTRA, EXTRA_GUID, VERSION, false);
    install(&f, CONFLICT, CONFLICT_GUID, VERSION, true);
    let request = import(&f, &[]);
    let before = snapshot(&f);

    let plan = planning::create(&f.state, request.clone()).unwrap();
    assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    for (node_id, kind) in [
        ("mod:Extra", ActionKind::Enable),
        ("mod:Conflict", ActionKind::Disable),
    ] {
        assert!(
            plan.result.actions.iter().any(|action| {
                action.node_id == node_id && action.kind == kind && action.caused_by == [MAP_ID]
            }),
            "{:?}",
            plan.result.actions
        );
    }
    execution::cancel(&f.state, &plan.plan_id).unwrap();

    running(&f, true);
    assert_eq!(
        planning::create(&f.state, request).unwrap_err(),
        "game.running"
    );
    assert_eq!(
        planning::create(&f.state, super::request(EXTRA, None, true)).unwrap_err(),
        "game.running"
    );
    assert_eq!(snapshot(&f), before);
}

#[tokio::test]
async fn map_zip_external_plugin_blocks_planning_and_a_later_game_start() {
    let f = setup(vec![], vec![]);
    let plugin = mods::plugin_blob("fixture.live-external", "External", VERSION);
    let request = import(&f, &[(EXTERNAL_DLL, &plugin)]);
    assert!(request.roots.is_empty());
    let before = snapshot(&f);

    running(&f, true);
    assert_eq!(
        planning::create(&f.state, request.clone()).unwrap_err(),
        "game.running"
    );
    assert_eq!(snapshot(&f), before);

    running(&f, false);
    let plan = planning::create(&f.state, request).unwrap();
    assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    assert!(
        plan.result.actions.iter().any(|action| {
            action.kind == ActionKind::Install
                && plan
                    .nodes
                    .iter()
                    .any(|node| node.node_id == action.node_id && node.kind == NodeKind::Mod)
        }),
        "{:?}",
        plan.result.actions
    );
    running(&f, true);
    assert_prepare_blocked(prepare(&f, &plan).await);
    assert_eq!(snapshot(&f), before);

    running(&f, false);
    let prepared = ready(prepare(&f, &plan).await);
    let mut session = take(&f, &prepared);
    running(&f, true);
    assert_eq!(
        execution::commit(&f.state, &mut session, |_| {}).unwrap_err(),
        "game.running"
    );
    assert_eq!(snapshot(&f), before);
}

#[tokio::test]
async fn map_only_actions_still_block_writes_of_nested_active_and_disabled_dlls() {
    for member in ["Nested.dll", "Nested.dll.disabled"] {
        let f = setup(vec![], vec![]);
        let path = format!(
            "{}/levels/{MAP_NAME}/nested/{member}",
            crate::framework::DIYLEVEL_DIR
        );
        let plugin = mods::plugin_blob("fixture.live-nested", "Nested", VERSION);
        let request = import(&f, &[(&path, &plugin)]);
        let before = snapshot(&f);

        running(&f, true);
        let plan = planning::create(&f.state, request).unwrap();
        assert_map_only(&plan);
        assert_prepare_blocked(prepare(&f, &plan).await);
        assert_eq!(snapshot(&f), before, "preparing {member}");

        running(&f, false);
        let prepared = ready(prepare(&f, &plan).await);
        let mut session = take(&f, &prepared);
        running(&f, true);
        assert_eq!(
            execution::commit(&f.state, &mut session, |_| {}).unwrap_err(),
            "game.running",
            "{member}"
        );
        assert_eq!(snapshot(&f), before, "committing {member}");
    }
}
