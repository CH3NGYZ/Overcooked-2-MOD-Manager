use super::*;
use crate::mod_changes::execution;
use crate::mod_changes::transaction::has_pending;
use std::path::PathBuf;

#[path = "missing_dll/cross_id.rs"]
mod cross_id;
#[path = "missing_dll/relocated.rs"]
mod relocated;
#[path = "missing_dll/same_id.rs"]
mod same_id;
#[path = "missing_dll/stale_installed.rs"]
mod stale_installed;

const ASSET: &str = "Selected.zip";
const OLD_TAG: &str = "v1.0.0";
const TARGET: &str = "BepInEx/plugins/Fixture.dll";
const OLD_RESOURCE: &str = "BepInEx/plugins/Fixture/settings.txt";
fn install_claim_error(path: &str, manifests: &str) -> String {
    crate::locale_tag::with_params(
        "modChanges.installTargetClaimedWithManifests",
        &[("path", path), ("manifests", manifests)],
    )
}

struct ReinstallFixture {
    fixture: Fixture,
    server: Server,
    binding: ModRepositoryBinding,
}

fn setup(with_digest: bool) -> ReinstallFixture {
    setup_asset(with_digest, zip(&[(DLL, &mod_dll())]))
}

fn setup_asset(with_digest: bool, bytes: Vec<u8>) -> ReinstallFixture {
    let server = Server::new(vec![bytes.clone()], false);
    let mut repo = repository(REPO, GUID, &["2.0.0"], vec![]);
    repo.install_target = "BepInEx/plugins".into();
    let mut asset = served(&server, 0, ASSET, &bytes);
    if !with_digest {
        asset.digest = None;
    }
    repo.cached_releases[0].assets = vec![asset];
    let binding = ModRepositoryBinding::from_release(&repo, TAG).unwrap();
    ReinstallFixture {
        fixture: versioned_fixture(repo),
        server,
        binding,
    }
}

impl ReinstallFixture {
    fn old_record(&self) -> archive::InstallManifest {
        let mut record = archive::new_manifest(
            REPO,
            OLD_TAG,
            BTreeMap::new(),
            BTreeMap::new(),
            vec![TARGET.into()],
            vec![],
        );
        record.repository_binding = Some(ModRepositoryBinding {
            tag: OLD_TAG.into(),
            ..self.binding.clone()
        });
        record
    }

    fn manifests_dir(&self) -> PathBuf {
        self.fixture.state.config.manifests_dir()
    }

    fn write_record(&self, record: &archive::InstallManifest) {
        archive::write_manifest(&self.manifests_dir(), record).unwrap();
    }

    fn fingerprints(&self) -> (Fingerprint, Fingerprint) {
        (
            Fingerprint::capture(&self.fixture.game).unwrap(),
            Fingerprint::capture(&self.manifests_dir()).unwrap(),
        )
    }

    async fn prepare(&self) -> PreparationOutcome {
        let plan = select_plan(&self.fixture, REPO, TAG, ASSET);
        let outcome = preparation::prepare(self.fixture.state.clone(), plan.plan_id, |_| {}).await;
        assert_eq!(self.server.request_count(), 1);
        outcome
    }
}

