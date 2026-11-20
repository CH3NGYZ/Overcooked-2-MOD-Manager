use super::*;
use crate::mod_changes::{execution, payload};
use crate::mod_preview::staging::{ORIGIN_FILE, PreviewOriginRecord, preview_root};

const DIYLEVEL: &str = "DIYLevel";
const DIYLEVEL_GUID: &str = mods::plugin_target::DIYLEVEL_GUID;
const WEB: &str = "DIYLevelWeb";
const WEB_GUID: &str = "oc2.oc2diylevelruntimewloader";
const MULTIPART: &str = "MultipartFix";
const MULTIPART_GUID: &str = "com.example.multipart";
const SORTING: &str = "ExperienceOptimization";
const SORTING_GUID: &str = "com.example.experience";
const BLOCKER: &str = "ResourceReadFailureBlocker";
const BLOCKER_GUID: &str = "com.example.resourcereadfailure";
const RENAMED_CURRENT: &str = "BepInEx/plugins/FastInit (2).dll";
const LEGACY_TARGET: &str = "BepInEx/plugins/FastInit.dll";
const LEGACY_OTHER: &str = "BepInEx/plugins/Z-FastInit/FastInit (old).dll";

fn conflicts(guid: &str, version: &str) -> RepositoryRelation {
    RepositoryRelation {
        kind: RelationKind::Conflicts,
        ..requires(guid, version)
    }
}

fn scenario() -> Fixture {
    // Keep the reported bundled declarations here so the regression runs even
    // on source archives that still lock an older repository-source revision.
    let mut framework = repository(
        DIYLEVEL,
        DIYLEVEL_GUID,
        &["0.11.1", "0.11.0", "0.10.0"],
        vec![],
    );
    framework.release_relations = vec![ReleaseRelationOverride {
        version: ">=0.10.0,<=0.11.1".into(),
        relations: vec![
            requires(MULTIPART_GUID, ">=0.2.3"),
            requires(SORTING_GUID, ">=1.14.2"),
            requires(GUID, ">=1.9.6"),
            conflicts(OLD_GUID, "==1.3.0"),
        ],
    }];
    let mut web = repository(WEB, WEB_GUID, &["4.0.0", "3.10.2"], vec![]);
    web.release_relations = vec![
        ReleaseRelationOverride {
            version: ">=3.10.2,<4.0.0".into(),
            relations: vec![requires(DIYLEVEL_GUID, ">=0.10.0,<0.11.0")],
        },
        ReleaseRelationOverride {
            version: ">=4.0.0,<5.0.0".into(),
            relations: vec![requires(DIYLEVEL_GUID, ">=0.11.0,<0.12.0")],
        },
    ];
    let mut fast = repository(
        FASTINIT,
        GUID,
        &["1.9.6", "1.9.5"],
        vec![requires(DIYLEVEL_GUID, "")],
    );
    fast.install_target = "BepInEx/plugins".into();
    fast.local_product_name = "FastInit".into();
    fast.local_original_filename = "FastInit.dll".into();
    fast.release_relations = vec![ReleaseRelationOverride {
        version: ">=1.9.0".into(),
        relations: vec![
            requires(DIYLEVEL_GUID, ">=0.10.0"),
            conflicts(BLOCKER_GUID, ""),
        ],
    }];
    let f = fixture(vec![
        framework,
        web,
        fast,
        repository(
            MULTIPART,
            MULTIPART_GUID,
            &["0.2.3", "0.2.2"],
            vec![requires(DIYLEVEL_GUID, "")],
        ),
        repository(
            SORTING,
            SORTING_GUID,
            &["1.14.2"],
            vec![requires(DIYLEVEL_GUID, "")],
        ),
        repository(
            BLOCKER,
            BLOCKER_GUID,
            &["0.4.0"],
            vec![
                requires(DIYLEVEL_GUID, "==0.10.0"),
                conflicts(GUID, ">=1.9.0"),
            ],
        ),
    ]);
    install(&f, DIYLEVEL, DIYLEVEL_GUID, "0.10.0", true);
    install(&f, WEB, WEB_GUID, "3.10.2", true);
    install(&f, MULTIPART, MULTIPART_GUID, "0.2.2", true);
    install(&f, BLOCKER, BLOCKER_GUID, "0.4.0", true);
    // The scan binds loose, physically renamed copies from their PE identity.
    write(
        &f.game.join(RENAMED_CURRENT),
        &installed_fastinit_bytes(GUID, "1.6.4"),
    );
    write(
        &f.game.join(LEGACY_TARGET),
        &installed_fastinit_bytes(OLD_GUID, "1.3.0"),
    );
    f
}

