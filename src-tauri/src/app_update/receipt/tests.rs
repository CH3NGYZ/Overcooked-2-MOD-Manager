use super::*;

struct Fixture {
    root: tempfile::TempDir,
    source: PathBuf,
    destination: PathBuf,
    attempt: Attempt,
}

impl Fixture {
    fn new(kind: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let base = root.path().canonicalize().unwrap();
        let executable = match kind {
            "app" => "OC2ModManager.app/Contents/MacOS/OC2ModManager",
            _ => "OC2ModManager.exe",
        };
        let source = base.join("from").join(executable);
        let destination = base.join("to").join(executable);
        let attempt = Attempt::create(
            &base.join("attempts"),
            &source,
            &destination,
            "99.1.0",
            kind,
            true,
        )
        .unwrap();
        Self {
            root,
            source,
            destination,
            attempt,
        }
    }

    fn inspect(
        &self,
        path: &Path,
        kind: &str,
        current: &str,
        boot_time: u64,
    ) -> Option<InstallationState> {
        assert!(self.root.path().exists());
        inspect(
            self.attempt.work.parent().unwrap(),
            path,
            kind,
            &version(current).unwrap(),
            boot_time,
        )
        .unwrap()
    }
}

fn bound(status: Installation) -> Option<InstallationState> {
    Some(InstallationState {
        status,
        version: Some(Version::new(99, 1, 0)),
    })
}

#[test]
fn failed_attempt_is_bound_to_installation_and_format_and_clears_after_manual_upgrade() {
    let fixture = Fixture::new("exe");
    fixture.attempt.record_outcome(Outcome::Failed).unwrap();
    fs::write(fixture.attempt.work.join("update.exe"), b"temporary asset").unwrap();
    let boot = fixture.attempt.boot_time;
    let other_executable = fixture.source.with_file_name("other.exe");
    assert_eq!(
        fixture.inspect(&other_executable, "exe", "99.0.9", boot),
        None
    );
    assert_eq!(
        fixture.inspect(&fixture.source, "msi", "99.0.9", boot),
        None
    );
    assert_eq!(
        fixture.inspect(&fixture.source, "exe", "99.0.9", boot),
        bound(Installation::Failed)
    );
    assert_eq!(
        fixture.inspect(&fixture.destination, "exe", "99.1.0-rc.1", boot),
        bound(Installation::Failed)
    );
    assert_eq!(
        fixture.inspect(&fixture.destination, "exe", "99.1.0+manual", boot),
        None
    );
    assert!(!fixture.attempt.work.exists());
}

#[test]
fn pending_is_in_progress_only_while_locked_and_committed_failure_survives_the_lock() {
    let fixture = Fixture::new("exe");
    let boot = fixture.attempt.boot_time;
    let lock = fixture.attempt.lock_work().unwrap();
    assert_eq!(
        fixture.inspect(&fixture.source, "exe", "99.0.0", boot),
        bound(Installation::Applying)
    );
    fixture.attempt.record_outcome(Outcome::Failed).unwrap();
    assert_eq!(
        fixture.inspect(&fixture.source, "exe", "99.0.0", boot),
        bound(Installation::Failed)
    );
    assert_eq!(
        fixture.inspect(&fixture.source, "exe", "99.2.0", boot),
        None
    );
    assert!(fixture.attempt.work.exists());
    drop(lock);
    assert_eq!(
        fixture.inspect(&fixture.source, "exe", "99.2.0", boot),
        None
    );
    assert!(!fixture.attempt.work.exists());
}

#[test]
fn unlocked_pending_detects_interrupted_apply_and_abandoned_stage_is_removed() {
    let fixture = Fixture::new("exe");
    let boot = fixture.attempt.boot_time;
    assert_eq!(
        fixture.inspect(&fixture.source, "exe", "99.0.0", boot),
        bound(Installation::Failed)
    );
    fixture.attempt.record_outcome(Outcome::Abandoned).unwrap();
    assert_eq!(
        fixture.inspect(&fixture.source, "exe", "99.0.0", boot),
        None
    );
    assert!(!fixture.attempt.work.exists());
}

#[test]
fn msi_reboot_required_is_not_marked_as_a_failed_apply_before_reboot() {
    let fixture = Fixture::new("msi");
    fixture
        .attempt
        .record_outcome(Outcome::RebootRequired)
        .unwrap();
    let boot = fixture.attempt.boot_time;
    assert_eq!(
        fixture.inspect(&fixture.source, "msi", "99.0.0", boot),
        bound(Installation::RebootRequired)
    );
    assert_eq!(
        fixture.inspect(&fixture.source, "msi", "99.0.0", boot + 30),
        bound(Installation::Failed)
    );
    assert_eq!(
        fixture.inspect(&fixture.source, "msi", "99.1.0", boot),
        None
    );
    assert!(!fixture.attempt.work.exists());
}

#[test]
fn mac_bundle_identity_applies_even_if_the_executable_name_changes() {
    let fixture = Fixture::new("app");
    fixture
        .attempt
        .record_outcome(Outcome::RestartFailed)
        .unwrap();
    let boot = fixture.attempt.boot_time;
    let renamed = fixture.destination.with_file_name("NewManager");
    assert_eq!(
        fixture.inspect(&renamed, "app", "99.0.0", boot),
        bound(Installation::RestartFailed)
    );
    assert_eq!(fixture.inspect(&renamed, "app", "99.2.0", boot), None);
    assert!(!fixture.attempt.work.exists());
}
