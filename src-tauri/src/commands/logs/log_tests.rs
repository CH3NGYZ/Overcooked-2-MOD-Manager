use super::*;
use std::fs;

#[test]
fn successive_startups_create_fresh_logs_and_keep_five_per_kind() {
    let temp = tempfile::tempdir().unwrap();
    let mut previous = Vec::new();
    for launch in 0..7 {
        let mut history = History::start(temp.path(), MAX_RUST_BYTES).unwrap();
        for kind in LogKind::ALL {
            let entries = history.list(kind).unwrap();
            assert_eq!(entries.len(), usize::min(5, launch + 1));
            let newest = &entries[0];
            assert!(!previous.contains(&newest.name));
            previous.push(newest.name.clone());
            assert!(history.read(kind, &newest.id).unwrap().content.is_empty());
            history
                .append(kind, format!("launch {launch}\n").as_bytes())
                .unwrap();
            if launch > 0 {
                assert_eq!(
                    history.read(kind, &entries[1].id).unwrap().content,
                    format!("launch {}\n", launch - 1)
                );
            }
        }
    }
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 15);
    for first_file in &previous[..6] {
        assert!(!temp.path().join(first_file).exists());
    }
}

#[test]
fn rust_rollover_shares_retention_with_legacy_segments_and_preserves_existing_ids() {
    let temp = tempfile::tempdir().unwrap();
    let make = |name| fs::write(temp.path().join(name), name).unwrap();
    make("rust.log");
    make("rust_2026-01-05_02-03-04.log");
    make("rust_2026-01-04_02-03-04.log.bak");
    make("rust_2026-01-03_02-03-04.log");
    make("rust_2026-01-02_02-03-04.log");
    make("rust_unrelated.log");
    make("frontend.log");
    make("crash.log");
    let mut history = History::start(temp.path(), MAX_RUST_BYTES).unwrap();
    let rust = history.list(LogKind::Rust).unwrap();
    let frontend = history.list(LogKind::Frontend).unwrap();
    let crash = history.list(LogKind::Crash).unwrap();
    assert_eq!(rust.len(), 5);
    assert!(!temp.path().join("rust_2026-01-02_02-03-04.log").exists());
    assert!(
        rust.iter()
            .any(|entry| entry.name == "rust_2026-01-04_02-03-04.log.bak")
    );
    assert_eq!(frontend[1].name, "frontend.log");
    assert_eq!(crash[1].name, "crash.log");
    let previous = &rust[0];
    let full_segment = vec![b'A'; MAX_RUST_BYTES as usize];
    history.append(LogKind::Rust, &full_segment).unwrap();
    history.append(LogKind::Rust, b"new segment\n").unwrap();
    assert_eq!(
        fs::metadata(temp.path().join(&previous.name))
            .unwrap()
            .len(),
        MAX_RUST_BYTES
    );
    let rotated = history.list(LogKind::Rust).unwrap();
    assert_eq!(rotated.len(), 5);
    assert_eq!(rotated[1].id, previous.id);
    assert_eq!(
        history.read(LogKind::Rust, &previous.id).unwrap().content,
        "A".repeat(512 * 1024)
    );
    assert_eq!(
        history.read(LogKind::Rust, &rotated[0].id).unwrap().content,
        "new segment\n"
    );
    for _ in 0..5 {
        history.append(LogKind::Rust, &full_segment).unwrap();
    }
    assert!(!temp.path().join(&previous.name).exists());
    assert!(history.clear(LogKind::Rust, &previous.id).is_err());
    assert!(temp.path().join("rust_unrelated.log").exists());
    assert_eq!(history.list(LogKind::Frontend).unwrap().len(), 2);
    assert_eq!(history.list(LogKind::Crash).unwrap().len(), 2);
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 10);
}

#[test]
fn reopening_a_log_only_reads_the_tail_and_preserves_timestamps() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("frontend.log");
    let content = format!("{}final existing record\n", "old record\n".repeat(100_000));
    fs::write(&path, &content).unwrap();
    let before = fs::metadata(&path).unwrap().modified().unwrap();
    let mut history = History::start(temp.path(), MAX_RUST_BYTES).unwrap();
    let files = history.list(LogKind::Frontend).unwrap();
    let id = &files[1].id;
    let first = history.read(LogKind::Frontend, id).unwrap();
    let second = history.read(LogKind::Frontend, id).unwrap();
    assert_eq!(first.content.len(), 512 * 1024);
    assert!(first.content.ends_with("final existing record\n"));
    assert_eq!(first.content, second.content);
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), before);
    assert_eq!(fs::metadata(&path).unwrap().len(), content.len() as u64);
}

