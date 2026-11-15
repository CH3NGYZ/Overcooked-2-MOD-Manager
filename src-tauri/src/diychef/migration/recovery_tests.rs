//! Recovery cases extend the existing migration fixtures and real recycle journal.
use super::*;
use crate::config::ConfigService;

fn archive(
    w: &Workspace,
    folder: &str,
    version: &str,
    files: &[(&str, &[u8])],
    deleted_at: &str,
) -> String {
    let old = w.plugin(folder, version);
    let mut paths = vec![old.dll_file];
    for (relative, bytes) in files {
        let path = format!("BepInEx/plugins/{folder}/Resources/{relative}");
        w.write(&path, bytes);
        paths.push(path);
    }
    let entry = recycle_bin::stage(
        &w.recycle,
        &w.manifests,
        &w.game,
        "historical DIYChef".into(),
        None,
        None,
        vec![],
        paths,
        None,
        &mut || Ok(()),
    )
    .unwrap();
    // Fix archive recency explicitly instead of depending on timer resolution.
    let path = w.recycle.join(&entry.id).join("record.json");
    let mut record: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    record["deleted_at"] = deleted_at.into();
    fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
    entry.id
}

fn run_recovery(w: &Workspace, installed: &[InstalledMod]) -> Result<(), String> {
    let selected = target::select(&target::candidates(installed, 1), None)?.unwrap();
    let (_, recovered) = available_legacy_targets(installed, &w.game, &w.recycle, 1);
    migrate_targets(
        &w.game,
        &w.manifests,
        &w.recycle,
        &selected,
        &legacy_targets(installed, 1),
        &recovered,
        installed,
        &mut recycle_bin::set_diychef_resources_imported,
    )
}

fn chef_folders(w: &Workspace, folder: &str) -> Vec<PathBuf> {
    let mut result = fs::read_dir(w.game.join(format!("BepInEx/plugins/{folder}/Resources")))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("Fox"))
        })
        .collect::<Vec<_>>();
    result.sort();
    result
}

fn editor_state(w: &Workspace) -> AppState {
    let state = AppState::with_config(ConfigService::for_test(w.root.join("manager")));
    let mut config = state.config.load().unwrap();
    config.game.path = w.game.to_string_lossy().into_owned();
    state.config.save(&config).unwrap();
    state
}

fn open_after_scan(state: &AppState) -> (u64, crate::models::ChefEditorOpen) {
    let root = super::super::super::session::game_root(state).unwrap();
    let installed = mods::scan_mods_with_manifests(
        &root,
        &state.config.load().unwrap(),
        Some(&state.config.manifests_dir()),
    );
    let generation = state.local_mod_snapshot.lock().unwrap().begin(Some(&root));
    state
        .local_mod_snapshot
        .lock()
        .unwrap()
        .publish(generation, Some(&root), installed)
        .unwrap();
    let opened = super::super::super::session::open(state, generation, None, |_| 0).unwrap();
    (generation, opened)
}

#[test]
fn archived_legacy_identity_uses_guid_even_when_the_dll_filename_changed() {
    let w = Workspace::new();
    let old = w.plugin("old", "1.9");
    let renamed = "BepInEx/plugins/old/LegacyModelLoader.dll";
    fs::rename(w.game.join(&old.dll_file), w.game.join(renamed)).unwrap();
    let model = "BepInEx/plugins/old/Resources/Fox/Body.obj";
    w.write(model, b"renamed plugin model");
    recycle_bin::stage(
        &w.recycle,
        &w.manifests,
        &w.game,
        "historical model loader".into(),
        None,
        None,
        vec![],
        vec![renamed.into(), model.into()],
        None,
        &mut || Ok(()),
    )
    .unwrap();
    let installed = [w.plugin("new", "2.2")];
    let (legacy, _) = available_legacy_targets(&installed, &w.game, &w.recycle, 1);
    assert_eq!(
        legacy.len(),
        1,
        "DLL filename must not hide verified legacy resources"
    );
    run_recovery(&w, &installed).unwrap();
    assert_eq!(
        fs::read(w.game.join("BepInEx/plugins/new/Resources/Fox/Body.obj")).unwrap(),
        b"renamed plugin model"
    );
}

