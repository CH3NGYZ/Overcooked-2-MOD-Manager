use super::*;
use crate::http_diagnostics::capture_test_logs;

#[tokio::test]
async fn whole_chain_log_correlates_plan_verification_and_commit() {
    let name = "ChainLogWitness";
    let guid = "fixture.chain-log-witness";
    let fixture = fixture(vec![repository(name, guid, &["1.0.0"], Vec::new())]);
    let file = install(&fixture, name, guid, "1.0.0", false);
    let planning_log = capture_test_logs("app_lib::mod_changes::planning", "整链开始求解");
    let solver_log = capture_test_logs("app_lib::mod_changes::solve::trace", name);
    let plan_log = capture_test_logs("app_lib::mod_changes::session", name);
    let prepared_log = capture_test_logs("app_lib::mod_changes::preparation", "整链复核结论");
    let execution_log = capture_test_logs("app_lib::mod_changes::execution", "op=");

    let plan = planning::create(&fixture.state, request(name, None, true)).unwrap();
    assert_eq!(plan.result.status, SolveStatus::Ready);
    let operation = fixture.state.mod_changes.lock().unwrap().plans[&plan.plan_id]
        .operation_id()
        .to_owned();
    let prepared = preparation::prepare(fixture.state.clone(), plan.plan_id, |_| {}).await;
    assert_eq!(prepared.error, None);
    let mut session = take(&fixture, &prepared.plan.unwrap());
    super::super::execution::commit(&fixture.state, &mut session, |_| {}).unwrap();

    let contains = |messages: Vec<String>, markers: &[&str]| {
        assert!(
            messages.iter().any(|line| {
                line.contains(&format!("op={operation}"))
                    && markers.iter().all(|marker| line.contains(marker))
            }),
            "expected {markers:?} for {operation}; got {messages:?}"
        );
    };
    contains(
        planning_log.messages(),
        &["stage=initial", "policy=KeepInstalled"],
    );
    contains(
        planning_log.messages(),
        &["stage=verification", "policy=KeepInstalled"],
    );
    contains(
        solver_log.messages(),
        &["目标", "enabled=false", "requested_enabled=true"],
    );
    contains(
        solver_log.messages(),
        &["结果", "decision=enable_installed"],
    );
    contains(
        plan_log.messages(),
        &["整链行动", "stage=awaiting_apply", "action=Enable"],
    );
    contains(
        prepared_log.messages(),
        &["approval_changed=false", "rounds=1"],
    );
    contains(
        execution_log.messages(),
        &["整链事务已提交", "txn=", "elapsed_ms="],
    );
    assert!(fixture.game.join(file).is_file());
}

#[test]
fn whole_chain_log_detail_redacts_urls_secrets_and_caps_output() {
    let raw = "脚本 https://example.invalid/test?token=fixture-private Bearer fixture-secret\n";
    let result = super::super::session::log_detail(format_args!("{raw}{}", "字".repeat(2000)));
    assert!(result.contains("[REDACTED_SECRET]"), "{result}");
    assert!(!result.contains("https://"));
    assert!(!result.contains("fixture-private"));
    assert!(!result.contains("fixture-secret"));
    assert!(!result.contains('\n'));
    assert_eq!(result.chars().count(), 1401);
    assert!(result.ends_with('…'));
}
