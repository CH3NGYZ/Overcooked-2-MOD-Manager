use super::*;

const OLD_ID: &str = "84f598a5-9cf8-4ad8-a2c0-a945e60b4512";
const OTHER_ID: &str = "b03189d7-94e3-40e3-9b1a-0b35b5d5b933";

fn previous_record(test: &ReinstallFixture) -> archive::InstallManifest {
    let mut old = test.old_record();
    old.repository_id = OLD_ID.into();
    old.files.push(OLD_RESOURCE.into());
    old
}

fn previous_dll() -> Vec<u8> {
    let mut dll = PE.to_vec();
    dll.extend(mods::plugin_blob(GUID, REPO, "1.0.0"));
    dll
}

fn configure_old_id(test: &ReinstallFixture) {
    let mut config = test.fixture.state.config.load().unwrap();
    let mut duplicate = repository(OLD_ID, "com.example.other", &[], vec![]);
    duplicate.platform = Some(
        if cfg!(target_os = "macos") {
            "windows"
        } else {
            "macos"
        }
        .into(),
    );
    config.repositories.push(duplicate);
    test.fixture.state.config.save(&config).unwrap();
}

#[tokio::test]
async fn cross_id_reinstall_publishes_new_manifest_and_retires_the_vacant_old_record() {
    let test = setup(true);
    test.write_record(&previous_record(&test));
    let before = test.fingerprints();

    let ready = prepared_plan(test.prepare().await, TAG);
    assert!(ready.untracked_overwrites.is_empty());
    assert_eq!(test.fingerprints(), before);

    let mut session = take(&test.fixture, &ready);
    let changes = session.mutations.as_ref().unwrap();
    let file_targets: Vec<_> = changes
        .iter()
        .map(|owned| match &owned.mutation {
            Mutation::File(change) => (change.target.relative.as_str(), change.source.is_some()),
            _ => panic!("expected only file mutations for this reinstall"),
        })
        .collect();
    let new_record = format!("manifests/{REPO}.toml");
    let old_record = format!("manifests/{OLD_ID}.toml");
    assert_eq!(
        file_targets,
        [
            (TARGET, true),
            (new_record.as_str(), true),
            (old_record.as_str(), false)
        ]
    );
    execution::commit(&test.fixture.state, &mut session, |_| {}).unwrap();
    assert_eq!(fs::read(test.fixture.game.join(TARGET)).unwrap(), mod_dll());
    assert!(!test.fixture.game.join(OLD_RESOURCE).exists());
    let manifests = test.manifests_dir();
    assert!(!manifests.join(format!("{OLD_ID}.toml")).exists());
    let installed = archive::read_manifest(&manifests, REPO).unwrap();
    assert_eq!(installed.repository_id, REPO);
    assert_eq!(installed.release_tag, TAG);
    assert_eq!(installed.repository_binding, Some(test.binding.clone()));
    assert_eq!(installed.files, [TARGET]);
}

#[tokio::test]
async fn cross_id_reinstall_rejects_unproven_or_occupied_records_without_writes() {
    for case in [
        "no digest",
        "no binding",
        "wrong provider",
        "wrong repository",
        "wrong channel",
        "inconsistent tag",
        "manual",
        "old ID still configured",
        "duplicate previous ID",
        "same UUID with different spelling",
        "another record for the same source",
        "competing manifest for an old-only resource",
        "new record occupied",
        "new record path occupied by a directory",
        "old resource still present",
        "disabled DLL still present",
        "recognized disabled DLL still present",
        "enabled DLL still present",
    ] {
        let test = setup(case != "no digest");
        let mut old = previous_record(&test);
        match case {
            "no digest" => (),
            "no binding" => old.repository_binding = None,
            "wrong provider" => old.repository_binding.as_mut().unwrap().provider = Provider::Gitee,
            "wrong repository" => {
                old.repository_binding.as_mut().unwrap().repository = "Other".into()
            }
            "wrong channel" => old.repository_binding.as_mut().unwrap().channel = Channel::Stable,
            "inconsistent tag" => old.repository_binding.as_mut().unwrap().tag = TAG.into(),
            "manual" => old.manual_install = Some(true),
            "old ID still configured" => configure_old_id(&test),
            "duplicate previous ID" => write(
                &test.manifests_dir().join("duplicate.toml"),
                toml::to_string(&old).unwrap().as_bytes(),
            ),
            "same UUID with different spelling" => {
                let mut other = old.clone();
                other.repository_id = OLD_ID.to_ascii_uppercase();
                other.repository_binding = None;
                other.files = vec!["BepInEx/plugins/Another.dll".into()];
                write(
                    &test.manifests_dir().join("duplicate.toml"),
                    toml::to_string(&other).unwrap().as_bytes(),
                );
            }
            "another record for the same source" => {
                let mut other = old.clone();
                other.repository_id = OTHER_ID.into();
                other.files = vec!["BepInEx/plugins/Another.dll".into()];
                test.write_record(&other);
            }
            "competing manifest for an old-only resource" => {
                let mut other = old.clone();
                other.repository_id = OTHER_ID.into();
                other.repository_binding = None;
                other.files = vec![OLD_RESOURCE.into()];
                test.write_record(&other);
            }
            "new record occupied" => {
                let mut current = test.old_record();
                current.files = vec!["BepInEx/plugins/Another.dll".into()];
                test.write_record(&current);
            }
            "new record path occupied by a directory" => {
                fs::create_dir_all(test.manifests_dir().join(format!("{REPO}.toml"))).unwrap();
            }
            "old resource still present" => {
                write(&test.fixture.game.join(OLD_RESOURCE), b"old settings")
            }
            "disabled DLL still present" => write(
                &test.fixture.game.join(mods::disabled_dll_file(TARGET)),
                b"existing disabled DLL",
            ),
            "recognized disabled DLL still present" => write(
                &test.fixture.game.join(mods::disabled_dll_file(TARGET)),
                &previous_dll(),
            ),
            "enabled DLL still present" => write(&test.fixture.game.join(TARGET), &previous_dll()),
            _ => unreachable!(),
        }
        test.write_record(&old);
        let before = test.fingerprints();
        let outcome = test.prepare().await;
        assert_eq!(outcome.progress.stage, PreparationStage::Failed, "{case}");
        assert!(outcome.plan.is_none(), "{case}");
        assert_eq!(test.fingerprints(), before, "{case}");
        if case.ends_with("DLL still present") {
            assert_eq!(
                outcome.error,
                Some(install_claim_error(
                    TARGET,
                    &format!("manifests/{OLD_ID}.toml"),
                )),
                "{case}"
            );
        }
    }
}

