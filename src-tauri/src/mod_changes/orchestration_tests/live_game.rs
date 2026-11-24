//! Exercise live-game rules with actual ZIP parsing, solving and transaction writes.
use super::super::{execution, progress::PreparationOutcome};
use super::*;
use std::{io::Write, path::PathBuf, sync::atomic::Ordering};

#[path = "live_game/identity.rs"]
mod identity;
#[path = "live_game/map_payload.rs"]
mod map_payload;

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
            download_id: None,
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
async fn local_zip_keeps_its_identity_with_conflicting_lagging_map_sources() {
    let shared = requires(mods::plugin_target::DIYLEVEL_GUID, ">=0.11.1");
    let first_only = requires("fixture.first-only", ">=1.0.0");
    let second_only = requires("fixture.second-only", ">=1.0.0");
    let f = setup(vec![], vec![shared.clone(), first_only.clone()]);
    let mut config = f.state.config.load_or_default();
    let mut mirror = config.map_repositories[0].clone();
    mirror.id = "live-map-mirror".into();
    mirror.source_name = "Mirror".into();
    mirror.relations = vec![second_only.clone(), shared.clone()];
    config.map_repositories.push(mirror);
    f.state.config.save(&config).unwrap();

    let before = snapshot(&f);
    let plan = planning::create(&f.state, import(&f, &[])).unwrap();
    assert_map_only(&plan);
    assert_eq!(plan.result.notices.len(), 1);
    assert!(plan.result.notices[0].starts_with("levelSet.localMapSourceConflict?"));
    let prepared = ready(prepare(&f, &plan).await);
    assert_eq!(snapshot(&f), before);
    execution::commit(&f.state, &mut take(&f, &prepared), |_| {}).unwrap();
    assert_eq!(fs::read(installed_map(&f).join("scene")).unwrap(), SCENE);

    let roots = crate::level_sets::LevelSetRoots::new(
        f.game.clone(),
        installed_map(&f)
            .parent()
            .and_then(|path| path.parent())
            .map(|path| path.to_path_buf()),
    );
    let mut info = crate::level_sets::scan_level_sets(&roots)
        .into_iter()
        .find(|set| set.dir_name == MAP_NAME)
        .map(crate::level_sets::view::level_set_to_info)
        .unwrap();
    let snapshots = archive::read_map_relation_snapshots(&f.state.config.manifests_dir());
    crate::level_sets::view::bind_map_relation_snapshot(&mut info, &config, &snapshots);
    assert!(info.local_import);
    assert_eq!(info.map_id, None);
    assert!(info.installed_mapset_version.is_empty());
    assert_eq!(info.relations, vec![shared]);
}

#[test]
fn local_zip_reports_both_directories_when_existing_map_identity_is_duplicated() {
    let fixture = setup(vec![], vec![]);
    let info = live_game_info::bundle(MAP_NAME, AUTHOR, "0.9.0");
    for directory in ["Old-A", "Old-B"] {
        write(
            &fixture.game.join(format!(
                "BepInEx/plugins/{FRAMEWORK}/levels/{directory}/info"
            )),
            &info,
        );
    }
    let before = snapshot(&fixture);
    let error = planning::create(&fixture.state, import(&fixture, &[])).unwrap_err();
    assert_eq!(
        error,
        crate::locale_tag::with_params(
            "levelSet.mapDuplicateInstallTargets",
            &[
                ("map", MAP_NAME),
                (
                    "paths",
                    "BepInEx/plugins/DIYLevel/levels/Old-A\nBepInEx/plugins/DIYLevel/levels/Old-B"
                ),
            ],
        ),
    );
    assert_eq!(snapshot(&fixture), before);
}