#[tokio::test]
async fn online_reinstall_restores_manually_deleted_dll_and_replaces_its_record() {
    for (old_tag, legacy, with_digest) in [
        (TAG, false, true),
        (OLD_TAG, false, false),
        (OLD_TAG, true, true),
    ] {
        let test = setup(with_digest);
        let mut record = test.old_record();
        record.release_tag = old_tag.into();
        if legacy {
            record.repository_binding = None;
        } else {
            record.repository_binding.as_mut().unwrap().tag = old_tag.into();
        }
        let target = test.fixture.game.join(TARGET);
        let disabled = test.fixture.game.join(mods::disabled_dll_file(TARGET));
        if old_tag == TAG {
            let installed = prepared_plan(test.prepare().await, TAG);
            let mut session = take(&test.fixture, &installed);
            execution::commit(&test.fixture.state, &mut session, |_| {}).unwrap();
            assert_eq!(fs::read(&target).unwrap(), mod_dll());
        } else {
            let mut old_dll = PE.to_vec();
            old_dll.extend(mods::plugin_blob(GUID, REPO, "1.0.0"));
            write(&target, &old_dll);
            test.write_record(&record);
        }
        fs::remove_file(&target).unwrap();
        let before = test.fingerprints();

        let ready = prepared_plan(test.prepare().await, TAG);
        assert!(ready.untracked_overwrites.is_empty());
        assert_eq!(test.fingerprints(), before);
        assert!(!target.exists() && !disabled.exists());

        let mut session = take(&test.fixture, &ready);
        execution::commit(&test.fixture.state, &mut session, |_| {}).unwrap();
        assert_eq!(fs::read(&target).unwrap(), mod_dll());
        assert!(!disabled.exists());
        let manifests = archive::read_manifests(&test.manifests_dir());
        let [updated] = manifests.as_slice() else {
            panic!("expected exactly one installation record: {manifests:?}");
        };
        assert_eq!(updated.repository_id, REPO);
        assert_eq!(updated.release_tag, TAG);
        assert_eq!(updated.repository_binding.as_ref(), Some(&test.binding));
        assert_eq!(updated.files, [TARGET]);
    }
}

#[tokio::test]
async fn reinstall_leaves_unsafe_or_incomplete_old_records_unchanged() {
    for (case, with_digest) in [
        ("conflicting source", true),
        ("shared path", true),
        ("duplicate record", true),
        ("surviving old resource", true),
        ("legacy without digest", false),
    ] {
        let test = setup(with_digest);
        let mut old = test.old_record();
        match case {
            "conflicting source" => {
                old.repository_binding.as_mut().unwrap().owner = "AnotherOwner".into();
            }
            "shared path" => {
                let mut other = old.clone();
                other.repository_id = "AnotherRepository".into();
                test.write_record(&other);
            }
            "duplicate record" => {
                write(
                    &test.manifests_dir().join("duplicate.toml"),
                    toml::to_string(&old).unwrap().as_bytes(),
                );
            }
            "surviving old resource" => {
                write(&test.fixture.game.join(OLD_RESOURCE), b"old settings");
                old.files.push(OLD_RESOURCE.into());
            }
            "legacy without digest" => old.repository_binding = None,
            _ => unreachable!(),
        }
        test.write_record(&old);
        let before = test.fingerprints();

        let outcome = test.prepare().await;
        assert_eq!(outcome.progress.stage, PreparationStage::Failed, "{case}");
        assert!(outcome.plan.is_none(), "{case}");
        let manifests = match case {
            "conflicting source" => Some("manifests/FixtureMod.toml"),
            "shared path" => Some("manifests/AnotherRepository.toml"),
            "duplicate record" => Some("manifests/FixtureMod.toml\nmanifests/duplicate.toml"),
            _ => None,
        };
        if let Some(manifests) = manifests {
            assert_eq!(
                outcome.error,
                Some(install_claim_error(TARGET, manifests)),
                "{case}"
            );
        }
        assert_eq!(test.fingerprints(), before, "{case}");
    }
}

#[tokio::test]
async fn approved_reinstall_leaves_an_externally_restored_disabled_dll_untouched() {
    let test = setup(true);
    test.write_record(&test.old_record());
    let ready = prepared_plan(test.prepare().await, TAG);
    let mut session = take(&test.fixture, &ready);
    let restored = test.fixture.game.join(mods::disabled_dll_file(TARGET));
    write(&restored, &mod_dll());
    let before = test.fingerprints();

    execution::commit(&test.fixture.state, &mut session, |_| {}).unwrap_err();
    assert_eq!(test.fingerprints(), before);
    assert_eq!(fs::read(restored).unwrap(), mod_dll());
    assert!(!test.fixture.game.join(TARGET).exists());
}