#[test]
fn records_from_one_physical_root_merge_once_and_newest_archive_wins() {
    let w = Workspace::new();
    archive(
        &w,
        "old",
        "1.7",
        &[
            ("Fox/Body.obj", b"older mesh"),
            ("Fox/old-only.png", b"older unique texture"),
            ("Fox/shared.png", b"older archived texture"),
        ],
        "2026-11-13T00:00:00Z",
    );
    archive(
        &w,
        "old",
        "1.9",
        &[
            ("Fox/Body.obj", b"newer mesh"),
            ("Fox/new-only.png", b"newer unique texture"),
            ("Fox/shared.png", b"newer archived texture"),
        ],
        "2026-11-14T00:00:00Z",
    );
    // User additions can survive in this root after the last old DLL was recycled.
    w.write(
        "BepInEx/plugins/old/Resources/Fox/shared.png",
        b"live texture",
    );
    w.write("BepInEx/plugins/old/Resources/Fox/INFO", b"ID=7\n");
    let installed = [w.plugin("new", "2.2")];
    run_recovery(&w, &installed).unwrap();
    let folders = chef_folders(&w, "new");
    assert_eq!(folders.len(), 1, "one old root must produce one Fox model");
    assert_eq!(
        fs::read(folders[0].join("Body.obj")).unwrap(),
        b"newer mesh"
    );
    assert_eq!(
        fs::read(folders[0].join("old-only.png")).unwrap(),
        b"older unique texture"
    );
    assert_eq!(
        fs::read(folders[0].join("new-only.png")).unwrap(),
        b"newer unique texture"
    );
    assert_eq!(
        fs::read(folders[0].join("shared.png")).unwrap(),
        b"live texture"
    );
    assert_eq!(fs::read(folders[0].join("INFO")).unwrap(), b"ID=7\n");
    assert!(!w.game.join("BepInEx/plugins/old").exists());
    let (legacy, _) = available_legacy_targets(&installed, &w.game, &w.recycle, 2);
    assert!(legacy.is_empty(), "both imported archives must be marked");
}

#[test]
fn active_legacy_resources_merge_with_archive_and_live_bytes_take_priority() {
    let w = Workspace::new();
    archive(
        &w,
        "old",
        "1.7",
        &[
            ("Fox/Body.obj", b"archived mesh"),
            ("Fox/archive-only.png", b"archived texture"),
            ("HATS/Cap/t_Cap.png", b"archived hat texture"),
        ],
        "2026-11-13T00:00:00Z",
    );
    let installed = [w.plugin("new", "2.2"), w.plugin("old", "1.9")];
    w.write("BepInEx/plugins/old/Resources/Fox/Body.obj", b"live mesh");
    w.write("BepInEx/plugins/old/Resources/Fox/INFO", b"ID=7\n");
    w.write(
        "BepInEx/plugins/old/Resources/HATS/Cap/m_Cap.obj",
        b"live hat mesh",
    );
    w.write(
        "BepInEx/plugins/new/Resources/Fox/Body.obj",
        b"existing mesh",
    );
    run_recovery(&w, &installed).unwrap();
    let folders = chef_folders(&w, "new");
    assert_eq!(folders.len(), 2, "existing model plus one combined import");
    let imported = folders
        .iter()
        .find(|path| path.file_name().unwrap() != "Fox")
        .unwrap();
    assert_eq!(fs::read(imported.join("Body.obj")).unwrap(), b"live mesh");
    assert_eq!(fs::read(imported.join("INFO")).unwrap(), b"ID=7\n");
    assert_eq!(
        fs::read(imported.join("archive-only.png")).unwrap(),
        b"archived texture"
    );
    assert_eq!(
        fs::read(
            w.game
                .join("BepInEx/plugins/new/Resources/HATS/Cap/t_Cap.png")
        )
        .unwrap(),
        b"archived hat texture"
    );
    assert_eq!(
        fs::read(
            w.game
                .join("BepInEx/plugins/new/Resources/HATS/Cap/m_Cap.obj")
        )
        .unwrap(),
        b"live hat mesh"
    );
    assert_eq!(
        fs::read(w.game.join("BepInEx/plugins/new/Resources/Fox/Body.obj")).unwrap(),
        b"existing mesh"
    );
    assert!(!w.game.join("BepInEx/plugins/old").exists());
}

