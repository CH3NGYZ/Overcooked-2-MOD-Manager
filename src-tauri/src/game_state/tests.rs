use super::*;
use crate::models::{BepInExInfo, WinhttpState};
use std::io::Write;

fn snapshot() -> GameInfo {
    GameInfo {
        path: Some("game".into()),
        steam_library: None,
        steam_root: None,
        wine_prefix: None,
        game_exists: true,
        bepinex: BepInExInfo {
            installed: false,
            has_residue: false,
            has_config: false,
            version: None,
            version_source: None,
            backups: Vec::new(),
        },
        kitchen_running: false,
        winhttp_state: WinhttpState::Missing,
    }
}

#[test]
fn stable_game_state_does_not_repeat_events() {
    let info = snapshot();
    let mut previous = None;
    assert_eq!(
        changed_snapshot(&mut previous, info.clone()),
        Some(info.clone())
    );
    for _ in 0..10 {
        assert!(changed_snapshot(&mut previous, info.clone()).is_none());
    }
}

#[test]
fn game_start_stop_and_restart_each_publish_once() {
    let mut info = snapshot();
    let mut previous = Some(info.clone());
    for running in [true, false, true] {
        info.kitchen_running = running;
        assert_eq!(
            changed_snapshot(&mut previous, info.clone()),
            Some(info.clone())
        );
        assert!(changed_snapshot(&mut previous, info.clone()).is_none());
    }
}

#[test]
fn framework_changes_publish_without_losing_game_fields() {
    let mut info = snapshot();
    let mut previous = Some(info.clone());
    info.bepinex.installed = true;
    info.winhttp_state = WinhttpState::ModEnabled;
    let event = changed_snapshot(&mut previous, info).unwrap();
    assert_eq!(event.path.as_deref(), Some("game"));
    assert!(event.bepinex.installed);
    assert_eq!(event.winhttp_state, WinhttpState::ModEnabled);
    assert!(changed_snapshot(&mut previous, event).is_none());
}

