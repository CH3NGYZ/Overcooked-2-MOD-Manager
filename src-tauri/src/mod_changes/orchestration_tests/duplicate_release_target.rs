use super::*;

#[cfg(windows)]
#[path = "duplicate_release_target/dependencies.rs"]
mod dependencies;
#[path = "duplicate_release_target/release_contents.rs"]
mod release_contents;
#[path = "duplicate_release_target/tied_copies.rs"]
mod tied_copies;

const FASTINIT: &str = "FastInit";
const GUID: &str = "oc2.diylevel.fastinit";
const OLD_GUID: &str = "yang.oc2.diylevel.fastinit";
const CURRENT: &str = "BepInEx/plugins/FastInit/FastInit.dll";
const OLDER: &str = "BepInEx/plugins/FastInit-1.3/FastInit.dll";
const DISABLED_OLDER: &str = "BepInEx/plugins/FastInit-1.4/FastInit.dll.disabled";
const NODE: &str = "mod:FastInit";
const DEPENDENT: &str = "OldGuidConsumer";
const DEPENDENT_GUID: &str = "fixture-old-guid-consumer";
const TARGET: &str = "v1.9.6";

fn installed_fastinit_bytes(guid: &str, version: &str) -> Vec<u8> {
    // Exercise the real file reader; equal source IDs alone do not prove that
    // a historical GUID belongs to the core plugin.
    let mut bytes = include_bytes!("duplicate_release_target/fastinit_version.pe").to_vec();
    let name = if guid == OLD_GUID {
        "Legacy FastInit"
    } else {
        FASTINIT
    };
    bytes.extend(mods::plugin_blob(guid, name, version));
    bytes
}

fn retained_repo_copy(f: &Fixture, manifest_id: &str, path: &str, guid: &str, version: &str) {
    let tag = format!("v{version}");
    let config = f.state.config.load_or_default();
    let repo = config
        .repositories
        .iter()
        .find(|r| r.id == FASTINIT)
        .unwrap();
    write(&f.game.join(path), &installed_fastinit_bytes(guid, version));
    let mut manifest = archive::new_manifest(
        manifest_id,
        &tag,
        BTreeMap::new(),
        BTreeMap::new(),
        vec![path.into()],
        vec![],
    );
    manifest.repository_binding = ModRepositoryBinding::from_release(repo, &tag);
    archive::write_manifest(&f.state.config.manifests_dir(), &manifest).unwrap();
}

fn fastinit_release_fixture(
    asset_name: &str,
    required_guid: &str,
    enabled_guid: &str,
    disabled_guid: &str,
) -> Fixture {
    let mut repo = repository(FASTINIT, GUID, &["1.9.6"], vec![]);
    repo.asset_rules[0].value = asset_name.into();
    repo.cached_releases[0].assets[0].name = asset_name.into();
    let f = fixture(vec![
        repo,
        repository(
            DEPENDENT,
            DEPENDENT_GUID,
            &["1.0.0"],
            vec![requires(required_guid, ">=1.0")],
        ),
    ]);
    install(&f, FASTINIT, GUID, "1.9.5", true);
    write(
        &f.game.join(CURRENT),
        &installed_fastinit_bytes(GUID, "1.9.5"),
    );
    retained_repo_copy(&f, "FastInit-legacy-enabled", OLDER, enabled_guid, "1.3");
    retained_repo_copy(
        &f,
        "FastInit-legacy-disabled",
        DISABLED_OLDER,
        disabled_guid,
        "1.4",
    );
    install(&f, DEPENDENT, DEPENDENT_GUID, "1.0.0", true);
    f
}

#[cfg(windows)]
fn renamed_fastinit_fixture() -> Fixture {
    fastinit_release_fixture("FastInit.dll", OLD_GUID, OLD_GUID, OLD_GUID)
}

fn same_guid_fastinit_fixture() -> Fixture {
    fastinit_release_fixture("FastInit.dll", GUID, GUID, GUID)
}

