use super::*;
use std::fs;

struct Workspace(PathBuf);
impl Workspace {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("oc2-chef-bindings-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(p.join("game/mod")).unwrap();
        fs::create_dir_all(p.join("manager")).unwrap();
        Self(p)
    }
    fn game(&self) -> PathBuf {
        self.0.join("game")
    }
    fn root(&self) -> PathBuf {
        self.game().join("mod")
    }
    fn directory(&self) -> PathBuf {
        self.0.join("manager").join("diychef")
    }
    fn read(&self) -> Stored {
        read(&self.game(), &self.root(), "id", &self.directory()).unwrap()
    }
    fn save(
        &self,
        baseline: &Stored,
        bytes: &[u8],
        map: &BTreeMap<String, HatReference>,
        create: bool,
    ) -> Result<Stored, String> {
        write(
            &self.game(),
            &self.root(),
            "id",
            &self.directory(),
            &baseline.source.revision,
            bytes,
            map,
            create,
        )
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        let path = self.0.canonicalize().unwrap();
        let temp = std::env::temp_dir().canonicalize().unwrap();
        assert!(path.starts_with(&temp) && path != temp);
        fs::remove_dir_all(path).unwrap();
    }
}
fn hat(value: &str) -> HatReference {
    HatReference::Known {
        hat_id: value.into(),
    }
}
fn map(value: &str) -> BTreeMap<String, HatReference> {
    BTreeMap::from([("custom:disabled".into(), hat(value))])
}

#[test]
fn bindings_only_changes_survive_reopen_and_target_id_changes() {
    let w = Workspace::new();
    let bytes = b"Chef_Black_cat\n";
    let initial = w.save(&w.read(), bytes, &map("hat:None"), true).unwrap();
    let saved = w.save(&initial, bytes, &map("hat:"), false).unwrap();
    assert_ne!(saved.source.revision, initial.source.revision);
    assert_eq!(fs::read(w.root().join("prefer.txt")).unwrap(), bytes);
    let reopened = read(&w.game(), &w.root(), "dll.disabled", &w.directory()).unwrap();
    assert_eq!(reopened.hat_bindings, map("hat:"));
    assert_eq!(reopened.bindings_path, saved.bindings_path);
    assert!(
        reopened
            .bindings_path
            .starts_with(w.0.join("manager").canonicalize().unwrap())
    );
    assert!(!reopened.bindings_path.starts_with(w.game()));
}

#[test]
fn canonical_game_and_mod_roots_have_separate_binding_records() {
    let w = Workspace::new();
    let first = w
        .save(&w.read(), b"Chef_Black_cat\n", &map("hat:None"), true)
        .unwrap();
    let other = w.game().join("other");
    fs::create_dir(&other).unwrap();
    let same_game = read(&w.game(), &other, "id", &w.directory()).unwrap();
    let second_game = w.0.join("other-game");
    fs::create_dir(&second_game).unwrap();
    let foreign = read(&second_game, &second_game, "id", &w.directory()).unwrap();
    assert_ne!(first.bindings_path, same_game.bindings_path);
    assert_ne!(first.bindings_path, foreign.bindings_path);
    assert!(same_game.hat_bindings.is_empty() && foreign.hat_bindings.is_empty());
}

#[test]
fn external_binding_edit_is_a_revision_conflict_without_overwrite() {
    let w = Workspace::new();
    let saved = w
        .save(&w.read(), b"Chef_Black_cat\n", &map("hat:None"), true)
        .unwrap();
    let external = bindings::encode(&map("hat:")).unwrap();
    fs::write(&saved.bindings_path, &external).unwrap();
    assert_eq!(
        w.save(&saved, b"Chef_Beard\n", &map("hat:None"), false)
            .err()
            .unwrap(),
        "diychef.conflict"
    );
    assert_eq!(fs::read(&saved.bindings_path).unwrap(), external);
    assert_eq!(fs::read(&saved.path).unwrap(), b"Chef_Black_cat\n");
}

#[test]
fn failures_after_prefer_restore_existing_pair_and_remove_initialization() {
    for create in [false, true] {
        let w = Workspace::new();
        let before = if create {
            w.read()
        } else {
            w.save(&w.read(), b"Chef_Black_cat\n", &map("hat:None"), true)
                .unwrap()
        };
        let result = write_with_hook(
            &w.game(),
            &w.root(),
            "id",
            &w.directory(),
            &before.source.revision,
            b"Chef_Beard\n",
            &map("hat:"),
            create,
            || Err("diychef.saveFailed".into()),
        );
        assert_eq!(result.err().unwrap(), "diychef.saveFailed");
        let after = w.read();
        assert_eq!(after.source.revision, before.source.revision);
        assert_eq!(after.prefer_bytes, before.prefer_bytes);
        assert_eq!(after.bindings_bytes, before.bindings_bytes);
        assert!(fs::read_dir(w.directory()).unwrap().all(|item| {
            !item
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".oc2-")
        }));
    }
}

