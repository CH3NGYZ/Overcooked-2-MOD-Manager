use super::*;

#[test]
fn staged_installer_is_locked_against_write_and_rename() {
    let work = tempfile::tempdir().unwrap();
    let stage = work.path().join("stage");
    fs::create_dir(&stage).unwrap();
    let path = package_path(&stage, "exe").unwrap();
    write_new(&path, &mut &b"signed package"[..]).unwrap();
    assert!(write_new(&path, &mut &b"replacement"[..]).is_err());
    let digest = archive::sha256_reader(&b"signed package"[..]).unwrap();
    let (directory, file) = lock_verified(&stage, &path, &digest).unwrap();

    assert!(std::fs::write(&path, b"replacement").is_err());
    assert!(std::fs::rename(&path, stage.join("other.exe")).is_err());
    assert!(std::fs::rename(&stage, work.path().join("other")).is_err());

    drop((directory, file));
    std::fs::write(&path, b"replacement").unwrap();
    assert!(lock_verified(&stage, &path, &digest).is_err());
}

#[test]
fn validates_only_declared_attempt_tokens() {
    let id = Uuid::new_v4();
    assert_eq!(
        parse_attempt_id(Some(id.to_string().into()), None).unwrap(),
        id
    );
    assert!(parse_attempt_id(Some(id.to_string().into()), Some("extra".into())).is_err());
    assert!(parse_attempt_id(Some(Uuid::nil().to_string().into()), None).is_err());
    assert!(parse_attempt_id(Some("C:\\outside".into()), None).is_err());
}

#[test]
fn standalone_installers_cannot_overlap_and_do_not_reuse_existing_mutexes() {
    let name = format!(r"Global\OC2ModManager-installer-test-{}", Uuid::new_v4());
    let name = OsStr::new(&name);
    assert!(!InstallerSlot::visible(name));
    let first = InstallerSlot::open(name).unwrap();
    assert!(InstallerSlot::visible(name));
    assert!(matches!(InstallerSlot::open(name), Err(error) if error == "update.inProgress"));
    drop(first);
    assert!(!InstallerSlot::visible(name));
    assert!(InstallerSlot::open(name).is_ok());
}

#[test]
fn does_not_overwrite_a_renamed_manager() {
    let work = tempfile::tempdir().unwrap();
    let renamed = work.path().join("other.exe");
    assert!(
        matches!(validated_destination(&renamed, "exe"), Err(reason) if reason == "update.formatUnsupported")
    );
}
