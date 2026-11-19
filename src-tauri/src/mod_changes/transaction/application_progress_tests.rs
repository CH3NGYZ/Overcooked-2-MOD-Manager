use super::{durable, tests::Workspace, types::JournalState};
use crate::mod_changes::application_progress::{ItemKey, Recorder};
use serde_json::{Value, json};
use std::cell::Cell;

fn event(node_id: &str, member_key: Option<&str>, stage: &str) -> Value {
    json!({ "plan_id": "fixture", "items": [{ "node_id": node_id, "member_key": member_key, "stage": stage }] })
}

#[test]
fn progress_uses_exact_member_and_waits_for_all_records_and_durable_commit() {
    let w = Workspace::new();
    w.write("one.dll", b"old #1");
    w.write("two.dll", b"old #2");
    w.write("extra.dll", b"old extra");
    let pending = w.prepare(vec![
        w.file("one.dll", Some(b"new #1")),
        w.file("extra.dll", Some(b"new extra")),
        w.file("two.dll", Some(b"new #2")),
    ]);
    let journal = w.journal_root.join(pending.id());
    let package = ItemKey::new("package", None);
    let extra = ItemKey::new("package", Some("extra.dll"));
    let mut updates = vec![];
    let mut progress = Recorder::new(
        "fixture",
        [
            ItemKey::new("package", None),
            ItemKey::new("package", Some("extra.dll")),
            ItemKey::new("package", Some("unrelated.dll")),
        ],
        vec![[package.clone()].into(), [extra].into(), [package].into()],
        |update| {
            let update = serde_json::to_value(update).unwrap();
            if update == event("package", None, "processed") {
                assert_eq!(w.read("one.dll"), b"new #1");
                assert_eq!(w.read("two.dll"), b"new #2");
                assert_eq!(
                    durable::read_journal(&journal).unwrap().state,
                    JournalState::Applying
                );
            }
            if update["items"][0]["stage"] == "applied" {
                assert_eq!(
                    durable::read_journal(&journal).unwrap().state,
                    JournalState::Committed
                );
            }
            updates.push(update);
        },
    );
    pending
        .apply_with_cancel(|| false, |event| progress.observe(event))
        .unwrap();
    assert_eq!(
        updates,
        vec![
            event("package", None, "applying"),
            event("package", Some("extra.dll"), "applying"),
            event("package", Some("extra.dll"), "processed"),
            event("package", None, "processed"),
            json!({ "plan_id": "fixture", "items": [
            { "node_id": "package", "member_key": null, "stage": "applied" },
            { "node_id": "package", "member_key": "extra.dll", "stage": "applied" }
        ] }),
        ]
    );
    assert!(!journal.exists());
    assert_eq!(w.read("extra.dll"), b"new extra");
}

#[test]
fn later_write_failure_retracts_processed_items_and_leaves_unstarted_items_unmarked() {
    let w = Workspace::new();
    for file in ["first.dll", "failed.dll", "last.dll"] {
        w.write(file, b"old");
    }
    let pending = w.prepare(vec![
        w.file("first.dll", Some(b"new")),
        w.file("failed.dll", Some(b"new")),
        w.file("last.dll", Some(b"new")),
    ]);
    let journal = w.journal_root.join(pending.id());
    let mut updates = vec![];
    let fail_next = Cell::new(false);
    let mut progress = Recorder::new(
        "fixture",
        [
            ItemKey::new("first", None),
            ItemKey::new("failing", None),
            ItemKey::new("last", None),
        ],
        vec![
            [ItemKey::new("first", None)].into(),
            [ItemKey::new("failing", None)].into(),
            [ItemKey::new("last", None)].into(),
        ],
        |update| {
            let update = serde_json::to_value(update).unwrap();
            if update == event("first", None, "processed") {
                assert_eq!(w.read("first.dll"), b"new");
                fail_next.set(true);
            }
            updates.push(update);
        },
    );
    let error = pending
        .apply_with_cancel(
            || {
                if fail_next.replace(false) {
                    // Corrupt only the next private payload after the first live write.
                    std::fs::write(journal.join("after/1"), b"broken").unwrap();
                }
                false
            },
            |event| progress.observe(event),
        )
        .unwrap_err();
    assert_eq!(error, "recycle.payloadModified");
    assert_eq!(w.read("first.dll"), b"old");
    assert_eq!(w.read("failed.dll"), b"old");
    assert_eq!(w.read("last.dll"), b"old");
    progress.failed();
    assert_eq!(
        updates,
        vec![
            event("first", None, "applying"),
            event("first", None, "processed"),
            event("failing", None, "applying"),
            json!({ "plan_id": "fixture", "items": [
            { "node_id": "failing", "member_key": null, "stage": "failed" },
            { "node_id": "first", "member_key": null, "stage": "failed" }
        ] }),
        ]
    );
    assert!(!journal.exists());
}