fn update_repository(f: &Fixture, id: &str, edit: impl FnOnce(&mut ModRepositoryConfig)) {
    let mut config = f.state.config.load_or_default();
    edit(
        config
            .repositories
            .iter_mut()
            .find(|repo| repo.id == id)
            .unwrap(),
    );
    f.state.config.save(&config).unwrap();
}

#[test]
fn a_cached_fastinit_upgrade_can_be_required_by_web_or_diylevel() {
    let f = scenario();
    let before = game_and_manifest_bytes(&f);
    let snapshot = inventory::build(&f.state).unwrap();
    let copies = snapshot
        .files
        .values()
        .filter(|entry| entry.summary.repository_id.as_deref() == Some(FASTINIT));
    assert_eq!(
        copies.count(),
        2,
        "both physically distinct DLLs must bind to FastInit"
    );
    assert!(snapshot.config.repositories.iter().any(|repo| {
        repo.id == FASTINIT
            && super::super::super::candidates::for_repository(repo, None, false, &snapshot.config)
                .iter()
                .any(|candidate| candidate.tag.as_deref() == Some(TARGET))
    }));
    let direct = planning::create(&f.state, request(FASTINIT, Some(TARGET), true)).unwrap();
    assert_eq!(
        direct.result.status,
        SolveStatus::Ready,
        "direct FastInit upgrade: {:?}",
        direct.result.blockers
    );
    execution::cancel(&f.state, &direct.plan_id).unwrap();
    for (repo, tag) in [(WEB, "v4.0.0"), (DIYLEVEL, "v0.11.1")] {
        let plan = planning::create(&f.state, request(repo, Some(tag), true)).unwrap();
        assert_eq!(
            plan.result.status,
            SolveStatus::Ready,
            "{repo}: {:?}",
            plan.result.blockers
        );
        assert!(
            plan.result
                .actions
                .iter()
                .any(|action| action.node_id == NODE
                    && action.kind == ActionKind::Upgrade
                    && action.to_tag.as_deref() == Some(TARGET))
        );
        assert_eq!(plan.retired_mod_copies[NODE][0].dll_file, LEGACY_TARGET);
        for (id, kind) in [
            (MULTIPART, ActionKind::Upgrade),
            (SORTING, ActionKind::Install),
            (BLOCKER, ActionKind::Disable),
        ] {
            assert!(
                plan.result
                    .actions
                    .iter()
                    .any(|action| action.node_id == format!("mod:{id}") && action.kind == kind),
                "{id}: {:?}",
                plan.result.actions
            );
        }
        execution::cancel(&f.state, &plan.plan_id).unwrap();
        assert_eq!(game_and_manifest_bytes(&f), before);
    }
}

#[test]
fn dependency_upgrade_replaces_the_canonical_dll_only_after_confirmation() {
    let f = scenario();
    let before = game_and_manifest_bytes(&f);
    let plan = planning::create(&f.state, request(WEB, Some("v4.0.0"), true)).unwrap();
    assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    assert_eq!(plan.retired_mod_copies[NODE][0].dll_file, LEGACY_TARGET);
    assert_eq!(game_and_manifest_bytes(&f), before);

    let mut session = take(&f, &plan);
    verify_selected(&f, &mut session);
    preparation::finish(&f.state, &mut session).unwrap();
    assert_eq!(session.plan.retired_mod_copies, plan.retired_mod_copies);
    assert_eq!(
        execution::commit(&f.state, &mut session, |_| {}).unwrap_err(),
        "modChanges.notPrepared"
    );
    assert_eq!(game_and_manifest_bytes(&f), before);

    session.plan.prepared = true;
    execution::commit(&f.state, &mut session, |_| {}).unwrap();
    assert!(!f.game.join(RENAMED_CURRENT).exists());
    let installed = inventory::build(&f.state).unwrap();
    let copies = installed
        .installed_mods
        .iter()
        .filter(|item| item.repository_id.as_deref() == Some(FASTINIT))
        .collect::<Vec<_>>();
    assert_eq!(copies.len(), 1, "{copies:?}");
    assert_eq!(copies[0].dll_file, LEGACY_TARGET);
    assert_eq!(copies[0].guid.as_deref(), Some(GUID));
    assert_eq!(copies[0].version.as_deref(), Some("1.9.6"));
    assert!(copies[0].enabled);
    let manifest = archive::read_manifest(&f.state.config.manifests_dir(), FASTINIT).unwrap();
    assert_eq!(manifest.release_tag, TARGET);
    assert_eq!(manifest.files, [LEGACY_TARGET]);
    assert_eq!(manifest.repository_binding.as_ref().unwrap().tag, TARGET);
}

