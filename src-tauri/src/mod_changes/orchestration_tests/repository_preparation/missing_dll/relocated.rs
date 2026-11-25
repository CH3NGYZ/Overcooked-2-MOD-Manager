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

#[test]
fn duplicate_runtime_distinguishes_a_retired_github_binding_and_a_catalog_candidate() {
    let test = setup_relocation();
    let mut config = test.fixture.state.config.load_or_default();
    let repo = config
        .repositories
        .iter_mut()
        .find(|repo| repo.id == REPO)
        .unwrap();
    repo.provider = Provider::Gitee;
    repo.username = "MirrorOwner".into();
    repo.name = "Planned HostUtilities".into();
    test.fixture.state.config.save(&config).unwrap();
    write(&test.fixture.game.join(OTHER_COPY), &mod_dll());
    let mut installed = test.old_record();
    installed.repository_id = "7134b722-0196-4529-8deb-10dc40f2307c".into();
    installed.files = vec![OTHER_COPY.into()];
    installed
        .package_name_i18n
        .insert("en".into(), "Original HostUtilities".into());
    installed.repository_binding = Some(ModRepositoryBinding {
        provider: Provider::Github,
        owner: "OriginalOwner".into(),
        ..installed.repository_binding.take().unwrap()
    });
    test.write_record(&installed);

    let mut selection = request(REPO, Some(TAG), true);
    selection.roots[0].asset_name = Some(ASSET.into());
    let before = test.fingerprints();
    let error = planning::create(&test.fixture.state, selection).unwrap_err();
    let evidence = runtime_evidence(&error);
    assert_eq!(evidence["runtime_conflict"], true);
    let rows = evidence["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    let local = rows
        .iter()
        .find(|row| row["origin"] == "installed")
        .unwrap();
    assert_eq!(local["path"], OTHER_COPY);
    assert_eq!(local["installed_enabled"], true);
    assert_eq!(local["name_i18n"]["en"], "Original HostUtilities");
    assert_eq!(local["repository"]["provider"], "github");
    assert_eq!(local["repository"]["owner"], "OriginalOwner");
    let planned = rows.iter().find(|row| row["origin"] == "planned").unwrap();
    assert_eq!(planned["label"], "Planned HostUtilities");
    assert_eq!(planned["repository"]["provider"], "gitee");
    assert_eq!(planned["repository"]["owner"], "MirrorOwner");
    assert_eq!(planned["repository"]["repository"], test.binding.repository);
    assert!(planned["path"].is_null() && planned["projected_path"].is_null());
    assert!(rows.iter().all(|row| row["source"].is_null()));
    assert_eq!(test.server.request_count(), 0);
    assert_eq!(test.fingerprints(), before);
}

#[tokio::test]
async fn duplicate_runtime_after_package_verification_shows_the_exact_projected_path() {
    const EXTRA_GUID: &str = "com.example.extra-provider";
    const EXTRA: &str = "MiXeD Extra/Runtime.dll";
    const TARGET_EXTRA: &str = "BepInEx/plugins/FixtureMod/v2.0.0/MiXeD Extra/Runtime.dll";
    let mut extra = PE.to_vec();
    extra.extend(mods::plugin_blob(EXTRA_GUID, "Runtime", "2.0.0"));
    let test = setup_asset(
        true,
        zip(&[
            ("FixtureMod/v2.0.0/Fixture.dll", &mod_dll()),
            (&format!("FixtureMod/v2.0.0/{EXTRA}"), &extra),
        ]),
    );
    let local = "BepInEx/plugins/InstalledRuntime/Runtime.dll";
    let mut old = PE.to_vec();
    old.extend(mods::plugin_blob(EXTRA_GUID, "Runtime", "1.0.0"));
    write(&test.fixture.game.join(local), &old);
    let before = Fingerprint::capture(&test.fixture.game).unwrap();
    let outcome = test.prepare().await;
    assert_eq!(outcome.progress.stage, PreparationStage::Failed);
    let evidence = runtime_evidence(outcome.error.as_deref().unwrap());
    assert_eq!(evidence["guid"], EXTRA_GUID);
    let rows = evidence["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    let installed = rows
        .iter()
        .find(|row| row["origin"] == "installed")
        .unwrap();
    assert_eq!(installed["path"], local);
    assert!(installed["repository"].is_null());
    let planned = rows.iter().find(|row| row["origin"] == "planned").unwrap();
    assert_eq!(planned["projected_path"], TARGET_EXTRA);
    assert!(planned["path"].is_null());
    assert_eq!(Fingerprint::capture(&test.fixture.game).unwrap(), before);
}

fn runtime_evidence(error: &str) -> serde_json::Value {
    let query = error
        .strip_prefix("modChanges.duplicatePluginRuntime?")
        .unwrap();
    let (_, evidence) = url::form_urlencoded::parse(query.as_bytes())
        .find(|(key, _)| key == "evidence")
        .unwrap();
    serde_json::from_str(&evidence).unwrap()
}
