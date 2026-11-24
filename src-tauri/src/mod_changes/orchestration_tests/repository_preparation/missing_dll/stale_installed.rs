use super::*;
use serde_json::{Value, json};

const CURRENT: &str = "BepInEx/plugins/FixtureMod/v1.5.0/Fixture.dll";
const RESIDUE: &str = "BepInEx/plugins/FixtureMod/v1.0.0/previous.txt";

fn setup_installed() -> ReinstallFixture {
    with_installed(relocated::setup_relocation())
}

fn with_installed(test: ReinstallFixture) -> ReinstallFixture {
    let mut installed = PE.to_vec();
    installed.extend(mods::plugin_blob(GUID, REPO, "1.5.0"));
    write(&test.fixture.game.join(CURRENT), &installed);
    test
}

fn stale_record(test: &ReinstallFixture) -> archive::InstallManifest {
    archive::InstallManifest {
        files: vec![relocated::PREVIOUS.into()],
        ..test.old_record()
    }
}

fn manifest_path(test: &ReinstallFixture) -> PathBuf {
    test.manifests_dir().join(format!("{REPO}.toml"))
}

fn write_stale_record_with_residue(test: &ReinstallFixture) {
    let mut record = stale_record(test);
    record.files.push(RESIDUE.into());
    write(&test.fixture.game.join(RESIDUE), b"previous resource");
    test.write_record(&record);
}

fn overwrite_offer(outcome: &PreparationOutcome) -> Value {
    serde_json::to_value(outcome).unwrap()["manifestOverwrite"].clone()
}

