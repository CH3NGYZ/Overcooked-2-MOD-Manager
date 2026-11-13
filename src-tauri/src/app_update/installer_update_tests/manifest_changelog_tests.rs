use super::*;
use serde_json::{Value, json};

fn inspect(document: &Value, current: &str) -> Result<Option<AppUpdateInfo>, String> {
    inspect_installer_manifest(
        &direct(Provider::Github),
        document,
        target(),
        &semver::Version::parse(current).unwrap(),
    )
}

fn body(document: &Value, current: &str) -> Option<String> {
    inspect(document, current).unwrap().unwrap().body
}

#[test]
fn changelog_uses_numeric_order_open_current_closed_target_and_first_duplicate() {
    let mut document = fixture(Provider::Github);
    document["changelog"] = json!([
        {"version":"9.0.0", "notes":"stale target"},
        {"version":"2.10.0", "notes":"Ten"},
        {"version":"2.9.0", "notes":"Nine"},
        {"version":"2.10.0", "notes":"later duplicate"},
        {"version":"2.0.0", "notes":"current is excluded"},
        {"version":"1.9.9", "notes":"older is excluded"},
        {"version":"9.0.1", "notes":"future is excluded"}
    ]);
    let info = inspect(&document, "2.0.0").unwrap().unwrap();
    assert_eq!(
        info.body.as_deref(),
        Some("### v2.9.0\n\nNine\n\n### v2.10.0\n\nTen\n\n### v9.0.0\n\nRelease notes")
    );
    let original = inspect(&fixture(Provider::Github), "2.0.0")
        .unwrap()
        .unwrap();
    assert_eq!(info.version, original.version);
    assert_eq!(info.installer_url, original.installer_url);
    assert_eq!(info.installer_signature, original.installer_signature);
    assert_eq!(info.installer_name, original.installer_name);
    assert_eq!(info.installer_type, original.installer_type);
    assert_eq!(info.provider, original.provider);
    assert_eq!(info.source_channel, original.source_channel);
}

#[test]
fn changelog_adds_missing_target_and_preserves_the_authoritative_notes_verbatim() {
    let mut document = fixture(Provider::Github);
    document["notes"] = json!("\nTarget **body**  \n");
    document["changelog"] = json!([{"version":"2.1.0", "notes":"Earlier"}]);
    assert_eq!(
        body(&document, "2.0.0").as_deref(),
        Some("### v2.1.0\n\nEarlier\n\n### v9.0.0\n\n\nTarget **body**  \n")
    );
    document.as_object_mut().unwrap().remove("notes");
    assert_eq!(
        body(&document, "2.0.0").as_deref(),
        Some("### v2.1.0\n\nEarlier\n\n### v9.0.0\n\n")
    );
}

#[test]
fn missing_empty_and_non_intermediate_history_keep_original_notes_without_headings() {
    let original = "\nOriginal notes  \n";
    for history in [
        None,
        Some(Value::Null),
        Some(json!({"version":"2.1.0", "notes":"wrong container"})),
        Some(json!([])),
        Some(json!([{"version":"9.0.0", "notes":"not authoritative"}])),
        Some(json!([
            {"version":"1.0.0", "notes":"old"},
            {"version":"2.0.0", "notes":"current"},
            {"version":"9.0.1", "notes":"future"}
        ])),
    ] {
        let mut document = fixture(Provider::Github);
        document["notes"] = original.into();
        if let Some(history) = history {
            document["changelog"] = history;
        }
        assert_eq!(body(&document, "2.0.0").as_deref(), Some(original));
        document.as_object_mut().unwrap().remove("notes");
        assert!(body(&document, "2.0.0").is_none());
        document["notes"] = json!("");
        assert_eq!(body(&document, "2.0.0").as_deref(), Some(""));
    }
}

#[test]
fn any_malformed_history_entry_falls_back_without_invalidating_the_manifest() {
    let mut malformed = vec![
        Value::Null,
        json!(42),
        json!("not an entry"),
        json!({}),
        json!({"notes":"missing version"}),
        json!({"version":"2.2.0"}),
        json!({"version":2, "notes":"wrong type"}),
        json!({"version":"2.2.0", "notes":null}),
        json!({"version":"2.2.0", "notes":42}),
    ];
    for version in [
        "v2.2.0",
        "2.2",
        "2.02.0",
        "2.2.0-beta",
        "2.2.0+build",
        "2.2.0.1",
        " 2.2.0",
        "2.2.0 ",
        "18446744073709551616.0.0",
        "invalid",
    ] {
        malformed.push(json!({"version":version, "notes":"invalid version"}));
    }
    for entry in malformed {
        let mut document = fixture(Provider::Github);
        document["changelog"] = json!([
            {"version":"2.1.0", "notes":"otherwise valid intermediate"},
            entry
        ]);
        assert_eq!(body(&document, "2.0.0").as_deref(), Some("Release notes"));
    }
}

