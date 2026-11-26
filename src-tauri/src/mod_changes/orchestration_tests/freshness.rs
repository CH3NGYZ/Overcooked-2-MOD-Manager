use super::*;
use crate::mod_changes::execution;

const TARGET: &str = "BepInEx/plugins/online/online.dll";
const EXTERNAL: &str = "BepInEx/plugins/external/external.dll";

fn online() -> (Fixture, PlanRequest) {
    (
        fixture(vec![repository(
            "online",
            "fixture-online",
            &["1.0.0"],
            vec![],
        )]),
        request("online", Some("v1.0.0"), true),
    )
}

fn add_external(fixture: &Fixture) {
    write(
        &fixture.game.join(EXTERNAL),
        &mods::plugin_blob("external", "External", "1.0.0"),
    );
}

fn selected(fixture: &Fixture, request: PlanRequest) -> PlanSession {
    let plan = planning::create(&fixture.state, request).unwrap();
    let mut session = take(fixture, &plan);
    verify_selected(fixture, &mut session);
    planning::remember_payloads(&mut session).unwrap();
    session
}

#[test]
fn refreshing_release_cache_does_not_change_the_installation_scope() {
    let (fixture, request) = online();
    let mut session = selected(&fixture, request);
    let before = Fingerprint::capture(&fixture.game).unwrap();
    let mut config = fixture.state.config.load().unwrap();
    config.repositories[0].releases_cached_at = Some(2);
    config.repositories[0].etag = Some("refreshed-etag".into());
    config.repositories[0].cached_releases[0].assets[0].download_count = Some(25);
    fixture.state.config.save(&config).unwrap();
    preparation::finish(&fixture.state, &mut session).unwrap();
    assert_eq!(Fingerprint::capture(&fixture.game).unwrap(), before);

    session.plan.prepared = true;
    config.repositories[0].releases_cached_at = Some(3);
    fixture.state.config.save(&config).unwrap();
    execution::commit(&fixture.state, &mut session, |_| {}).unwrap();
    assert!(fixture.game.join(TARGET).is_file());
}

#[test]
fn changes_to_release_asset_proof_still_require_a_fresh_plan() {
    let (fixture, request) = online();
    let mut session = selected(&fixture, request.clone());
    let scope = session.plan.replan_scope.clone();
    let before = Fingerprint::capture(&fixture.game).unwrap();
    let mut config = fixture.state.config.load().unwrap();
    config.repositories[0].cached_releases[0].assets[0].digest = Some("sha256:changed".into());
    fixture.state.config.save(&config).unwrap();
    assert_eq!(
        preparation::finish(&fixture.state, &mut session)
            .unwrap_err()
            .message,
        "modChanges.inventoryChanged"
    );

    let next = planning::create(&fixture.state, request).unwrap();
    assert_eq!(next.replan_scope, scope);
    assert_eq!(Fingerprint::capture(&fixture.game).unwrap(), before);
}

#[test]
fn cache_timestamp_order_keeps_the_exact_installed_prerelease_verdict_fresh() {
    let (fixture, _) = online();
    let mut config = fixture.state.config.load().unwrap();
    let repository = &mut config.repositories[0];
    let tag = "v1.0.0";
    let binding = ModRepositoryBinding::from_release(repository, tag).unwrap();
    repository
        .release_prerelease_metadata
        .insert(tag.into(), true);
    config.staged_release_cache.insert(
        "older-source".into(),
        StagedReleaseCache {
            repository_key: Some(crate::releases::repository_cache_key(
                binding.provider.clone(),
                &binding.owner,
                &binding.repository,
            )),
            release_prerelease_metadata: BTreeMap::from([(tag.into(), false)]),
            cached_at: Some(2),
            ..Default::default()
        },
    );
    let revision =
        |config: &AppConfig| inventory::revision(&fixture.state, &fixture.game, config).unwrap();
    let before = revision(&config);
    assert!(!mods::installed_prerelease(
        &config,
        Some(&binding),
        false,
        None
    ));
    config
        .staged_release_cache
        .get_mut("older-source")
        .unwrap()
        .cached_at = Some(5);
    assert_eq!(revision(&config), before);
    config.repositories[0].releases_cached_at = Some(6);
    assert!(mods::installed_prerelease(
        &config,
        Some(&binding),
        false,
        None
    ));
    assert_ne!(revision(&config), before);
}