#[test]
fn directory_casing_changes_keep_live_and_archived_model_parts_together() {
    let w = Workspace::new();
    archive(
        &w,
        "old",
        "1.9",
        &[
            ("Fox/Body.obj", b"archived mesh"),
            ("HATS/Cap/t_Cap.png", b"hat texture"),
        ],
        "2026-11-13T00:00:00Z",
    );
    w.write("BepInEx/plugins/old/Resources/fox/INFO", b"ID=7\n");
    w.write(
        "BepInEx/plugins/old/Resources/hats/cap/m_Cap.obj",
        b"live hat mesh",
    );
    run_recovery(&w, &[w.plugin("new", "2.2")]).unwrap();
    let resources = w.game.join("BepInEx/plugins/new/Resources");
    let chefs = fs::read_dir(&resources)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            !path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .eq_ignore_ascii_case("HATS")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        chefs.len(),
        1,
        "case-only changes must not split one model into two"
    );
    assert_eq!(
        fs::read(chefs[0].join("Body.obj")).unwrap(),
        b"archived mesh"
    );
    assert_eq!(fs::read(chefs[0].join("INFO")).unwrap(), b"ID=7\n");
    let hats = fs::read_dir(resources.join("HATS"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(
        hats.len(),
        1,
        "hat mesh and texture must remain in one folder"
    );
    let name = hats[0].file_name().unwrap().to_string_lossy();
    assert_eq!(name, "cap", "live folder spelling takes priority");
    assert_eq!(
        fs::read(hats[0].join(format!("t_{name}.png"))).unwrap(),
        b"hat texture"
    );
    assert_eq!(
        fs::read(hats[0].join(format!("m_{name}.obj"))).unwrap(),
        b"live hat mesh"
    );
}

#[test]
fn successful_live_migration_does_not_offer_its_own_new_recycle_record_on_reopen() {
    let w = Workspace::new();
    w.plugin("new", "2.2");
    w.plugin("old", "1.9");
    w.write("BepInEx/plugins/old/Resources/Fox/Body.obj", b"old mesh");
    let state = editor_state(&w);
    let (generation, opened) = open_after_scan(&state);
    assert!(opened.migration_available);
    migrate(&state, generation, &opened.snapshot.unwrap().target.id).unwrap();
    let (_, reopened) = open_after_scan(&state);
    assert_eq!(reopened.status, "ready");
    assert!(
        !reopened.migration_available,
        "migration must be idempotent"
    );
    assert!(reopened.legacy_targets.is_empty());
    assert_eq!(chef_folders(&w, "new").len(), 1);
    assert_eq!(recycle_bin::list(&w.recycle).unwrap().len(), 1);
}

#[test]
fn same_directory_upgrade_imports_archive_without_recycling_current_plugin_or_live_models() {
    let w = Workspace::new();
    archive(
        &w,
        "shared",
        "1.9",
        &[("Fox/Body.obj", b"archived mesh")],
        "2026-11-13T00:00:00Z",
    );
    let modern = w.plugin("shared", "2.2");
    w.write(
        "BepInEx/plugins/shared/Resources/Fox/Body.obj",
        b"current mesh",
    );
    w.write(
        "BepInEx/plugins/shared/prefer.txt",
        b"keep current preference",
    );
    let dll_before = fs::read(w.game.join(&modern.dll_file)).unwrap();
    let state = editor_state(&w);
    let (generation, opened) = open_after_scan(&state);
    assert!(opened.migration_available);
    migrate(&state, generation, &opened.snapshot.unwrap().target.id).unwrap();
    assert_eq!(fs::read(w.game.join(&modern.dll_file)).unwrap(), dll_before);
    assert_eq!(
        fs::read(w.game.join("BepInEx/plugins/shared/prefer.txt")).unwrap(),
        b"keep current preference"
    );
    assert_eq!(
        fs::read(w.game.join("BepInEx/plugins/shared/Resources/Fox/Body.obj")).unwrap(),
        b"current mesh"
    );
    let folders = chef_folders(&w, "shared");
    assert_eq!(folders.len(), 2);
    let imported = folders
        .iter()
        .find(|path| path.file_name().unwrap() != "Fox")
        .unwrap();
    assert_eq!(
        fs::read(imported.join("Body.obj")).unwrap(),
        b"archived mesh"
    );
    assert_eq!(recycle_bin::list(&w.recycle).unwrap().len(), 1);
    let (_, reopened) = open_after_scan(&state);
    assert!(!reopened.migration_available);
    assert!(reopened.legacy_targets.is_empty());
}

#[test]
fn rejected_recycling_removes_only_imports_and_clears_archive_markers() {
    let w = Workspace::new();
    let id = archive(
        &w,
        "old",
        "1.7",
        &[("Fox/archive-only.png", b"archived texture")],
        "2026-11-13T00:00:00Z",
    );
    let installed = [w.plugin("new", "2.2"), w.plugin("old", "1.9")];
    w.write("BepInEx/plugins/old/Resources/Fox/Body.obj", b"live mesh");
    w.write(
        "BepInEx/plugins/new/Resources/Fox/Body.obj",
        b"existing mesh",
    );
    // The recycle journal must reject a file where the manifest directory belongs.
    fs::write(&w.manifests, b"blocks manifest enumeration").unwrap();
    assert!(run_recovery(&w, &installed).is_err());
    assert_eq!(chef_folders(&w, "new").len(), 1);
    assert_eq!(
        fs::read(w.game.join("BepInEx/plugins/new/Resources/Fox/Body.obj")).unwrap(),
        b"existing mesh"
    );
    assert_eq!(
        fs::read(w.game.join("BepInEx/plugins/old/Resources/Fox/Body.obj")).unwrap(),
        b"live mesh"
    );
    assert!(w.game.join(&installed[1].dll_file).is_file());
    let record: serde_json::Value =
        serde_json::from_slice(&fs::read(w.recycle.join(id).join("record.json")).unwrap()).unwrap();
    assert_eq!(record["diychef_resources_imported"], false);
}

#[test]
fn failed_marker_rollback_keeps_imported_models_as_recovery_material() {
    let w = Workspace::new();
    let mut ids = [
        archive(
            &w,
            "old-a",
            "1.7",
            &[("Fox/Body.obj", b"first archived mesh")],
            "2026-11-13T00:00:00Z",
        ),
        archive(
            &w,
            "old-b",
            "1.9",
            &[("Fox/Body.obj", b"second archived mesh")],
            "2026-11-14T00:00:00Z",
        ),
    ];
    ids.sort();
    w.write("BepInEx/plugins/old-a/Resources/Fox/INFO", b"ID=7\n");
    w.write("BepInEx/plugins/old-b/Resources/Fox/INFO", b"ID=8\n");
    let installed = [w.plugin("new", "2.2")];
    let selected = target::select(&target::candidates(&installed, 1), None)
        .unwrap()
        .unwrap();
    let (_, recovered) = available_legacy_targets(&installed, &w.game, &w.recycle, 1);
    assert_eq!(recovered.len(), 2);
    let mut calls = Vec::new();
    let result = migrate_targets(
        &w.game,
        &w.manifests,
        &w.recycle,
        &selected,
        &[],
        &recovered,
        &installed,
        &mut |root, id, imported| {
            calls.push((id.to_owned(), imported));
            if (id == ids[1] && imported) || (id == ids[0] && !imported) {
                return Err("injected marker write failure".into());
            }
            recycle_bin::set_diychef_resources_imported(root, id, imported)
        },
    );
    assert_eq!(result.unwrap_err(), "recycle.partialFailure");
    assert_eq!(
        calls,
        vec![
            (ids[0].clone(), true),
            (ids[1].clone(), true),
            (ids[0].clone(), false),
        ],
        "exercise successful persistence, a later write failure, and failed rollback"
    );
    let folders = chef_folders(&w, "new");
    assert_eq!(folders.len(), 2, "never delete imported recovery copies");
    let meshes = folders
        .iter()
        .map(|path| fs::read(path.join("Body.obj")).unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        meshes,
        [
            b"first archived mesh".to_vec(),
            b"second archived mesh".to_vec()
        ]
        .into_iter()
        .collect()
    );
    for folder in ["old-a", "old-b"] {
        assert!(
            w.game
                .join(format!("BepInEx/plugins/{folder}/Resources/Fox/INFO"))
                .is_file()
        );
        assert!(
            !w.game
                .join(format!("BepInEx/plugins/{folder}/DIYChef.dll"))
                .exists()
        );
    }
    assert!(w.game.join(&installed[0].dll_file).is_file());
    assert_eq!(recycle_bin::list(&w.recycle).unwrap().len(), 2);
    for (id, imported) in [(&ids[0], true), (&ids[1], false)] {
        let record: serde_json::Value =
            serde_json::from_slice(&fs::read(w.recycle.join(id).join("record.json")).unwrap())
                .unwrap();
        assert_eq!(record["diychef_resources_imported"], imported);
    }
}

#[test]
#[cfg(any(windows, target_os = "macos"))]
fn link_outside_resources_blocks_whole_root_preflight_before_importing() {
    let w = Workspace::new();
    let installed = [w.plugin("new", "2.2"), w.plugin("old", "1.9")];
    w.write("BepInEx/plugins/old/Resources/Fox/Body.obj", b"live mesh");
    let outside = w.root.join("outside-game");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("sentinel.txt"), b"must remain untouched").unwrap();
    let link = w.game.join("BepInEx/plugins/old/readme-link");
    #[cfg(windows)]
    {
        let output = std::process::Command::new("cmd.exe")
            .args(["/D", "/C", "mklink", "/J"])
            .arg(&link)
            .arg(&outside)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "junction fixture could not be created"
        );
    }
    #[cfg(target_os = "macos")]
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    let result = w.execute(&installed);
    #[cfg(windows)]
    fs::remove_dir(&link).unwrap();
    #[cfg(target_os = "macos")]
    fs::remove_file(&link).unwrap();
    assert_eq!(result.unwrap_err(), "recycle.unsafePath");
    assert!(!w.game.join("BepInEx/plugins/new/Resources").exists());
    assert!(w.game.join(&installed[1].dll_file).is_file());
    assert_eq!(
        fs::read(outside.join("sentinel.txt")).unwrap(),
        b"must remain untouched"
    );
    assert!(recycle_bin::list(&w.recycle).unwrap().is_empty());
}