#[test]
fn clear_only_affects_the_selected_file_and_active_log_can_still_append() {
    let temp = tempfile::tempdir().unwrap();
    let archived = temp.path().join("rust.log");
    fs::write(&archived, "older file\n").unwrap();
    let mut history = History::start(temp.path(), 28).unwrap();
    history.append(LogKind::Rust, b"active record\n").unwrap();
    let files = history.list(LogKind::Rust).unwrap();
    let prior = history.clear(LogKind::Rust, &files[1].id).unwrap();
    assert_eq!(prior.path, crate::display_path::display_path(&archived));
    assert!(prior.content.is_empty());
    assert_eq!(fs::read(&archived).unwrap(), b"");
    assert_eq!(
        history.read(LogKind::Rust, &files[0].id).unwrap().content,
        "active record\n"
    );
    assert!(
        history
            .clear(LogKind::Rust, &files[0].id)
            .unwrap()
            .content
            .is_empty()
    );
    history
        .append(LogKind::Rust, b"active after clear\n")
        .unwrap();
    assert_eq!(history.list(LogKind::Rust).unwrap().len(), 2);
    assert_eq!(
        history.read(LogKind::Rust, &files[0].id).unwrap().content,
        "active after clear\n"
    );
}

#[test]
fn log_selection_refuses_a_path_cross_kind_hardlink_and_replacement() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("logs");
    fs::create_dir(&data).unwrap();
    let outside = temp.path().join("outside.log");
    fs::write(&outside, "must not be touched").unwrap();
    fs::hard_link(&outside, data.join("crash.log")).unwrap();
    let legacy = data.join("frontend.log");
    fs::write(&legacy, "legacy").unwrap();
    let mut history = History::start(&data, MAX_RUST_BYTES).unwrap();
    assert_eq!(history.list(LogKind::Crash).unwrap().len(), 1);
    let frontend = history.list(LogKind::Frontend).unwrap();
    let old_id = &frontend[1].id;
    assert!(LogKind::parse("anything").is_err());
    assert!(history.clear(LogKind::Rust, old_id).is_err());
    assert!(history.read(LogKind::Frontend, "../outside.log").is_err());
    assert!(history.clear(LogKind::Frontend, "frontend.log").is_err());
    assert!(
        history
            .selected_path(LogKind::Frontend, "../outside.log")
            .is_err()
    );
    fs::rename(&legacy, data.join("renamed.log")).unwrap();
    fs::write(&legacy, "replacement").unwrap();
    assert!(history.clear(LogKind::Frontend, old_id).is_err());
    let refreshed = history.list(LogKind::Frontend).unwrap();
    assert_ne!(old_id, &refreshed[1].id);
    assert_eq!(fs::read_to_string(&legacy).unwrap(), "replacement");
    assert_eq!(fs::read_to_string(outside).unwrap(), "must not be touched");
}

#[test]
fn reparse_log_directory_and_symlink_file_are_not_followed() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target");
    let directory_link = temp.path().join("directory-link");
    fs::create_dir(&target).unwrap();
    #[cfg(windows)]
    let create = std::os::windows::fs::symlink_dir(&target, &directory_link);
    #[cfg(unix)]
    let create = std::os::unix::fs::symlink(&target, &directory_link);
    if let Err(error) = create {
        // Windows may require elevation or Developer Mode to create a link.
        #[cfg(windows)]
        if error.raw_os_error() == Some(1314) {
            return;
        }
        panic!("Could not build log link fixture: {error}");
    }
    assert!(History::start(&directory_link, MAX_RUST_BYTES).is_err());
    let file = target.join("actual.txt");
    fs::write(&file, "sentinel").unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&file, target.join("rust.log")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&file, target.join("rust.log")).unwrap();
    let mut history = History::start(&target, MAX_RUST_BYTES).unwrap();
    assert_eq!(history.list(LogKind::Rust).unwrap().len(), 1);
    assert_eq!(fs::read_to_string(file).unwrap(), "sentinel");
}

#[test]
fn fallback_appends_to_the_panic_log_without_history() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("nested/oc2-modmanager-crash.log");
    append_fallback(&path, b"first\n").unwrap();
    append_fallback(&path, b"second\n").unwrap();
    assert_eq!(fs::read(path).unwrap(), b"first\nsecond\n");
}

#[test]
#[ignore = "Overwrites the system clipboard; run only in a disposable desktop session"]
fn native_clipboard_contains_the_selected_log_file_list() {
    let temp = tempfile::tempdir().unwrap();
    let mut history = History::start(temp.path(), MAX_RUST_BYTES).unwrap();
    let files = history.list(LogKind::Crash).unwrap();
    let selected = history.selected_path(LogKind::Crash, &files[0].id).unwrap();
    copy_to_clipboard(&selected).unwrap();
    let mut clipboard = arboard::Clipboard::new().unwrap();
    assert_eq!(
        clipboard.get().file_list().unwrap(),
        vec![temp.path().join(&files[0].name)]
    );
}
