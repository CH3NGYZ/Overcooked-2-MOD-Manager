//! The map's bundled runtime and an installed standalone copy share a loader GUID.
use super::*;
use crate::{asset_bundle::LevelSetMetadata, level_sets};

const RUNTIME_GUID: &str = "fixture.young-levels";
const EXTERNAL: &str = "BepInEx/plugins/YoungLevelRuntimeLoader/YoungLevels.dll";
const EMBEDDED: &str = "YoungLevels.dll";
const MAP_ID: &str = "map:young";

fn setup(
    external_version: &str,
    map_requirements: Vec<RepositoryRelation>,
) -> (Fixture, std::path::PathBuf, LevelSetMetadata) {
    let framework_guid = mods::plugin_target::DIYLEVEL_GUID;
    let f = fixture(vec![
        repository("DIYLevel", framework_guid, &["0.11.1", "0.12.0"], vec![]),
        repository(
            "Dependent",
            "fixture.runtime-dependent",
            &["1.0.0"],
            vec![requires(RUNTIME_GUID, ">=1.3.0")],
        ),
    ]);
    install(&f, "DIYLevel", framework_guid, "0.11.1", true);
    install(&f, "Dependent", "fixture.runtime-dependent", "1.0.0", true);
    write(
        &f.game.join(EXTERNAL),
        &mods::plugin_blob(RUNTIME_GUID, "YoungLevelsRuntime", external_version),
    );
    let disabled = level_sets::disabled_root(&f.game).join("young");
    write(
        &disabled.join(mods::disabled_dll_file(EMBEDDED)),
        &mods::plugin_blob(RUNTIME_GUID, "YoungLevelsRuntime", "1.4.2"),
    );
    write(&disabled.join("level"), b"preserve map content");
    let metadata = LevelSetMetadata {
        name: "羊的回合".into(),
        author: "Integration".into(),
        version: "0.7.5".into(),
        ..Default::default()
    };
    configure_map_requirements(&f, &metadata, map_requirements);
    write(&disabled.join("info_young"), b"fixture info");
    level_sets::seed_info_metadata(&disabled.join("info_young"), metadata.clone());
    (f, disabled, metadata)
}

fn configure_map_requirements(
    f: &Fixture,
    metadata: &LevelSetMetadata,
    relations: Vec<RepositoryRelation>,
) {
    if relations.is_empty() {
        return;
    }
    let id = "young-fixture";
    archive::write_map_relation_snapshots(
        &f.state.config.manifests_dir(),
        &crate::models::MapRelationSnapshots {
            schema_version: crate::models::default_snapshot_schema_version(),
            entries: std::collections::BTreeMap::from([(
                id.into(),
                crate::models::MapRelationSnapshot {
                    repository_id: id.into(),
                    level_set_name: metadata.name.clone(),
                    level_set_name_zh: String::new(),
                    author: metadata.author.clone(),
                    group_i18n: Default::default(),
                    group: String::new(),
                    relations,
                    release_relations: vec![],
                    installed_mapset_version: metadata.version.clone(),
                },
            )]),
        },
    )
    .unwrap();
}

fn enable_young() -> PlanRequest {
    PlanRequest {
        roots: vec![RootRequest {
            intent: RootIntent {
                node_id: MAP_ID.into(),
                candidate_id: None,
                enabled: true,
                member_key: None,
            },
            asset_name: None,
        }],
        ..Default::default()
    }
}

#[test]
fn map_runtime_duplicate_disables_standalone_in_same_confirmed_transaction() {
    assert_safe_runtime_switch(vec![]);
}

#[test]
fn map_runtime_duplicate_satisfies_its_own_requirement_with_bundled_runtime() {
    assert_safe_runtime_switch(vec![requires(RUNTIME_GUID, ">=1.3.0")]);
}