#[test]
fn framework_file_changes_invalidate_the_snapshot() {
    let root = std::env::temp_dir().join(format!("oc2-game-state-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("Overcooked2.exe"), []).unwrap();
    let settings = GameConfig {
        path: root.to_string_lossy().into_owned(),
        runtime: crate::models::GameRuntime::Wine,
    };
    let before = game::detect_game_from_settings(&settings);
    let backups = root.join("backups");
    let original_stamps = game_file_stamps(Some(&before), &backups);
    assert!(original_stamps == game_file_stamps(Some(&before), &backups));
    fs::create_dir_all(root.join("BepInEx/core")).unwrap();
    fs::write(root.join("BepInEx/core/BepInEx.dll"), []).unwrap();
    assert!(original_stamps != game_file_stamps(Some(&before), &backups));
    let after = game::detect_game_from_settings(&settings);
    assert!(after.bepinex.installed);
    let mut previous = Some(before);
    assert!(changed_snapshot(&mut previous, after).is_some());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn framework_config_creation_and_removal_invalidate_and_publish_once() {
    let root = std::env::temp_dir().join(format!("oc2-framework-config-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(root.join("BepInEx/config")).unwrap();
    let settings = GameConfig {
        path: root.to_string_lossy().into_owned(),
        runtime: crate::models::GameRuntime::Wine,
    };
    let backups = root.join("backups");
    let before = game::detect_game_from_settings(&settings);
    assert!(!before.bepinex.has_config);
    let mut previous = Some(before.clone());
    let mut stamps = game_file_stamps(Some(&before), &backups);
    let config = root.join("BepInEx/config/BepInEx.cfg");

    for exists in [true, false] {
        if exists {
            fs::write(&config, "[Logging.Console]\nEnabled = false\n").unwrap();
        } else {
            fs::remove_file(&config).unwrap();
        }
        let next_stamps = game_file_stamps(Some(&before), &backups);
        assert!(stamps != next_stamps);
        let next = game::detect_game_from_settings(&settings);
        assert_eq!(next.bepinex.has_config, exists);
        let event = changed_snapshot(&mut previous, next.clone()).unwrap();
        assert_eq!(event.bepinex.has_config, exists);
        assert!(changed_snapshot(&mut previous, next).is_none());
        assert!(next_stamps == game_file_stamps(Some(&before), &backups));
        stamps = next_stamps;
    }

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn framework_enable_disable_renames_invalidate_and_publish_once() {
    let root = std::env::temp_dir().join(format!("oc2-winhttp-observer-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("Overcooked2.exe"), []).unwrap();
    fs::write(root.join("winhttp.dll"), b"loader").unwrap();
    let settings = GameConfig {
        path: root.to_string_lossy().into_owned(),
        runtime: crate::models::GameRuntime::Wine,
    };
    let backups = root.join("backups");
    let enabled = game::detect_game_from_settings(&settings);
    assert_eq!(enabled.winhttp_state, WinhttpState::ModEnabled);
    let mut previous = Some(enabled.clone());
    let mut stamps = game_file_stamps(Some(&enabled), &backups);
    assert!(
        stamps
            .iter()
            .any(|(path, _)| path == &root.join("winhttp.dll.disabled"))
    );

    for (source, destination, expected) in [
        (
            "winhttp.dll",
            "winhttp.dll.disabled",
            WinhttpState::ModDisabled,
        ),
        (
            "winhttp.dll.disabled",
            "winhttp.dll",
            WinhttpState::ModEnabled,
        ),
    ] {
        fs::rename(root.join(source), root.join(destination)).unwrap();
        let next = game::detect_game_from_settings(&settings);
        let next_stamps = game_file_stamps(Some(&next), &backups);
        assert!(stamps != next_stamps);
        assert_eq!(next.winhttp_state, expected);
        assert_eq!(
            changed_snapshot(&mut previous, next.clone()),
            Some(next.clone())
        );
        assert!(changed_snapshot(&mut previous, next.clone()).is_none());
        assert!(next_stamps == game_file_stamps(Some(&next), &backups));
        stamps = next_stamps;
    }

    fs::remove_file(root.join("Overcooked2.exe")).unwrap();
    fs::remove_file(root.join("winhttp.dll")).unwrap();
    fs::remove_dir(root).unwrap();
}

#[test]
fn appending_game_logs_does_not_trigger_framework_detection() {
    let root = std::env::temp_dir().join(format!("oc2-game-log-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(root.join("BepInEx/core")).unwrap();
    fs::write(root.join("Overcooked2.exe"), []).unwrap();
    fs::write(root.join("BepInEx/core/BepInEx.dll"), []).unwrap();
    let log = root.join("BepInEx/LogOutput.log");
    fs::write(&log, "existing game log\n").unwrap();
    let mut info = snapshot();
    info.path = Some(root.to_string_lossy().into_owned());
    let backups = root.join("backups");
    let original = game_file_stamps(Some(&info), &backups);
    fs::OpenOptions::new()
        .append(true)
        .open(log)
        .unwrap()
        .write_all(b"next game log record\n")
        .unwrap();
    assert!(original == game_file_stamps(Some(&info), &backups));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn missing_game_probe_notices_a_later_installation_without_repeating_changes() {
    let root = std::env::temp_dir().join(format!("oc2-discovery-{}", uuid::Uuid::new_v4()));
    let steam_roots = vec![root.clone()];
    let executable = root.join("steamapps/common/Overcooked! 2/Overcooked2.exe");
    let mut observer = GameDiscoveryObserver::default();
    assert!(observer.refresh(&steam_roots, None, "Overcooked2.exe"));
    for _ in 0..10 {
        assert!(!observer.refresh(&steam_roots, None, "Overcooked2.exe"));
    }
    fs::create_dir_all(executable.parent().unwrap()).unwrap();
    fs::write(&executable, []).unwrap();
    assert!(observer.refresh(&steam_roots, None, "Overcooked2.exe"));
    assert!(!observer.refresh(&steam_roots, None, "Overcooked2.exe"));
    fs::remove_file(&executable).unwrap();
    assert!(observer.refresh(&steam_roots, None, "Overcooked2.exe"));
    assert!(!observer.refresh(&steam_roots, None, "Overcooked2.exe"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn missing_game_probe_notices_a_new_library_index() {
    let root = std::env::temp_dir().join(format!("oc2-library-{}", uuid::Uuid::new_v4()));
    let steam = root.join("Steam");
    let library = root.join("additional-library");
    let executable = library.join("steamapps/common/Overcooked! 2/Overcooked2.exe");
    fs::create_dir_all(executable.parent().unwrap()).unwrap();
    fs::write(&executable, []).unwrap();
    let steam_roots = vec![steam.clone()];
    let mut observer = GameDiscoveryObserver::default();
    assert!(observer.refresh(&steam_roots, None, "Overcooked2.exe"));
    assert!(!observer.refresh(&steam_roots, None, "Overcooked2.exe"));
    fs::create_dir_all(steam.join("steamapps")).unwrap();
    let escaped_library = library.to_string_lossy().replace('\\', "\\\\");
    fs::write(
        steam.join("steamapps/libraryfolders.vdf"),
        format!("\"libraryfolders\" {{ \"0\" {{ \"path\" \"{escaped_library}\" }} }}"),
    )
    .unwrap();
    assert!(observer.refresh(&steam_roots, None, "Overcooked2.exe"));
    assert!(
        observer
            .candidates
            .contains(&fs::canonicalize(&executable).unwrap())
    );
    assert!(!observer.refresh(&steam_roots, None, "Overcooked2.exe"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn configured_missing_directory_notices_a_later_game_executable() {
    let root = std::env::temp_dir().join(format!("oc2-configured-{}", uuid::Uuid::new_v4()));
    let settings = GameConfig {
        path: root.to_string_lossy().into_owned(),
        runtime: crate::models::GameRuntime::Wine,
    };
    let before = game::detect_game_from_settings(&settings);
    assert!(!before.game_exists);
    let before_stamps = game_file_stamps(Some(&before), &root.join("backups"));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("Overcooked2.exe"), []).unwrap();
    assert!(before_stamps != game_file_stamps(Some(&before), &root.join("backups")));
    let after = game::detect_game_from_settings(&settings);
    assert!(after.game_exists);
    assert!(changed_snapshot(&mut Some(before), after).is_some());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn discovery_runtime_switch_replaces_executable_candidates_without_repeated_events() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/verification/discovery-runtime")
        .join(uuid::Uuid::new_v4().to_string());
    let mut observer = GameDiscoveryObserver::default();
    let steam_roots = vec![root];
    // 只覆盖本项目会产出的两种主程序名；`Overcooked2.x86_64` 随 Linux 支持移除。
    for executable in ["Overcooked2.exe", "Overcooked2.app"] {
        assert!(observer.refresh(&steam_roots, None, executable));
        assert!(
            observer
                .candidates
                .iter()
                .all(|path| path.ends_with(executable))
        );
        assert!(!observer.refresh(&steam_roots, None, executable));
    }
}

#[test]
fn game_stamps_notice_the_actual_native_mac_bundle_and_contents() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/verification/native-mac-observer")
        .join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(&root).unwrap();
    let mut info = snapshot();
    info.path = Some(root.to_string_lossy().into_owned());
    let backups = root.join("backups");
    let before = game_file_stamps(Some(&info), &backups);
    let app = root.join("Overcooked2.app");
    fs::create_dir_all(&app).unwrap();
    let bundled = game_file_stamps(Some(&info), &backups);
    assert!(before != bundled);
    assert!(
        bundled
            .iter()
            .any(|(path, stamp)| path == &app && stamp.is_some())
    );
    let contents = app.join("Contents/MacOS");
    fs::create_dir_all(&contents).unwrap();
    let complete = game_file_stamps(Some(&info), &backups);
    assert!(bundled != complete);
    assert!(
        complete
            .iter()
            .any(|(path, stamp)| path == &contents && stamp.is_some())
    );
    fs::remove_dir_all(root).unwrap();
}
