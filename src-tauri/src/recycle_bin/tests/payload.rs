use super::Workspace;
use crate::file_check;
use crate::recycle_bin::errors::VERIFY_FILES;
use crate::recycle_bin::records::{RecordState, load_record, save_record};
use crate::recycle_bin::staging::stage_with_related_manifests;
use crate::recycle_bin::{
    restore, set_diychef_resources_imported, stage, verified_payload_records,
};
use std::fs;

const FILE: &str = "BepInEx/plugins/DIYChef.dll";

fn stage_legacy_chef(workspace: &Workspace) -> String {
    workspace.write(FILE, b"legacy chef dll");
    stage(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        "DIYChef legacy fixture".into(),
        None,
        None,
        Vec::new(),
        vec![FILE.into()],
        None,
        &mut || Ok(()),
    )
    .unwrap()
    .id
}

#[test]
fn verified_payload_records_require_canonical_game_root_match() {
    let workspace = Workspace::new();
    let id = stage_legacy_chef(&workspace);
    let records = verified_payload_records(&workspace.recycle, &workspace.game, |_| true).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].id, id);
    assert_eq!(
        records[0].game_path,
        fs::canonicalize(&workspace.game).unwrap()
    );
    assert_eq!(records[0].files.len(), 1);
    assert_eq!(records[0].files[0].relative, FILE);
    assert_eq!(
        fs::read(&records[0].files[0].path).unwrap(),
        b"legacy chef dll"
    );
    assert_eq!(
        verified_payload_records(
            &workspace.recycle,
            &fs::canonicalize(&workspace.game).unwrap(),
            |_| true
        )
        .unwrap()
        .len(),
        1
    );

    let other_game = workspace.directory.join("other-game");
    fs::create_dir_all(&other_game).unwrap();
    assert!(
        verified_payload_records(&workspace.recycle, &other_game, |_| true)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn changed_payload_fingerprint_is_not_exposed() {
    let workspace = Workspace::new();
    let id = stage_legacy_chef(&workspace);
    for changed in [b"changed".as_slice(), b"LEGACY CHEF DLL".as_slice()] {
        fs::write(
            workspace.recycle.join(&id).join("files").join(FILE),
            changed,
        )
        .unwrap();

        assert!(
            verified_payload_records(&workspace.recycle, &workspace.game, |_| true)
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn restore_logs_expected_and_actual_fingerprints_without_logging_payload_bytes() {
    let workspace = Workspace::new();
    let id = stage_legacy_chef(&workspace);
    let record_dir = workspace.recycle.join(&id);
    let staged = record_dir.join("files").join(FILE);
    let record = load_record(&record_dir, &id).unwrap();
    let replacement = b"private test fixture contents";
    fs::write(&staged, replacement).unwrap();
    let (actual_size, actual_hash) =
        crate::recycle_bin::file_io::fingerprint(&staged, None).unwrap();
    let captured = crate::http_diagnostics::capture_test_logs("app_lib::recycle_bin::file_io", &id);

    assert_eq!(
        restore(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            &id
        )
        .unwrap_err(),
        file_check::report(VERIFY_FILES, &staged, file_check::CONTENT)
    );
    let messages = captured.messages();
    assert_eq!(messages.len(), 1);
    let message = &messages[0];
    assert!(message.contains(VERIFY_FILES));
    assert!(message.contains("DIYChef.dll"));
    assert!(message.contains(&format!("预期字节数={}", record.files[0].size)));
    assert!(message.contains(&format!("实际字节数={actual_size}")));
    assert!(message.contains(&record.files[0].sha256));
    assert!(message.contains(&actual_hash));
    assert!(!message.contains(std::str::from_utf8(replacement).unwrap()));
    assert_eq!(fs::read(staged).unwrap(), replacement);
    assert!(!workspace.game.join(FILE).exists());
}

#[test]
fn missing_or_undeclared_payload_files_are_not_exposed() {
    let workspace = Workspace::new();
    let id = stage_legacy_chef(&workspace);
    let payload = workspace.recycle.join(&id).join("files");
    fs::remove_file(payload.join(FILE)).unwrap();
    assert!(
        verified_payload_records(&workspace.recycle, &workspace.game, |_| true)
            .unwrap()
            .is_empty()
    );

    fs::write(payload.join(FILE), b"legacy chef dll").unwrap();
    fs::write(payload.join("unrecorded.obj"), b"undeclared resource").unwrap();
    assert!(
        verified_payload_records(&workspace.recycle, &workspace.game, |_| true)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        fs::read(payload.join("unrecorded.obj")).unwrap(),
        b"undeclared resource"
    );
}

#[test]
fn missing_or_wrong_type_payload_reports_the_exact_staged_file() {
    let workspace = Workspace::new();
    let id = stage_legacy_chef(&workspace);
    let staged = workspace.recycle.join(&id).join("files").join(FILE);
    fs::remove_file(&staged).unwrap();
    for reason in [file_check::MISSING, file_check::TYPE] {
        assert_eq!(
            restore(
                &workspace.recycle,
                &workspace.manifests,
                &workspace.game,
                &id
            )
            .unwrap_err(),
            file_check::report(VERIFY_FILES, &staged, reason)
        );
        assert!(!workspace.game.join(FILE).exists());
        assert_eq!(
            load_record(&workspace.recycle.join(&id), &id)
                .unwrap()
                .state,
            RecordState::Ready
        );
        if reason == file_check::MISSING {
            fs::create_dir(&staged).unwrap();
        }
    }
}

#[test]
fn case_only_extra_payload_file_is_rejected_on_case_sensitive_volumes() {
    let workspace = Workspace::new();
    let id = stage_legacy_chef(&workspace);
    let payload = workspace.recycle.join(&id).join("files");
    let alias = payload.join("BepInEx/plugins/diychef.dll");
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&alias)
    {
        Ok(file) => drop(file),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            eprintln!("case-sensitive payload assertion skipped: fixture volume ignores case");
            return;
        }
        Err(error) => panic!("cannot create case-only payload fixture: {error}"),
    }
    assert!(
        verified_payload_records(&workspace.recycle, &workspace.game, |_| true)
            .unwrap()
            .is_empty()
    );
    assert!(
        restore(
            &workspace.recycle,
            &workspace.manifests,
            &workspace.game,
            &id
        )
        .is_err()
    );
    assert!(alias.exists());
    assert!(payload.join(FILE).exists());
}

#[test]
fn non_ready_records_are_not_exposed_or_marked() {
    let workspace = Workspace::new();
    let id = stage_legacy_chef(&workspace);
    let directory = workspace.recycle.join(&id);
    let mut record = load_record(&directory, &id).unwrap();
    for state in [RecordState::Preparing, RecordState::Restored] {
        record.state = state;
        save_record(&directory, &record).unwrap();
        assert!(
            verified_payload_records(&workspace.recycle, &workspace.game, |_| true)
                .unwrap()
                .is_empty()
        );
        assert!(set_diychef_resources_imported(&workspace.recycle, &id, true).is_err());
        assert_eq!(load_record(&directory, &id).unwrap().state, state);
    }
}

#[test]
fn invalid_records_do_not_hide_valid_siblings() {
    let workspace = Workspace::new();
    let invalid_id = stage_legacy_chef(&workspace);
    fs::write(
        workspace.recycle.join(&invalid_id).join("record.json"),
        b"invalid json",
    )
    .unwrap();
    let valid_id = stage_legacy_chef(&workspace);

    let records = verified_payload_records(&workspace.recycle, &workspace.game, |_| true).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].id, valid_id);
}