fn game_and_manifest_bytes(f: &Fixture) -> (BTreeMap<String, Vec<u8>>, BTreeMap<String, Vec<u8>>) {
    fn read_files(root: &Path) -> BTreeMap<String, Vec<u8>> {
        walkdir::WalkDir::new(root)
            .into_iter()
            .map(Result::unwrap)
            .filter(|entry| entry.file_type().is_file())
            .map(|entry| {
                let name = entry
                    .path()
                    .strip_prefix(root)
                    .unwrap()
                    .display()
                    .to_string();
                (name, fs::read(entry.path()).unwrap())
            })
            .collect()
    }
    (
        read_files(&f.game),
        read_files(&f.state.config.manifests_dir()),
    )
}

#[test]
#[cfg(windows)]
fn exact_release_retires_proven_old_guids_only_in_the_confirmed_transaction() {
    let f = renamed_fastinit_fixture();
    let before = game_and_manifest_bytes(&f);
    let dependent = format!("BepInEx/plugins/{DEPENDENT}/{DEPENDENT}.dll");
    let dependent_node = format!("mod:{DEPENDENT}");
    let installed = inventory::build(&f.state).unwrap();
    let dependent_before = super::super::evaluate(&installed.inventory)
        .into_iter()
        .find(|evaluation| evaluation.node_id == dependent_node)
        .unwrap();
    assert_eq!(
        dependent_before.runtime[0].state,
        EvaluationState::Satisfied
    );
    let plan = planning::create(&f.state, request(FASTINIT, Some(TARGET), true)).unwrap();
    assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    let copies = plan.retired_mod_copies[NODE]
        .iter()
        .map(|copy| (copy.dll_file.as_str(), copy.version.as_str()))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(copies, [(OLDER, "1.3"), (DISABLED_OLDER, "1.4")].into());
    assert!(!plan.prepared);
    assert_eq!(game_and_manifest_bytes(&f), before);

    let mut session = take(&f, &plan);
    assert_eq!(
        super::super::execution::commit(&f.state, &mut session, |_| {}).unwrap_err(),
        "modChanges.notPrepared"
    );
    verify_selected(&f, &mut session);
    preparation::finish(&f.state, &mut session).unwrap();
    assert_eq!(game_and_manifest_bytes(&f), before);
    assert_eq!(session.plan.retired_mod_copies, plan.retired_mod_copies);
    let actions = &session.plan.result.actions;
    assert!(
        actions.iter().any(|action| {
            action.node_id == NODE
                && action.kind == ActionKind::Upgrade
                && action.from_tag.as_deref() == Some("v1.9.5")
                && action.to_tag.as_deref() == Some(TARGET)
        }),
        "{actions:?}"
    );
    assert!(
        actions.iter().any(|action| {
            action.node_id == dependent_node
                && action.kind == ActionKind::Disable
                && action
                    .reason_details
                    .iter()
                    .any(|reason| reason.target_guid.as_deref() == Some(OLD_GUID))
        }),
        "{actions:?}"
    );

    let mut updates = Vec::new();
    session.plan.prepared = true;
    super::super::execution::commit(&f.state, &mut session, |update| {
        updates.push(serde_json::to_value(update).unwrap());
    })
    .unwrap();
    let final_update = updates.last().unwrap();
    for copy in &plan.retired_mod_copies[NODE] {
        assert!(
            final_update["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| {
                    item["node_id"] == NODE
                        && item["member_key"] == copy.member_key
                        && item["stage"] == "applied"
                }),
            "{final_update}"
        );
    }
    assert_eq!(
        fs::read(f.game.join(CURRENT)).unwrap(),
        mods::plugin_blob(GUID, FASTINIT, "1.9.6")
    );
    for path in [OLDER, DISABLED_OLDER, &dependent] {
        assert!(!f.game.join(path).exists(), "{path} was not retired");
    }
    assert_eq!(
        fs::read(f.game.join(mods::disabled_dll_file(&dependent))).unwrap(),
        mods::plugin_blob(DEPENDENT_GUID, DEPENDENT, "1.0.0")
    );
    let installed_state = inventory::build(&f.state).unwrap();
    let dependent_after = super::super::evaluate(&installed_state.inventory)
        .into_iter()
        .find(|evaluation| evaluation.node_id == dependent_node)
        .unwrap();
    assert!(!dependent_after.enabled);
    assert!(dependent_after.runtime.is_empty());
    assert_eq!(dependent_after.readiness[0].state, EvaluationState::Missing);
    let manifests = f.state.config.manifests_dir();
    let installed = archive::read_manifest(&manifests, FASTINIT).unwrap();
    assert_eq!(installed.release_tag, TARGET);
    assert_eq!(installed.files, [CURRENT]);
    for manifest in ["FastInit-legacy-enabled", "FastInit-legacy-disabled"] {
        assert!(!manifests.join(format!("{manifest}.toml")).exists());
    }
}

