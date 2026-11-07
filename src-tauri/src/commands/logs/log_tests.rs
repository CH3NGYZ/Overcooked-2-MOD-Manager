use super::*;
use uuid::Uuid;

#[test]
fn reopening_a_log_preserves_content_and_timestamps() {
    let path = std::env::temp_dir().join(format!("oc2-log-read-{}.log", Uuid::new_v4()));
    let content = "[2026-10-07 01:02:03 UTC][info] installed\n";
    fs::write(&path, content).unwrap();
    let before = fs::metadata(&path).unwrap().modified().unwrap();
    let first = read_log_at_path(&path).unwrap();
    let second = read_log_at_path(&path).unwrap();
    assert_eq!(first.content, content);
    assert_eq!(first.content, second.content);
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), before);
    fs::remove_file(path).unwrap();
}

#[test]
fn large_log_reads_are_bounded_to_the_existing_tail() {
    let path = std::env::temp_dir().join(format!("oc2-log-tail-{}.log", Uuid::new_v4()));
    let content = format!("{}final existing record\n", "old record\n".repeat(100_000));
    fs::write(&path, content.as_bytes()).unwrap();
    let log = read_log_at_path(&path).unwrap();
    assert_eq!(log.content.len(), 512 * 1024);
    assert!(log.content.ends_with("final existing record\n"));
    assert_eq!(fs::metadata(&path).unwrap().len(), content.len() as u64);
    fs::remove_file(path).unwrap();
}
