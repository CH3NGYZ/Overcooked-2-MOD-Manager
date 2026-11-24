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
    let mut prepared = selected(&fixture, request);
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
    assert_eq!(
        execution::commit(&fixture.state, &mut prepared, |_| panic!(
            "live write started"
        ))
        .unwrap_err(),
        "modChanges.inventoryChanged"
    );
    assert!(prepared.plan.prepared);
    assert_eq!(Fingerprint::capture(&fixture.game).unwrap(), before);
    assert!(!fixture.state.config.manifests_dir().exists());
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
fn changed_payload_and_frozen_target_do_not_request_automatic_replan() {
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
    assert_eq!(fs::read(target).unwrap(), b"edited live target");
}