fn only_historical_guids() -> Fixture {
    let f = scenario();
    fs::remove_file(f.game.join(RENAMED_CURRENT)).unwrap();
    retained_repo_copy(
        &f,
        "FastInit-second-legacy",
        LEGACY_OTHER,
        OLD_GUID,
        "1.3.0",
    );
    f
}

#[test]
fn web_requires_modern_fastinit_and_retires_all_proven_old_guid_copies() {
    let f = only_historical_guids();
    let before = game_and_manifest_bytes(&f);
    let plan = planning::create(&f.state, request(WEB, Some("v4.0.0"), true)).unwrap();
    assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    assert_eq!(
        plan.retired_mod_copies[NODE]
            .iter()
            .map(|copy| copy.dll_file.as_str())
            .collect::<std::collections::BTreeSet<_>>(),
        [LEGACY_TARGET, LEGACY_OTHER].into()
    );
    let mut session = take(&f, &plan);
    verify_selected(&f, &mut session);
    preparation::finish(&f.state, &mut session).unwrap();
    assert_eq!(session.plan.retired_mod_copies, plan.retired_mod_copies);
    assert_eq!(game_and_manifest_bytes(&f), before);
    session.plan.prepared = true;
    execution::commit(&f.state, &mut session, |_| {}).unwrap();

    let installed = inventory::build(&f.state).unwrap();
    let mut providers = installed.installed_mods.iter().filter(|item| {
        item.guid
            .as_deref()
            .is_some_and(|guid| [GUID, OLD_GUID].contains(&guid))
    });
    let current = providers.next().unwrap();
    assert!(providers.next().is_none());
    assert_eq!(current.guid.as_deref(), Some(GUID));
    assert_eq!(current.version.as_deref(), Some("1.9.6"));
    assert!(current.enabled);
    assert!(!f.game.join(LEGACY_OTHER).exists());
    assert!(
        archive::read_manifest(&f.state.config.manifests_dir(), "FastInit-second-legacy").is_none()
    );
}

#[test]
fn a_single_historical_fastinit_can_be_replaced_directly_or_as_a_dependency() {
    let f = scenario();
    fs::remove_file(f.game.join(RENAMED_CURRENT)).unwrap();
    let before = game_and_manifest_bytes(&f);
    for (repo, tag) in [(FASTINIT, TARGET), (WEB, "v4.0.0")] {
        let plan = planning::create(&f.state, request(repo, Some(tag), true)).unwrap();
        assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
        let [old] = plan.retired_mod_copies[NODE].as_slice() else {
            panic!("exactly one historical DLL should be retired");
        };
        assert_eq!(old.dll_file, LEGACY_TARGET);
        let mut session = take(&f, &plan);
        verify_selected(&f, &mut session);
        preparation::finish(&f.state, &mut session).unwrap();
        assert_eq!(session.plan.retired_mod_copies, plan.retired_mod_copies);
        assert_eq!(game_and_manifest_bytes(&f), before);
        execution::cleanup(&f.state, &session.staging).unwrap();
    }
}

