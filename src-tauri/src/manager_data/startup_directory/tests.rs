use super::*;
use crate::mod_changes::transaction::{
    FileChange, Fingerprint, Mutation, RootKind, StagedFile, TransactionPath, TransactionRoots,
};

struct Fixture {
    _temp: tempfile::TempDir,
    location: PathBuf,
    source: PathBuf,
    selected: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(temp.path()).unwrap();
        let selected = base.join("MovedDocuments").join("OC2ModManager");
        fs::create_dir_all(&selected).unwrap();
        fs::write(base.join("Documents.lnk"), b"shortcut fixture").unwrap();
        Self {
            _temp: temp,
            location: base.join("bootstrap").join("data-location.json"),
            source: base.join("Documents").join("OC2ModManager"),
            selected,
        }
    }

    fn choose(&self) -> Option<Issue> {
        use_existing(&self.location, &self.source, &self.selected, || {
            Ok(self.source.clone())
        })
        .unwrap()
    }

    fn config(&self) -> String {
        let mut config = crate::config::default_config();
        let game = self.selected.parent().unwrap().join("game");
        config.game.path = game.to_string_lossy().into_owned();
        toml::to_string(&config).unwrap()
    }
}

#[test]
fn shortcut_can_reselect_existing_config_without_moving_it() {
    let fixture = Fixture::new();
    let config = fixture.config();
    let file = fixture.selected.join("config.toml");
    fs::write(&file, &config).unwrap();
    let recycle = fixture.selected.join("recycle-bin");
    fs::create_dir(&recycle).unwrap();
    fs::write(recycle.join("keep"), b"existing recycle data").unwrap();

    assert!(fixture.choose().is_none());
    let root = user_paths::locate_root_with_default(&fixture.location, || unreachable!()).unwrap();
    assert_eq!(root.path, fixture.selected);
    assert!(root.custom && root.documents_alias.is_none());
    assert_eq!(fs::read_to_string(file).unwrap(), config);
    assert_eq!(
        fs::read(recycle.join("keep")).unwrap(),
        b"existing recycle data"
    );
    assert!(!fixture.source.exists());
}

#[test]
fn rejecting_empty_or_invalid_configuration_does_not_publish_a_path() {
    let fixture = Fixture::new();
    assert!(matches!(fixture.choose(), Some(Issue::InvalidSelection)));
    fs::write(fixture.selected.join("config.toml"), b"not valid TOML").unwrap();
    assert!(matches!(fixture.choose(), Some(Issue::InvalidSelection)));
    assert!(!fixture.location.exists());
}

#[test]
fn selecting_the_previous_config_does_not_create_a_replacement() {
    let fixture = Fixture::new();
    let config = fixture.config();
    let backup = fixture.selected.join("config.previous.toml");
    fs::write(&backup, &config).unwrap();

    assert!(fixture.choose().is_none());
    assert!(!fixture.selected.join("config.toml").exists());
    assert_eq!(fs::read_to_string(backup).unwrap(), config);
    assert_eq!(
        user_paths::locate_root(&fixture.location).unwrap().path,
        fixture.selected
    );
}

#[test]
fn choosing_a_directory_does_not_discard_an_unreadable_data_operation() {
    let fixture = Fixture::new();
    fs::write(fixture.selected.join("config.toml"), fixture.config()).unwrap();
    let control = fixture.location.parent().unwrap();
    fs::create_dir_all(control).unwrap();
    let journal = control.join(".OC2ModManager-reset.json");
    fs::write(&journal, b"preserve unreadable journal").unwrap();

    assert!(matches!(fixture.choose(), Some(Issue::RecoveryBlocked)));
    assert!(!fixture.location.exists());
    assert_eq!(fs::read(journal).unwrap(), b"preserve unreadable journal");
}

#[test]
fn reselecting_keeps_transactions_only_at_their_verified_root() {
    for moved in [false, true] {
        let fixture = Fixture::new();
        fs::write(fixture.selected.join("config.toml"), fixture.config()).unwrap();
        let base = fixture.location.parent().unwrap().parent().unwrap();
        let data_root = if moved {
            base.join("original").join("OC2ModManager")
        } else {
            fixture.selected.clone()
        };
        let game_root = base.join("game");
        fs::create_dir_all(&data_root).unwrap();
        fs::create_dir(&game_root).unwrap();
        let payload = data_root.join("payload.dll");
        fs::write(&payload, b"pending mod").unwrap();
        let transaction_root = data_root.join(transaction::DIRECTORY_NAME);
        let pending = transaction::prepare(
            &transaction_root,
            &TransactionRoots {
                game_root: game_root.clone(),
                data_root,
            },
            vec![Mutation::File(FileChange {
                target: TransactionPath {
                    root: RootKind::Game,
                    relative: "mod.dll".into(),
                },
                source: Some(StagedFile::capture(payload).unwrap()),
                expected: Fingerprint::Missing,
                require_single_link: false,
            })],
        )
        .unwrap();
        let relative = Path::new(pending.id()).join("journal.json");
        let original = fs::read(transaction_root.join(&relative)).unwrap();
        let selected_transactions = fixture.selected.join(transaction::DIRECTORY_NAME);
        if moved {
            fs::rename(transaction_root, &selected_transactions).unwrap();
        }

        let result = fixture.choose();
        if moved {
            assert!(matches!(result, Some(Issue::RecoveryBlocked)));
            assert!(!fixture.location.exists());
        } else {
            assert!(result.is_none());
            assert_eq!(
                user_paths::locate_root(&fixture.location).unwrap().path,
                fixture.selected
            );
        }
        assert!(!game_root.join("mod.dll").exists());
        assert_eq!(
            fs::read(selected_transactions.join(relative)).unwrap(),
            original
        );
    }
}