#[test]
fn retiring_a_disabled_same_guid_copy_preserves_an_enabled_compatible_consumer() {
    let f = same_guid_fastinit_fixture();
    let plan = planning::create(&f.state, request(FASTINIT, Some(TARGET), true)).unwrap();
    assert!(
        plan.retired_mod_copies[NODE]
            .iter()
            .any(|copy| copy.dll_file == DISABLED_OLDER)
    );
    let mut session = take(&f, &plan);
    verify_selected(&f, &mut session);
    preparation::finish(&f.state, &mut session).unwrap();
    let dependent_node = format!("mod:{DEPENDENT}");
    assert!(
        session
            .plan
            .result
            .actions
            .iter()
            .all(|action| action.node_id != dependent_node)
    );
    session.plan.prepared = true;
    super::super::execution::commit(&f.state, &mut session, |_| {}).unwrap();
    let installed = inventory::build(&f.state).unwrap();
    let dependent_after = super::super::evaluate(&installed.inventory)
        .into_iter()
        .find(|evaluation| evaluation.node_id == dependent_node)
        .unwrap();
    assert!(dependent_after.enabled);
    assert_eq!(dependent_after.runtime[0].state, EvaluationState::Satisfied);
    assert!(!f.game.join(DISABLED_OLDER).exists());
}

#[test]
fn duplicate_retirement_does_not_change_other_explicit_roots() {
    let f = same_guid_fastinit_fixture();
    let before = game_and_manifest_bytes(&f);
    let mut selection = request(FASTINIT, Some(TARGET), true);
    selection.roots.extend(request(DEPENDENT, None, true).roots);
    assert_eq!(
        planning::create(&f.state, selection).unwrap_err(),
        "modChanges.multipleInstalledCopies"
    );
    assert_eq!(game_and_manifest_bytes(&f), before);
}

#[test]
fn exact_release_that_reuses_a_legacy_path_reports_its_retirement_and_keeps_new_dll() {
    let f = same_guid_fastinit_fixture();
    let manifests = f.state.config.manifests_dir();
    // The old DLL is at the repo's install target. The newer current copy
    // was installed in an alternate directory, both under the same DLL name.
    let canonical = fs::read(f.game.join(CURRENT)).unwrap();
    let legacy = fs::read(f.game.join(OLDER)).unwrap();
    write(&f.game.join(CURRENT), &legacy);
    write(&f.game.join(OLDER), &canonical);
    for (id, path) in [(FASTINIT, OLDER), ("FastInit-legacy-enabled", CURRENT)] {
        let mut manifest = archive::read_manifest(&manifests, id).unwrap();
        manifest.files = vec![path.into()];
        archive::write_manifest(&manifests, &manifest).unwrap();
    }
    let before = game_and_manifest_bytes(&f);
    let plan = planning::create(&f.state, request(FASTINIT, Some(TARGET), true)).unwrap();
    let retired = plan.retired_mod_copies[NODE]
        .iter()
        .find(|copy| copy.dll_file == CURRENT)
        .expect("the existing legacy DLL at the replacement path is part of the plan");
    assert_eq!(retired.version, "1.3");
    let mut session = take(&f, &plan);
    verify_selected(&f, &mut session);
    preparation::finish(&f.state, &mut session).unwrap();
    assert_eq!(game_and_manifest_bytes(&f), before);
    session.plan.prepared = true;
    let mut applied = false;
    super::super::execution::commit(&f.state, &mut session, |update| {
        let update = serde_json::to_value(update).unwrap();
        applied |= update["items"].as_array().unwrap().iter().any(|item| {
            item["node_id"] == NODE
                && item["member_key"] == retired.member_key
                && item["stage"] == "applied"
        });
    })
    .unwrap();
    assert!(applied);
    assert_eq!(
        fs::read(f.game.join(CURRENT)).unwrap(),
        mods::plugin_blob(GUID, FASTINIT, "1.9.6")
    );
    assert!(!f.game.join(OLDER).exists());
    assert!(!f.game.join(DISABLED_OLDER).exists());
    assert!(!manifests.join("FastInit-legacy-enabled.toml").exists());
    assert_eq!(
        archive::read_manifest(&manifests, FASTINIT).unwrap().files,
        [CURRENT]
    );
}