#[test]
fn conflict_and_old_copies_leave_before_new_writes_and_restore_on_write_failure() {
    let f = only_historical_guids();
    let before = game_and_manifest_bytes(&f);
    let plan = planning::create(&f.state, request(WEB, Some("v4.0.0"), true)).unwrap();
    let mut session = take(&f, &plan);
    verify_selected(&f, &mut session);
    preparation::finish(&f.state, &mut session).unwrap();
    let changes = session
        .mutations
        .take()
        .unwrap()
        .into_iter()
        .map(|change| change.mutation)
        .collect::<Vec<_>>();
    let first_payload = changes
        .iter()
        .position(|mutation| matches!(mutation, Mutation::File(change) if change.source.is_some()))
        .unwrap();
    // The canonical destination still belongs to the old GUID. Replace it
    // once, atomically, after other retirements and before dependent writes.
    assert!(matches!(&changes[first_payload], Mutation::File(change)
        if change.target.relative == LEGACY_TARGET && change.require_single_link));
    let manifest_dir = f.state.config.manifests_dir();
    let data_root = manifest_dir.parent().unwrap().to_path_buf();
    let journal_root = data_root.join("mod-change-transactions");
    let roots = TransactionRoots {
        game_root: f.game.clone(),
        data_root,
    };
    let _operation = f.state.operation.lock().unwrap();
    let batch = prepare(&journal_root, &roots, changes).unwrap();
    let missing_payload = journal_root
        .join(batch.id())
        .join(format!("after/{first_payload}"));
    let mut observed = false;
    let outcome = batch.apply_with_cancel(
        || false,
        |event| {
            if matches!(event, RecordProgress::Applying(index) if index == first_payload) {
                assert!(!f.game.join(LEGACY_OTHER).exists());
                assert!(!manifest_dir.join("FastInit-second-legacy.toml").exists());
                let conflict = format!("BepInEx/plugins/{BLOCKER}/{BLOCKER}.dll");
                assert!(!f.game.join(&conflict).exists());
                assert!(f.game.join(mods::disabled_dll_file(&conflict)).exists());
                assert_eq!(
                    fs::read(f.game.join(LEGACY_TARGET)).unwrap(),
                    installed_fastinit_bytes(OLD_GUID, "1.3.0")
                );
                // Simulate a staged file disappearing after preflight. The real
                // writer must roll back every already-completed conflict change.
                fs::remove_file(&missing_payload).unwrap();
                observed = true;
            }
        },
    );
    assert!(observed);
    assert_eq!(outcome.unwrap_err(), "recycle.payloadModified");
    assert_eq!(game_and_manifest_bytes(&f), before);
}

#[test]
fn cached_upgrade_reports_ambiguous_active_and_disabled_fastinit_copies() {
    let f = scenario();
    write(
        &f.game.join(mods::disabled_dll_file(LEGACY_TARGET)),
        &installed_fastinit_bytes(OLD_GUID, "1.3.0"),
    );
    let before = game_and_manifest_bytes(&f);
    let plan = planning::create(&f.state, request(WEB, Some("v4.0.0"), true)).unwrap();
    assert_ne!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    assert!(plan.result.actions.is_empty());
    assert!(plan.retired_mod_copies.is_empty());
    assert!(
        plan.result
            .blockers
            .iter()
            .flat_map(|blocker| &blocker.traces)
            .any(|trace| {
                trace.last().is_some_and(|step| {
                    step.reason == "modChanges.multipleInstalledCopies"
                        && step
                            .relation
                            .as_ref()
                            .is_some_and(|r| r.guid == GUID && r.version == ">=1.9.6")
                })
            }),
        "{:?}",
        plan.result.blockers
    );
    execution::cancel(&f.state, &plan.plan_id).unwrap();
    assert_eq!(game_and_manifest_bytes(&f), before);
}

#[test]
fn repairing_installed_diylevel_preserves_its_version_and_can_upgrade_fastinit() {
    let f = scenario();
    let before = game_and_manifest_bytes(&f);
    let plan = planning::create(&f.state, request(DIYLEVEL, None, true)).unwrap();
    assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    assert_eq!(plan.roots[0].node_id, "mod:DIYLevel");
    assert!(plan.roots[0].tag.is_none());
    assert_eq!(plan.retired_mod_copies[NODE][0].dll_file, LEGACY_TARGET);
    assert!(
        plan.result
            .actions
            .iter()
            .any(|action| action.node_id == NODE
                && action.kind == ActionKind::Upgrade
                && action.to_tag.as_deref() == Some(TARGET))
    );
    for unchanged in [DIYLEVEL, WEB] {
        assert!(
            !plan
                .result
                .actions
                .iter()
                .any(|action| action.node_id == format!("mod:{unchanged}"))
        );
    }
    execution::cancel(&f.state, &plan.plan_id).unwrap();
    assert_eq!(game_and_manifest_bytes(&f), before);
}

