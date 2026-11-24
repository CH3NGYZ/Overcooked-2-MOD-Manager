use super::*;
use crate::{
    app_state::AppState,
    config::ConfigService,
    mod_changes::{
        planning,
        session::{MapImport, PlanRequest},
    },
};
use std::{
    fs,
    path::{Path, PathBuf},
};

const NTFS_GAME: &str = "OC2_VOLUME_TEST_NTFS_GAME";
const NTFS_DATA: &str = "OC2_VOLUME_TEST_NTFS_DATA";
const EXFAT: &str = "OC2_VOLUME_TEST_EXFAT";
const FIXTURE_ROOT: &str = "OC2ModManager-VolumeFixtures";
const MARKER: &str = "marker.txt";
const BEFORE: &[u8] = b"before";
const AFTER: &[u8] = b"after";

struct Volumes {
    game: tempfile::TempDir,
    data: tempfile::TempDir,
    roots: TransactionRoots,
}

impl Volumes {
    fn new(game: &str, data: &str) -> Self {
        let (game, game_root) = isolated(game);
        let (data, data_root) = isolated(data);
        fs::write(game_root.join(MARKER), BEFORE).unwrap();
        fs::write(data_root.join(MARKER), BEFORE).unwrap();
        Self {
            game,
            data,
            roots: TransactionRoots {
                game_root,
                data_root,
            },
        }
    }

    fn journal(&self) -> PathBuf {
        self.roots.data_root.join(DIRECTORY_NAME)
    }

    fn assert_markers(&self, bytes: &[u8]) {
        assert_eq!(fs::read(self.roots.game_root.join(MARKER)).unwrap(), bytes);
        assert_eq!(fs::read(self.roots.data_root.join(MARKER)).unwrap(), bytes);
    }

    fn close(self) {
        self.game.close().unwrap();
        self.data.close().unwrap();
    }
}

fn isolated(variable: &str) -> (tempfile::TempDir, PathBuf) {
    let path =
        PathBuf::from(std::env::var_os(variable).unwrap_or_else(|| panic!("set {variable}")));
    assert_eq!(path.file_name().unwrap(), FIXTURE_ROOT);
    paths::validate_absolute(&path).unwrap();
    let parent = path.canonicalize().unwrap();
    assert_eq!(parent.file_name().unwrap(), FIXTURE_ROOT);
    let fixture = tempfile::Builder::new()
        .prefix("run-")
        .tempdir_in(&parent)
        .unwrap();
    let physical = fixture.path().canonicalize().unwrap();
    assert_eq!(physical.parent(), Some(parent.as_path()));
    println!("{variable}: {}", physical.display());
    (fixture, physical)
}

fn assert_exfat_error(error: &str, root: &Path) {
    let expected = crate::locale_tag::with_params(
        "modChanges.writeVolumeUnsupported",
        &[
            ("path", &crate::display_path::display_path(root)),
            ("filesystem", "exFAT"),
        ],
    );
    assert_eq!(error, expected);
}

fn rejected_exfat(game: &str, data: &str, exfat: RootKind) {
    let volumes = Volumes::new(game, data);
    let unsupported = match exfat {
        RootKind::Game => &volumes.roots.game_root,
        RootKind::Data => &volumes.roots.data_root,
    };
    let result = prepare(
        &volumes.journal(),
        &volumes.roots,
        vec![Mutation::File(FileChange {
            target: TransactionPath {
                root: exfat,
                relative: MARKER.into(),
            },
            source: None,
            expected: Fingerprint::Missing,
            require_single_link: false,
        })],
    );
    let Err(error) = result else {
        panic!("exFAT must be refused before journal or target fingerprint creation");
    };
    assert_exfat_error(&error, unsupported);
    assert!(!volumes.journal().exists());
    volumes.assert_markers(BEFORE);

    let state = AppState::with_config(ConfigService::for_test(volumes.roots.data_root.clone()));
    let mut config = crate::config::default_config();
    config.game.path = volumes.roots.game_root.to_string_lossy().into_owned();
    state.config.save(&config).unwrap();
    let request = PlanRequest {
        map_import: Some(MapImport {
            zip_path: volumes
                .roots
                .data_root
                .join("not-opened.zip")
                .to_string_lossy()
                .into_owned(),
            download_id: None,
            selected: vec!["Fixture".into()],
            origin: None,
        }),
        ..Default::default()
    };
    let error = planning::create(&state, request).unwrap_err();
    assert_exfat_error(&error, unsupported);
    assert!(!volumes.journal().exists());
    assert!(!volumes.roots.data_root.join("mod-change-staging").exists());
    volumes.assert_markers(BEFORE);
    drop(state);
    volumes.close();
}

#[test]
#[ignore = "requires explicit exFAT and NTFS OC2ModManager-VolumeFixtures roots"]
fn real_exfat_game_root_is_refused_before_staging_and_journal() {
    rejected_exfat(EXFAT, NTFS_DATA, RootKind::Game);
}

#[test]
#[ignore = "requires explicit NTFS and exFAT OC2ModManager-VolumeFixtures roots"]
fn real_exfat_data_root_is_refused_before_staging_and_journal() {
    rejected_exfat(NTFS_GAME, EXFAT, RootKind::Data);
}

#[test]
#[ignore = "requires explicit OC2ModManager-VolumeFixtures roots on two NTFS volumes"]
fn real_separate_ntfs_roots_commit_and_roll_back_together() {
    let volumes = Volumes::new(NTFS_GAME, NTFS_DATA);
    assert_ne!(
        crate::file_metadata::write_volume(&volumes.roots.game_root).unwrap(),
        crate::file_metadata::write_volume(&volumes.roots.data_root).unwrap(),
    );
    let staged = volumes.roots.data_root.join("replacement.txt");
    fs::write(&staged, AFTER).unwrap();
    let changes = || {
        [RootKind::Game, RootKind::Data]
            .into_iter()
            .map(|root| {
                let target = TransactionPath {
                    root,
                    relative: MARKER.into(),
                };
                let expected =
                    Fingerprint::capture(&target.resolve(&volumes.roots).unwrap()).unwrap();
                Mutation::File(FileChange {
                    target,
                    source: Some(StagedFile::capture(staged.clone()).unwrap()),
                    expected,
                    require_single_link: false,
                })
            })
            .collect()
    };
    let prepare = || super::prepare(&volumes.journal(), &volumes.roots, changes()).unwrap();
    let rollback = prepare().apply_checked(
        || false,
        || Ok(()),
        || {
            volumes.assert_markers(AFTER);
            Err("fixture: rollback after both writes".into())
        },
        |_| {},
    );
    assert_eq!(
        rollback.err().as_deref(),
        Some("fixture: rollback after both writes")
    );
    volumes.assert_markers(BEFORE);
    assert!(!has_pending(&volumes.journal()).unwrap());

    prepare().apply().unwrap();
    volumes.assert_markers(AFTER);
    assert!(!has_pending(&volumes.journal()).unwrap());
    volumes.close();
}