#[test]
fn retiring_copies_changes_neither_files_nor_manifests_if_prepared_plan_is_cancelled() {
    let f = same_guid_fastinit_fixture();
    let before = game_and_manifest_bytes(&f);
    let plan = planning::create(&f.state, request(FASTINIT, Some(TARGET), true)).unwrap();
    let stage = {
        let mut lease = SessionLease::take(&f.state, &plan.plan_id).unwrap();
        let session = lease.session.as_mut().unwrap();
        verify_selected(&f, session);
        preparation::finish(&f.state, session).unwrap();
        session.plan.prepared = true;
        assert!(session.staging.is_dir());
        session.staging.clone()
    };
    super::super::execution::cancel(&f.state, &plan.plan_id).unwrap();
    assert!(!stage.exists());
    assert_eq!(
        SessionLease::take(&f.state, &plan.plan_id).err().as_deref(),
        Some("modChanges.planExpired")
    );
    assert_eq!(game_and_manifest_bytes(&f), before);
}

#[test]
fn cancelling_after_retirement_writes_restores_dlls_and_their_manifests_together() {
    use std::cell::Cell;

    let f = same_guid_fastinit_fixture();
    let before = game_and_manifest_bytes(&f);
    let plan = planning::create(&f.state, request(FASTINIT, Some(TARGET), true)).unwrap();
    let mut session = take(&f, &plan);
    verify_selected(&f, &mut session);
    preparation::finish(&f.state, &mut session).unwrap();
    let mut changes = session
        .mutations
        .take()
        .unwrap()
        .into_iter()
        .map(|change| change.mutation)
        .collect::<Vec<_>>();
    // Keep one other real retirement last to guarantee a cancellation boundary
    // after the first DLL and its manifest have already been removed.
    let last = changes
        .iter()
        .position(|mutation| {
            matches!(mutation, Mutation::File(change)
                if change.target.root == RootKind::Game
                && change.target.relative == DISABLED_OLDER
                && change.source.is_none())
        })
        .expect("the transaction must delete the disabled older copy");
    let last = changes.remove(last);
    changes.push(last);
    let manifest_dir = f.state.config.manifests_dir();
    let data_root = manifest_dir.parent().unwrap().to_path_buf();
    let journal_root = data_root.join("mod-change-transactions");
    let roots = TransactionRoots {
        game_root: f.game.clone(),
        data_root,
    };
    let _operation = f.state.operation.lock().unwrap();
    let batch = prepare(&journal_root, &roots, changes).unwrap();
    let seen_writes = Cell::new(false);
    let outcome = batch.apply_with_cancel(
        || {
            if f.game.join(OLDER).exists()
                || manifest_dir.join("FastInit-legacy-enabled.toml").exists()
            {
                return false;
            }
            // Cancel after both writes have really occurred and before commit.
            seen_writes.set(true);
            true
        },
        |_| {},
    );
    assert!(seen_writes.get());
    assert_eq!(outcome.unwrap_err(), "download.cancelled");
    assert_eq!(game_and_manifest_bytes(&f), before);
}

