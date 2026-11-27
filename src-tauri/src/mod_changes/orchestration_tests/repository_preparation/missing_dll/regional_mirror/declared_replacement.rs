use super::*;
use crate::models::ReplacedModRepository;

fn removed_gitee_fixture() -> RegionalFixture {
    let mut test = setup_mirrors();
    let mut config = test.fixture.state.config.load().unwrap();
    config.repositories.retain(|repo| repo.id != GLOBAL_ID);
    let next = config
        .repositories
        .iter_mut()
        .find(|repo| repo.id == CN_ID)
        .unwrap();
    next.provider = Provider::Github;
    next.username = "NewPublisher".into();
    next.channel = Channel::Stable;
    next.source_id = Some(crate::repository_sources::DEFAULT_INDEX_ID.into());
    next.replaces_revision = Some(test.fixture.state.config.data_selection().1);
    let previous = test.old.repository_binding.as_mut().unwrap();
    previous.provider = Provider::Gitee;
    previous.owner = "OldPublisher".into();
    previous.channel = Channel::Stable;
    next.replaces = vec![ReplacedModRepository {
        provider: previous.provider.clone(),
        username: previous.owner.clone(),
        repository: previous.repository.clone(),
        channel: previous.channel,
    }];
    test.binding = ModRepositoryBinding::from_release(next, TAG).unwrap();
    test.fixture.state.config.save(&config).unwrap();
    archive::write_manifest(&test.fixture.state.config.manifests_dir(), &test.old).unwrap();
    test
}

fn verified_orphan() -> RegionalFixture {
    let test = removed_gitee_fixture();
    let state = &test.fixture.state;
    let mut config = state.config.load().unwrap();
    let next = config
        .repositories
        .iter_mut()
        .find(|repo| repo.id == CN_ID)
        .unwrap();
    let bytes = zip(&[(ENTRY, &mod_dll())]);
    next.cached_releases[0].assets[0] = served(&test.server, 0, ASSET, &bytes);
    state.config.save(&config).unwrap();
    test
}

fn withdraw_declaration(test: &RegionalFixture) {
    let state = &test.fixture.state;
    let mut config = state.config.load().unwrap();
    config
        .repositories
        .iter_mut()
        .find(|repo| repo.id == CN_ID)
        .unwrap()
        .replaces
        .clear();
    state.config.save(&config).unwrap();
}

#[tokio::test]
async fn declared_release_replacement_cancels_or_moves_the_removed_gitee_install_atomically() {
    let mut test = removed_gitee_fixture();
    let old_dll = test.install_previous_release();
    let before = test.fingerprints();
    let intent = test.offer().await;
    let ready = prepared_plan(test.prepare(Some(intent)).await.unwrap(), TAG);
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
async fn declared_orphan_requires_published_zip_digest_to_claim_a_missing_primary() {
    let test = removed_gitee_fixture();
    let before = test.fingerprints();
    let blocked = test.prepare(None).await.unwrap();
    assert_eq!(
        blocked.error,
        Some(install_claim_error(
            DESTINATION,
            &format!("manifests/{GLOBAL_ID}.toml"),
        )),
    );
    assert!(blocked.manifest_overwrite.is_none());
    assert_eq!(test.fingerprints(), before);
}

#[tokio::test]
async fn withdrawing_declared_replacement_invalidates_prepared_source_switch() {
    let mut test = removed_gitee_fixture();
    test.install_previous_release();
    let before = test.fingerprints();
    let intent = test.offer().await;
    let ready = prepared_plan(test.prepare(Some(intent)).await.unwrap(), TAG);
    withdraw_declaration(&test);

    let mut session = take(&test.fixture, &ready);
    assert_eq!(
        execution::commit(&test.fixture.state, &mut session, |_| {}).unwrap_err(),
        "modChanges.targetChanged"
    );
    assert_eq!(test.fingerprints(), before);
    assert!(!test.manifest(CN_ID).exists());
}

#[tokio::test]
async fn declared_orphan_leaves_unproven_or_competing_ownership_untouched() {
    for case in [
        "missing declaration",
        "stale beta tag",
        "other record",
        "live DLL",
    ] {
        let mut test = verified_orphan();
        match case {
            "missing declaration" => withdraw_declaration(&test),
            "stale beta tag" => {
                let state = &test.fixture.state;
                let mut config = state.config.load().unwrap();
                let next = config
                    .repositories
                    .iter_mut()
                    .find(|repo| repo.id == CN_ID)
                    .unwrap();
                next.channel = Channel::All;
                next.replaces[0].channel = Channel::All;
                state.config.save(&config).unwrap();
                let previous = test.old.repository_binding.as_mut().unwrap();
                previous.channel = Channel::All;
                previous.tag = "v1.0.0-beta.1".into();
                test.old.release_tag.clone_from(&previous.tag);
                archive::write_manifest(&state.config.manifests_dir(), &test.old).unwrap();
            }
            "other record" => test.add_other_claim(),
            "live DLL" => write(&test.disabled_dll(), b"existing disabled file"),
            _ => unreachable!(),
        }
        let before = test.fingerprints();
        let blocked = test.prepare(None).await.unwrap();
        assert_eq!(blocked.progress.stage, PreparationStage::Failed, "{case}");
        assert!(
            blocked.plan.is_none() && blocked.manifest_overwrite.is_none(),
            "{case}"
        );
        assert_eq!(test.fingerprints(), before, "{case}");
    }
}

#[tokio::test]
async fn declared_orphan_rechecks_dll_vacancy_after_preparation() {
    let test = verified_orphan();
    let ready = prepared_plan(test.prepare(None).await.unwrap(), TAG);
    let mut session = take(&test.fixture, &ready);
    write(
        &test.disabled_dll(),
        b"external file appeared after approval",
    );
    let changed = test.fingerprints();
    assert!(execution::commit(&test.fixture.state, &mut session, |_| {}).is_err());
    assert_eq!(test.fingerprints(), changed);
    assert!(!test.manifest(CN_ID).exists());
}

#[tokio::test]
async fn withdrawing_declared_orphan_during_apply_rolls_back_files_and_records() {
    let test = verified_orphan();
    let before = test.fingerprints();
    let ready = prepared_plan(test.prepare(None).await.unwrap(), TAG);
    let mut session = take(&test.fixture, &ready);
    let state = &test.fixture.state;
    let mut withdrew = false;
    let result = execution::commit(state, &mut session, |_| {
        if !withdrew && test.manifest(CN_ID).exists() && !test.manifest(GLOBAL_ID).exists() {
            withdraw_declaration(&test);
            withdrew = true;
        }
    });
    assert!(withdrew, "the transaction entered its live-write phase");
    assert_eq!(result.unwrap_err(), "modChanges.targetChanged");
    assert_eq!(test.fingerprints(), before);
    assert!(!has_pending(&state.config.mod_change_transactions_dir()).unwrap());
}
