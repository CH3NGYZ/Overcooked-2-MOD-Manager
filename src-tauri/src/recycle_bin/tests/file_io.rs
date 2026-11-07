use crate::recycle_bin::errors::INCOMPLETE;
use crate::recycle_bin::file_io::copy_verified_and_remove;
use crate::recycle_bin::file_io::fingerprint;
use crate::recycle_bin::records::StagedFile;
use std::fs;

use super::Workspace;

#[test]
fn verified_copy_fallback_removes_source_only_after_integrity_check() {
    let workspace = Workspace::new();
    let source = workspace.directory.join("source.dll");
    let target = workspace.directory.join("target.dll");
    fs::write(&source, b"cross-volume-copy fixture").unwrap();
    let (size, sha256) = fingerprint(&source).unwrap();
    let file = StagedFile {
        relative: "fixture.dll".into(),
        size,
        sha256,
    };
    copy_verified_and_remove(&source, &target, &file).unwrap();
    assert!(!source.exists());
    assert_eq!(fs::read(&target).unwrap(), b"cross-volume-copy fixture");
    fs::write(&source, b"changed fixture").unwrap();
    let another = workspace.directory.join("failed-copy.dll");
    assert_eq!(
        copy_verified_and_remove(&source, &another, &file).unwrap_err(),
        INCOMPLETE
    );
    assert_eq!(fs::read(&source).unwrap(), b"changed fixture");
    assert!(!another.exists());
    assert!(copy_verified_and_remove(&source, &target, &file).is_err());
    assert_eq!(fs::read(target).unwrap(), b"cross-volume-copy fixture");
}
