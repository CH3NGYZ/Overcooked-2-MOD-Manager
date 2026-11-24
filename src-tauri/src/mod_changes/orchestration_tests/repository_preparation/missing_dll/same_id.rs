use super::*;
use std::collections::BTreeSet;

const DLL_PATH: &str = "BepInEx/plugins/FixtureMod/Fixture.dll";
const MISSING: &str = "BepInEx/plugins/FixtureMod/official-all.txt";
const CHANGED: &str = "BepInEx/plugins/FixtureMod/common.txt";
const IDENTICAL: &str = "BepInEx/plugins/FixtureMod/prefer.example.txt";
const OLD_ONLY: &str = "BepInEx/plugins/FixtureMod/previous.txt";
const USER_FILE: &str = "BepInEx/plugins/FixtureMod/prefer.txt";

fn residual_package() -> ReinstallFixture {
    let test = setup_asset(
        true,
        zip(&[
            ("FixtureMod/official-all.txt", b"release list"),
            ("FixtureMod/common.txt", b"new release resource"),
            ("FixtureMod/Fixture.dll", &mod_dll()),
            ("FixtureMod/prefer.example.txt", b"example"),
        ]),
    );
    let mut record = test.old_record();
    record.release_tag = TAG.into();
    record.repository_binding = Some(test.binding.clone());
    record.files = [MISSING, DLL_PATH, CHANGED, OLD_ONLY]
        .map(String::from)
        .to_vec();
    test.write_record(&record);
    for (path, bytes) in [
        (CHANGED, &b"older local resource"[..]),
        (IDENTICAL, &b"example"[..]),
        (OLD_ONLY, &b"previous release resource"[..]),
        (USER_FILE, &b"user customizations"[..]),
    ] {
        write(&test.fixture.game.join(path), bytes);
    }
    test
}

#[tokio::test]
async fn same_id_reinstall_keeps_files_and_record_when_cancelled_or_changed() {
    for changed in [None, Some(CHANGED), Some(IDENTICAL)] {
        let test = residual_package();
        let before = test.fingerprints();
        let ready = prepared_plan(test.prepare().await, TAG);
        assert_eq!(test.fingerprints(), before);
        if let Some(path) = changed {
            let mut session = take(&test.fixture, &ready);
            write(&test.fixture.game.join(path), b"changed after confirmation");
            let changed = test.fingerprints();
            let error = execution::commit(&test.fixture.state, &mut session, |_| {}).unwrap_err();
            let tag = if path == CHANGED {
                "modChanges.installTargetVerificationChanged"
            } else {
                "modChanges.installTargetResidueDiffers"
            };
            assert_eq!(
                error,
                crate::locale_tag::with_params(tag, &[("path", path)])
            );
            assert_eq!(test.fingerprints(), changed);
        } else {
            execution::cancel(&test.fixture.state, &ready.plan_id).unwrap();
            assert_eq!(test.fingerprints(), before);
        }
    }
}