#[test]
fn second_commit_failure_rolls_prefer_back_without_claiming_success() {
    let w = Workspace::new();
    let before = w
        .save(&w.read(), b"Chef_Black_cat\n", &map("hat:None"), true)
        .unwrap();
    let result = write_with_hook(
        &w.game(),
        &w.root(),
        "id",
        &w.directory(),
        &before.source.revision,
        b"Chef_Beard\n",
        &map("hat:"),
        false,
        || {
            for entry in fs::read_dir(w.directory()).unwrap() {
                let path = entry.unwrap().path();
                if path.extension().is_some_and(|ext| ext == "tmp") {
                    fs::remove_file(path).unwrap();
                }
            }
            Ok(())
        },
    );
    assert_eq!(result.err().unwrap(), "diychef.saveFailed");
    assert_eq!(w.read().source.revision, before.source.revision);
}

#[test]
fn sidecar_external_write_between_commits_is_preserved_and_prefer_rolls_back() {
    let w = Workspace::new();
    let before = w
        .save(&w.read(), b"Chef_Black_cat\n", &map("hat:None"), true)
        .unwrap();
    let external = bindings::encode(&map("hat:external")).unwrap();
    let result = write_with_hook(
        &w.game(),
        &w.root(),
        "id",
        &w.directory(),
        &before.source.revision,
        b"Chef_Beard\n",
        &map("hat:"),
        false,
        || {
            fs::write(&before.bindings_path, &external).unwrap();
            Ok(())
        },
    );
    assert_eq!(result.err().unwrap(), "diychef.conflict");
    assert_eq!(fs::read(&before.bindings_path).unwrap(), external);
    assert_eq!(fs::read(&before.path).unwrap(), b"Chef_Black_cat\n");
}

#[test]
fn readback_failure_restores_pair_but_never_overwrites_new_external_prefer() {
    for external in [false, true] {
        let w = Workspace::new();
        let before = w
            .save(&w.read(), b"Chef_Black_cat\n", &map("hat:None"), true)
            .unwrap();
        let encoded = bindings::encode(&map("hat:")).unwrap();
        let result: Result<(), String> = transaction::replace_pair(
            (
                &before.path,
                b"Chef_Beard\n",
                before.prefer_bytes.as_deref(),
            ),
            (
                &before.bindings_path,
                &encoded,
                before.bindings_bytes.as_deref(),
            ),
            false,
            || Ok(()),
            || Ok(()),
            || {
                if external {
                    fs::write(&before.path, b"external newer bytes").unwrap();
                }
                Err("diychef.writtenReadFailed".into())
            },
        );
        assert_eq!(result.unwrap_err(), "diychef.writtenReadFailed");
        assert_eq!(
            fs::read(&before.bindings_path).unwrap(),
            before.bindings_bytes.unwrap()
        );
        if external {
            assert_eq!(fs::read(&before.path).unwrap(), b"external newer bytes");
            assert!(fs::read_dir(w.root()).unwrap().any(|item| {
                item.unwrap()
                    .path()
                    .extension()
                    .is_some_and(|ext| ext == "rollback")
            }));
        } else {
            assert_eq!(w.read().source.revision, before.source.revision);
        }
    }
}

#[test]
#[cfg_attr(windows, ignore = "requires Windows symbolic-link permission")]
fn internal_symlink_and_directory_redirection_cannot_share_target_bindings() {
    let w = Workspace::new();
    let first = w
        .save(&w.read(), b"Chef_Black_cat\n", &map("hat:None"), true)
        .unwrap();
    let foreign = w.directory().join("foreign.json");
    fs::rename(&first.bindings_path, &foreign).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&foreign, &first.bindings_path).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&foreign, &first.bindings_path).unwrap();
    assert_eq!(
        read(&w.game(), &w.root(), "id", &w.directory())
            .err()
            .unwrap(),
        "archive.pathEscape"
    );
    fs::remove_file(&first.bindings_path).unwrap();
    fs::remove_file(&foreign).unwrap();
    fs::remove_dir(w.directory()).unwrap();
    let other = w.0.join("manager").join("other");
    fs::create_dir(&other).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(&other, w.directory()).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&other, w.directory()).unwrap();
    assert_eq!(
        read(&w.game(), &w.root(), "id", &w.directory())
            .err()
            .unwrap(),
        "archive.pathEscape"
    );
    // Explicitly unlink before the UUID fixture cleanup.
    #[cfg(windows)]
    fs::remove_dir(w.directory()).unwrap();
    #[cfg(unix)]
    fs::remove_file(w.directory()).unwrap();
}

#[test]
#[cfg(windows)]
fn internal_directory_junction_is_rejected_without_following_it() {
    let w = Workspace::new();
    let other = w.0.join("manager").join("other");
    fs::create_dir(&other).unwrap();
    let output = std::process::Command::new("cmd.exe")
        .args(["/D", "/C", "mklink", "/J"])
        .arg(w.directory())
        .arg(&other)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "failed to create UUID test junction: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result = read(&w.game(), &w.root(), "id", &w.directory());
    fs::remove_dir(w.directory()).unwrap();
    assert_eq!(result.err().unwrap(), "archive.pathEscape");
    assert!(fs::read_dir(other).unwrap().next().is_none());
}
