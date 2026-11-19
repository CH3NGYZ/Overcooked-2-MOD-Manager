use super::*;
use crate::mod_changes::{execution, planning, preparation, session::MapImport};
use crate::{config::ConfigService, mods};
use std::io::{Read, Write};

#[test]
fn map_sandbox_refuses_the_game_root_even_with_different_windows_path_spellings() {
    let mut f = fixture();
    let data = f._temp.path().join("data");
    let state = AppState::with_config(ConfigService::for_test(data.clone()));
    f.session.staging = data.join("stage");
    f.session.trusted.game_root = fs::canonicalize(&f.session.staging).unwrap();
    let sandbox = f.session.staging.join("never-created");
    assert_eq!(
        create_sandbox(&state, &f.session, &sandbox).unwrap_err(),
        "archive.pathEscape"
    );
    assert!(!sandbox.exists());
}

#[test]
fn map_sandbox_refuses_a_reparse_point_in_its_parent_chain() {
    let mut f = fixture();
    let other = f._temp.path().join("outside-data");
    fs::create_dir(&other).unwrap();
    // `mklink /J` accepts the ordinary Windows paths used by ConfigService.
    let link = f._temp.path().join("data").join("redirected-stage");
    let state = AppState::with_config(ConfigService::for_test(f._temp.path().join("data")));
    let result = std::process::Command::new("cmd.exe")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(&link)
        .arg(&other)
        .output()
        .unwrap();
    assert!(result.status.success(), "could not create test junction");
    f.session.staging = link;
    assert_eq!(
        create_sandbox(&state, &f.session, &f.session.staging.join("never-created")).unwrap_err(),
        "recycle.unsafePath"
    );
    assert_eq!(fs::read_dir(other).unwrap().count(), 0);
}

#[test]
#[ignore = "需要本地 .tmp/rw.zip；单独运行，以便隔离地图解析缓存"]
fn real_zip_update_accepts_normal_windows_data_path() {
    let zip = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.tmp/rw.zip");
    assert!(zip.is_file(), "需要本地真实地图 ZIP：{}", zip.display());
    let zip = fs::canonicalize(zip).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("OC2ModManager");
    assert_ne!(
        data,
        fs::canonicalize(temp.path()).unwrap().join("OC2ModManager")
    );
    let game = fs::canonicalize(temp.path()).unwrap().join("game");
    let package = game.join(crate::framework::DIYLEVEL_DIR);
    let stage = level_sets::inspect_zip_with_progress(&zip, 1, 1, &|_, _, _| {}).unwrap();
    let [selected] = stage.as_slice() else {
        panic!("测试包中应只有一张地图");
    };
    let map = package.join("levels").join(&selected.dir_name);
    let mut previous = selected.metadata.clone();
    previous.version = "0.72".into();
    assert_ne!(previous.version, selected.metadata.version);
    write(&map, "info", b"previous bundle");
    write(&map, "keep-until-commit", b"previous map bytes");
    level_sets::seed_info_metadata(&map.join("info"), previous);
    write(&game, "BepInEx/core/BepInEx.dll", b"fixture loader");
    write(
        &package,
        "DIYLevel.dll",
        &mods::plugin_blob(mods::plugin_target::DIYLEVEL_GUID, "DIYLevel", "0.11.1"),
    );

    // The real scanner uses the process root for its content cache. This one
    // ignored test runs in an isolated process; never write the user's cache.
    fs::create_dir(&data).unwrap();
    crate::user_paths::freeze_root(&data).unwrap();
    let state = AppState::with_config(ConfigService::for_test(data.clone()));
    archive::write_manifest(
        &state.config.manifests_dir(),
        &archive::new_manifest(
            "framework-fixture",
            "0.11.1",
            BTreeMap::new(),
            BTreeMap::new(),
            vec![format!("{}/DIYLevel.dll", crate::framework::DIYLEVEL_DIR)],
            vec![],
        ),
    )
    .unwrap();
    let mut config = crate::config::default_config();
    config.game.path = game.to_string_lossy().into_owned();
    state.config.save(&config).unwrap();
    let request = PlanRequest {
        map_import: Some(MapImport {
            zip_path: zip.to_string_lossy().into_owned(),
            selected: vec![selected.dir_name.clone()],
            origin: None,
        }),
        ..Default::default()
    };
    let plan = planning::create(&state, request).unwrap();
    assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    assert_eq!(plan.result.actions.len(), 1);
    assert_eq!(plan.result.actions[0].kind, ActionKind::Upgrade);
    let mut session = state
        .mod_changes
        .lock()
        .unwrap()
        .plans
        .remove(&plan.plan_id)
        .unwrap();
    preparation::finish(&state, &mut session).unwrap();
    assert_eq!(fs::read(map.join("info")).unwrap(), b"previous bundle");
    session.plan.prepared = true;
    execution::commit(&state, &mut session, |_| {}).unwrap();
    assert!(!map.join("keep-until-commit").exists());
    assert_eq!(
        level_sets::read_info_metadata(&map.join("info")).unwrap(),
        selected.metadata
    );
}

