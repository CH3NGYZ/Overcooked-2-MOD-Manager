use super::*;

#[path = "regional_mirror/declared_replacement.rs"]
mod declared_replacement;
#[path = "regional_mirror/recovery_scope.rs"]
mod recovery_scope;

const GLOBAL_ID: &str = "608a4a22-c039-4a37-be2f-c6bea98d4ba3";
const CN_ID: &str = "8c5a97dc-48f9-49c4-a1b1-20bc6fe164ee";
const SOURCE_ID: &str = "repositories-arcade-fixture";
const ENTRY: &str = "ArcadeFixture/v2.0.0/Fixture.dll";
const DESTINATION: &str = "BepInEx/plugins/ArcadeFixture/v2.0.0/Fixture.dll";

struct RegionalFixture {
    fixture: Fixture,
    server: Server,
    old: archive::InstallManifest,
    binding: ModRepositoryBinding,
    target: &'static str,
}

fn setup_mirrors() -> RegionalFixture {
    setup_sources(false)
}

fn setup_sources(same_region_different_lines: bool) -> RegionalFixture {
    setup_switch(same_region_different_lines, "BepInEx/plugins", DESTINATION)
}

fn setup_switch(
    same_region_different_lines: bool,
    install_target: &str,
    target: &'static str,
) -> RegionalFixture {
    let bytes = zip(&[(ENTRY, &mod_dll())]);
    let server = Server::new(vec![bytes.clone()], false);
    let mut global = repository(REPO, GUID, &[], vec![]);
    global.id = GLOBAL_ID.into();
    global.source_id = Some(SOURCE_ID.into());
    global.region = Some("global".into());
    global.username = "Fixture".into();
    global.channel = Channel::All;
    global.install_target = "BepInEx/plugins".into();
    let mut cn = repository(REPO, GUID, &["2.0.0"], vec![]);
    cn.id = CN_ID.into();
    cn.provider = Provider::Gitee;
    cn.source_id = Some(SOURCE_ID.into());
    cn.region = Some("cn".into());
    cn.username = "fixture".into();
    cn.channel = Channel::All;
    cn.install_target = install_target.into();
    cn.asset_rules[0].value = ASSET.into();
    let mut asset = served(&server, 0, ASSET, &bytes);
    asset.digest = None;
    cn.cached_releases[0].assets = vec![asset];
    if same_region_different_lines {
        global.provider = Provider::Gitee;
        global.region = Some("cn".into());
        global.repository = "Fixture-Preview".into();
        global.relations.push(conflicts_with(false));
        cn.repository = "Fixture-Stable".into();
        cn.relations.push(conflicts_with(true));
    }
    let binding = ModRepositoryBinding::from_release(&cn, TAG).unwrap();
    let fixture = versioned_fixture(cn);
    let mut config = fixture.state.config.load().unwrap();
    config.repository_sources.push(RepositorySource {
        id: SOURCE_ID.into(),
        nickname: "Arcade fixture".into(),
        nickname_i18n: BTreeMap::new(),
        note_i18n: BTreeMap::new(),
        url: "https://example.invalid/arcade.toml".into(),
        region: None,
        builtin: true,
        parent_id: Some(crate::repository_sources::DEFAULT_INDEX_ID.into()),
        network_plan: Default::default(),
    });
    let mut old = archive::new_manifest(
        GLOBAL_ID,
        TAG,
        BTreeMap::new(),
        BTreeMap::new(),
        vec![DESTINATION.into()],
        vec![],
    );
    old.repository_binding = ModRepositoryBinding::from_release(&global, TAG);
    config.repositories.push(global);
    fixture
        .state
        .config
        .save_with_selection(&mut config, Some("cn"), None)
        .unwrap();
    assert_eq!(config.region, "cn");
    let configured: Vec<_> = config
        .repositories
        .iter()
        .filter(|repo| repo.enabled)
        .collect();
    assert_eq!(configured.len(), 2);
    assert!(configured.iter().any(|repo| repo.id == GLOBAL_ID));
    assert!(configured.iter().any(|repo| repo.id == CN_ID));
    archive::write_manifest(&fixture.state.config.manifests_dir(), &old).unwrap();
    RegionalFixture {
        fixture,
        server,
        old,
        binding,
        target,
    }
}