#[test]
fn relative_path_filter_selects_candidates_before_complete_payload_verification() {
    let workspace = Workspace::new();
    let id = stage_legacy_chef(&workspace);
    assert!(
        verified_payload_records(&workspace.recycle, &workspace.game, |relative| {
            relative.ends_with("Unrelated.dll")
        })
        .unwrap()
        .is_empty()
    );
    assert_eq!(
        verified_payload_records(&workspace.recycle, &workspace.game, |relative| relative
            == FILE)
        .unwrap()
        .len(),
        1
    );

    // A matching DLL does not exempt the rest of its journal from verification.
    fs::write(
        workspace.recycle.join(&id).join("files/unrecorded.obj"),
        b"unrecorded resource",
    )
    .unwrap();
    assert!(
        verified_payload_records(&workspace.recycle, &workspace.game, |relative| relative
            == FILE)
        .unwrap()
        .is_empty()
    );
}

#[test]
fn records_are_sorted_by_newest_instant_then_id() {
    let workspace = Workspace::new();
    let mut ids = (0..3)
        .map(|_| stage_legacy_chef(&workspace))
        .collect::<Vec<_>>();
    ids.sort();
    for (id, deleted_at) in [
        (&ids[0], "2026-11-15T01:00:00Z"),
        (&ids[1], "2026-11-15T09:00:00+08:00"),
        (&ids[2], "2026-11-15T02:00:00Z"),
    ] {
        let directory = workspace.recycle.join(id);
        let mut record = load_record(&directory, id).unwrap();
        record.deleted_at = deleted_at.into();
        save_record(&directory, &record).unwrap();
    }

    let records = verified_payload_records(&workspace.recycle, &workspace.game, |_| true).unwrap();
    assert_eq!(
        records.iter().map(|record| &record.id).collect::<Vec<_>>(),
        vec![&ids[2], &ids[0], &ids[1]]
    );
    assert_eq!(records[0].deleted_at, "2026-11-15T02:00:00Z");
}