#[tokio::test]
async fn initial_prepare_distinguishes_inventory_change_and_keeps_replan_scope() {
    let (fixture, request) = online();
    let original = planning::create(&fixture.state, request.clone()).unwrap();
    add_external(&fixture);
    let before = Fingerprint::capture(&fixture.game).unwrap();
    let outcome =
        preparation::prepare(fixture.state.clone(), original.plan_id.clone(), |_| {}).await;
    assert_eq!(
        outcome.error.as_deref(),
        Some("modChanges.inventoryChanged")
    );
    assert!(outcome.plan.is_none());
    let fresh = planning::create(&fixture.state, request).unwrap();
    assert_ne!(fresh.plan_id, original.plan_id);
    let scope = original.replan_scope.as_deref().unwrap();
    assert_eq!(scope.len(), 64);
    assert!(scope.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert_eq!(fresh.replan_scope, original.replan_scope);
    assert_eq!(Fingerprint::capture(&fixture.game).unwrap(), before);
}

#[test]
fn final_prepare_and_execute_distinguish_inventory_change_before_any_live_write() {
    let (fixture, request) = online();
    let mut preparing = selected(&fixture, request.clone());
    let mut prepared = selected(&fixture, request.clone());
    preparation::finish(&fixture.state, &mut prepared).unwrap();
    prepared.plan.prepared = true;
    add_external(&fixture);
    let before = Fingerprint::capture(&fixture.game).unwrap();

    assert_eq!(
        preparation::finish(&fixture.state, &mut preparing)
            .unwrap_err()
            .message,
        "modChanges.inventoryChanged"
    );
    let error = execution::commit(&fixture.state, &mut prepared, |_| {
        panic!("live write started")
    })
    .unwrap_err();
    assert_eq!(error, "modChanges.inventoryChanged");
    assert!(execution::can_replan_failure(&prepared, &error));
    assert!(prepared.plan.prepared);
    assert_eq!(Fingerprint::capture(&fixture.game).unwrap(), before);
    assert!(!fixture.state.config.manifests_dir().exists());

    let mut fresh = selected(&fixture, request);
    preparation::finish(&fixture.state, &mut fresh).unwrap();
    fresh.plan.prepared = true;
    execution::commit(&fixture.state, &mut fresh, |_| {}).unwrap();
    assert!(fixture.game.join(TARGET).is_file());
    assert!(!execution::can_replan_failure(&fresh, &error));
}

#[test]
fn switched_config_source_and_game_stay_generic_even_if_inventory_changed() {
    let (fixture, request) = online();
    let mut session = selected(&fixture, request.clone());
    let initial_scope = session.plan.replan_scope.clone();
    add_external(&fixture);
    let mut config = fixture.state.config.load().unwrap();
    config.ui_zoom += 0.25;
    fixture.state.config.save(&config).unwrap();
    assert_eq!(
        preparation::finish(&fixture.state, &mut session)
            .unwrap_err()
            .message,
        "modChanges.targetChanged"
    );
    let mut session = selected(&fixture, request.clone());
    let config_scope = session.plan.replan_scope.clone();
    assert_ne!(config_scope, initial_scope);

    fixture
        .state
        .config
        .select_data_provider(DataProvider::EdgeOne);
    assert_eq!(
        preparation::finish(&fixture.state, &mut session)
            .unwrap_err()
            .message,
        "modChanges.targetChanged"
    );
    let mut session = selected(&fixture, request);
    assert_ne!(session.plan.replan_scope, config_scope);
    let other = fixture.game.with_file_name("other-game");
    write(&other.join("BepInEx/core/BepInEx.dll"), b"another game");
    let mut config = fixture.state.config.load().unwrap();
    config.game.path = other.to_string_lossy().into_owned();
    fixture.state.config.save(&config).unwrap();
    assert_eq!(
        preparation::finish(&fixture.state, &mut session)
            .unwrap_err()
            .message,
        "modChanges.targetChanged"
    );
}

#[test]
fn changed_staged_payload_is_rejected_and_changed_target_can_only_be_replanned() {
    let (fixture, request) = online();
    let mut preparing = selected(&fixture, request.clone());
    let mut prepared = selected(&fixture, request);
    preparation::finish(&fixture.state, &mut prepared).unwrap();
    prepared.plan.prepared = true;
    add_external(&fixture);
    let payload = preparing.payloads.values().next().unwrap();
    let source = payload.stage_root.join(&payload.files[0]);
    fs::write(&source, b"edited staged payload").unwrap();
    assert_eq!(
        preparation::finish(&fixture.state, &mut preparing)
            .unwrap_err()
            .message,
        "modChanges.payloadChanged"
    );

    let target = fixture.game.join(TARGET);
    write(&target, b"edited live target");
    let error = execution::commit(&fixture.state, &mut prepared, |_| {
        panic!("live write started")
    })
    .unwrap_err();
    assert_eq!(
        crate::locale_tag::diagnostic_tag(&error),
        "modChanges.targetChanged"
    );
    assert!(execution::can_replan_failure(&prepared, &error));
    assert!(!execution::can_replan_failure(
        &prepared,
        "modChanges.payloadChanged"
    ));
    assert_eq!(fs::read(target).unwrap(), b"edited live target");
}
