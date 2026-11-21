use super::*;

pub(super) const PREVIOUS: &str = "BepInEx/plugins/FixtureMod/v1.0.0/Fixture.dll";
pub(super) const DESTINATION: &str = "BepInEx/plugins/FixtureMod/v2.0.0/Fixture.dll";
const OTHER_COPY: &str = "BepInEx/plugins/Unbound/Fixture.dll";

pub(super) fn setup_relocation() -> ReinstallFixture {
    setup_asset(true, zip(&[("FixtureMod/v2.0.0/Fixture.dll", &mod_dll())]))
}

#[tokio::test]
async fn same_release_source_can_reinstall_into_a_new_version_directory() {
    let test = setup_relocation();
    let previous = test.fixture.game.join(PREVIOUS);
    write(&previous, &mod_dll());
    let mut record = test.old_record();
    record.files = vec![PREVIOUS.into()];
    test.write_record(&record);
    fs::remove_file(&previous).unwrap();
    let before = test.fingerprints();

    let ready = prepared_plan(test.prepare().await, TAG);
    assert!(ready.untracked_overwrites.is_empty());
    assert_eq!(test.fingerprints(), before);
    let mut session = take(&test.fixture, &ready);
    execution::commit(&test.fixture.state, &mut session, |_| {}).unwrap();

    assert!(!previous.exists());
    assert_eq!(
        fs::read(test.fixture.game.join(DESTINATION)).unwrap(),
        mod_dll()
    );
    let manifests = archive::read_manifests(&test.manifests_dir());
    assert_eq!(manifests.len(), 1);
    assert_eq!(manifests[0].files, [DESTINATION]);
    assert_eq!(
        manifests[0].repository_binding.as_ref(),
        Some(&test.binding)
    );
}

#[tokio::test]
async fn relocation_requires_known_source_and_vacant_old_and_new_targets() {
    for condition in [
        "unbound record",
        "empty record",
        "other source",
        "other claim",
        "new disabled copy",
    ] {
        let test = setup_relocation();
        let mut old = test.old_record();
        old.files = vec![PREVIOUS.into()];
        match condition {
            "unbound record" => old.repository_binding = None,
            "empty record" => old.files.clear(),
            "other source" => {
                old.repository_binding.as_mut().unwrap().owner = "AnotherOwner".into()
            }
            "other claim" => test.write_record(&archive::InstallManifest {
                repository_id: "AnotherRepository".into(),
                files: vec![DESTINATION.into()],
                repository_binding: None,
                ..old.clone()
            }),
            "new disabled copy" => write(
                &test.fixture.game.join(mods::disabled_dll_file(DESTINATION)),
                b"unclaimed disabled DLL",
            ),
            _ => unreachable!(),
        }
        test.write_record(&old);
        let before = test.fingerprints();

        let outcome = test.prepare().await;
        assert_eq!(
            outcome.progress.stage,
            PreparationStage::Failed,
            "{condition}"
        );
        assert!(outcome.plan.is_none(), "{condition}");
        assert_eq!(test.fingerprints(), before, "{condition}");
    }
}

#[tokio::test]
async fn approved_relocation_leaves_restored_old_and_new_variants_untouched() {
    for relative in [PREVIOUS, DESTINATION] {
        let test = setup_relocation();
        let mut old = test.old_record();
        old.files = vec![PREVIOUS.into()];
        test.write_record(&old);
        let ready = prepared_plan(test.prepare().await, TAG);
        let mut session = take(&test.fixture, &ready);

        let restored = test.fixture.game.join(mods::disabled_dll_file(relative));
        write(&restored, &mod_dll());
        let before = test.fingerprints();
        execution::commit(&test.fixture.state, &mut session, |_| {}).unwrap_err();

        assert_eq!(test.fingerprints(), before, "{relative}");
        assert!(!test.fixture.game.join(DESTINATION).exists(), "{relative}");
    }
}

#[tokio::test]
async fn relocation_rejects_an_old_file_as_a_new_parent_directory() {
    let test = setup_asset(
        true,
        zip(&[
            ("FixtureMod/v2.0.0/Fixture.dll", &mod_dll()),
            ("FixtureMod/settings/default.txt", b"defaults"),
        ]),
    );
    let mut old = test.old_record();
    old.files = vec![
        PREVIOUS.into(),
        "BepInEx/plugins/FixtureMod/settings".into(),
    ];
    test.write_record(&old);
    let before = test.fingerprints();

    let outcome = test.prepare().await;
    assert_eq!(outcome.progress.stage, PreparationStage::Failed);
    assert_eq!(
        outcome.error.as_deref(),
        Some(
            "modChanges.installTargetPathCollision?path=BepInEx/plugins/FixtureMod/settings/default.txt"
        )
    );
    assert_eq!(test.fingerprints(), before);
}

#[tokio::test]
async fn relocation_cannot_reclaim_a_record_with_a_live_copy_from_another_source() {
    let test = setup_relocation();
    let mut old = test.old_record();
    old.files = vec![PREVIOUS.into()];
    test.write_record(&old);
    let other = archive::InstallManifest {
        repository_id: "AnotherRepository".into(),
        files: vec![OTHER_COPY.into()],
        repository_binding: Some(ModRepositoryBinding {
            owner: "AnotherOwner".into(),
            ..test.binding.clone()
        }),
        ..old
    };
    let mut config = test.fixture.state.config.load_or_default();
    let mut other_repo = repository(&other.repository_id, GUID, &["2.0.0"], vec![]);
    other_repo.username = "AnotherOwner".into();
    other_repo.repository = test.binding.repository.clone();
    config.repositories.push(other_repo);
    test.fixture.state.config.save(&config).unwrap();
    write(&test.fixture.game.join(OTHER_COPY), &mod_dll());
    test.write_record(&other);
    let before = test.fingerprints();

    let mut selection = request(REPO, Some(TAG), true);
    selection.roots[0].asset_name = Some(ASSET.into());
    let error = planning::create(&test.fixture.state, selection).unwrap_err();
    assert!(
        error.starts_with("modChanges.duplicatePluginRuntime?"),
        "{error}"
    );
    assert_eq!(test.server.request_count(), 0);
    assert_eq!(test.fingerprints(), before);
}
