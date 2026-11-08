use super::*;

#[test]
fn relocation_rejects_stale_or_wrong_installed_versions() {
    assert!(version_matches("1.1.0", "1.1.0.0"));
    assert!(version_matches("1.2.3-beta.1", "1.2.3.1001"));
    for actual in ["1.0.9.0", "1.1.1.0", "1.1.0", "garbage"] {
        assert!(!version_matches("1.1.0", actual));
    }
}

#[cfg(windows)]
#[test]
fn confirms_matching_relocated_process_and_rejects_a_launch_that_exits() {
    let directory = std::env::temp_dir().join(format!("oc2-launch-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&directory).unwrap();
    let fixture = directory.join("OC2LaunchFixture.exe");
    let cmd = PathBuf::from(std::env::var_os("WINDIR").unwrap()).join("System32/cmd.exe");
    fs::copy(cmd, &fixture).unwrap();
    let mut command = Command::new(&fixture);
    command.args(["/C", "ping", "-n", "20", "127.0.0.1", ">nul"]);
    hide_console(&mut command);
    let mut child = command.spawn().unwrap();
    let confirmed = wait_for_application(&fixture, Duration::from_secs(5));
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(confirmed);
    let mut command = Command::new(&fixture);
    command.args(["/C", "exit", "0"]);
    hide_console(&mut command);
    command.spawn().unwrap().wait().unwrap();
    assert!(!wait_for_application(&fixture, Duration::from_secs(3)));
    fs::remove_dir_all(directory).unwrap();
}

struct Workspace(PathBuf);
impl Workspace {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("oc2-migration-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("game/BepInEx/plugins/Manager")).unwrap();
        Self(root)
    }
    fn source(&self) -> PathBuf {
        self.0.join("game/BepInEx/plugins/Manager")
    }
    fn target(&self) -> PathBuf {
        self.0.join("default/OC2ModManager")
    }
    fn work(&self) -> PathBuf {
        self.0.join("work")
    }
    fn write(&self, path: &Path, bytes: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    fn prepare(&self) -> Migration {
        self.write(&self.source().join("OC2ModManager.exe"), b"old manager");
        Migration::prepare(&self.source(), &self.target(), &self.work()).unwrap()
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn redirects_only_plugins_installations_to_the_default_directory() {
    let workspace = Workspace::new();
    let normal = workspace.target().join("OC2ModManager.exe");
    assert_eq!(update_executable(&normal).unwrap(), normal);
    let unsafe_exe = workspace.source().join("OC2ModManager.exe");
    assert_eq!(
        update_executable(&unsafe_exe).unwrap(),
        dirs::data_local_dir()
            .unwrap()
            .join("OC2ModManager/OC2ModManager.exe")
    );
}

#[test]
fn fresh_install_keeps_legacy_data_and_removes_only_manager_binaries() {
    let workspace = Workspace::new();
    for name in [
        "config.toml",
        "config.previous.toml",
        "manifests/mod.json",
        "recycle-bin/item/data.dll",
        "backups/framework/plugins/mod.dll",
        "logs/rust.log",
        "cache/mod.zip",
    ] {
        workspace.write(&workspace.source().join(name), b"source data");
    }
    workspace.write(
        &workspace.source().join("uninstall.exe"),
        b"old uninstaller",
    );
    workspace.write(&workspace.source().join("AnotherMod.dll"), b"unrelated MOD");
    workspace.write(&workspace.target().join("config.toml"), b"previous config");
    workspace.write(&workspace.target().join("foreign.txt"), b"keep");
    let migration = workspace.prepare();
    assert_eq!(
        fs::read(workspace.source().join("OC2ModManager.exe")).unwrap(),
        b"old manager"
    );
    assert_eq!(
        fs::read(workspace.target().join("config.toml")).unwrap(),
        b"previous config"
    );
    assert!(!workspace.work().join("source/config.toml").exists());
    migration.cleanup().unwrap();
    assert!(!workspace.source().join("OC2ModManager.exe").exists());
    assert!(!workspace.source().join("uninstall.exe").exists());
    assert_eq!(
        fs::read(workspace.source().join("config.toml")).unwrap(),
        b"source data"
    );
    assert!(workspace.source().join("backups").is_dir());
    assert_eq!(
        fs::read(workspace.source().join("AnotherMod.dll")).unwrap(),
        b"unrelated MOD"
    );
    assert_eq!(
        fs::read(workspace.target().join("foreign.txt")).unwrap(),
        b"keep"
    );
}

#[test]
fn preserves_generic_directories_when_manager_is_in_plugins_itself() {
    let workspace = Workspace::new();
    let plugins = workspace.source().parent().unwrap().to_owned();
    workspace.write(&plugins.join("OC2ModManager.exe"), b"manager");
    workspace.write(&plugins.join("config.toml"), b"manager config");
    workspace.write(&plugins.join("logs/other-mod.log"), b"foreign log");
    workspace.write(&plugins.join("OtherMod.dll"), b"foreign MOD");
    let migration = Migration::prepare(&plugins, &workspace.target(), &workspace.work()).unwrap();
    migration.cleanup().unwrap();
    assert!(!plugins.join("OC2ModManager.exe").exists());
    assert!(plugins.is_dir());
    assert_eq!(
        fs::read(plugins.join("logs/other-mod.log")).unwrap(),
        b"foreign log"
    );
    assert_eq!(
        fs::read(plugins.join("OtherMod.dll")).unwrap(),
        b"foreign MOD"
    );
}

#[test]
fn relocation_does_not_read_or_replace_old_configuration() {
    let workspace = Workspace::new();
    workspace.write(&workspace.source().join("config.toml"), b"source config");
    workspace.write(
        &workspace.source().join("config.previous.toml"),
        b"source previous",
    );
    workspace.write(&workspace.target().join("config.toml"), b"target config");
    let migration = workspace.prepare();
    assert!(!migration.work.join("source/config.previous.toml").exists());
    assert_eq!(
        fs::read(workspace.target().join("config.toml")).unwrap(),
        b"target config"
    );
    assert!(!workspace.target().join("config.previous.toml").exists());
    assert_eq!(
        fs::read(workspace.source().join("config.toml")).unwrap(),
        b"source config"
    );
    assert!(workspace.source().join("OC2ModManager.exe").is_file());
}

#[test]
fn restores_binaries_removed_by_an_unsuccessful_installer() {
    let workspace = Workspace::new();
    workspace.write(
        &workspace.source().join("uninstall.exe"),
        b"old uninstaller",
    );
    let migration = workspace.prepare();
    fs::remove_file(workspace.source().join("OC2ModManager.exe")).unwrap();
    fs::remove_file(workspace.source().join("uninstall.exe")).unwrap();
    migration.restore_old_binaries().unwrap();
    assert_eq!(
        fs::read(workspace.source().join("OC2ModManager.exe")).unwrap(),
        b"old manager"
    );
    assert_eq!(
        fs::read(workspace.source().join("uninstall.exe")).unwrap(),
        b"old uninstaller"
    );
}

#[test]
fn cleanup_preserves_files_changed_since_the_snapshot_and_new_files() {
    let workspace = Workspace::new();
    workspace.write(&workspace.source().join("config.toml"), b"old config");
    let migration = workspace.prepare();
    workspace.write(&workspace.source().join("config.toml"), b"changed config");
    workspace.write(&workspace.source().join("new-mod.dll"), b"new MOD");
    migration.cleanup().unwrap();
    assert_eq!(
        fs::read(workspace.source().join("config.toml")).unwrap(),
        b"changed config"
    );
    assert_eq!(
        fs::read(workspace.source().join("new-mod.dll")).unwrap(),
        b"new MOD"
    );
}

#[test]
fn rejects_forged_records_and_unsafe_or_overlapping_destinations() {
    let workspace = Workspace::new();
    let mut migration = workspace.prepare();
    migration.source = workspace.target();
    assert!(migration.cleanup().is_err());
    assert!(
        Migration::prepare(
            &workspace.source(),
            &workspace.source().join("new"),
            &workspace.work()
        )
        .is_err()
    );
    assert!(workspace.source().join("OC2ModManager.exe").exists());
}

#[cfg(windows)]
#[test]
fn leaves_linked_legacy_data_untouched_without_traversing_it() {
    let workspace = Workspace::new();
    workspace.write(
        &workspace.source().join("OC2ModManager.exe"),
        b"old manager",
    );
    let link = workspace.source().join("backups");
    let junction = std::process::Command::new("cmd")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(link.to_string_lossy().replace('/', "\\"))
        .arg(workspace.source().to_string_lossy().replace('/', "\\"))
        .output()
        .unwrap();
    assert!(
        junction.status.success(),
        "{}",
        String::from_utf8_lossy(&junction.stderr)
    );
    let migration =
        Migration::prepare(&workspace.source(), &workspace.target(), &workspace.work()).unwrap();
    assert!(!migration.work.join("source/backups").exists());
    assert!(!workspace.target().exists());
    assert!(workspace.source().join("OC2ModManager.exe").is_file());
    migration.cleanup().unwrap();
    assert!(fs::symlink_metadata(&link).is_ok());
    fs::remove_dir(link).unwrap();
}