#[test]
fn installed_switches_do_not_authorize_fastinit_retirement() {
    let f = scenario();
    let before = game_and_manifest_bytes(&f);
    let single = super::super::unbound_toggle::toggle(&f, "mod:DIYLevel", None, true);
    let mut bulk = request(DIYLEVEL, None, true);
    bulk.bulk_toggle = true;
    for toggle in [single, bulk] {
        let plan = planning::create(&f.state, toggle).unwrap();
        assert!(plan.retired_mod_copies.is_empty());
        assert!(
            !plan
                .result
                .actions
                .iter()
                .any(|action| action.node_id == NODE)
        );
        execution::cancel(&f.state, &plan.plan_id).unwrap();
    }
    assert_eq!(game_and_manifest_bytes(&f), before);
}

#[test]
fn required_legacy_guid_still_blocks_retirement() {
    let f = only_historical_guids();
    // The new root explicitly keeps an enabled consumer that still requires
    // the retired GUID. The new actual GUID cannot satisfy that relation.
    update_repository(&f, WEB, |repo| {
        repo.release_relations[1]
            .relations
            .push(requires(DEPENDENT_GUID, "*"));
    });
    let mut config = f.state.config.load_or_default();
    config.repositories.push(repository(
        DEPENDENT,
        DEPENDENT_GUID,
        &["1.0.0"],
        vec![requires(OLD_GUID, ">=1.3")],
    ));
    f.state.config.save(&config).unwrap();
    install(&f, DEPENDENT, DEPENDENT_GUID, "1.0.0", true);
    let before = game_and_manifest_bytes(&f);
    let plan = planning::create(&f.state, request(WEB, Some("v4.0.0"), true)).unwrap();
    assert_ne!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    assert!(plan.retired_mod_copies.is_empty());
    assert!(
        plan.result
            .blockers
            .iter()
            .flat_map(|blocker| &blocker.traces)
            .any(|trace| {
                trace
                    .last()
                    .and_then(|step| step.relation.as_ref())
                    .is_some_and(|relation| relation.guid == OLD_GUID)
            }),
        "{:?}",
        plan.result.blockers
    );
    execution::cancel(&f.state, &plan.plan_id).unwrap();
    assert_eq!(game_and_manifest_bytes(&f), before);
}

/// Feed exact offline Release provenance through the real payload checker,
/// then cache precisely the candidate observed in its DLL for the next solve.
fn verify_asset(f: &Fixture, session: &mut PlanSession, repo_id: &str, actual_version: &str) {
    let node_id = format!("mod:{repo_id}");
    let candidate = session
        .solved
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|node| node.id == node_id)
        .unwrap()
        .current
        .as_ref()
        .unwrap()
        .clone();
    assert!(candidate.verification_pending);
    let repo = session
        .trusted
        .config
        .repositories
        .iter()
        .find(|repo| repo.id == repo_id)
        .unwrap();
    let preview_id = uuid::Uuid::new_v4().to_string();
    let directory = preview_root(&f.state).join(&preview_id);
    write(
        &directory.join(format!("{repo_id}.dll")),
        &mods::plugin_blob(&repo.plugin_guid, &repo.name, actual_version),
    );
    write(
        &directory.join(ORIGIN_FILE),
        &serde_json::to_vec(&PreviewOriginRecord {
            repository_id: repo_id.into(),
            release_tag: candidate.tag.clone().unwrap(),
            prerelease: false,
        })
        .unwrap(),
    );
    let verified = payload::from_preview(
        &f.state,
        &session.trusted,
        &preview_id,
        &node_id,
        &candidate,
        None,
        None,
        &session.staging.join(format!("verified-{preview_id}")),
    )
    .unwrap();
    assert_eq!(verified.candidate.version.as_deref(), Some(actual_version));
    let candidates = &mut session
        .trusted
        .inventory
        .nodes
        .iter_mut()
        .find(|node| node.id == node_id)
        .unwrap()
        .candidates;
    *candidates
        .iter_mut()
        .find(|entry| entry.id == candidate.id)
        .unwrap() = verified.candidate.clone();
    session
        .verified_payloads
        .insert((node_id, candidate.id), verified);
    planning::remember_payloads(session).unwrap();
}

