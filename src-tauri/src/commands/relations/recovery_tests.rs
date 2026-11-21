use super::*;
use crate::config::ConfigService;
use crate::mod_changes::transaction::{
    self, FileChange, Fingerprint, Mutation, RootKind, StagedFile, TransactionPath,
    TransactionRoots,
};
use crate::models::{Provider, RelationKind, RepositoryRelation};
use std::fs;

#[test]
fn dependency_snapshot_sync_keeps_manifests_when_an_applying_journal_cannot_recover() {
    let temporary = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temporary.path()).unwrap();
    let game = root.join("game");
    fs::create_dir_all(game.join("BepInEx").join("plugins")).unwrap();
    let data = root.join("data");
    let state = AppState::with_config(ConfigService::for_test(data.clone()));
    let mut repository = crate::community::repository_from_coordinates(
        &Provider::Github,
        "Fixture",
        "recovery-relations",
        "",
        "",
    )
    .unwrap();
    repository.id = "recovery-relations".into();
    repository.relations = vec![RepositoryRelation {
        kind: RelationKind::Requires,
        guid: "fixture-dependency".into(),
        version: ">=2.0.0".into(),
        prerelease: None,
        optional: false,
    }];
    let mut config = crate::config::default_config();
    config.game.path = game.to_string_lossy().into_owned();
    config.repositories = vec![repository];
    state.config.save(&config).unwrap();
    let manifests = state.config.manifests_dir();
    let mut manifest = archive::new_manifest(
        "recovery-relations",
        "v1.0.0",
        BTreeMap::new(),
        BTreeMap::new(),
        vec!["BepInEx/plugins/fixture.dll".into()],
        vec![],
    );
    archive::write_manifest(&manifests, &manifest).unwrap();
    let target = manifests.join("recovery-relations.toml");
    manifest.release_tag = "v2.0.0".into();
    let source = data.join("replacement.toml");
    fs::write(&source, toml::to_string_pretty(&manifest).unwrap()).unwrap();
    let journal_root = data.join("mod-change-transactions");
    let pending = transaction::prepare(
        &journal_root,
        &TransactionRoots {
            game_root: game,
            data_root: data.clone(),
        },
        vec![Mutation::File(FileChange {
            target: TransactionPath {
                root: RootKind::Data,
                relative: target
                    .strip_prefix(&data)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
            },
            source: Some(StagedFile::capture(source.clone()).unwrap()),
            expected: Fingerprint::capture(&target).unwrap(),
            require_single_link: false,
        })],
    )
    .unwrap();
    drop(pending);

    // Simulate a crash after the live write, then an external edit before restart.
    let directory = fs::read_dir(&journal_root)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let journal_path = directory.join("journal.json");
    let mut journal: serde_json::Value =
        serde_json::from_slice(&fs::read(&journal_path).unwrap()).unwrap();
    journal["state"] = "applying".into();
    journal["records"][0]["phase"] = "applied".into();
    fs::write(&journal_path, serde_json::to_vec(&journal).unwrap()).unwrap();
    let mut edited = fs::read(&source).unwrap();
    edited.extend_from_slice(b"\n# Keep the user's post-crash edit.\n");
    fs::write(&target, &edited).unwrap();
    let snapshots = manifests.join("map-relations.json");
    let snapshot_bytes = br#"{"schema_version":1,"entries":{"old-map":{"repository_id":"old-map","level_set_name":"Map","level_set_name_zh":"","author":"Fixture","relations":[{"kind":"requires","guid":"fixture-map-dependency"}]}}}"#;
    fs::write(&snapshots, snapshot_bytes).unwrap();
    let config_before = fs::read(state.config.config_path()).unwrap();

    assert_eq!(
        sync_dependency_snapshots_in(&state).err().as_deref(),
        Some("modChanges.recoveryRequired")
    );

    assert_eq!(fs::read(&target).unwrap(), edited);
    assert_eq!(fs::read(&snapshots).unwrap(), snapshot_bytes);
    assert_eq!(fs::read(state.config.config_path()).unwrap(), config_before);
    assert!(transaction::has_pending(&journal_root).unwrap());
    assert!(directory.join("before").join("0").is_file());
    assert!(directory.join("after").join("0").is_file());
}

#[test]
fn dependency_snapshot_sync_keeps_explicit_map_origin_without_an_asset_version() {
    use crate::level_sets::{self, LevelSetOnDisk, view};

    let temp = tempfile::tempdir().unwrap();
    let state = AppState::with_config(ConfigService::for_test(temp.path().join("data")));
    let mut config = crate::config::default_config();
    config.map_repositories.clear();
    state.config.save(&config).unwrap();
    let manifests = state.config.manifests_dir();
    let mut snapshots = MapRelationSnapshots::default();
    let origin = MapRelationSnapshot {
        repository_id: "removed-source".into(),
        level_set_name: "Map".into(),
        level_set_name_zh: String::new(),
        author: "Fixture".into(),
        group_i18n: BTreeMap::new(),
        group: String::new(),
        relations: vec![],
        release_relations: vec![],
        installed_mapset_version: String::new(),
    };
    let id = format!("{}map", level_sets::INSTALLED_PREFIX);
    let cleared = format!("{}cleared", level_sets::INSTALLED_PREFIX);
    snapshots.entries.insert(
        cleared.clone(),
        MapRelationSnapshot {
            repository_id: String::new(),
            ..origin.clone()
        },
    );
    // A legacy catalog snapshot can have no fields beyond its identity and source ID.
    snapshots
        .entries
        .insert(origin.repository_id.clone(), origin.clone());
    let map_info = || {
        view::level_set_to_info(LevelSetOnDisk {
            dir_name: "Map".into(),
            enabled: true,
            path: temp.path().join("Map"),
            metadata: Some(crate::asset_bundle::LevelSetMetadata {
                name: "Map".into(),
                author: "Fixture".into(),
                ..Default::default()
            }),
            error: None,
            required_runtime: None,
            plugins: vec![],
        })
    };
    let mut info = map_info();
    view::bind_map_relation_snapshot(&mut info, &config, &snapshots);
    assert!(info.has_map_source_snapshot);

    snapshots.entries.insert(id.clone(), origin.clone());
    archive::write_map_relation_snapshots(&manifests, &snapshots).unwrap();
    sync_dependency_snapshots_in(&state).unwrap();
    let retained = archive::read_map_relation_snapshots(&manifests);
    assert_eq!(retained.entries.get(&id), Some(&origin));
    assert!(!retained.entries.contains_key(&cleared));
    let mut info = map_info();
    view::bind_map_relation_snapshot(&mut info, &config, &retained);
    assert!(info.map_id.is_none());
    assert!(info.installed_mapset_version.is_empty());
    assert_eq!(
        serde_json::to_value(&info).unwrap()["has_map_source_snapshot"],
        true
    );
}