fn overwrite_intent(offer: &Value) -> Value {
    assert_eq!(offer["repositoryId"], REPO);
    assert!(
        offer["fingerprint"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
    );
    json!({ "repositoryId": REPO, "fingerprint": offer["fingerprint"] })
}

async fn prepare_with_intent(
    test: &ReinstallFixture,
    intent: Value,
) -> Result<PreparationOutcome, String> {
    let mut selection: PlanRequest = serde_json::from_value(json!({ "manifestOverwrite": intent }))
        .expect("the explicit overwrite intent must deserialize");
    let mut roots = request(REPO, Some(TAG), true).roots;
    roots[0].asset_name = Some(ASSET.into());
    selection.roots = roots;
    prepare_selection(test, selection).await
}

async fn prepare_selection(
    test: &ReinstallFixture,
    selection: PlanRequest,
) -> Result<PreparationOutcome, String> {
    let plan = planning::create(&test.fixture.state, selection)?;
    assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    assert!(!plan.prepared);
    Ok(preparation::prepare(test.fixture.state.clone(), plan.plan_id, |_| {}).await)
}

fn assert_failed(outcome: &PreparationOutcome, case: &str) {
    assert_eq!(outcome.progress.stage, PreparationStage::Failed, "{case}");
    assert!(outcome.plan.is_none(), "{case}");
    assert!(outcome.error.is_some(), "{case}");
}

fn assert_updated(test: &ReinstallFixture) {
    let game = &test.fixture.game;
    assert!(!game.join(CURRENT).exists());
    assert!(!game.join(relocated::PREVIOUS).exists());
    assert_eq!(
        fs::read(game.join(relocated::DESTINATION)).unwrap(),
        mod_dll()
    );
    assert!(manifest_path(test).is_file());
    let records = archive::read_manifests(&test.manifests_dir());
    let [updated] = records.as_slice() else {
        panic!("expected one updated record: {records:?}");
    };
    assert_eq!(updated.repository_id, REPO);
    assert_eq!(updated.release_tag, TAG);
    assert_eq!(updated.repository_binding.as_ref(), Some(&test.binding));
    assert_eq!(updated.files, [relocated::DESTINATION]);
}

#[tokio::test]
async fn installed_update_replaces_stale_same_source_record_in_place() {
    for with_digest in [true, false] {
        let test = with_installed(setup_asset(
            with_digest,
            zip(&[("FixtureMod/v2.0.0/Fixture.dll", &mod_dll())]),
        ));
        test.write_record(&stale_record(&test));
        let before = test.fingerprints();
        let trusted = inventory::build(&test.fixture.state).unwrap();
        let installed = &trusted.files[&format!("mod:{REPO}")];
        assert_eq!(installed.summary.repository_id.as_deref(), Some(REPO));
        assert_eq!(installed.summary.dll_files, [CURRENT]);
        assert!(installed.manifests.is_empty());

        let outcome = test.prepare().await;
        assert_eq!(outcome.error, None, "with_digest={with_digest}");
        let ready = prepared_plan(outcome, TAG);
        assert_eq!(test.fingerprints(), before);
        assert!(ready.untracked_overwrites.is_empty());
        let mut session = take(&test.fixture, &ready);
        execution::commit(&test.fixture.state, &mut session, |_| {}).unwrap();

        assert_updated(&test);
    }
}

#[tokio::test]
async fn force_overwrite_retries_read_only_and_never_spends_old_game_file_claims() {
    let test = setup_installed();
    write_stale_record_with_residue(&test);
    let before = test.fingerprints();

    let blocked = test.prepare().await;
    assert_failed(&blocked, "no explicit intent");
    let offer = overwrite_offer(&blocked);
    assert!(offer["manifestPath"].as_str().is_some_and(|path| {
        path.replace('\\', "/")
            .ends_with("manifests/FixtureMod.toml")
    }));
    let mut recorded_paths = offer["recordedPaths"].as_array().unwrap().clone();
    recorded_paths.sort_by(|left, right| left.as_str().cmp(&right.as_str()));
    assert_eq!(recorded_paths, [json!(relocated::PREVIOUS), json!(RESIDUE)]);
    assert_eq!(test.fingerprints(), before);
    let intent = overwrite_intent(&offer);

    for confirm in [false, true] {
        let prepared = prepare_with_intent(&test, intent.clone()).await.unwrap();
        let ready = prepared_plan(prepared, TAG);
        assert!(ready.untracked_overwrites.is_empty());
        assert_eq!(test.fingerprints(), before);
        if !confirm {
            execution::cancel(&test.fixture.state, &ready.plan_id).unwrap();
            assert_eq!(test.fingerprints(), before);
        } else {
            let mut session = take(&test.fixture, &ready);
            execution::commit(&test.fixture.state, &mut session, |_| {}).unwrap();
            assert_updated(&test);
        }
        assert_eq!(
            fs::read(test.fixture.game.join(RESIDUE)).unwrap(),
            b"previous resource"
        );
    }
}

#[tokio::test]
async fn stale_record_replacement_names_the_competing_manifest_and_its_claim() {
    let test = setup_installed();
    let stale = stale_record(&test);
    test.write_record(&stale);
    let mut competing = stale.clone();
    competing.repository_id = "AnotherRepository".into();
    competing.repository_binding = None;
    test.write_record(&competing);
    let before = test.fingerprints();

    let blocked = test.prepare().await;
    assert_failed(&blocked, "old path claimed by another manifest");
    assert_eq!(
        blocked.error,
        Some(install_claim_error(
            relocated::PREVIOUS,
            "manifests/AnotherRepository.toml"
        )),
    );
    assert_eq!(overwrite_offer(&blocked), Value::Null);
    assert_eq!(test.fingerprints(), before);
}

#[tokio::test]
async fn stale_record_without_binding_requires_explicit_overwrite() {
    let test = setup_installed();
    let mut unbound = stale_record(&test);
    unbound.repository_binding = None;
    test.write_record(&unbound);
    let before = test.fingerprints();

    let blocked = test.prepare().await;
    assert_failed(&blocked, "old record is not bound to a source");
    assert_eq!(test.fingerprints(), before);
    let intent = overwrite_intent(&overwrite_offer(&blocked));
    let ready = prepared_plan(prepare_with_intent(&test, intent).await.unwrap(), TAG);
    assert_eq!(test.fingerprints(), before);

    let mut session = take(&test.fixture, &ready);
    execution::commit(&test.fixture.state, &mut session, |_| {}).unwrap();
    assert_updated(&test);
}

#[tokio::test]
async fn force_overwrite_ties_retry_to_the_offered_record() {
    for change in [
        "different bytes",
        "duplicate record",
        "foreign source",
        "occupied release target",
    ] {
        let test = setup_installed();
        write_stale_record_with_residue(&test);
        let offered = test.prepare().await;
        let intent = overwrite_intent(&overwrite_offer(&offered));
        let mut old = stale_record(&test);
        old.files.push(RESIDUE.into());
        match change {
            "different bytes" => test.write_record(&stale_record(&test)),
            "duplicate record" => write(
                &test.manifests_dir().join("duplicate.toml"),
                toml::to_string(&old).unwrap().as_bytes(),
            ),
            "foreign source" => {
                old.repository_binding.as_mut().unwrap().owner = "OtherOwner".into();
                test.write_record(&old);
            }
            "occupied release target" => write(
                &test.fixture.game.join(relocated::DESTINATION),
                b"unrecognized existing DLL",
            ),
            _ => unreachable!(),
        }
        let before = test.fingerprints();
        match prepare_with_intent(&test, intent).await {
            Ok(outcome) => assert_failed(&outcome, change),
            Err(error) => assert!(!error.is_empty(), "{change}"),
        }
        assert_eq!(test.fingerprints(), before, "{change}");
        assert_eq!(
            test.fixture.game.join(relocated::DESTINATION).exists(),
            change == "occupied release target"
        );
    }
}

#[tokio::test]
async fn approved_stale_record_update_aborts_without_partial_writes_when_endpoints_change() {
    for (force, changed) in [
        (false, "TOML"),
        (false, "old disabled DLL"),
        (true, "TOML"),
        (true, "TOML replaced with same bytes"),
        (true, "release target"),
    ] {
        let test = setup_installed();
        let prepared = if force {
            write_stale_record_with_residue(&test);
            let intent = overwrite_intent(&overwrite_offer(&test.prepare().await));
            prepare_with_intent(&test, intent).await.unwrap()
        } else {
            test.write_record(&stale_record(&test));
            test.prepare().await
        };
        let ready = prepared_plan(prepared, TAG);
        let mut session = take(&test.fixture, &ready);
        match changed {
            "TOML" => write(&manifest_path(&test), b"changed after approval"),
            "TOML replaced with same bytes" => {
                let path = manifest_path(&test);
                let substitute = path.with_extension("tmp");
                fs::copy(&path, &substitute).unwrap();
                fs::remove_file(&path).unwrap();
                fs::rename(substitute, path).unwrap();
            }
            "old disabled DLL" => write(
                &test
                    .fixture
                    .game
                    .join(mods::disabled_dll_file(relocated::PREVIOUS)),
                b"external DLL",
            ),
            "release target" => write(
                &test.fixture.game.join(relocated::DESTINATION),
                b"external target",
            ),
            _ => unreachable!(),
        }
        let before = test.fingerprints();

        execution::commit(&test.fixture.state, &mut session, |_| {}).unwrap_err();

        assert_eq!(test.fingerprints(), before, "{force}: {changed}");
        assert!(test.fixture.game.join(CURRENT).is_file());
        assert!(
            !has_pending(
                &test
                    .manifests_dir()
                    .with_file_name("mod-change-transactions")
            )
            .unwrap()
        );
    }
}

#[tokio::test]
async fn stale_record_update_rolls_back_when_old_dll_returns_after_live_writes() {
    let test = setup_installed();
    test.write_record(&stale_record(&test));
    let manifest = manifest_path(&test);
    let previous_record = fs::read(&manifest).unwrap();
    let current = test.fixture.game.join(CURRENT);
    let current_bytes = fs::read(&current).unwrap();
    let destination = test.fixture.game.join(relocated::DESTINATION);
    let external = test
        .fixture
        .game
        .join(mods::disabled_dll_file(relocated::PREVIOUS));
    let ready = prepared_plan(test.prepare().await, TAG);
    let mut session = take(&test.fixture, &ready);
    let mut restored = false;
    let mut rolled_back = false;

    let commit = execution::commit(&test.fixture.state, &mut session, |progress| {
        let event = serde_json::to_value(progress).unwrap();
        let items = event["items"].as_array().unwrap();
        rolled_back |= items.iter().any(|item| item["stage"] == "rolled_back");
        if !restored && items.iter().any(|item| item["stage"] == "processed") {
            assert!(!current.exists());
            assert_eq!(fs::read(&destination).unwrap(), mod_dll());
            assert_ne!(fs::read(&manifest).unwrap(), previous_record);
            write(&external, b"externally restored old DLL");
            restored = true;
        }
    });

    assert!(commit.is_err());
    assert!(restored && rolled_back);
    assert_eq!(fs::read(&current).unwrap(), current_bytes);
    assert_eq!(fs::read(&manifest).unwrap(), previous_record);
    assert_eq!(fs::read(external).unwrap(), b"externally restored old DLL");
    assert!(!destination.exists());
}

#[tokio::test]
async fn stale_record_recovery_rejects_unverifiable_cleanup_of_old_paths() {
    for obstacle in [
        "enabled file",
        "disabled file",
        "linked old file",
        "map registration",
        "duplicate path alias",
        "other source claim",
        "inconsistent tag",
    ] {
        let test = setup_installed();
        let mut old = stale_record(&test);
        let previous = test.fixture.game.join(relocated::PREVIOUS);
        match obstacle {
            "enabled file" => write(&previous, b"retained enabled bytes"),
            "disabled file" => write(
                &test
                    .fixture
                    .game
                    .join(mods::disabled_dll_file(relocated::PREVIOUS)),
                b"retained disabled bytes",
            ),
            "linked old file" => {
                let other = test.fixture.game.join("linked-game-file.bin");
                write(&other, b"retained linked bytes");
                fs::create_dir_all(previous.parent().unwrap()).unwrap();
                fs::hard_link(other, previous).unwrap();
            }
            "map registration" => write(
                &test
                    .manifests_dir()
                    .join(crate::level_sets::LEVEL_SET_PLUGIN_REGISTRY),
                &serde_json::to_vec(&json!({
                    "version": 1,
                    "entries": [{ "dll": relocated::PREVIOUS, "owner": { "name": "Fixture Map" } }],
                }))
                .unwrap(),
            ),
            "duplicate path alias" => old.files.push(relocated::PREVIOUS.to_ascii_lowercase()),
            "other source claim" => test.write_record(&archive::InstallManifest {
                repository_id: "AnotherRepository".into(),
                repository_binding: None,
                ..old.clone()
            }),
            "inconsistent tag" => old.repository_binding.as_mut().unwrap().tag = TAG.into(),
            _ => unreachable!(),
        }
        test.write_record(&old);
        let before = test.fingerprints();
        let mut selection = request(REPO, Some(TAG), true);
        selection.roots[0].asset_name = Some(ASSET.into());
        match prepare_selection(&test, selection).await {
            Ok(outcome) => assert_failed(&outcome, obstacle),
            Err(error) => {
                assert!(
                    matches!(
                        obstacle,
                        "enabled file" | "disabled file" | "linked old file"
                    ),
                    "{obstacle}: {error}"
                );
                assert_eq!(
                    crate::locale_tag::diagnostic_tag(&error),
                    "modChanges.multipleInstalledCopies",
                    "{obstacle}"
                );
            }
        }
        assert_eq!(test.fingerprints(), before, "{obstacle}");
    }
}

#[tokio::test]
async fn unsafe_toml_conflicts_do_not_offer_force_overwrite() {
    for obstacle in [
        "directory",
        "invalid TOML",
        "hard link",
        "noncanonical filename",
        "duplicate same-ID record",
        "foreign binding",
        "different channel",
    ] {
        let test = setup_installed();
        let mut old = stale_record(&test);
        let target = manifest_path(&test);
        match obstacle {
            "directory" => fs::create_dir_all(&target).unwrap(),
            "invalid TOML" => write(&target, b"[invalid TOML"),
            "hard link" => {
                test.write_record(&old);
                fs::hard_link(&target, test.fixture.game.join("linked-record.bin")).unwrap();
            }
            "noncanonical filename" => {
                write(
                    &test
                        .manifests_dir()
                        .join(format!("{}.toml", REPO.to_ascii_lowercase())),
                    toml::to_string(&old).unwrap().as_bytes(),
                );
            }
            "duplicate same-ID record" => {
                test.write_record(&old);
                write(
                    &test.manifests_dir().join("duplicate.toml"),
                    toml::to_string(&old).unwrap().as_bytes(),
                );
            }
            "foreign binding" => {
                old.repository_binding.as_mut().unwrap().owner = "AnotherOwner".into();
                test.write_record(&old);
            }
            "different channel" => {
                old.repository_binding.as_mut().unwrap().channel = match test.binding.channel {
                    Channel::Beta => Channel::Stable,
                    _ => Channel::Beta,
                };
                test.write_record(&old);
            }
            _ => unreachable!(),
        }
        let before = test.fingerprints();

        let outcome = test.prepare().await;

        assert_failed(&outcome, obstacle);
        assert!(overwrite_offer(&outcome).is_null(), "{obstacle}");
        assert_eq!(test.fingerprints(), before, "{obstacle}");
    }
}

#[tokio::test]
async fn stale_record_paths_cannot_authorize_conflicting_release_writes() {
    for (previous, incoming, exists) in [
        (RESIDUE, "FixtureMod/v1.0.0/previous.txt", false),
        (RESIDUE, "FixtureMod/v1.0.0/previous.txt", true),
        (
            "BepInEx/plugins/FixtureMod/v1.0.0/caf\u{e9}.txt",
            "FixtureMod/v1.0.0/cafe\u{301}.txt",
            false,
        ),
    ] {
        let test = with_installed(setup_asset(
            true,
            zip(&[
                ("FixtureMod/v2.0.0/Fixture.dll", &mod_dll()),
                (incoming, b"new resource"),
            ]),
        ));
        let mut record = stale_record(&test);
        record.files.push(previous.into());
        if exists {
            write(&test.fixture.game.join(previous), b"previous resource");
        }
        test.write_record(&record);
        let before = test.fingerprints();

        let outcome = test.prepare().await;

        assert_failed(&outcome, previous);
        assert!(overwrite_offer(&outcome).is_null());
        assert_eq!(test.fingerprints(), before);
    }
}