#[test]
fn an_unselected_verified_fastinit_upgrade_cannot_retire_copies_after_resolve() {
    let f = scenario();
    update_repository(&f, DIYLEVEL, |repo| {
        repo.release_relations.push(ReleaseRelationOverride {
            version: "==0.11.0".into(),
            relations: vec![
                requires(MULTIPART_GUID, ">=0.2.2"),
                requires(GUID, ">=1.6.4"),
            ],
        });
    });
    let before = game_and_manifest_bytes(&f);
    let plan = planning::create(&f.state, request(WEB, Some("v4.0.0"), true)).unwrap();
    assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    assert!(
        plan.result
            .actions
            .iter()
            .any(|action| action.node_id == "mod:DIYLevel"
                && action.to_tag.as_deref() == Some("v0.11.1"))
    );
    assert!(plan.retired_mod_copies.contains_key(NODE));
    let mut session = take(&f, &plan);
    verify_asset(&f, &mut session, FASTINIT, "1.9.6");
    // A previous preparation attempt may have selected this verified payload.
    let fastinit =
        session.verified_payloads[&(NODE.into(), format!("release:{FASTINIT}:{TARGET}"))].clone();
    session.payloads.insert(NODE.into(), fastinit);
    verify_asset(&f, &mut session, MULTIPART, "0.2.2");
    planning::resolve(&f.state, &mut session).unwrap();
    assert_eq!(
        session.plan.result.status,
        SolveStatus::Ready,
        "{:?}",
        session.plan.result
    );
    assert!(
        session
            .plan
            .result
            .actions
            .iter()
            .any(|action| action.node_id == "mod:DIYLevel"
                && action.to_tag.as_deref() == Some("v0.11.0"))
    );
    assert!(session.plan.retired_mod_copies.is_empty());
    assert!(
        !session
            .plan
            .result
            .actions
            .iter()
            .any(|action| action.node_id == NODE)
    );

    verify_asset(&f, &mut session, DIYLEVEL, "0.11.0");
    verify_asset(&f, &mut session, WEB, "4.0.0");
    planning::resolve(&f.state, &mut session).unwrap();
    preparation::select_payloads(&mut session).unwrap();
    assert!(!session.payloads.contains_key(NODE));
    assert!(session.verified_payloads.keys().any(|(id, _)| id == NODE));
    preparation::finish(&f.state, &mut session).unwrap();
    let untouched = [RENAMED_CURRENT, LEGACY_TARGET].map(inventory::key);
    let writes_to_copy = |path: &TransactionPath| {
        let relative = inventory::key(&path.relative);
        path.root == RootKind::Game
            && untouched.iter().any(|copy| {
                copy == &relative
                    || copy
                        .strip_prefix(&relative)
                        .is_some_and(|tail| tail.starts_with('/'))
            })
    };
    for change in session.mutations.as_ref().unwrap() {
        assert!(
            !match &change.mutation {
                Mutation::File(change) => writes_to_copy(&change.target),
                Mutation::Tree(change) => writes_to_copy(&change.target),
                Mutation::Move(change) =>
                    writes_to_copy(&change.from) || writes_to_copy(&change.to),
            },
            "unexpected FastInit write: {change:?}"
        );
    }
    assert_eq!(game_and_manifest_bytes(&f), before);
    session.plan.prepared = true;
    execution::commit(&f.state, &mut session, |_| {}).unwrap();

    for (path, guid, version) in [
        (RENAMED_CURRENT, GUID, "1.6.4"),
        (LEGACY_TARGET, OLD_GUID, "1.3.0"),
    ] {
        assert_eq!(
            fs::read(f.game.join(path)).unwrap(),
            installed_fastinit_bytes(guid, version)
        );
    }
    let manifests = f.state.config.manifests_dir();
    assert!(archive::read_manifest(&manifests, FASTINIT).is_none());
    assert_eq!(
        archive::read_manifest(&manifests, DIYLEVEL)
            .unwrap()
            .release_tag,
        "v0.11.0"
    );
}