#[tokio::test]
async fn online_bundle_uses_secondary_map_requirements_without_its_stale_source_default() {
    let runtime_guid = "oc2.oc2diylevelruntimewloader";
    let bonus = "BonusMap";
    let required = requires(runtime_guid, ">=3.8.8");
    let fixture = setup(
        vec![repository("Runtime", runtime_guid, &["3.10.2"], vec![])],
        vec![requires(mods::plugin_target::DIYLEVEL_GUID, ">=0.11.1")],
    );
    install(&fixture, "Runtime", runtime_guid, "3.10.2", true);
    let mut config = fixture.state.config.load_or_default();
    config.map_repositories.push(
        serde_json::from_value(serde_json::json!({
            "id": "bonus-source", "level_set_name": bonus, "author": AUTHOR,
            "release_derived": true, "latest_info_version": "2.24",
            "relations": [requires("fixture.stale-dependency", ">=1.0.0")],
        }))
        .unwrap(),
    );
    fixture.state.config.save(&config).unwrap();
    let directory = format!("{}/levels/{bonus}", crate::framework::DIYLEVEL_DIR);
    let info = live_game_info::bundle(bonus, AUTHOR, "2.25");
    let mut request = import(
        &fixture,
        &[
            (&format!("{directory}/info"), &info),
            (&format!("{directory}/requires.txt"), b"3.8.8"),
        ],
    );
    let map = request.map_import.as_mut().unwrap();
    map.selected.push(bonus.into());
    map.origin = Some(MapInstallOrigin {
        repository_id: Some(MAP_REPO.into()),
        level_set_name: MAP_NAME.into(),
        level_set_name_zh: String::new(),
        author: AUTHOR.into(),
        mapset_version: "v1.0.0".into(),
    });
    let plan = planning::create(&fixture.state, request).unwrap();
    assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    assert_eq!(plan.result.actions.len(), 2);
    let prepared = ready(prepare(&fixture, &plan).await);
    execution::commit(&fixture.state, &mut take(&fixture, &prepared), |_| {}).unwrap();
    let trusted = inventory::build(&fixture.state).unwrap();
    let map = trusted
        .inventory
        .nodes
        .iter()
        .find(|node| node.id == format!("map:{bonus}"))
        .unwrap();
    let relations = &map.current.as_ref().unwrap().relations;
    assert!(relations.iter().any(|owned| owned.relation == required));
    assert!(
        !relations
            .iter()
            .any(|owned| owned.relation.guid == "fixture.stale-dependency")
    );
}