#[tokio::test]
async fn cross_id_reinstall_does_not_transfer_a_record_when_the_main_dll_path_changes() {
    let test = relocated::setup_relocation();
    let mut old = previous_record(&test);
    old.files = vec![relocated::PREVIOUS.into()];
    test.write_record(&old);

    let ready = prepared_plan(test.prepare().await, TAG);
    let mut session = take(&test.fixture, &ready);
    execution::commit(&test.fixture.state, &mut session, |_| {}).unwrap();
    assert!(test.manifests_dir().join(format!("{OLD_ID}.toml")).exists());
    assert!(test.manifests_dir().join(format!("{REPO}.toml")).exists());
    assert!(test.fixture.game.join(relocated::DESTINATION).exists());
}

#[tokio::test]
async fn cross_id_reinstall_does_not_remove_a_live_record_when_the_main_dll_path_changes() {
    let test = relocated::setup_relocation();
    let mut old = previous_record(&test);
    old.files = vec![relocated::PREVIOUS.into()];
    test.write_record(&old);
    write(
        &test.fixture.game.join(relocated::PREVIOUS),
        &previous_dll(),
    );
    let before = test.fingerprints();

    let outcome = test.prepare().await;
    let claimed = crate::locale_tag::with_params(
        "modChanges.installTargetClaimedWithManifests",
        &[
            ("path", relocated::PREVIOUS),
            ("manifests", &format!("manifests/{OLD_ID}.toml")),
        ],
    );
    assert_eq!(outcome.progress.stage, PreparationStage::Failed);
    assert_eq!(outcome.error.as_deref(), Some(claimed.as_str()));
    assert!(outcome.plan.is_none());
    assert_eq!(test.fingerprints(), before);
}

#[tokio::test]
async fn cross_id_reinstall_rechecks_config_and_both_manifest_paths_after_approval() {
    for changed in ["old ID restored", "old TOML modified", "new TOML created"] {
        let test = setup(true);
        let old = previous_record(&test);
        test.write_record(&old);
        let ready = prepared_plan(test.prepare().await, TAG);
        let mut session = take(&test.fixture, &ready);
        match changed {
            "old ID restored" => configure_old_id(&test),
            "old TOML modified" => {
                let mut old = old;
                old.files.push("BepInEx/plugins/External.dll".into());
                test.write_record(&old);
            }
            "new TOML created" => test.write_record(&test.old_record()),
            _ => unreachable!(),
        }
        let before = test.fingerprints();
        execution::commit(&test.fixture.state, &mut session, |_| {}).unwrap_err();
        assert_eq!(test.fingerprints(), before, "{changed}");
    }
}

#[tokio::test]
async fn cross_id_reinstall_restores_old_record_if_a_disabled_dll_reappears_after_retirement() {
    let test = setup(true);
    test.write_record(&previous_record(&test));
    let ready = prepared_plan(test.prepare().await, TAG);
    let mut session = take(&test.fixture, &ready);
    let old = test.manifests_dir().join(format!("{OLD_ID}.toml"));
    let new = test.manifests_dir().join(format!("{REPO}.toml"));
    let old_bytes = fs::read(&old).unwrap();
    let target = test.fixture.game.join(TARGET);
    let disabled = mods::disabled_dll_file(TARGET);
    let restored = test.fixture.game.join(&disabled);
    let mut raced = false;
    let mut rolled_back = false;
    let result = execution::commit(&test.fixture.state, &mut session, |progress| {
        let event = serde_json::to_value(progress).unwrap();
        let items = event["items"].as_array().unwrap();
        rolled_back |= items.iter().any(|item| item["stage"] == "rolled_back");
        if !raced && items.iter().any(|item| item["stage"] == "processed") {
            assert!(!old.exists());
            assert!(new.is_file());
            assert_eq!(fs::read(&target).unwrap(), mod_dll());
            write(&restored, b"external disabled DLL");
            raced = true;
        }
    });
    assert!(raced && rolled_back);
    assert_eq!(
        result.unwrap_err(),
        crate::locale_tag::with_params(
            "modChanges.installTargetVerificationChanged",
            &[("path", &disabled)],
        )
    );
    assert_eq!(fs::read(&old).unwrap(), old_bytes);
    assert!(!new.exists() && !target.exists());
    assert_eq!(fs::read(restored).unwrap(), b"external disabled DLL");
    assert!(
        !has_pending(
            &test
                .manifests_dir()
                .with_file_name("mod-change-transactions")
        )
        .unwrap()
    );
}