fn conflicts_with(prerelease: bool) -> RepositoryRelation {
    RepositoryRelation {
        kind: RelationKind::Conflicts,
        guid: GUID.into(),
        version: String::new(),
        prerelease: Some(prerelease),
        optional: false,
    }
}

impl RegionalFixture {
    fn manifest(&self, repository: &str) -> PathBuf {
        self.fixture
            .state
            .config
            .manifests_dir()
            .join(format!("{repository}.toml"))
    }

    fn fingerprints(&self) -> (Fingerprint, Fingerprint) {
        (
            Fingerprint::capture(&self.fixture.game).unwrap(),
            Fingerprint::capture(&self.fixture.state.config.manifests_dir()).unwrap(),
        )
    }

    fn disabled_dll(&self) -> PathBuf {
        self.fixture.game.join(mods::disabled_dll_file(DESTINATION))
    }

    fn change_old_manifest(&self) {
        let mut old = self.old.clone();
        old.files.push(OLD_RESOURCE.into());
        archive::write_manifest(&self.fixture.state.config.manifests_dir(), &old).unwrap();
    }

    fn replace_old_manifest_with_same_bytes(&self) {
        let manifest = self.manifest(GLOBAL_ID);
        let original = self.fixture.game.parent().unwrap().join("old-toml");
        let before = Fingerprint::capture(&manifest).unwrap();
        fs::rename(&manifest, &original).unwrap();
        write(&manifest, &fs::read(&original).unwrap());
        fs::set_permissions(&manifest, fs::metadata(&original).unwrap().permissions()).unwrap();
        assert_eq!(Fingerprint::capture(&manifest).unwrap(), before);
    }

    fn add_other_claim(&self) {
        archive::write_manifest(
            &self.fixture.state.config.manifests_dir(),
            &archive::InstallManifest {
                repository_id: "AnotherRepository".into(),
                repository_binding: None,
                ..self.old.clone()
            },
        )
        .unwrap();
    }

    fn install_previous_release(&mut self) -> Vec<u8> {
        let mut old_dll = PE.to_vec();
        old_dll.extend(mods::plugin_blob(GUID, REPO, "1.0.0"));
        self.old.release_tag = OLD_TAG.into();
        self.old.repository_binding.as_mut().unwrap().tag = OLD_TAG.into();
        archive::write_manifest(&self.fixture.state.config.manifests_dir(), &self.old).unwrap();
        write(&self.fixture.game.join(DESTINATION), &old_dll);
        old_dll
    }

    fn install_preview_release(&mut self) {
        const PREVIEW: &str = "v3.0.0-beta.1";
        let mut old_dll = PE.to_vec();
        old_dll.extend(mods::plugin_blob(GUID, REPO, "3.0.0-beta.1"));
        self.old.release_tag = PREVIEW.into();
        self.old.repository_binding.as_mut().unwrap().tag = PREVIEW.into();
        self.old.prerelease = true;
        self.old.relations = vec![conflicts_with(false)];
        archive::write_manifest(&self.fixture.state.config.manifests_dir(), &self.old).unwrap();
        write(&self.fixture.game.join(DESTINATION), &old_dll);
    }

    async fn prepare(
        &self,
        intent: Option<ManifestOverwriteIntent>,
    ) -> Result<PreparationOutcome, String> {
        let mut selection = request(CN_ID, Some(TAG), true);
        selection.roots[0].asset_name = Some(ASSET.into());
        selection.manifest_overwrite = intent;
        let plan = planning::create(&self.fixture.state, selection)?;
        assert!(!plan.prepared);
        Ok(preparation::prepare(self.fixture.state.clone(), plan.plan_id, |_| {}).await)
    }

    async fn offer(&self) -> ManifestOverwriteIntent {
        let before = self.fingerprints();
        let outcome = self.prepare(None).await.unwrap();
        assert_eq!(self.fingerprints(), before);
        self.checked_offer(outcome)
    }