#[tokio::test]
#[ignore = "requires OC2_MAP_IMPORT_REPLAY_ZIP, the real 2.25 map archive"]
async fn real_local_zip_updates_older_map_without_binding_a_mirror() {
    let path = PathBuf::from(
        std::env::var_os("OC2_MAP_IMPORT_REPLAY_ZIP").expect("set OC2_MAP_IMPORT_REPLAY_ZIP"),
    );
    let sets = crate::level_sets::inspect_zip(&path).unwrap();
    let [set] = sets.as_slice() else {
        panic!("expected one map, got {}", sets.len());
    };
    assert_eq!(set.metadata.version, "2.25");
    assert!(set.plugins.is_empty());
    let runtime_guid = "oc2.oc2diylevelruntimewloader";
    let runtime = repository("Runtime", runtime_guid, &["3.8.8", "3.10.2"], vec![]);
    let fastinit = repository(
        "FastInit",
        "oc2.diylevel.fastinit",
        &["1.7.0", "1.9.2"],
        vec![],
    );
    let fixture = setup(vec![runtime, fastinit], vec![]);
    install(&fixture, "Runtime", runtime_guid, "3.8.8", true);
    install(&fixture, "FastInit", "oc2.diylevel.fastinit", "1.7.0", true);
    let old = fixture
        .game
        .join("BepInEx/plugins/DIYLevel/levels/KeepOldDirectory");
    let new_directory = old.with_file_name(&set.dir_name);
    assert_ne!(old, new_directory);
    write(
        &old.join("info"),
        &live_game_info::bundle_with_chinese_name(
            &set.metadata.name,
            set.metadata.name_zh.as_deref(),
            &set.metadata.author,
            "2.21",
        ),
    );
    write(&old.join("old_scene"), SCENE);
    let mut config = fixture.state.config.load_or_default();
    for (id, label) in [("previous", "Primary"), ("mirror", "Mirror")] {
        let mut source: MapRepositoryConfig = serde_json::from_value(serde_json::json!({
            "id": id, "source_name": label, "level_set_name": set.metadata.name,
            "level_set_name_zh": set.metadata.name_zh, "author": set.metadata.author,
            "latest_info_version": "2.24", "release_derived": true,
        }))
        .unwrap();
        source.release_relations.push(ReleaseRelationOverride {
            version: ">=1.4".into(),
            relations: vec![requires("oc2.diylevel.fastinit", ">=1.7.0")],
        });
        config.map_repositories.push(source);
    }
    fixture.state.config.save(&config).unwrap();
    let request = PlanRequest {
        map_import: Some(MapImport {
            zip_path: path.to_string_lossy().into_owned(),
            download_id: None,
            selected: vec![set.dir_name.clone()],
            origin: None,
        }),
        ..Default::default()
    };
    let before = snapshot(&fixture);
    let plan = planning::create(&fixture.state, request).unwrap();
    assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    assert_eq!(plan.result.actions.len(), 1, "{:?}", plan.result.actions);
    assert_eq!(plan.result.actions[0].node_id, "map:KeepOldDirectory");
    assert!(plan.result.notices.is_empty());
    let prepared = ready(prepare(&fixture, &plan).await);
    assert_eq!(snapshot(&fixture), before);
    execution::commit(&fixture.state, &mut take(&fixture, &prepared), |_| {}).unwrap();
    assert!(!old.join("old_scene").exists());
    assert!(!new_directory.exists());
    assert_eq!(
        fs::read_to_string(old.join("requires.txt")).unwrap().trim(),
        "3.8.8"
    );
    let trusted = inventory::build(&fixture.state).unwrap();
    let files = &trusted.files["map:KeepOldDirectory"];
    assert_eq!(files.map_metadata.as_ref().unwrap().version, "2.25");
    assert_eq!(files.summary.map_id, None);
    let map = trusted
        .inventory
        .nodes
        .iter()
        .find(|node| node.id == "map:KeepOldDirectory")
        .unwrap();
    let candidate = map.current.as_ref().unwrap();
    assert!(
        candidate
            .relations
            .iter()
            .any(|relation| relation.relation == requires(runtime_guid, ">=3.8.8"))
    );
    assert!(
        candidate
            .relations
            .iter()
            .any(|relation| relation.relation == requires("oc2.diylevel.fastinit", ">=1.7.0"))
    );
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

#[tokio::test]
async fn running_game_can_disable_pure_maps_with_dll_like_directory_names() {
    for suffix in [".dll", ".dll.disabled"] {
        let relations = vec![requires(mods::plugin_target::DIYLEVEL_GUID, ">=0.11.1")];
        let f = setup(vec![], relations.clone());
        let plan = planning::create(&f.state, import(&f, &[])).unwrap();
        let prepared = ready(prepare(&f, &plan).await);
        execution::commit(&f.state, &mut take(&f, &prepared), |_| {}).unwrap();

        let name = format!("{MAP_NAME}{suffix}");
        let source = installed_map(&f).with_file_name(&name);
        fs::rename(installed_map(&f), &source).unwrap();
        let destination = crate::level_sets::disabled_root(&f.game).join(&name);
        let map = Fingerprint::capture(&source).unwrap();
        let manifests = f.state.config.manifests_dir();
        let before = snapshot(&f);

        running(&f, true);
        let id = format!("map:{name}");
        let request = super::unbound_toggle::toggle(&f, &id, None, false);
        let plan = planning::create(&f.state, request).unwrap();
        let [action] = plan.result.actions.as_slice() else {
            panic!("expected only the map disable: {:?}", plan.result);
        };
        assert_eq!(action.node_id, id);
        assert_eq!(action.kind, ActionKind::Disable);
        let prepared = ready(prepare(&f, &plan).await);
        assert_eq!(snapshot(&f), before);
        execution::commit(&f.state, &mut take(&f, &prepared), |_| {}).unwrap();

        assert_eq!(Fingerprint::capture(&source).unwrap(), Fingerprint::Missing);
        assert_eq!(Fingerprint::capture(&destination).unwrap(), map);
        assert_eq!(fs::read(destination.join("scene")).unwrap(), SCENE);
        assert_eq!(Fingerprint::capture(&manifests).unwrap(), before[1]);
        let snapshots = archive::read_map_relation_snapshots(&manifests);
        assert_eq!(snapshots.entries[MAP_REPO].relations, relations);
        assert!(!has_pending(&journal_root(&f)).unwrap());
    }
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