#[test]
#[ignore = "需要本地 .tmp/plugin-zips/young.zip；单独运行，以便隔离地图解析缓存"]
fn real_young_info_update_reports_disabled_standalone_dll_without_writing_live_files() {
    let sample = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.tmp/plugin-zips/young.zip");
    let mut archive = zip::ZipArchive::new(fs::File::open(sample).unwrap()).unwrap();
    let info = (0..archive.len())
        .find_map(|index| {
            let mut entry = archive.by_index(index).unwrap();
            if !entry.name().is_ok_and(|name| name.ends_with("/info_young")) {
                return None;
            }
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            Some(bytes)
        })
        .expect("需要真实的 young 地图 info");

    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("OC2ModManager");
    let game = fs::canonicalize(temp.path()).unwrap().join("game");
    let package = game.join(crate::framework::DIYLEVEL_DIR);
    let old_map = level_sets::disabled_root(&game).join("young");
    let new_map = package.join("levels/young");
    let external = "BepInEx/plugins/YoungLevelRuntimeLoader/YoungLevels.dll";
    let standalone = mods::disabled_dll_file(external);
    let standalone_bytes = mods::plugin_blob("fixture.young-levels", "YoungLevelsRuntime", "1.3");
    write(&game, "BepInEx/core/BepInEx.dll", b"fixture loader");
    write(
        &package,
        "DIYLevel.dll",
        &mods::plugin_blob(mods::plugin_target::DIYLEVEL_GUID, "DIYLevel", "0.11.1"),
    );
    write(&old_map, "info_young", b"previous bundle");
    write(&old_map, "old-level", b"previous map bytes");
    write(&game, &standalone, &standalone_bytes);

    let zip = temp.path().join("young-update.zip");
    let mut writer = zip::ZipWriter::new(fs::File::create(&zip).unwrap());
    let candidate = mods::plugin_blob("fixture.young-levels", "YoungLevelsRuntime", "1.4");
    for (name, bytes) in [
        (
            "BepInEx/plugins/OC2DIYLevel/levels/young/info_young",
            info.as_slice(),
        ),
        (external, candidate.as_slice()),
    ] {
        writer
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap();

    fs::create_dir(&data).unwrap();
    crate::user_paths::freeze_root(&data).unwrap();
    let state = AppState::with_config(ConfigService::for_test(data.clone()));
    let mut config = crate::config::default_config();
    config.game.path = game.to_string_lossy().into_owned();
    state.config.save(&config).unwrap();
    let sets = level_sets::inspect_zip_with_progress(&zip, 1, 1, &|_, _, _| {}).unwrap();
    let [selected] = sets.as_slice() else {
        panic!("测试包中应只有一张地图");
    };
    let mut previous = selected.metadata.clone();
    previous.version = "0.4".into();
    level_sets::seed_info_metadata(&old_map.join("info_young"), previous);

    let error = planning::create(
        &state,
        PlanRequest {
            map_import: Some(MapImport {
                zip_path: zip.to_string_lossy().into_owned(),
                selected: vec![selected.dir_name.clone()],
                origin: None,
            }),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(
        error,
        format!("modChanges.fileOwnershipConflict?path={external}")
    );
    assert_eq!(
        fs::read(old_map.join("info_young")).unwrap(),
        b"previous bundle"
    );
    assert_eq!(
        fs::read(old_map.join("old-level")).unwrap(),
        b"previous map bytes"
    );
    assert_eq!(fs::read(game.join(standalone)).unwrap(), standalone_bytes);
    assert!(!game.join(external).exists());
    assert!(!new_map.exists());
    assert!(state.mod_changes.lock().unwrap().plans.is_empty());
    assert!(
        !state
            .config
            .manifests_dir()
            .join(level_sets::LEVEL_SET_PLUGIN_REGISTRY)
            .exists()
    );
}