    fn checked_offer(&self, outcome: PreparationOutcome) -> ManifestOverwriteIntent {
        assert_eq!(outcome.progress.stage, PreparationStage::Failed);
        assert!(outcome.plan.is_none());
        assert!(outcome.error.is_some());
        let evidence = serde_json::to_value(&outcome).unwrap();
        let offer = outcome
            .manifest_overwrite
            .unwrap_or_else(|| panic!("a safe source switch must offer confirmation: {evidence}"));
        let switch = &evidence["manifestOverwrite"]["sourceSwitch"];
        assert_eq!(
            switch["previousSource"],
            serde_json::to_value(&self.old.repository_binding).unwrap()
        );
        assert_eq!(
            switch["nextSource"],
            serde_json::to_value(&self.binding).unwrap()
        );
        assert_eq!(switch["targetPath"], self.target);
        if self.fixture.game.join(DESTINATION).exists() {
            assert_eq!(switch["installedPath"], DESTINATION);
        } else if self.disabled_dll().exists() {
            assert_eq!(
                switch["installedPath"],
                mods::disabled_dll_file(DESTINATION)
            );
        } else {
            assert!(switch.get("installedPath").is_none());
        }
        assert_eq!(offer.repository_id, CN_ID);
        assert!(
            offer
                .manifest_path
                .replace('\\', "/")
                .ends_with(&format!("manifests/{GLOBAL_ID}.toml"))
        );
        assert_eq!(offer.recorded_paths, [DESTINATION]);
        assert!(!offer.fingerprint.is_empty());
        serde_json::from_value(serde_json::json!({
            "repositoryId": offer.repository_id,
            "fingerprint": offer.fingerprint,
        }))
        .unwrap()
    }

    fn assert_new_install(&self) {
        assert_eq!(
            fs::read(self.fixture.game.join(self.target)).unwrap(),
            mod_dll()
        );
        if self.target != DESTINATION {
            assert!(!self.fixture.game.join(DESTINATION).exists());
        }
        assert!(!self.disabled_dll().exists());
        assert!(!self.manifest(GLOBAL_ID).exists());
        assert!(self.manifest(CN_ID).is_file());
        let manifests = archive::read_manifests(&self.fixture.state.config.manifests_dir());
        let [installed] = manifests.as_slice() else {
            panic!("expected exactly one migrated record: {manifests:?}");
        };
        assert_eq!(installed.repository_id, CN_ID);
        assert_eq!(installed.release_tag, TAG);
        assert_eq!(installed.repository_binding.as_ref(), Some(&self.binding));
        assert_eq!(installed.files, [self.target]);
    }
}

#[tokio::test]
async fn regional_mirror_reinstall_requires_explicit_overwrite_and_final_confirmation() {
    let test = setup_mirrors();
    let before = test.fingerprints();
    let blocked = test.prepare(None).await.unwrap();
    assert_eq!(
        blocked.error,
        Some(install_claim_error(
            DESTINATION,
            &format!("manifests/{GLOBAL_ID}.toml"),
        )),
    );
    assert_eq!(test.server.request_count(), 1);
    assert_eq!(test.fingerprints(), before);
    let intent = test.checked_offer(blocked);

    let canceled = prepared_plan(test.prepare(Some(intent)).await.unwrap(), TAG);
    assert_eq!(test.fingerprints(), before);
    execution::cancel(&test.fixture.state, &canceled.plan_id).unwrap();
    assert_eq!(
        execution::cancel(&test.fixture.state, &canceled.plan_id),
        Err("modChanges.planExpired".into())
    );
    assert_eq!(test.fingerprints(), before);

    let fresh_intent = test.offer().await;
    let ready = prepared_plan(test.prepare(Some(fresh_intent)).await.unwrap(), TAG);
    assert!(ready.untracked_overwrites.is_empty());
    assert_eq!(test.fingerprints(), before);
    let mut session = take(&test.fixture, &ready);
    execution::commit(&test.fixture.state, &mut session, |_| {}).unwrap();

    test.assert_new_install();
}

#[tokio::test]
async fn regional_mirror_can_switch_an_installed_release_without_manually_removing_the_dll() {
    let mut test = setup_mirrors();
    let old_dll = test.install_previous_release();
    let before = test.fingerprints();

    let current_offer = test.offer().await;
    let ready = prepared_plan(test.prepare(Some(current_offer)).await.unwrap(), TAG);
    assert!(ready.untracked_overwrites.is_empty());
    assert_eq!(test.fingerprints(), before);
    execution::cancel(&test.fixture.state, &ready.plan_id).unwrap();
    assert_eq!(test.fingerprints(), before);
    assert_eq!(
        fs::read(test.fixture.game.join(DESTINATION)).unwrap(),
        old_dll
    );

    let intent = test.offer().await;
    let ready = prepared_plan(test.prepare(Some(intent)).await.unwrap(), TAG);
    assert_eq!(test.fingerprints(), before);
    let mut session = take(&test.fixture, &ready);
    execution::commit(&test.fixture.state, &mut session, |_| {}).unwrap();
    test.assert_new_install();
}