#[test]
fn initial_imported_marker_is_part_of_preparing_and_ready_journal() {
    let workspace = Workspace::new();
    workspace.write(FILE, b"legacy chef dll");
    let entry = stage_with_related_manifests(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        "already imported".into(),
        None,
        None,
        vec![],
        vec![FILE.into()],
        None,
        vec![],
        true,
        &mut || {
            let entry = fs::read_dir(&workspace.recycle)
                .unwrap()
                .next()
                .unwrap()
                .unwrap();
            let id = entry.file_name().to_str().unwrap().to_owned();
            let record = load_record(&entry.path(), &id).unwrap();
            assert_eq!(record.state, RecordState::Preparing);
            assert!(record.diychef_resources_imported);
            Ok(())
        },
    )
    .unwrap();
    let record = load_record(&workspace.recycle.join(&entry.id), &entry.id).unwrap();
    assert_eq!(record.state, RecordState::Ready);
    assert!(record.diychef_resources_imported);
    assert!(
        verified_payload_records(&workspace.recycle, &workspace.game, |_| true)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn directory_recycle_preserves_initial_imported_marker() {
    let workspace = Workspace::new();
    workspace.write(FILE, b"legacy chef dll");
    let entry = crate::recycle_bin::recycle_directories(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        &["BepInEx/plugins".into()],
        "already imported directory".into(),
        true,
    )
    .unwrap();
    let record = load_record(&workspace.recycle.join(&entry.id), &entry.id).unwrap();
    assert_eq!(record.state, RecordState::Ready);
    assert!(record.diychef_resources_imported);
}

#[test]
fn imported_marker_hides_record_can_be_cleared_and_does_not_change_restore() {
    let workspace = Workspace::new();
    let id = stage_legacy_chef(&workspace);

    set_diychef_resources_imported(&workspace.recycle, &id, true).unwrap();
    assert!(
        verified_payload_records(&workspace.recycle, &workspace.game, |_| true)
            .unwrap()
            .is_empty()
    );

    set_diychef_resources_imported(&workspace.recycle, &id, false).unwrap();
    assert_eq!(
        verified_payload_records(&workspace.recycle, &workspace.game, |_| true)
            .unwrap()
            .len(),
        1
    );

    set_diychef_resources_imported(&workspace.recycle, &id, true).unwrap();
    restore(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        &id,
    )
    .unwrap();
    assert_eq!(
        fs::read(workspace.game.join(FILE)).unwrap(),
        b"legacy chef dll"
    );
}

#[test]
fn old_record_without_import_marker_remains_readable() {
    let workspace = Workspace::new();
    let id = stage_legacy_chef(&workspace);
    let record_path = workspace.recycle.join(&id).join("record.json");
    let mut record: serde_json::Value =
        serde_json::from_slice(&fs::read(&record_path).unwrap()).unwrap();
    record
        .as_object_mut()
        .unwrap()
        .remove("diychef_resources_imported");
    fs::write(&record_path, serde_json::to_vec(&record).unwrap()).unwrap();

    let records = verified_payload_records(&workspace.recycle, &workspace.game, |_| true).unwrap();
    assert_eq!(records.len(), 1);
    assert!(!records[0].diychef_resources_imported);
}
