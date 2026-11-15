//! Upgrade regressions exercise the real installer, scan and editor-open chain.
use super::*;
use crate::{config::ConfigService, models::ModDownloadMode};
use std::io::Write;

const MODEL: &str = "BepInEx/plugins/old/Resources/Fox/Body.obj";
const INFO: &str = "BepInEx/plugins/old/Resources/Fox/INFO";

fn install_fixture(
    workspace: &Workspace,
    repo: &crate::models::ModRepositoryConfig,
    folder: &str,
    version: &str,
    include_model: bool,
) {
    let asset = workspace.root.join(format!("{folder}.zip"));
    let mut zip = zip::ZipWriter::new(fs::File::create(&asset).unwrap());
    let mut entries = vec![(
        format!("{folder}/DIYChef.dll"),
        mods::plugin_blob(target::GUID, "DIYChef", version),
    )];
    if include_model {
        entries.extend([
            (
                format!("{folder}/Resources/Fox/Body.obj"),
                b"old model".to_vec(),
            ),
            (format!("{folder}/Resources/Fox/INFO"), b"ID=7\n".to_vec()),
        ]);
    }
    for (name, bytes) in entries {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&bytes).unwrap();
    }
    zip.finish().unwrap();
    mods::install::install_downloaded_mod_repository(
        &asset,
        &format!("{folder}.zip"),
        &workspace.game,
        &workspace.manifests,
        &workspace.recycle,
        repo,
        &format!("v{version}"),
        false,
        true,
    )
    .unwrap();
}

fn upgrade_then_open(recorded_resources: bool) {
    let workspace = Workspace::new();
    let mut repo = mods::test_support::repository("chef", ModDownloadMode::Release);
    repo.plugin_guid = target::GUID.into();
    install_fixture(&workspace, &repo, "old", "1.9", recorded_resources);
    if !recorded_resources {
        workspace.write(MODEL, b"old model");
        workspace.write(INFO, b"ID=7\n");
    }
    assert!(workspace.game.join(MODEL).is_file());
    install_fixture(&workspace, &repo, "new", "2.2", false);
    assert!(
        !workspace
            .game
            .join("BepInEx/plugins/old/DIYChef.dll")
            .exists()
    );
    // Both bundled and user-added Resources are journaled with the legacy DLL
    // before an upgrade removes the old plugin directory.  The migration
    // picker must recover them from that record after the DLL is gone.
    assert!(!workspace.game.join(MODEL).is_file());
    let recycled = recycle_bin::list(&workspace.recycle).unwrap();
    assert_eq!(recycled.len(), 1);
    assert_eq!(recycled[0].file_count, 3);
    assert_eq!(
        fs::read(
            workspace
                .recycle
                .join(&recycled[0].id)
                .join("files")
                .join(MODEL)
        )
        .unwrap(),
        b"old model"
    );

    let state = AppState::with_config(ConfigService::for_test(workspace.root.join("manager")));
    let mut config = state.config.load().unwrap();
    config.game.path = workspace.game.to_string_lossy().into_owned();
    config.repositories = vec![repo];
    state.config.save(&config).unwrap();
    let root = super::super::super::session::game_root(&state).unwrap();
    let installed = mods::scan_mods_with_manifests(&root, &config, Some(&workspace.manifests));
    assert_eq!(
        installed.len(),
        1,
        "the real update removes the old DLL from scanning"
    );
    assert_eq!(installed[0].version.as_deref(), Some("2.2"));
    let generation = state.local_mod_snapshot.lock().unwrap().begin(Some(&root));
    state
        .local_mod_snapshot
        .lock()
        .unwrap()
        .publish(generation, Some(&root), installed)
        .unwrap();
    let opened = super::super::super::session::open(&state, generation, None, |_| 0).unwrap();
    assert_eq!(opened.status, "ready");
    eprintln!(
        "orphan-repro: recorded_resources={recorded_resources}, scanned_dlls=1, recycle_files={}, legacy_targets={}, migration_available={}",
        recycled[0].file_count,
        opened.legacy_targets.len(),
        opened.migration_available
    );
    assert!(
        opened.migration_available,
        "updating away the old DLL must retain discovery of recoverable legacy resources"
    );
    let target = opened.snapshot.unwrap().target;
    migrate(&state, generation, &target.id).unwrap();
    assert_eq!(
        fs::read(
            workspace
                .game
                .join("BepInEx/plugins/new/Resources/Fox/Body.obj")
        )
        .unwrap(),
        b"old model"
    );
}

#[test]
fn upgrade_retains_migration_for_user_resources_left_without_the_old_dll() {
    upgrade_then_open(false);
}

#[test]
fn upgrade_retains_migration_for_legacy_resources_moved_into_the_recycle_bin() {
    upgrade_then_open(true);
}
