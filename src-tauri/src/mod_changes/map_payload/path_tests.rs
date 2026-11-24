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

fn isolated_map_replay(test: &str) -> bool {
    const CASE: &str = "OC2_REAL_MAP_REPLAY_CASE";
    let (_, module) = module_path!().split_once("::").unwrap();
    let name = format!("{module}::{test}");
    if std::env::var(CASE).as_deref() == Ok(name.as_str()) {
        return true;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", &name, "--nocapture"])
        .env(CASE, &name)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("1 passed; 0 failed"),
        "{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    false
}

fn rw_zip() -> PathBuf {
    let zip = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.tmp/rw.zip");
    assert!(zip.is_file(), "需要本地真实地图 ZIP：{}", zip.display());
    fs::canonicalize(zip).unwrap()
}

fn rw_fixture() -> (tempfile::TempDir, AppState, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("OC2ModManager");
    assert_ne!(
        data,
        fs::canonicalize(temp.path()).unwrap().join("OC2ModManager")
    );
    let game = fs::canonicalize(temp.path()).unwrap().join("game");
    let package = game.join(crate::framework::DIYLEVEL_DIR);
    write(&game, "BepInEx/core/BepInEx.dll", b"fixture loader");
    write(
        &package,
        "DIYLevel.dll",
        &mods::plugin_blob(mods::plugin_target::DIYLEVEL_GUID, "DIYLevel", "0.11.1"),
    );

    fs::create_dir(&data).unwrap();
    crate::user_paths::freeze_root(&data).unwrap();
    let state = AppState::with_config(ConfigService::for_test(data));
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
    (temp, state, package.join("levels"))
}

#[test]
#[ignore = "需要本地 .tmp/rw.zip"]
fn real_zip_update_accepts_normal_windows_data_path() {
    if !isolated_map_replay("real_zip_update_accepts_normal_windows_data_path") {
        return;
    }
    let zip = rw_zip();
    let (_temp, state, levels) = rw_fixture();
    let stage = level_sets::inspect_zip_with_progress(&zip, None, 1, 1, &|_, _, _| {}).unwrap();
    let [selected] = stage.as_slice() else {
        panic!("测试包中应只有一张地图");
    };
    let map = levels.join("rw-legacy-install");
    let mut previous = selected.metadata.clone();
    previous.version = "0.72".into();
    assert_ne!(previous.version, selected.metadata.version);
    write(&map, "info", b"previous bundle");
    write(&map, "keep-until-commit", b"previous map bytes");
    level_sets::seed_info_metadata(&map.join("info"), previous);

    let request = PlanRequest {
        map_import: Some(MapImport {
            zip_path: zip.to_string_lossy().into_owned(),
            download_id: None,
            selected: vec![selected.dir_name.clone()],
            origin: None,
        }),
        ..Default::default()
    };
    let plan = planning::create(&state, request).unwrap();
    assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    assert_eq!(plan.result.actions.len(), 1);
    assert_eq!(plan.result.actions[0].kind, ActionKind::Upgrade);
    assert_eq!(plan.result.actions[0].node_id, "map:rw-legacy-install");
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
    assert!(!levels.join("rw").exists());
    assert_eq!(
        level_sets::read_info_metadata(&map.join("info")).unwrap(),
        selected.metadata
    );
}

#[test]
#[ignore = "需要本地 .tmp/rw.zip"]
fn real_downloaded_zip_installs_under_its_recorded_asset_name() {
    if !isolated_map_replay("real_downloaded_zip_installs_under_its_recorded_asset_name") {
        return;
    }
    let (_temp, state, levels) = rw_fixture();
    let asset_name = "RW official.v0.73.ZIP";
    let receipt = uuid::Uuid::new_v4();
    let zip = state.config.cache_dir().join(format!("{receipt}.download"));
    fs::create_dir_all(zip.parent().unwrap()).unwrap();
    fs::copy(rw_zip(), &zip).unwrap();
    let sets =
        level_sets::inspect_zip_with_progress(&zip, Some(asset_name), 1, 1, &|_, _, _| {}).unwrap();
    let [selected] = sets.as_slice() else {
        panic!("测试包中应只有一张地图");
    };
    let scene = "s_rw_1";
    let mut scene_bytes = Vec::new();
    zip::ZipArchive::new(fs::File::open(&zip).unwrap())
        .unwrap()
        .by_name(scene)
        .unwrap()
        .read_to_end(&mut scene_bytes)
        .unwrap();
    state
        .downloaded_sources
        .record(
            receipt,
            &zip,
            Fingerprint::capture(&zip).unwrap(),
            crate::mod_changes::downloaded_source::Source::Map(None),
            asset_name,
        )
        .unwrap();
    let request = PlanRequest {
        map_import: Some(MapImport {
            zip_path: zip.to_string_lossy().into_owned(),
            download_id: Some(receipt),
            selected: vec![selected.dir_name.clone()],
            origin: None,
        }),
        ..Default::default()
    };
    let plan = planning::create(&state, request).unwrap();
    assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    assert_eq!(plan.result.actions.len(), 1);
    assert_eq!(plan.result.actions[0].kind, ActionKind::Install);
    let mut session = state
        .mod_changes
        .lock()
        .unwrap()
        .plans
        .remove(&plan.plan_id)
        .unwrap();
    preparation::finish(&state, &mut session).unwrap();
    let target = levels.join("RW official.v0.73");
    assert!(!target.exists());
    session.plan.prepared = true;
    execution::commit(&state, &mut session, |_| {}).unwrap();
    assert_eq!(
        level_sets::read_info_metadata(&target.join("info")).unwrap(),
        selected.metadata
    );
    assert_eq!(fs::read(target.join(scene)).unwrap(), scene_bytes);
    let installed = fs::read_dir(&levels)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(installed, ["RW official.v0.73"]);
}

#[tokio::test]
#[ignore = "需要本地 .tmp/plugin-zips/young.zip"]
async fn real_young_info_requires_download_approval_to_adopt_disabled_standalone_dll() {
    if !isolated_map_replay(
        "real_young_info_requires_download_approval_to_adopt_disabled_standalone_dll",
    ) {
        return;
    }
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
    let sets = level_sets::inspect_zip_with_progress(&zip, None, 1, 1, &|_, _, _| {}).unwrap();
    let [selected] = sets.as_slice() else {
        panic!("测试包中应只有一张地图");
    };
    let mut previous = selected.metadata.clone();
    previous.version = "0.4".into();
    level_sets::seed_info_metadata(&old_map.join("info_young"), previous);

    let request = |download_id| PlanRequest {
        map_import: Some(MapImport {
            zip_path: zip.to_string_lossy().into_owned(),
            download_id,
            selected: vec![selected.dir_name.clone()],
            origin: None,
        }),
        ..Default::default()
    };
    let error = planning::create(&state, request(None)).unwrap_err();
    assert_eq!(
        error,
        format!("modChanges.installTargetOwned?path={external}")
    );
    assert_eq!(
        fs::read(old_map.join("info_young")).unwrap(),
        b"previous bundle"
    );
    assert_eq!(
        fs::read(old_map.join("old-level")).unwrap(),
        b"previous map bytes"
    );
    assert_eq!(fs::read(game.join(&standalone)).unwrap(), standalone_bytes);
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
    let receipt = uuid::Uuid::new_v4();
    state
        .downloaded_sources
        .record(
            receipt,
            &zip,
            Fingerprint::capture(&zip).unwrap(),
            crate::mod_changes::downloaded_source::Source::Map(None),
            "young-update.zip",
        )
        .unwrap();
    for phase in ["cancel", "modified", "rollback", "counterpart", "commit"] {
        let plan = planning::create(&state, request(Some(receipt))).unwrap();
        assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
        let outcome = preparation::prepare(state.clone(), plan.plan_id.clone(), |_| {}).await;
        assert_eq!(outcome.error, None);
        let prepared = outcome.plan.unwrap();
        assert_ne!(prepared.plan_id, plan.plan_id);
        assert!(
            !state
                .mod_changes
                .lock()
                .unwrap()
                .plans
                .contains_key(&plan.plan_id)
        );
        let mut session = state
            .mod_changes
            .lock()
            .unwrap()
            .plans
            .remove(&prepared.plan_id)
            .unwrap();
        assert_eq!(session.plan.untracked_overwrites.len(), 1);
        assert_eq!(
            key(&session.plan.untracked_overwrites[0].path),
            key(&standalone)
        );
        match phase {
            "cancel" => {
                drop(session);
                assert_eq!(fs::read(game.join(&standalone)).unwrap(), standalone_bytes);
                assert_eq!(
                    fs::read(old_map.join("info_young")).unwrap(),
                    b"previous bundle"
                );
            }
            "modified" => {
                fs::write(game.join(&standalone), b"external change").unwrap();
                assert!(execution::commit(&state, &mut session, |_| {}).is_err());
                assert_eq!(
                    fs::read(old_map.join("info_young")).unwrap(),
                    b"previous bundle"
                );
                fs::write(game.join(&standalone), &standalone_bytes).unwrap();
            }
            "rollback" => {
                crate::archive::start_download();
                let mut saw_dll_write = false;
                let result = execution::commit(&state, &mut session, |_| {
                    if fs::read(game.join(&standalone)).is_ok_and(|bytes| bytes == candidate) {
                        saw_dll_write = true;
                        crate::archive::cancel_download();
                    }
                });
                crate::archive::start_download();
                assert!(saw_dll_write);
                assert_eq!(result.unwrap_err(), "download.cancelled");
                assert_eq!(fs::read(game.join(&standalone)).unwrap(), standalone_bytes);
                assert_eq!(
                    fs::read(old_map.join("info_young")).unwrap(),
                    b"previous bundle"
                );
                assert!(
                    level_sets::read_registered_plugins(&state.config.manifests_dir()).is_empty()
                );
            }
            "counterpart" => {
                let mut injected = false;
                let result = execution::commit(&state, &mut session, |_| {
                    if !injected
                        && fs::read(game.join(&standalone)).is_ok_and(|bytes| bytes == candidate)
                    {
                        write(&game, external, b"outside active copy");
                        injected = true;
                    }
                });
                assert!(injected);
                assert_eq!(
                    result.unwrap_err(),
                    format!(
                        "modChanges.installTargetHasBothStates?paths={external}%0A{external}.disabled"
                    )
                );
                assert_eq!(
                    fs::read(game.join(external)).unwrap(),
                    b"outside active copy"
                );
                assert_eq!(fs::read(game.join(&standalone)).unwrap(), standalone_bytes);
                assert_eq!(
                    fs::read(old_map.join("info_young")).unwrap(),
                    b"previous bundle"
                );
                assert!(
                    level_sets::read_registered_plugins(&state.config.manifests_dir()).is_empty()
                );
                fs::remove_file(game.join(external)).unwrap();
            }
            _ => {
                execution::commit(&state, &mut session, |_| {}).unwrap();
                assert_eq!(fs::read(game.join(&standalone)).unwrap(), candidate);
                assert!(!game.join(external).exists());
                assert!(!new_map.exists());
                assert_eq!(fs::read(old_map.join("info_young")).unwrap(), info);
                let registered = level_sets::read_registered_plugins(&state.config.manifests_dir());
                assert_eq!(registered.len(), 1);
                assert_eq!(key(&registered[0].dll), key(external));
            }
        }
    }
}
