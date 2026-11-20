//! Observe the live endpoints at the real journal boundaries, not only after rollback.
use super::*;
use crate::{asset_bundle::LevelSetMetadata, level_sets};

const DEP: &str = "ZDependency";
const DEP_GUID: &str = "com.example.dependency";
const CONFLICT: &str = "YConflict";
const CONFLICT_GUID: &str = "com.example.conflict";
const FRAMEWORK: &str = "DIYLevel";
const MAP: &str = "map:Target";
const MOD: &str = "mod:ATarget";
const MOD_GUID: &str = "com.example.target";

struct Scenario {
    f: Fixture,
    request: PlanRequest,
    id: &'static str,
    dependency: String,
    conflict: String,
    from: std::path::PathBuf,
    to: std::path::PathBuf,
}

fn scenario(map: bool) -> Scenario {
    let relations = vec![
        requires(DEP_GUID, ">=2.0.0"),
        RepositoryRelation {
            kind: RelationKind::Conflicts,
            ..requires(CONFLICT_GUID, "")
        },
    ];
    let fw = mods::plugin_target::DIYLEVEL_GUID;
    let f = fixture(vec![
        repository(FRAMEWORK, fw, &["0.11.1"], vec![]),
        repository(DEP, DEP_GUID, &["2.0.0", "1.0.0"], vec![]),
        repository(CONFLICT, CONFLICT_GUID, &["1.0.0"], vec![]),
        repository("ATarget", MOD_GUID, &["1.0.0"], relations.clone()),
    ]);
    install(&f, FRAMEWORK, fw, "0.11.1", true);
    let dependency = install(&f, DEP, DEP_GUID, "1.0.0", true);
    let conflict = install(&f, CONFLICT, CONFLICT_GUID, "1.0.0", true);
    let (id, key, from, to) = if map {
        let from = level_sets::disabled_root(&f.game).join("Target");
        write(&from.join("info"), b"fixture info");
        let metadata = LevelSetMetadata {
            name: "Target".into(),
            author: "Integration".into(),
            version: "1.0.0".into(),
            ..Default::default()
        };
        level_sets::seed_info_metadata(&from.join("info"), metadata.clone());
        archive::write_map_relation_snapshots(
            &f.state.config.manifests_dir(),
            &MapRelationSnapshots {
                schema_version: default_snapshot_schema_version(),
                entries: BTreeMap::from([(
                    "target".into(),
                    MapRelationSnapshot {
                        repository_id: "target".into(),
                        level_set_name: metadata.name,
                        level_set_name_zh: String::new(),
                        author: metadata.author,
                        group: String::new(),
                        group_i18n: BTreeMap::new(),
                        relations,
                        release_relations: vec![],
                        installed_mapset_version: metadata.version,
                    },
                )]),
            },
        )
        .unwrap();
        (
            MAP,
            None,
            from,
            f.game.join("BepInEx/plugins/DIYLevel/levels/Target"),
        )
    } else {
        let dll = install(&f, "ATarget", MOD_GUID, "1.0.0", false);
        (
            MOD,
            Some(inventory::key(&dll)),
            f.game.join(mods::disabled_dll_file(&dll)),
            f.game.join(dll),
        )
    };
    let request = PlanRequest {
        roots: vec![RootRequest {
            intent: RootIntent {
                node_id: id.into(),
                candidate_id: None,
                enabled: true,
                member_key: key,
            },
            asset_name: None,
        }],
        ..Default::default()
    };
    Scenario {
        f,
        request,
        id,
        dependency,
        conflict,
        from,
        to,
    }
}

impl Scenario {
    fn prepared(&self) -> PlanSession {
        let plan = planning::create(&self.f.state, self.request.clone()).unwrap();
        let mut session = take(&self.f, &plan);
        verify_selected(&self.f, &mut session);
        preparation::finish(&self.f.state, &mut session).unwrap();
        session.plan.prepared = true;
        session
    }

    fn assert_conflict_disabled(&self) {
        assert!(!self.f.game.join(&self.conflict).exists());
        assert!(
            self.f
                .game
                .join(mods::disabled_dll_file(&self.conflict))
                .is_file()
        );
    }

    fn assert_target_untouched(&self) {
        assert!(self.from.exists());
        assert!(!self.to.exists());
        if self.id == MAP {
            assert!(
                !self.to.parent().unwrap().exists(),
                "later target's parent was created early"
            );
        }
    }
}

