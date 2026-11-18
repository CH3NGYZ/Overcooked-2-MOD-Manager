use super::*;

struct Fixture {
    _temporary: tempfile::TempDir,
    base: PathBuf,
    file: PathBuf,
    root: PathBuf,
    default: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        // macOS 的系统临时目录可能经由 /var 别名进入；fixture 本身用真实路径。
        let base = fs::canonicalize(temporary.path()).unwrap();
        let root = base.join("chosen").join(DATA_DIRECTORY);
        fs::create_dir_all(&root).unwrap();
        Self {
            file: base.join("bootstrap/data-location.json"),
            default: base.join("documents").join(DATA_DIRECTORY),
            _temporary: temporary,
            base,
            root,
        }
    }

    fn read(&self) -> Result<PathBuf, String> {
        read_location_at(&self.file, &self.default)
    }

    fn write_raw(&self, bytes: impl AsRef<[u8]>) {
        fs::create_dir_all(self.file.parent().unwrap()).unwrap();
        fs::write(&self.file, bytes).unwrap();
    }

    fn write_root(&self, root: &Path) {
        self.write_raw(
            serde_json::to_vec(&serde_json::json!({"schema_version": 1, "root": root})).unwrap(),
        );
    }
}

fn raw_descendant(base: &Path, suffix: &str) -> PathBuf {
    // Windows 的 verbatim PathBuf::join 会消去 ..，直接拼 OsString 保留被测输入。
    let mut value = base.as_os_str().to_os_string();
    value.push(std::path::MAIN_SEPARATOR_STR);
    value.push(suffix.replace('/', std::path::MAIN_SEPARATOR_STR));
    PathBuf::from(value)
}

#[test]
fn missing_pointer_returns_default_without_creating_any_directories() {
    let fixture = Fixture::new();
    assert_eq!(fixture.read().unwrap(), fixture.default);
    assert!(!fixture.file.parent().unwrap().exists());
    assert!(!fixture.default.exists());
}