#[tokio::test]
async fn same_id_reinstall_rolls_back_resources_if_the_missing_dll_reappears_during_apply() {
    let test = residual_package();
    let ready = prepared_plan(test.prepare().await, TAG);
    let mut session = take(&test.fixture, &ready);
    let game = &test.fixture.game;
    let disabled = mods::disabled_dll_file(DLL_PATH);
    let old_manifest = Fingerprint::capture(&test.manifests_dir()).unwrap();
    let mut restored = false;
    let error = execution::commit(&test.fixture.state, &mut session, |progress| {
        let event = serde_json::to_value(progress).unwrap();
        if !restored
            && event["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["stage"] == "processed")
        {
            assert_eq!(
                fs::read(game.join(CHANGED)).unwrap(),
                b"new release resource"
            );
            write(&game.join(&disabled), b"external DLL");
            restored = true;
        }
    })
    .unwrap_err();
    assert!(restored);
    assert_eq!(
        error,
        crate::locale_tag::with_params(
            "modChanges.installTargetVerificationChanged",
            &[("path", &disabled)],
        )
    );
    assert_eq!(
        Fingerprint::capture(&test.manifests_dir()).unwrap(),
        old_manifest
    );
    assert_eq!(
        fs::read(game.join(CHANGED)).unwrap(),
        b"older local resource"
    );
    assert_eq!(fs::read(game.join(&disabled)).unwrap(), b"external DLL");
    assert!(!game.join(DLL_PATH).exists());
    assert!(!game.join(MISSING).exists());
}

#[tokio::test]
async fn same_id_reinstall_refuses_foreign_or_unverified_overwrites() {
    for obstacle in [
        "other record",
        "modified without record",
        "hard link",
        "directory",
    ] {
        let test = residual_package();
        let game = &test.fixture.game;
        let expected = match obstacle {
            "other record" => {
                test.write_record(&archive::InstallManifest {
                    repository_id: "OtherRepository".into(),
                    files: vec![CHANGED.into()],
                    repository_binding: None,
                    ..test.old_record()
                });
                ("modChanges.installTargetClaimed", CHANGED)
            }
            "modified without record" => {
                write(&game.join(IDENTICAL), b"different unregistered file");
                ("modChanges.installTargetResidueDiffers", IDENTICAL)
            }
            "hard link" => {
                fs::hard_link(game.join(CHANGED), game.join("external.txt")).unwrap();
                ("modChanges.installTargetAliasesOrLink", CHANGED)
            }
            "directory" => {
                fs::remove_file(game.join(CHANGED)).unwrap();
                fs::create_dir(game.join(CHANGED)).unwrap();
                ("modChanges.installTargetIsDirectory", CHANGED)
            }
            _ => unreachable!(),
        };
        let before = test.fingerprints();
        let outcome = test.prepare().await;
        assert_eq!(
            outcome.progress.stage,
            PreparationStage::Failed,
            "{obstacle}"
        );
        assert_eq!(
            outcome.error,
            Some(crate::locale_tag::with_params(
                expected.0,
                &[("path", expected.1)]
            )),
            "{obstacle}"
        );
        assert_eq!(test.fingerprints(), before, "{obstacle}");
    }
}

#[tokio::test]
async fn same_id_reinstall_offers_verified_overwrites_and_preserves_other_files() {
    let test = residual_package();
    let before = test.fingerprints();

    let ready = prepared_plan(test.prepare().await, TAG);
    let [overwrite] = ready.untracked_overwrites.as_slice() else {
        panic!(
            "expected one different resource: {:?}",
            ready.untracked_overwrites
        );
    };
    assert_eq!(overwrite.path, CHANGED);
    assert_eq!(test.fingerprints(), before);

    let mut session = take(&test.fixture, &ready);
    execution::commit(&test.fixture.state, &mut session, |_| {}).unwrap();
    let game = &test.fixture.game;
    assert_eq!(fs::read(game.join(DLL_PATH)).unwrap(), mod_dll());
    assert_eq!(fs::read(game.join(MISSING)).unwrap(), b"release list");
    assert_eq!(
        fs::read(game.join(CHANGED)).unwrap(),
        b"new release resource"
    );
    assert_eq!(fs::read(game.join(IDENTICAL)).unwrap(), b"example");
    assert_eq!(
        fs::read(game.join(OLD_ONLY)).unwrap(),
        b"previous release resource"
    );
    assert_eq!(
        fs::read(game.join(USER_FILE)).unwrap(),
        b"user customizations"
    );

    let manifests = archive::read_manifests(&test.manifests_dir());
    let [updated] = manifests.as_slice() else {
        panic!("expected the same single record: {manifests:?}");
    };
    assert_eq!(updated.repository_id, REPO);
    assert_eq!(updated.repository_binding.as_ref(), Some(&test.binding));
    assert_eq!(
        updated
            .files
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([DLL_PATH, MISSING, CHANGED, IDENTICAL]),
    );
}