#[tokio::test]
async fn regional_mirror_can_switch_from_preview_to_older_stable_in_the_same_region() {
    let mut test = setup_sources(true);
    test.install_preview_release();
    let before = test.fingerprints();

    let intent = test.offer().await;
    let ready = prepared_plan(test.prepare(Some(intent)).await.unwrap(), TAG);
    assert!(ready.untracked_overwrites.is_empty());
    assert_eq!(test.fingerprints(), before);
    let mut session = take(&test.fixture, &ready);
    execution::commit(&test.fixture.state, &mut session, |_| {}).unwrap();

    test.assert_new_install();
}

#[tokio::test]
async fn regional_mirror_can_install_over_a_disabled_same_version_dll() {
    let test = setup_mirrors();
    write(&test.disabled_dll(), &mod_dll());
    let before = test.fingerprints();

    let intent = test.offer().await;
    let ready = prepared_plan(test.prepare(Some(intent)).await.unwrap(), TAG);
    assert_eq!(test.fingerprints(), before);
    let mut session = take(&test.fixture, &ready);
    execution::commit(&test.fixture.state, &mut session, |_| {}).unwrap();
    test.assert_new_install();
}

#[tokio::test]
async fn regional_mirror_rechecks_old_dll_and_manifest_while_switching_directories() {
    const RELOCATED: &str = "BepInEx/plugins/Next/ArcadeFixture/v2.0.0/Fixture.dll";
    for race in [None, Some(DESTINATION), Some("manifest")] {
        let mut test = setup_switch(false, "BepInEx/plugins/Next", RELOCATED);
        let old_dll = test.install_previous_release();
        let before = test.fingerprints();
        let intent = test.offer().await;
        let ready = prepared_plan(test.prepare(Some(intent)).await.unwrap(), TAG);
        assert_eq!(test.fingerprints(), before);
        let mut session = take(&test.fixture, &ready);
        let mut intervened = None;
        let replacement_at = race.map(|path| {
            if path == "manifest" {
                test.manifest(GLOBAL_ID)
            } else {
                test.fixture.game.join(path)
            }
        });
        let result = execution::commit(&test.fixture.state, &mut session, |progress| {
            let Some(path) = &replacement_at else { return };
            let state = serde_json::to_value(progress).unwrap();
            if intervened.is_none()
                && state["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|item| item["stage"] == "applying")
            {
                let old = test
                    .fixture
                    .game
                    .parent()
                    .unwrap()
                    .join("untouched-original");
                fs::rename(path, &old).unwrap();
                write(path, &fs::read(&old).unwrap());
                fs::set_permissions(path, fs::metadata(&old).unwrap().permissions()).unwrap();
                intervened = crate::file_metadata::FileIdentity::capture(path).unwrap();
            }
        });
        if let Some(path) = replacement_at {
            assert!(intervened.is_some(), "race was not reached: {race:?}");
            result.unwrap_err();
            assert_eq!(
                crate::file_metadata::FileIdentity::capture(&path).unwrap(),
                intervened
            );
            assert_eq!(
                fs::read(test.fixture.game.join(DESTINATION)).unwrap(),
                old_dll
            );
            assert!(test.manifest(GLOBAL_ID).is_file());
            assert!(!test.manifest(CN_ID).exists());
            assert!(!test.fixture.game.join(RELOCATED).exists());
            assert_eq!(test.fingerprints(), before);
        } else {
            result.unwrap();
            test.assert_new_install();
        }
    }
}

#[tokio::test]
async fn regional_mirror_offer_is_bound_to_the_unchanged_manifest_and_vacancy() {
    for replacement in [
        "changed content",
        "same bytes in another file",
        "restored enabled DLL",
    ] {
        let test = setup_mirrors();
        let intent = test.offer().await;
        match replacement {
            "changed content" => test.change_old_manifest(),
            "same bytes in another file" => test.replace_old_manifest_with_same_bytes(),
            "restored enabled DLL" => write(&test.fixture.game.join(DESTINATION), &mod_dll()),
            _ => unreachable!(),
        }
        let before = test.fingerprints();

        let result = test.prepare(Some(intent)).await;
        if let Ok(outcome) = result {
            assert_eq!(
                outcome.progress.stage,
                PreparationStage::Failed,
                "{replacement}"
            );
            assert!(outcome.plan.is_none(), "{replacement}");
            assert!(outcome.error.is_some(), "{replacement}");
        }
        assert_eq!(test.fingerprints(), before, "{replacement}");
    }
}

#[tokio::test]
async fn regional_mirror_does_not_offer_overwrite_for_unproven_or_competing_claims() {
    for obstacle in [
        "unrecognized enabled file",
        "unrecognized disabled file",
        "third-party claim",
        "different source leaf",
        "different owner",
    ] {
        let mut test = setup_mirrors();
        match obstacle {
            "unrecognized enabled file" => {
                write(&test.fixture.game.join(DESTINATION), b"external file")
            }
            "unrecognized disabled file" => write(&test.disabled_dll(), b"external disabled file"),
            "third-party claim" => test.add_other_claim(),
            "different source leaf" => {
                let mut config = test.fixture.state.config.load().unwrap();
                let mut other_source = config
                    .repository_sources
                    .iter()
                    .find(|source| source.id == SOURCE_ID)
                    .unwrap()
                    .clone();
                other_source.id = "other-fixture".into();
                config
                    .repositories
                    .iter_mut()
                    .find(|repo| repo.id == GLOBAL_ID)
                    .unwrap()
                    .source_id = Some(other_source.id.clone());
                config.repository_sources.push(other_source);
                test.fixture.state.config.save(&config).unwrap();
            }
            "different owner" => {
                test.old.repository_binding.as_mut().unwrap().owner = "UnrelatedOwner".into();
                archive::write_manifest(&test.fixture.state.config.manifests_dir(), &test.old)
                    .unwrap();
            }
            _ => unreachable!(),
        }
        let before = test.fingerprints();
        if let Ok(outcome) = test.prepare(None).await {
            assert_eq!(
                outcome.progress.stage,
                PreparationStage::Failed,
                "{obstacle}"
            );
            assert!(outcome.plan.is_none(), "{obstacle}");
            assert!(outcome.manifest_overwrite.is_none(), "{obstacle}");
        }
        assert_eq!(test.fingerprints(), before, "{obstacle}");
    }
}

#[tokio::test]
async fn regional_mirror_confirmation_rechecks_the_old_manifest_and_vacant_paths() {
    for changed in [
        "enabled DLL",
        "disabled DLL",
        "third-party claim",
        "changed TOML",
        "replacement TOML",
    ] {
        let test = setup_mirrors();
        let intent = test.offer().await;
        let ready = prepared_plan(test.prepare(Some(intent)).await.unwrap(), TAG);
        let mut session = take(&test.fixture, &ready);
        match changed {
            "enabled DLL" => write(&test.fixture.game.join(DESTINATION), b"external file"),
            "disabled DLL" => write(&test.disabled_dll(), b"external disabled file"),
            "third-party claim" => test.add_other_claim(),
            "changed TOML" => test.change_old_manifest(),
            "replacement TOML" => test.replace_old_manifest_with_same_bytes(),
            _ => unreachable!(),
        }
        let before = test.fingerprints();

        execution::commit(&test.fixture.state, &mut session, |_| {}).unwrap_err();

        assert_eq!(test.fingerprints(), before, "{changed}");
        assert!(test.manifest(GLOBAL_ID).is_file(), "{changed}");
        assert!(!test.manifest(CN_ID).exists(), "{changed}");
    }
}

#[tokio::test]
async fn regional_mirror_rolls_back_if_a_disabled_dll_reappears_after_live_writes() {
    let test = setup_mirrors();
    let intent = test.offer().await;
    let ready = prepared_plan(test.prepare(Some(intent)).await.unwrap(), TAG);
    let mut session = take(&test.fixture, &ready);
    let old = test.manifest(GLOBAL_ID);
    let old_bytes = fs::read(&old).unwrap();
    let new = test.manifest(CN_ID);
    let target = test.fixture.game.join(DESTINATION);
    let restored = test.disabled_dll();
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
            &[("path", &mods::disabled_dll_file(DESTINATION))],
        ),
    );
    assert_eq!(fs::read(&old).unwrap(), old_bytes);
    assert!(!new.exists() && !target.exists());
    assert_eq!(fs::read(restored).unwrap(), b"external disabled DLL");
    assert!(!has_pending(&test.fixture.state.config.mod_change_transactions_dir()).unwrap());
}
