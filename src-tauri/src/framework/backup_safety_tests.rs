use super::*;

struct Workspace(PathBuf);

impl Workspace {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("oc2-backup-safety-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("game/BepInEx/plugins")).unwrap();
        fs::write(root.join("game/BepInEx/plugins/mod.dll"), b"original").unwrap();
        Self(root)
    }

    fn game(&self) -> PathBuf {
        self.0.join("game")
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn rejects_nested_and_same_directory_before_creating_a_backup() {
    let workspace = Workspace::new();
    let framework = workspace.game().join("BepInEx");
    let backup_root = framework.join("plugins/OC2ModManager/backups");
    assert!(backup_bepinex_dir(&workspace.game(), &backup_root).is_err());
    assert!(!backup_root.exists());
    assert!(
        copy_dir(&FrameworkStep::new(
            "test/copy_tree",
            &framework,
            &framework
        ))
        .is_err()
    );
    assert!(
        copy_dir(&FrameworkStep::new(
            "test/copy_tree",
            &framework,
            &workspace.game()
        ))
        .is_err()
    );
    assert_eq!(
        fs::read(framework.join("plugins/mod.dll")).unwrap(),
        b"original"
    );
}

#[test]
fn permits_sibling_destination_and_preserves_source_files() {
    let workspace = Workspace::new();
    let backup_root = workspace.game().join("BepInEx-backups");
    let backup = backup_bepinex_dir(&workspace.game(), &backup_root).unwrap();
    assert_eq!(
        fs::read(backup.join("plugins/mod.dll")).unwrap(),
        b"original"
    );
    assert_eq!(
        fs::read(workspace.game().join("BepInEx/plugins/mod.dll")).unwrap(),
        b"original"
    );
}

#[test]
fn rejects_nested_restore_before_removing_the_existing_framework() {
    let workspace = Workspace::new();
    let nested_backup = workspace.game().join("BepInEx/plugins/Manager/backups/old");
    fs::create_dir_all(&nested_backup).unwrap();
    fs::write(nested_backup.join("snapshot.txt"), b"backup").unwrap();
    assert!(restore_backup_dir(&nested_backup, &workspace.game()).is_err());
    assert_eq!(
        fs::read(workspace.game().join("BepInEx/plugins/mod.dll")).unwrap(),
        b"original"
    );
    assert_eq!(
        fs::read(nested_backup.join("snapshot.txt")).unwrap(),
        b"backup"
    );
}

#[test]
fn framework_failures_report_io_and_restore_staging_paths() {
    let workspace = Workspace::new();
    let game = workspace.game();
    let capture = crate::http_diagnostics::capture_test_logs(
        "app_lib::framework",
        workspace.0.file_name().unwrap().to_str().unwrap(),
    );
    let blocked_root = workspace.0.join("occupied-backup-root");
    fs::write(&blocked_root, b"do not overwrite").unwrap();
    assert!(backup_bepinex_dir(&game, &blocked_root).is_err());
    assert_eq!(fs::read(&blocked_root).unwrap(), b"do not overwrite");

    let backup = backup_bepinex_dir(&game, &workspace.0.join("snapshots")).unwrap();
    let framework = game.join("BepInEx");
    fs::remove_dir_all(&framework).unwrap();
    fs::write(&framework, b"unexpected file").unwrap();
    assert!(restore_backup_dir(&backup, &game).is_err());
    assert_eq!(fs::read(&framework).unwrap(), b"unexpected file");

    let messages = capture.messages();
    assert!(
        messages.iter().any(|line| {
            line.contains("stage=backup_directory/create_root")
                && line.contains("occupied-backup-root")
                && line.contains("os_code=Some(")
        }),
        "{messages:?}"
    );
    assert!(
        messages.iter().any(|line| {
            line.contains("stage=restore_directory/stage_existing_tree")
                && line.contains("reason=source_not_directory")
                && line.contains("staged=Some(")
                && line.contains("BepInEx.before-restore-")
        }),
        "{messages:?}"
    );
}

#[cfg(windows)]
#[test]
fn rejects_a_junction_into_the_source_before_creating_missing_directories() {
    let workspace = Workspace::new();
    let alias = workspace.0.join("backup-alias");
    let junction = std::process::Command::new("cmd")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(&alias)
        .arg(
            workspace
                .game()
                .join("BepInEx/plugins")
                .to_string_lossy()
                .replace('/', "\\"),
        )
        .output()
        .unwrap();
    assert!(
        junction.status.success(),
        "{}",
        String::from_utf8_lossy(&junction.stderr)
    );
    let destination = alias.join("new/backups");
    assert!(backup_bepinex_dir(&workspace.game(), &destination).is_err());
    assert!(!destination.exists());
    fs::remove_dir(alias).unwrap();
}
