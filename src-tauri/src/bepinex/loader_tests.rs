use super::*;

fn winhttp_state(root: &Path) -> WinhttpState {
    crate::bepinex::state(
        root,
        &GameConfig {
            path: String::new(),
            runtime: crate::models::GameRuntime::Wine,
        },
    )
}

#[test]
fn winhttp_state_machine() {
    let root = std::env::temp_dir().join(format!("oc2-winhttp-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    assert_eq!(winhttp_state(&root), WinhttpState::Missing);
    fs::write(root.join("winhttp.dll"), b"loader").unwrap();
    assert_eq!(winhttp_state(&root), WinhttpState::ModEnabled);
    fs::rename(root.join("winhttp.dll"), root.join("winhttp.dll.disabled")).unwrap();
    assert_eq!(winhttp_state(&root), WinhttpState::ModDisabled);
    fs::rename(root.join("winhttp.dll.disabled"), root.join("winhttp.dll")).unwrap();
    assert_eq!(winhttp_state(&root), WinhttpState::ModEnabled);
    assert_eq!(fs::read(root.join("winhttp.dll")).unwrap(), b"loader");
    fs::write(root.join("winhttp.dll.disabled"), b"disabled loader").unwrap();
    assert_eq!(winhttp_state(&root), WinhttpState::Conflict);
    fs::remove_file(root.join("winhttp.dll")).unwrap();
    fs::remove_file(root.join("winhttp.dll.disabled")).unwrap();
    assert_eq!(winhttp_state(&root), WinhttpState::Missing);
    fs::remove_dir(root).unwrap();
}

#[test]
fn winhttp_state_does_not_recognize_obsolete_marker() {
    let root = std::env::temp_dir().join(format!("oc2-winhttp-marker-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let obsolete = root.join("winhttp.dll.oc2mod.disabled");
    fs::write(&obsolete, b"obsolete loader").unwrap();
    assert_eq!(winhttp_state(&root), WinhttpState::Missing);
    fs::write(root.join("winhttp.dll"), b"loader").unwrap();
    assert_eq!(winhttp_state(&root), WinhttpState::ModEnabled);
    fs::remove_file(root.join("winhttp.dll")).unwrap();
    fs::remove_file(obsolete).unwrap();
    fs::remove_dir(root).unwrap();
}