#[test]
fn changing_a_retired_dll_after_prepare_blocks_the_whole_release_update() {
    let f = same_guid_fastinit_fixture();
    let plan = planning::create(&f.state, request(FASTINIT, Some(TARGET), true)).unwrap();
    let mut session = take(&f, &plan);
    verify_selected(&f, &mut session);
    preparation::finish(&f.state, &mut session).unwrap();
    session.plan.prepared = true;
    write(
        &f.game.join(DISABLED_OLDER),
        b"externally changed old copy after approval",
    );
    let before = game_and_manifest_bytes(&f);
    assert_eq!(
        super::super::execution::commit(&f.state, &mut session, |_| {}).unwrap_err(),
        "modChanges.targetChanged"
    );
    assert_eq!(game_and_manifest_bytes(&f), before);
}

#[test]
fn enabled_and_disabled_versions_of_one_retirement_path_block_planning() {
    let f = same_guid_fastinit_fixture();
    let twin = mods::disabled_dll_file(OLDER);
    write(&f.game.join(twin), &installed_fastinit_bytes(GUID, "1.2"));
    let before = game_and_manifest_bytes(&f);
    assert_eq!(
        planning::create(&f.state, request(FASTINIT, Some(TARGET), true)).unwrap_err(),
        "modChanges.multipleInstalledCopies"
    );
    assert_eq!(game_and_manifest_bytes(&f), before);
}

#[test]
fn exact_release_update_explains_ambiguous_installed_copies_before_download() {
    let id = "FastInit";
    let guid = "oc2.diylevel.fastinit";
    let f = fixture(vec![repository(id, guid, &["1.9.6"], vec![])]);
    let current = install(&f, id, guid, "1.9.5", true);
    // Matching GUID alone cannot grant ownership of an unmanaged DLL whose
    // exact file name differs from the actual remote plugin.
    let extra = "BepInEx/plugins/FastInit-1.3/FastInit-local-copy.dll";
    let extra = mods::disabled_dll_file(extra);
    let old_bytes = mods::plugin_blob(guid, id, "1.3");
    write(&f.game.join(&extra), &old_bytes);

    let inventory = inventory::build(&f.state).unwrap();
    assert_eq!(
        inventory
            .files
            .values()
            .filter(|files| files.summary.repository_id.as_deref() == Some(id))
            .count(),
        2
    );
    assert!(!inventory.files.contains_key("mod:FastInit"));
    assert_eq!(
        planning::create(&f.state, request(id, Some("v1.9.6"), true)).unwrap_err(),
        "modChanges.multipleInstalledCopies"
    );
    assert_eq!(
        planning::create(&f.state, request("Missing", Some("v1.0"), true)).unwrap_err(),
        "modChanges.targetChanged"
    );
    assert_eq!(
        planning::create(&f.state, request(id, Some("v1.0"), true)).unwrap_err(),
        "modChanges.targetChanged"
    );
    assert_eq!(fs::read(f.game.join(&extra)).unwrap(), old_bytes);
    assert_eq!(
        fs::read(f.game.join(&current)).unwrap(),
        mods::plugin_blob(guid, id, "1.9.5")
    );

    fs::remove_file(f.game.join(extra)).unwrap();
    let plan = planning::create(&f.state, request(id, Some("v1.9.6"), true)).unwrap();
    let mut session = take(&f, &plan);
    verify_selected(&f, &mut session);
    let chosen = session
        .solved
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|node| node.id == "mod:FastInit")
        .unwrap();
    assert_eq!(
        chosen.current.as_ref().unwrap().tag.as_deref(),
        Some("v1.9.6")
    );
    preparation::finish(&f.state, &mut session).unwrap();
    session.plan.prepared = true;
    super::super::execution::commit(&f.state, &mut session, |_| {}).unwrap();
    assert_eq!(
        fs::read(f.game.join(current)).unwrap(),
        mods::plugin_blob(guid, id, "1.9.6")
    );
}