#[test]
fn pointer_roundtrips_only_the_schema_and_root_and_replaces_atomically() {
    let fixture = Fixture::new();
    write_location_at(&fixture.file, &fixture.root).unwrap();
    assert_eq!(fixture.read().unwrap(), fixture.root);
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(&fixture.file).unwrap()).unwrap();
    assert_eq!(value.as_object().unwrap().len(), 2);
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["root"], fixture.root.to_string_lossy().as_ref());

    let replacement = fixture.base.join("replacement").join(DATA_DIRECTORY);
    fs::create_dir_all(&replacement).unwrap();
    write_location_at(&fixture.file, &replacement).unwrap();
    assert_eq!(fixture.read().unwrap(), replacement);
    assert_eq!(
        fs::read_dir(fixture.file.parent().unwrap())
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn malformed_or_extended_pointer_never_falls_back_to_default() {
    let fixture = Fixture::new();
    for bytes in [
        Vec::new(),
        b"not json".to_vec(),
        b"null".to_vec(),
        b"{}".to_vec(),
        b"{\"schema_version\":1}".to_vec(),
        serde_json::to_vec(&serde_json::json!({"schema_version": 2, "root": fixture.root}))
            .unwrap(),
        serde_json::to_vec(&serde_json::json!({
            "schema_version": 1, "root": fixture.root, "config": {}
        }))
        .unwrap(),
    ] {
        fixture.write_raw(&bytes);
        assert_eq!(fixture.read().unwrap_err(), ROOT_UNAVAILABLE, "{bytes:?}");
    }
}

#[test]
fn oversize_pointer_is_rejected_even_when_it_contains_valid_json() {
    let fixture = Fixture::new();
    let mut bytes = serde_json::to_vec(&serde_json::json!({
        "schema_version": 1, "root": fixture.root
    }))
    .unwrap();
    bytes.resize(MAX_LOCATION_BYTES as usize + 1, b' ');
    fixture.write_raw(bytes);
    assert_eq!(fixture.read().unwrap_err(), ROOT_UNAVAILABLE);
}

#[test]
fn invalid_roots_are_rejected_for_both_read_and_write() {
    let fixture = Fixture::new();
    let invalid = [
        PathBuf::new(),
        PathBuf::from(DATA_DIRECTORY),
        fixture.base.join("other-name"),
        raw_descendant(&fixture.base, "chosen/../chosen/OC2ModManager"),
        fixture.base.join("BepInEx").join(DATA_DIRECTORY),
    ];
    for root in invalid {
        fixture.write_root(&root);
        assert_eq!(fixture.read().unwrap_err(), ROOT_UNAVAILABLE, "{root:?}");
        assert_eq!(
            write_location_at(&fixture.file, &root).unwrap_err(),
            ROOT_UNAVAILABLE,
            "{root:?}"
        );
    }
}

#[test]
fn non_directory_root_and_parent_are_rejected() {
    let fixture = Fixture::new();
    fs::remove_dir(&fixture.root).unwrap();
    fs::write(&fixture.root, b"not a directory").unwrap();
    fixture.write_root(&fixture.root);
    assert_eq!(fixture.read().unwrap_err(), ROOT_UNAVAILABLE);
    assert_eq!(
        write_location_at(&fixture.file, &fixture.root).unwrap_err(),
        ROOT_UNAVAILABLE
    );

    let parent_file = fixture.base.join("parent-file");
    fs::write(&parent_file, b"not a directory").unwrap();
    fixture.write_root(&parent_file.join(DATA_DIRECTORY));
    assert_eq!(fixture.read().unwrap_err(), ROOT_UNAVAILABLE);
}

#[test]
fn cleared_root_is_recreatable_only_while_its_immediate_parent_remains_available() {
    let fixture = Fixture::new();
    write_location_at(&fixture.file, &fixture.root).unwrap();
    fs::remove_dir(&fixture.root).unwrap();
    assert_eq!(fixture.read().unwrap(), fixture.root);
    assert!(fixture.file.is_file(), "清除数据根不能清除固定定位文件");
    assert_eq!(
        write_location_at(&fixture.file, &fixture.root).unwrap_err(),
        ROOT_UNAVAILABLE,
        "只有迁移后的目标实际存在才允许发布定位文件"
    );
    fs::remove_dir(fixture.root.parent().unwrap()).unwrap();
    assert_eq!(fixture.read().unwrap_err(), ROOT_UNAVAILABLE);
}

#[test]
fn invalid_pointer_paths_do_not_replace_an_existing_location() {
    let fixture = Fixture::new();
    write_location_at(&fixture.file, &fixture.root).unwrap();
    let before = fs::read(&fixture.file).unwrap();
    for file in [
        PathBuf::from("relative.json"),
        raw_descendant(&fixture.base, "bootstrap/../bootstrap/data-location.json"),
        fixture.base.clone(),
    ] {
        assert_eq!(
            read_location_at(&file, &fixture.default).unwrap_err(),
            ROOT_UNAVAILABLE
        );
        assert_eq!(
            write_location_at(&file, &fixture.root).unwrap_err(),
            ROOT_UNAVAILABLE
        );
        assert_eq!(fs::read(&fixture.file).unwrap(), before);
    }
}

#[cfg(unix)]
fn directory_link(alias: &Path, target: &Path) {
    std::os::unix::fs::symlink(target, alias).unwrap();
}

#[cfg(windows)]
fn directory_link(alias: &Path, target: &Path) {
    let result = std::process::Command::new("cmd")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(alias)
        .arg(target)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn linked_root_parent_and_bootstrap_are_rejected_without_following_them() {
    let fixture = Fixture::new();
    let actual_parent = fixture.base.join("actual");
    fs::create_dir_all(actual_parent.join(DATA_DIRECTORY)).unwrap();
    let linked_parent = fixture.base.join("linked-parent");
    directory_link(&linked_parent, &actual_parent);
    let linked_root = fixture.base.join("linked-root").join(DATA_DIRECTORY);
    fs::create_dir_all(linked_root.parent().unwrap()).unwrap();
    directory_link(&linked_root, &fixture.root);
    for root in [linked_root, linked_parent.join(DATA_DIRECTORY)] {
        fixture.write_root(&root);
        assert_eq!(fixture.read().unwrap_err(), ROOT_UNAVAILABLE);
        assert_eq!(
            write_location_at(&fixture.file, &root).unwrap_err(),
            ROOT_UNAVAILABLE
        );
    }

    let bootstrap_alias = fixture.base.join("linked-bootstrap");
    directory_link(&bootstrap_alias, &actual_parent);
    let pointer = bootstrap_alias.join("data-location.json");
    assert_eq!(
        read_location_at(&pointer, &fixture.default).unwrap_err(),
        ROOT_UNAVAILABLE
    );
    assert_eq!(
        write_location_at(&pointer, &fixture.root).unwrap_err(),
        ROOT_UNAVAILABLE
    );
    assert!(!actual_parent.join("data-location.json").exists());
}

#[cfg(unix)]
#[test]
fn linked_pointer_file_is_rejected_without_changing_its_target() {
    let fixture = Fixture::new();
    let target = fixture.base.join("actual-location.json");
    fs::write(&target, b"untouched").unwrap();
    fs::create_dir_all(fixture.file.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&target, &fixture.file).unwrap();
    assert_eq!(fixture.read().unwrap_err(), ROOT_UNAVAILABLE);
    assert_eq!(
        write_location_at(&fixture.file, &fixture.root).unwrap_err(),
        ROOT_UNAVAILABLE
    );
    assert_eq!(fs::read(target).unwrap(), b"untouched");
}