#[tokio::test]
async fn reinstall_does_not_reclaim_a_new_member_claimed_by_another_package() {
    const OTHER_DLL: &str = "Other.dll";
    const OTHER_TARGET: &str = "BepInEx/plugins/Other.dll";
    let mut other = PE.to_vec();
    other.extend(mods::plugin_blob(
        "com.example.map-tool",
        "Map Tool",
        "1.0.0",
    ));
    for map_claim in [false, true] {
        let test = setup_asset(true, zip(&[(DLL, &mod_dll()), (OTHER_DLL, &other)]));
        let old = test.old_record();
        test.write_record(&old);
        let expected = if map_claim {
            let registration = serde_json::json!({
                "version": 1,
                "entries": [{ "dll": OTHER_TARGET, "owner": { "name": "Fixture Map" } }],
            });
            write(
                &test
                    .manifests_dir()
                    .join(crate::level_sets::LEVEL_SET_PLUGIN_REGISTRY),
                &serde_json::to_vec(&registration).unwrap(),
            );
            crate::locale_tag::with_params(
                "modChanges.installTargetMapProtected",
                &[("path", OTHER_TARGET)],
            )
        } else {
            test.write_record(&archive::InstallManifest {
                repository_id: "AnotherRepository".into(),
                files: vec![OTHER_TARGET.into()],
                repository_binding: None,
                ..old
            });
            crate::locale_tag::with_params(
                "modChanges.installTargetClaimedWithManifests",
                &[
                    ("path", OTHER_TARGET),
                    ("manifests", "manifests/AnotherRepository.toml"),
                ],
            )
        };
        let before = test.fingerprints();

        let outcome = test.prepare().await;
        assert_eq!(outcome.progress.stage, PreparationStage::Failed);
        assert_eq!(outcome.error, Some(expected));
        assert_eq!(test.fingerprints(), before);
    }
}

#[tokio::test]
async fn reinstall_rolls_back_when_disabled_dll_reappears_during_application() {
    for (test, old, target) in [
        (setup(true), TARGET, TARGET),
        (
            relocated::setup_relocation(),
            relocated::PREVIOUS,
            relocated::DESTINATION,
        ),
    ] {
        let mut record = test.old_record();
        record.files = vec![old.into()];
        test.write_record(&record);
        assert_apply_rolls_back_on_disabled_restore(&test, target).await;
    }
}

async fn assert_apply_rolls_back_on_disabled_restore(test: &ReinstallFixture, destination: &str) {
    let ready = prepared_plan(test.prepare().await, TAG);
    let mut session = take(&test.fixture, &ready);
    let target = test.fixture.game.join(destination);
    let disabled = mods::disabled_dll_file(destination);
    let restored = test.fixture.game.join(&disabled);
    let manifest = test.manifests_dir().join(format!("{REPO}.toml"));
    let old_record = fs::read(&manifest).unwrap();
    let mut restored_during_apply = false;
    let mut rolled_back = false;

    let result = execution::commit(&test.fixture.state, &mut session, |progress| {
        let event = serde_json::to_value(progress).unwrap();
        let items = event["items"].as_array().unwrap();
        rolled_back |= items.iter().any(|item| item["stage"] == "rolled_back");
        if !restored_during_apply && items.iter().any(|item| item["stage"] == "processed") {
            assert_eq!(fs::read(&target).unwrap(), mod_dll());
            assert_ne!(fs::read(&manifest).unwrap(), old_record);
            write(&restored, b"externally restored disabled DLL");
            restored_during_apply = true;
        }
    });

    assert!(restored_during_apply);
    assert_eq!(
        result.unwrap_err(),
        crate::locale_tag::with_params(
            "modChanges.installTargetVerificationChanged",
            &[("path", &disabled)]
        )
    );
    assert!(rolled_back);
    assert!(
        !has_pending(
            &test
                .manifests_dir()
                .with_file_name("mod-change-transactions")
        )
        .unwrap()
    );
    assert!(!target.exists());
    assert_eq!(fs::read(&manifest).unwrap(), old_record);
    assert_eq!(
        fs::read(restored).unwrap(),
        b"externally restored disabled DLL"
    );
}
