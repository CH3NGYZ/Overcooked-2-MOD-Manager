use super::*;
use crate::display_path::display_path;
use crate::mod_changes::transaction::recovery_control::DOCUMENTS_ALIAS_FOR_TEST;
use std::os::windows::fs::FileTypeExt;

const MANIFEST: &str = "installed.toml";

struct RedirectedDocuments {
    workspace: Workspace,
    roots: TransactionRoots,
    transaction_root: PathBuf,
    journal: PathBuf,
}

impl RedirectedDocuments {
    fn new(paused: bool) -> Self {
        let workspace = Workspace::new();
        let old_root = &workspace.roots.data_root;
        let physical_root = old_root.with_file_name("redirected-documents");
        fs::write(old_root.join(MANIFEST), b"old manifest").unwrap();
        workspace.write("a.dll", b"old dll");
        let Mutation::File(mut data) = workspace.file(MANIFEST, Some(b"new manifest")) else {
            unreachable!()
        };
        data.target.root = RootKind::Data;
        data.expected = Fingerprint::capture(&old_root.join(MANIFEST)).unwrap();
        let mut pending = workspace.prepare(vec![
            workspace.file("a.dll", Some(b"new dll")),
            Mutation::File(data),
        ]);
        pending.journal.state = JournalState::Applying;
        pending.persist().unwrap();
        pending.apply_record(0, || {}).unwrap();
        pending.apply_record(1, || {}).unwrap();
        if paused {
            pause(&workspace, &get(&workspace));
        }

        fs::rename(old_root, &physical_root).unwrap();
        let transaction_root = physical_root.join(workspace.journal_root.file_name().unwrap());
        let journal = transaction_root
            .join(pending.directory.file_name().unwrap())
            .join(JOURNAL_FILE);
        let roots = TransactionRoots {
            game_root: workspace.roots.game_root.clone(),
            data_root: physical_root,
        };
        Self {
            workspace,
            roots,
            transaction_root,
            journal,
        }
    }

    fn link(&self) {
        let output = std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(display_path(&self.workspace.roots.data_root))
            .arg(display_path(&self.roots.data_root))
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
    }

    fn register_alias(&self, target: &Path) {
        DOCUMENTS_ALIAS_FOR_TEST.set(Some((
            self.workspace.roots.data_root.clone(),
            target.to_path_buf(),
        )));
    }

    fn entry(&self) -> RecoveryEntry {
        let mut entries = list_recoveries(&self.transaction_root, Some(&self.roots)).unwrap();
        assert_eq!(entries.len(), 1);
        entries.pop().unwrap()
    }

    fn assert_recovered(&self) {
        assert_eq!(self.workspace.read("a.dll"), b"old dll");
        assert_eq!(
            fs::read(self.roots.data_root.join(MANIFEST)).unwrap(),
            b"old manifest"
        );
        assert!(!has_retained_material(&self.transaction_root).unwrap());
    }
}

impl Drop for RedirectedDocuments {
    fn drop(&mut self) {
        DOCUMENTS_ALIAS_FOR_TEST.set(None);
        let old_root = &self.workspace.roots.data_root;
        if fs::symlink_metadata(old_root)
            .is_ok_and(|metadata| metadata.file_type().is_symlink_dir())
        {
            let _ = fs::remove_dir(old_root);
        }
    }
}

#[test]
fn verified_documents_alias_recovers_paused_transaction_from_the_same_directory() {
    let fixture = RedirectedDocuments::new(true);
    let before = fs::read(&fixture.journal).unwrap();
    assert!(!fixture.entry().scope_known);
    fixture.register_alias(&fixture.roots.data_root);
    assert!(!fixture.entry().scope_known);
    fixture.link();

    let entry = fixture.entry();
    assert_eq!(entry.state, RecoveryDecisionState::Paused);
    assert!(entry.scope_known);
    assert!(
        entry
            .paths
            .iter()
            .any(|path| path.path == fixture.roots.data_root.join(MANIFEST))
    );
    assert_eq!(fs::read(&fixture.journal).unwrap(), before);
    let result = retry_recovery(
        &fixture.transaction_root,
        &fixture.roots,
        &entry.id,
        &entry.revision,
    )
    .unwrap();
    assert_eq!(result.recovered, [entry.id]);
    fixture.assert_recovered();
}

#[test]
fn automatic_recovery_requires_the_registered_documents_target() {
    let fixture = RedirectedDocuments::new(false);
    fixture.link();
    let other_root = fixture.roots.data_root.with_file_name("another-directory");
    fs::create_dir(&other_root).unwrap();
    fixture.register_alias(&other_root);
    let entry = fixture.entry();
    assert_eq!(entry.state, RecoveryDecisionState::Automatic);
    assert!(!entry.scope_known);
    let before = fs::read(&fixture.journal).unwrap();
    let recover = || recover_all(&fixture.transaction_root, &fixture.roots).unwrap();
    assert_eq!(recover().blocked, [entry.id.clone()]);
    assert_eq!(fs::read(&fixture.journal).unwrap(), before);
    assert_eq!(fixture.workspace.read("a.dll"), b"new dll");
    assert_eq!(
        fs::read(fixture.roots.data_root.join(MANIFEST)).unwrap(),
        b"new manifest"
    );

    fixture.register_alias(&fixture.roots.data_root);
    let result = recover();
    assert!(result.blocked.is_empty());
    assert_eq!(result.recovered, [entry.id]);
    fixture.assert_recovered();
}
