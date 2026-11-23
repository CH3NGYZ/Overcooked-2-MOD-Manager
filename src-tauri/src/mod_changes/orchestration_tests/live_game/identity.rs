use super::*;
use crate::models::MapRelationSnapshot;

const CHINESE_NAME: &str = "旧关卡集";
const OLD_VERSION: &str = "0.9.0";

#[tokio::test]
async fn long_package_name_keeps_both_map_suffixes_after_projection() {
    let fixture = setup(vec![], vec![]);
    let stem = "MapCollection".repeat(5);
    let archive = fixture.game.parent().unwrap().join(format!("{stem}.zip"));
    let mut zip = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
    let maps = [
        ("First", "第一图", b"first map".as_slice()),
        ("Second", "第二图", b"second map".as_slice()),
    ];
    for (name, chinese, scene) in maps {
        let info = live_game_info::bundle_with_chinese_name(name, Some(chinese), AUTHOR, VERSION);
        for (file, bytes) in [("info_custom", info.as_slice()), ("scene", scene)] {
            zip.start_file(
                format!("wrapper/{name}/{file}"),
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
            zip.write_all(bytes).unwrap();
        }
    }
    zip.finish().unwrap();
    let sets = crate::level_sets::inspect_zip(&archive).unwrap();
    let selected = sets
        .iter()
        .map(|set| set.dir_name.clone())
        .collect::<Vec<_>>();
    assert_eq!(selected.len(), maps.len());
    let request = PlanRequest {
        map_import: Some(MapImport {
            zip_path: archive.to_string_lossy().into_owned(),
            download_id: None,
            selected: selected.clone(),
            origin: None,
        }),
        ..Default::default()
    };
    let plan = planning::create(&fixture.state, request).unwrap();
    let prepared = ready(prepare(&fixture, &plan).await);
    assert_eq!(prepared.result.actions.len(), maps.len());
    execution::commit(&fixture.state, &mut take(&fixture, &prepared), |_| {}).unwrap();

    let levels = fixture.game.join("BepInEx/plugins/DIYLevel/levels");
    assert_eq!(fs::read_dir(&levels).unwrap().count(), maps.len());
    for ((name, chinese, scene), directory) in maps.into_iter().zip(selected) {
        let prefix = directory.strip_suffix(&format!("_{chinese}")).unwrap();
        assert!(stem.starts_with(prefix));
        let target = levels.join(directory);
        let info = crate::level_sets::read_info_metadata(&target.join("info")).unwrap();
        assert_eq!(info.name, name);
        assert_eq!(fs::read(target.join("scene")).unwrap(), scene);
    }
}

fn install_old_map(fixture: &Fixture) -> PathBuf {
    let directory = installed_map(fixture);
    write(
        &directory.join("info"),
        &live_game_info::bundle_with_chinese_name(
            MAP_NAME,
            Some(CHINESE_NAME),
            AUTHOR,
            OLD_VERSION,
        ),
    );
    write(&directory.join("old_scene"), SCENE);
    directory
}

#[tokio::test]
async fn local_zip_without_chinese_name_preserves_another_map_and_its_asset_version() {
    let fixture = setup(vec![], vec![]);
    let old = install_old_map(&fixture);
    let mut config = fixture.state.config.load_or_default();
    config.map_repositories.push(
        serde_json::from_value(serde_json::json!({
            "id": MAP_REPO, "level_set_name": MAP_NAME, "level_set_name_zh": CHINESE_NAME,
            "author": AUTHOR,
        }))
        .unwrap(),
    );
    fixture.state.config.save(&config).unwrap();
    let manifests = fixture.state.config.manifests_dir();
    let mut snapshots = archive::read_map_relation_snapshots(&manifests);
    let history = MapRelationSnapshot {
        repository_id: MAP_REPO.into(),
        level_set_name: MAP_NAME.into(),
        level_set_name_zh: CHINESE_NAME.into(),
        author: AUTHOR.into(),
        installed_mapset_version: "0.9.0-fix".into(),
        group_i18n: Default::default(),
        group: String::new(),
        relations: Vec::new(),
        release_relations: Vec::new(),
    };
    snapshots.entries.insert(MAP_REPO.into(), history.clone());
    let history_key = format!("installed:({AUTHOR:?}, {MAP_NAME:?}, {CHINESE_NAME:?})");
    snapshots
        .entries
        .insert(history_key.clone(), history.clone());
    archive::write_map_relation_snapshots(&manifests, &snapshots).unwrap();

    let before = snapshot(&fixture);
    let plan = planning::create(&fixture.state, import(&fixture, &[])).unwrap();
    assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    let [action] = plan.result.actions.as_slice() else {
        panic!("expected one map action: {:?}", plan.result.actions);
    };
    assert_ne!(action.node_id, MAP_ID);
    assert_eq!(action.kind, ActionKind::Install);
    let prepared = ready(prepare(&fixture, &plan).await);
    assert_eq!(snapshot(&fixture), before);
    execution::commit(&fixture.state, &mut take(&fixture, &prepared), |_| {}).unwrap();
    assert_eq!(fs::read(old.join("old_scene")).unwrap(), SCENE);
    assert!(!old.join("scene").exists());

    let trusted = inventory::build(&fixture.state).unwrap();
    let files = &trusted.files[&action.node_id];
    assert_eq!(files.map_metadata.as_ref().unwrap().version, VERSION);
    assert!(files.summary.map_id.is_none());
    let new = fixture.game.join(files.map_path.as_ref().unwrap());
    assert_eq!(fs::read(new.join("scene")).unwrap(), SCENE);
    let snapshots = archive::read_map_relation_snapshots(&manifests);
    assert_eq!(snapshots.entries[&history_key], history);
    assert_eq!(
        snapshots.entries[MAP_REPO].installed_mapset_version,
        "0.9.0-fix"
    );
    assert!(crate::level_sets::is_local_import(
        &snapshots, MAP_NAME, None, AUTHOR
    ));
    assert!(!crate::level_sets::is_local_import(
        &snapshots,
        MAP_NAME,
        Some(CHINESE_NAME),
        AUTHOR
    ));
}

#[test]
fn local_zip_without_chinese_name_cannot_reassign_another_maps_external_dll() {
    let fixture = setup(vec![], vec![]);
    install_old_map(&fixture);
    let plugin = mods::plugin_blob("fixture.live-external", "External", VERSION);
    write(&fixture.game.join(EXTERNAL_DLL), &plugin);
    let registry = serde_json::json!({
        "version": 1, "entries": [{
            "dll": EXTERNAL_DLL,
            "owner": {"name": MAP_NAME, "name_zh": CHINESE_NAME, "author": AUTHOR},
        }],
    });
    write(
        &fixture
            .state
            .config
            .manifests_dir()
            .join(crate::level_sets::LEVEL_SET_PLUGIN_REGISTRY),
        &serde_json::to_vec(&registry).unwrap(),
    );
    let request = import(&fixture, &[(EXTERNAL_DLL, &plugin)]);
    let before = snapshot(&fixture);
    let error = planning::create(&fixture.state, request).unwrap_err();
    assert_eq!(
        error,
        crate::locale_tag::with_params(
            "modChanges.installTargetMapProtected",
            &[("path", EXTERNAL_DLL)],
        ),
    );
    assert_eq!(snapshot(&fixture), before);
}