#[test]
fn changelog_bounds_include_the_target_and_accept_exactly_256_entries() {
    let mut entries = (0..255)
        .map(|patch| json!({"version":format!("2.0.{patch}"), "notes":"History"}))
        .collect::<Vec<_>>();
    entries.push(json!({"version":"9.0.0", "notes":"stale target"}));
    let mut document = fixture(Provider::Github);
    document["changelog"] = entries.clone().into();
    assert_eq!(
        body(&document, "1.0.0").unwrap().matches("### v").count(),
        256
    );
    entries.push(json!({"version":"2.1.0", "notes":"too many input entries"}));
    document["changelog"] = entries.into();
    assert_eq!(body(&document, "1.0.0").as_deref(), Some("Release notes"));
    document["changelog"] = (0..256)
        .map(|patch| json!({"version":format!("2.0.{patch}"), "notes":"History"}))
        .collect::<Vec<_>>()
        .into();
    assert_eq!(body(&document, "1.0.0").as_deref(), Some("Release notes"));
}

#[test]
fn note_limits_count_utf8_bytes_with_inclusive_single_and_total_boundaries() {
    const NOTE_LIMIT: usize = 128 * 1024;
    let notes = format!("{}ab", "界".repeat(NOTE_LIMIT / 3));
    assert_eq!(notes.len(), NOTE_LIMIT);
    let mut document = fixture(Provider::Github);
    document["notes"] = notes.clone().into();
    let mut entries = (0..5)
        .map(|patch| json!({"version":format!("2.0.{patch}"), "notes":notes}))
        .collect::<Vec<_>>();
    // Duplicate notes do not consume a second budget, and target history is
    // replaced by the selected manifest notes before counting the total.
    entries.push(json!({"version":"2.0.0", "notes":notes}));
    entries.push(json!({"version":"9.0.0", "notes":"stale"}));
    document["changelog"] = entries.clone().into();
    let aggregated = body(&document, "1.0.0").unwrap();
    assert_eq!(aggregated.matches("### v").count(), 6);
    assert_eq!(aggregated.matches('界').count(), (NOTE_LIMIT / 3) * 6);
    entries.push(json!({"version":"2.1.0", "notes":"x"}));
    document["changelog"] = entries.into();
    assert_eq!(body(&document, "1.0.0").as_deref(), Some(notes.as_str()));

    document["notes"] = "Release notes".into();
    document["changelog"] = json!([
        {"version":"2.1.0", "notes":"界".repeat(NOTE_LIMIT / 3 + 1)}
    ]);
    assert_eq!(body(&document, "1.0.0").as_deref(), Some("Release notes"));
    document["changelog"] = json!([{"version":"2.1.0", "notes":"valid"}]);
    let oversized_target = "x".repeat(NOTE_LIMIT + 1);
    document["notes"] = oversized_target.clone().into();
    assert_eq!(
        body(&document, "1.0.0").as_deref(),
        Some(oversized_target.as_str())
    );
}

#[test]
fn history_cannot_bypass_platform_url_signature_or_newer_version_checks() {
    for history in [
        json!([{"version":"2.1.0", "notes":"valid history"}]),
        json!([{"version":"v2.1.0", "notes":"invalid history"}]),
    ] {
        let mut valid = fixture(Provider::Github);
        valid["changelog"] = history;
        for current in ["9.0.0", "10.0.0"] {
            assert!(inspect(&valid, current).unwrap().is_none());
        }
        let key = target().manifest_key();
        let mut invalid_url = valid.clone();
        invalid_url["platforms"][&key]["url"] = json!(
            "https://github.com/untrusted/repo/releases/download/v9.0.0/OC2ModManager_9.0.0_x64-setup.exe"
        );
        assert_eq!(
            inspect(&invalid_url, "1.0.0").unwrap_err(),
            "update.manifestPlatformMismatch"
        );
        let mut invalid_signature = valid.clone();
        invalid_signature["platforms"][&key]["signature"] = " ".into();
        assert_eq!(
            inspect(&invalid_signature, "1.0.0").unwrap_err(),
            "update.manifestPlatformMismatch"
        );
        let mut missing_platform = valid.clone();
        missing_platform["platforms"]
            .as_object_mut()
            .unwrap()
            .remove(&key);
        assert_eq!(
            inspect(&missing_platform, "1.0.0").unwrap_err(),
            "update.noMatchingPackage"
        );
        valid["version"] = "9.0.0-beta".into();
        assert_eq!(
            inspect(&valid, "1.0.0").unwrap_err(),
            "update.manifestNotStable"
        );
    }
}
