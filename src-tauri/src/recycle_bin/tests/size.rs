use crate::recycle_bin::records::{RecordState, RecycleRecord, StagedFile, list};
use crate::recycle_bin::selection::recycle_package;

use super::{A, DATA, Workspace};

fn record_with_sizes(sizes: &[u64]) -> RecycleRecord {
    RecycleRecord {
        id: "00000000-0000-0000-0000-000000000001".into(),
        name: "fixture".into(),
        name_i18n: Default::default(),
        deleted_at: "2026-11-16T00:00:00Z".into(),
        game_path: "/fixture/game".into(),
        package_id: None,
        release_tag: None,
        removed_manifest_files: Vec::new(),
        related_manifests: Vec::new(),
        files: sizes
            .iter()
            .enumerate()
            .map(|(index, size)| StagedFile {
                relative: format!("file-{index}.bin"),
                size: *size,
                sha256: "0".repeat(64),
            })
            .collect(),
        state: RecordState::Ready,
        diychef_resources_imported: false,
    }
}

#[test]
fn staged_entry_reports_sum_of_recorded_file_sizes() {
    let workspace = Workspace::new();
    workspace.write(A, b"12345");
    workspace.write(DATA, b"12345678");
    workspace.manifest("package", &[A, DATA]);

    let entry = recycle_package(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        "package",
        None,
    )
    .unwrap();

    assert_eq!(entry.file_count, 2);
    assert_eq!(entry.size_bytes, 13);
    let listed = list(&workspace.recycle).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].size_bytes, 13);
}

#[test]
fn empty_record_reports_zero_size() {
    assert_eq!(record_with_sizes(&[]).display().size_bytes, 0);
}

#[test]
fn size_sum_saturates_at_u64_max() {
    assert_eq!(
        record_with_sizes(&[u64::MAX - 1, 1, 1])
            .display()
            .size_bytes,
        u64::MAX
    );
}

#[test]
fn legacy_entry_without_size_deserializes_as_zero() {
    let entry: crate::models::ModRecycleEntry = serde_json::from_value(serde_json::json!({
        "id": "00000000-0000-0000-0000-000000000001",
        "name": "fixture",
        "deleted_at": "2026-11-16T00:00:00Z",
        "file_count": 0,
        "files": [],
        "game_path": "/fixture/game",
        "package_id": null,
        "release_tag": null
    }))
    .unwrap();

    assert_eq!(entry.size_bytes, 0);
}