#[test]
fn map_and_single_mod_start_only_after_conflict_and_dependency_writes() {
    for map in [true, false] {
        let test = scenario(map);
        let f = &test.f;
        let mut session = test.prepared();
        test.assert_target_untouched();
        let mut observed = false;
        super::super::execution::commit(&f.state, &mut session, |progress| {
            let items = serde_json::to_value(progress).unwrap();
            if items["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["node_id"] == test.id && item["stage"] == "applying")
            {
                test.assert_target_untouched();
                test.assert_conflict_disabled();
                assert_eq!(
                    fs::read(f.game.join(&test.dependency)).unwrap(),
                    mods::plugin_blob(DEP_GUID, DEP, "2.0.0")
                );
                assert_eq!(
                    archive::read_manifest(&f.state.config.manifests_dir(), DEP)
                        .unwrap()
                        .release_tag,
                    "v2.0.0"
                );
                observed = true;
            }
        })
        .unwrap();
        assert!(observed);
        assert!(!test.from.exists());
        assert!(test.to.exists());
    }
}

#[test]
fn dependency_repair_of_disabled_map_or_mod_keeps_its_files_and_switch() {
    for map in [true, false] {
        let mut test = scenario(map);
        test.request = serde_json::from_value(serde_json::json!({
            "repair_dependencies_of": test.id,
        }))
        .unwrap();
        let before = Fingerprint::capture(&test.from).unwrap();
        let owner_manifest =
            archive::read_manifest(&test.f.state.config.manifests_dir(), "ATarget");
        let mut session = test.prepared();
        assert_eq!(session.plan.roots[0].node_id, test.id);
        let actions = &session.plan.result.actions;
        assert_eq!(actions.len(), 2, "{actions:?}");
        assert!(actions.iter().all(|action| action.node_id != test.id));
        let evaluation = session
            .plan
            .result
            .evaluations
            .iter()
            .find(|evaluation| evaluation.node_id == test.id)
            .unwrap();
        assert!(!evaluation.enabled && evaluation.runtime.is_empty());
        assert!(
            evaluation
                .readiness
                .iter()
                .any(|edge| edge.relation.guid == DEP_GUID)
        );
        assert!(
            evaluation
                .readiness
                .iter()
                .all(|edge| !edge.state.is_issue())
        );
        let check_target = || {
            test.assert_target_untouched();
            assert_eq!(Fingerprint::capture(&test.from).unwrap(), before);
        };
        check_target();
        super::super::execution::commit(&test.f.state, &mut session, |_| check_target()).unwrap();
        check_target();
        test.assert_conflict_disabled();
        assert_eq!(
            fs::read(test.f.game.join(&test.dependency)).unwrap(),
            mods::plugin_blob(DEP_GUID, DEP, "2.0.0")
        );
        assert_eq!(
            serde_json::to_value(archive::read_manifest(
                &test.f.state.config.manifests_dir(),
                "ATarget"
            ))
            .unwrap(),
            serde_json::to_value(owner_manifest).unwrap()
        );
    }
}

#[test]
fn a_repair_cannot_be_combined_with_a_target_switch_or_choose_an_uninstalled_owner() {
    let test = scenario(false);
    let mut mixed = test.request.clone();
    mixed.repair_dependencies_of = Some(test.id.into());
    assert_eq!(
        planning::create(&test.f.state, mixed).unwrap_err(),
        "modChanges.installationBlocked"
    );
    let nonexistent = PlanRequest {
        repair_dependencies_of: Some("mod:Unavailable".into()),
        ..Default::default()
    };
    assert_eq!(
        planning::create(&test.f.state, nonexistent).unwrap_err(),
        "modChanges.targetChanged"
    );
    test.assert_target_untouched();
}

#[test]
fn failed_dependency_write_never_reaches_map_or_mod_and_restores_prerequisites() {
    for map in [true, false] {
        let test = scenario(map);
        let f = &test.f;
        let mut session = test.prepared();
        let manifests = f.state.config.manifests_dir();
        let old_game = Fingerprint::capture(&f.game).unwrap();
        let old_manifests = Fingerprint::capture(&manifests).unwrap();
        let data_root = manifests.parent().unwrap().to_path_buf();
        let journal_root = data_root.join("mod-change-transactions");
        let roots = TransactionRoots {
            game_root: f.game.clone(),
            data_root,
        };
        let changes = session.mutations.take().unwrap();
        let payload_index = changes.iter().position(|owned| {
            matches!(&owned.mutation, Mutation::File(change)
                if change.target.root == RootKind::Game && change.target.relative == test.dependency && change.source.is_some())
        }).unwrap();
        let _operation = f.state.operation.lock().unwrap();
        let batch = prepare(
            &journal_root,
            &roots,
            changes.into_iter().map(|owned| owned.mutation).collect(),
        )
        .unwrap();
        let payload = journal_root
            .join(batch.id())
            .join(format!("after/{payload_index}"));
        let mut injected = false;
        let error = batch
            .apply_with_cancel(
                || false,
                |event| {
                    // This also checks all earlier writes, including creation of parent directories.
                    test.assert_target_untouched();
                    if matches!(event, RecordProgress::Applying(index) if index == payload_index) {
                        test.assert_conflict_disabled();
                        assert_eq!(
                            archive::read_manifest(&manifests, DEP).unwrap().release_tag,
                            "v2.0.0"
                        );
                        fs::remove_file(&payload).unwrap();
                        injected = true;
                    }
                },
            )
            .unwrap_err();
        assert!(injected);
        assert_eq!(error, "recycle.payloadModified");
        assert_eq!(Fingerprint::capture(&f.game).unwrap(), old_game);
        assert_eq!(Fingerprint::capture(&manifests).unwrap(), old_manifests);
        assert!(!has_pending(&journal_root).unwrap());
    }
}
