use super::*;
use std::sync::Barrier;

fn legacy_failure() -> ResetRecord {
    serde_json::from_value(serde_json::json!({
        "token": uuid::Uuid::new_v4(),
        "owner_pid": 1,
        "owner_started": 0,
        "phase": "failed",
        "error": "tools.migrateDataUnsafe"
    }))
    .unwrap()
}

#[test]
fn previous_record_without_locale_or_stage_remains_readable() {
    let record = legacy_failure();
    assert_eq!(record.locale, None);
    assert_eq!(record.failed_from, None);
    assert!(!record.reported);
    assert!(matches!(record.action, DataAction::Clear));
    let text = format_failure(&record, None, "en");
    assert!(text.contains("Stage: unknown"));
    assert!(text.contains("failed a safety check"));
    assert!(!text.contains("tools.migrateDataUnsafe"));
}

#[test]
fn supported_languages_render_stage_paths_and_decode_target_reason() {
    let mut record = legacy_failure();
    let source = Path::new("中文源目录/OC2ModManager");
    let target = Path::new("中文目标目录/OC2ModManager");
    let encoded_target = r"D:\胡闹厨房+100%&?=\OC2ModManager";
    record.action = DataAction::Migrate {
        source: source.to_owned(),
        target: target.to_owned(),
    };
    record.failed_from = Some(ResetPhase::Copying);
    record.error = Some(crate::locale_tag::with_params(
        "tools.migrateDataTargetExists",
        &[("path", encoded_target)],
    ));
    for locale in ["zh-CN", "zh-TW", "en", "ko", "ja"] {
        let text = format_failure(&record, Some(source), locale);
        for line in [
            message("dataOperationCopying", locale).to_owned(),
            path_line("dataOperationSource", locale, source),
            path_line("dataOperationTarget", locale, target),
            path_line("dataOperationCurrent", locale, source),
            message("migrateDataTargetExists", locale).replace("{path}", encoded_target),
            message("dataOperationRetry", locale).to_owned(),
        ] {
            assert!(text.lines().any(|actual| actual == line));
        }
        assert!(!text.contains("tools.migrateDataTargetExists"));
        assert_eq!(language(Some(locale)), locale);
    }
}

#[test]
fn verified_phase_uses_recorded_pointer_to_choose_next_action() {
    let mut record = legacy_failure();
    let source = Path::new("before/OC2ModManager");
    let target = Path::new("after/OC2ModManager");
    record.failed_from = Some(ResetPhase::Verified);
    record.action = DataAction::Migrate {
        source: source.to_owned(),
        target: target.to_owned(),
    };
    assert!(
        format_failure(&record, Some(source), "en").ends_with(message("dataOperationRetry", "en"))
    );
    assert!(
        format_failure(&record, Some(target), "en")
            .ends_with(message("dataOperationSwitched", "en"))
    );
    assert!(format_failure(&record, None, "en").ends_with(message("dataOperationPreserve", "en")));
}

#[test]
fn persisted_migration_diagnostics_correlate_the_pointer_without_error_payload() {
    let mut record = legacy_failure();
    let source = Path::new("before/OC2ModManager");
    let target = Path::new("after/OC2ModManager");
    record.action = DataAction::Migrate {
        source: source.to_owned(),
        target: target.to_owned(),
    };
    record.failed_from = Some(ResetPhase::Cleaning);
    record.error = Some(crate::locale_tag::with_params(
        "tools.migrateDataFailed",
        &[("reason", "fixture-secret (os error 32)")],
    ));
    let failure = diagnostics(&record, target);
    assert!(failure.contains(&record.token.to_string()));
    assert!(failure.contains("状态=Failed，失败前阶段=Cleaning"));
    assert!(failure.contains(&format!("当前数据根={:?}", display_path(target))));
    assert!(failure.contains("原因=tools.migrateDataFailed"));
    assert!(failure.contains("指向=目标"));
    assert!(failure.contains("OS码=Some(32)"));
    assert!(!failure.contains("fixture-secret"));

    record.phase = ResetPhase::Completed;
    record.failed_from = None;
    record.error = None;
    let completed = diagnostics(&record, target);
    assert!(completed.contains("操作已完成"));
    assert!(completed.contains(&format!("目标数据根=Some({:?})", display_path(target))));
}

#[test]
fn simultaneous_notifiers_only_claim_one_and_persist_reported() {
    let temp = tempfile::tempdir().unwrap();
    let paths = ResetPaths::new(&temp.path().join("OC2ModManager")).unwrap();
    let record = legacy_failure();
    paths.write_record(&record).unwrap();
    let barrier = Barrier::new(2);
    let token = record.token;
    let claims = std::thread::scope(|workers| {
        let claim = || {
            barrier.wait();
            paths.claim_failure_notice(token).unwrap()
        };
        let helper = workers.spawn(claim);
        let startup = workers.spawn(claim);
        [helper.join().unwrap(), startup.join().unwrap()]
    });
    assert_eq!(claims.into_iter().flatten().count(), 1);
    assert!(paths.record().unwrap().unwrap().reported);
    assert!(paths.claim_failure_notice(token).unwrap().is_none());
}

#[test]
fn stale_token_cannot_claim_or_replace_new_failure() {
    let temp = tempfile::tempdir().unwrap();
    let paths = ResetPaths::new(&temp.path().join("OC2ModManager")).unwrap();
    let previous = legacy_failure();
    let current = legacy_failure();
    paths.write_record(&previous).unwrap();
    paths.write_record(&current).unwrap();
    assert!(
        paths
            .claim_failure_notice(previous.token)
            .unwrap()
            .is_none()
    );
    assert_eq!(paths.record().unwrap().unwrap().token, current.token);
    assert!(paths.claim_failure_notice(current.token).unwrap().is_some());
}

#[test]
fn system_codes_map_to_supported_translations() {
    for (code, expected) in [
        ("zh-Hant", Some("zh-TW")),
        ("zh-HK", Some("zh-TW")),
        ("zh_CN.UTF-8", Some("zh-CN")),
        ("en-GB", Some("en")),
        ("ko-KR", Some("ko")),
        ("ja_JP", Some("ja")),
        ("de-DE", None),
    ] {
        assert_eq!(system_code(code), expected);
    }
}