fn assert_safe_runtime_switch(map_requirements: Vec<RepositoryRelation>) {
    let (f, disabled, metadata) = setup("1.3.3", map_requirements);
    let old_runtime = fs::read(f.game.join(EXTERNAL)).unwrap();
    let new_runtime = fs::read(disabled.join(mods::disabled_dll_file(EMBEDDED))).unwrap();
    let plan = planning::create(&f.state, enable_young()).unwrap();
    assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    assert_eq!(plan.result.actions.len(), 2);
    let provider_wire = serde_json::to_value(&plan.provider_choices).unwrap();
    let [group] = provider_wire.as_array().unwrap().as_slice() else {
        panic!("{provider_wire}")
    };
    assert_eq!(group["guid"], RUNTIME_GUID);
    let rows = group["rows"].as_array().unwrap();
    assert!(
        rows.iter()
            .any(|row| row["path"] == EXTERNAL && row["origin"] == "installed")
    );
    let embedded =
        inventory::relative(&f.game, &disabled.join(mods::disabled_dll_file(EMBEDDED))).unwrap();
    assert!(rows.iter().any(|row| row["node_id"] == MAP_ID
        && row["origin"] == "selected"
        && row["version"] == "1.4.2"
        && row["path"] == embedded
        && row["installed_enabled"] == false));
    assert!(plan.result.actions.iter().any(|action| {
        action.kind == ActionKind::Disable
            && action.node_id == format!("dll:{}", inventory::key(EXTERNAL))
            && action.caused_by == [MAP_ID]
    }));
    assert!(
        plan.result
            .actions
            .iter()
            .any(|action| { action.kind == ActionKind::Enable && action.node_id == MAP_ID })
    );
    assert!(f.game.join(EXTERNAL).is_file());
    let mut session = take(&f, &plan);
    preparation::finish(&f.state, &mut session).unwrap();
    assert!(f.game.join(EXTERNAL).is_file());
    assert!(disabled.is_dir());

    session.plan.prepared = true;
    let enabled = f.game.join("BepInEx/plugins/DIYLevel/levels/young");
    let mut saw_map = false;
    super::super::execution::commit(&f.state, &mut session, |event| {
        let payload = serde_json::to_value(event).unwrap();
        if payload["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["node_id"] == MAP_ID && item["stage"] == "applying")
        {
            assert!(disabled.is_dir());
            assert!(!enabled.exists());
            assert!(!f.game.join(EXTERNAL).exists());
            assert_eq!(
                fs::read(f.game.join(mods::disabled_dll_file(EXTERNAL))).unwrap(),
                old_runtime
            );
            saw_map = true;
        }
    })
    .unwrap();
    assert!(saw_map);
    assert!(!disabled.exists());
    assert!(!f.game.join(EXTERNAL).exists());
    assert_eq!(
        fs::read(f.game.join(mods::disabled_dll_file(EXTERNAL))).unwrap(),
        old_runtime
    );
    assert_eq!(fs::read(enabled.join(EMBEDDED)).unwrap(), new_runtime);
    assert_eq!(
        fs::read(enabled.join("level")).unwrap(),
        b"preserve map content"
    );
    level_sets::seed_info_metadata(&enabled.join("info_young"), metadata);
    let live = inventory::build(&f.state).unwrap();
    let providers = super::super::evaluate::targets(&live.inventory, RUNTIME_GUID, true);
    assert_eq!(providers.len(), 1);
    assert_eq!(live.inventory.nodes[providers[0].0].id, MAP_ID);
    assert_eq!(providers[0].1.version.as_deref(), Some("1.4.2"));
    let evaluations = super::super::evaluate::evaluate(&live.inventory);
    let map = evaluations
        .iter()
        .find(|row| row.node_id == MAP_ID)
        .unwrap();
    assert!(map.enabled);
    assert!(map.runtime.iter().all(|issue| !issue.state.is_issue()));
    let dependent = evaluations
        .iter()
        .find(|row| row.node_id == "mod:Dependent")
        .unwrap();
    assert!(dependent.enabled);
    assert_eq!(dependent.runtime[0].state, EvaluationState::Satisfied);
    assert!(
        live.inventory.nodes[providers[0].0]
            .current
            .as_ref()
            .unwrap()
            .relations
            .iter()
            .all(|owned| owned.relation.kind != RelationKind::Conflicts)
    );
}

#[test]
fn map_runtime_duplicate_with_both_standalone_filenames_is_not_switched() {
    let (f, disabled, _) = setup("1.3.3", vec![]);
    let occupied = f.game.join(mods::disabled_dll_file(EXTERNAL));
    write(&occupied, b"occupied plugin filename");
    assert_duplicate_untouched(&f, &disabled);
}

#[test]
fn map_runtime_duplicate_with_newer_standalone_is_not_switched() {
    let (f, disabled, _) = setup("1.5.0", vec![]);
    assert_duplicate_untouched(&f, &disabled);
}

#[test]
fn map_runtime_duplicate_with_multiple_candidates_is_not_switched() {
    let (f, disabled, _) = setup("1.3.3", vec![]);
    write(
        &f.game.join("BepInEx/plugins/Other/Other.dll"),
        &mods::plugin_blob(RUNTIME_GUID, "OtherRuntime", "1.2.0"),
    );
    assert_duplicate_untouched(&f, &disabled);
}

#[test]
fn map_runtime_duplicate_cannot_break_the_requested_map_dependency() {
    let (f, disabled, _) = setup("1.3.3", vec![requires(RUNTIME_GUID, ">=1.3.0,<1.4.0")]);
    // Bind the standalone DLL to known metadata so an unrelated unknown edge
    // cannot satisfy this test while bypassing the requested version guard.
    let mut config = f.state.config.load_or_default();
    config.repositories.push(repository(
        "YoungLevelRuntimeLoader",
        RUNTIME_GUID,
        &[],
        vec![],
    ));
    f.state.config.save(&config).unwrap();
    let before = Fingerprint::capture(&f.game).unwrap();
    let plan = planning::create(&f.state, enable_young()).unwrap();
    assert_ne!(plan.result.status, SolveStatus::Ready);
    assert!(plan.result.actions.is_empty());
    let map = plan
        .result
        .evaluations
        .iter()
        .find(|row| row.node_id == MAP_ID)
        .unwrap();
    assert!(map.readiness.iter().any(|issue| {
        issue.relation.guid == RUNTIME_GUID && issue.relation.version == ">=1.3.0,<1.4.0"
    }));
    assert!(
        plan.result
            .blockers
            .iter()
            .any(|blocker| blocker.code == "relation_has_no_solution"
                && blocker.target_guid.as_deref() == Some(RUNTIME_GUID)),
        "{:?}",
        plan.result.blockers
    );
    assert!(
        plan.result.blockers.iter().any(|blocker| {
            blocker.owner_id.as_deref() == Some(MAP_ID)
                && blocker
                    .message
                    .starts_with("modChanges.duplicatePluginRuntime?")
                && blocker.message.contains(RUNTIME_GUID)
        }),
        "{:?}",
        plan.result.blockers
    );
    assert_eq!(Fingerprint::capture(&f.game).unwrap(), before);
    assert!(disabled.is_dir());
}

#[test]
fn map_runtime_duplicate_preserves_the_unrelated_missing_dependency() {
    let missing = "fixture-missing-map-prerequisite";
    let (f, disabled, _) = setup("1.5.0", vec![requires(missing, ">=1.0.0")]);
    let before = Fingerprint::capture(&f.game).unwrap();
    let plan = planning::create(&f.state, enable_young()).unwrap();
    assert_ne!(plan.result.status, SolveStatus::Ready);
    assert!(plan.result.actions.is_empty());
    assert!(plan.result.blockers.iter().any(|blocker| {
        blocker.owner_id.as_deref() == Some(MAP_ID)
            && blocker.target_guid.as_deref() == Some(missing)
    }));
    assert!(plan.result.blockers.iter().all(|blocker| {
        !blocker
            .message
            .starts_with("modChanges.duplicatePluginRuntime?")
    }));
    assert_eq!(Fingerprint::capture(&f.game).unwrap(), before);
    assert!(disabled.is_dir());
}

#[test]
fn map_runtime_duplicate_rejects_a_stale_switch_after_preparation() {
    let (f, disabled, _) = setup("1.3.3", vec![]);
    let plan = planning::create(&f.state, enable_young()).unwrap();
    let mut session = take(&f, &plan);
    preparation::finish(&f.state, &mut session).unwrap();
    session.plan.prepared = true;

    let occupied = f.game.join(mods::disabled_dll_file(EXTERNAL));
    write(&occupied, b"occupied after confirmation");
    let before = Fingerprint::capture(&f.game).unwrap();
    assert_eq!(
        super::super::execution::commit(&f.state, &mut session, |_| {}).unwrap_err(),
        "modChanges.targetChanged"
    );
    assert_eq!(Fingerprint::capture(&f.game).unwrap(), before);
    assert!(disabled.is_dir());
}

fn assert_duplicate_untouched(f: &Fixture, disabled: &Path) {
    let before = Fingerprint::capture(&f.game).unwrap();
    let identifies_duplicate = |message: &str| {
        message.starts_with("modChanges.duplicatePluginRuntime?") && message.contains(RUNTIME_GUID)
    };
    match planning::create(&f.state, enable_young()) {
        Ok(plan) => {
            assert_ne!(plan.result.status, SolveStatus::Ready);
            assert!(plan.result.actions.is_empty());
            assert!(
                plan.result
                    .blockers
                    .iter()
                    .any(|b| identifies_duplicate(&b.message))
            );
            let mut session = take(f, &plan);
            assert_eq!(
                super::super::execution::commit(&f.state, &mut session, |_| {}).unwrap_err(),
                "modChanges.notPrepared"
            );
        }
        Err(error) => assert!(identifies_duplicate(&error), "{error}"),
    }
    assert_eq!(Fingerprint::capture(&f.game).unwrap(), before);
    assert!(disabled.is_dir());
}
