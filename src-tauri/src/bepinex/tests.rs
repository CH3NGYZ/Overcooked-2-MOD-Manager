use super::*;

#[test]
fn package_selection_keeps_native_and_wine_separate_on_each_platform() {
    for (os, runtime, expected, executable) in [
        (
            "windows",
            GameRuntime::Native,
            Package::Windows,
            "Overcooked2.exe",
        ),
        (
            "windows",
            GameRuntime::Wine,
            Package::Windows,
            "Overcooked2.exe",
        ),
        (
            // 任何非 macOS 平台都按 Windows 包处理：这是 `for_platform`
            // 的兜底分支，Linux 发布目标移除后仍要覆盖它。
            "freebsd",
            GameRuntime::Native,
            Package::Windows,
            "Overcooked2.exe",
        ),
        (
            "macos",
            GameRuntime::Wine,
            Package::Windows,
            "Overcooked2.exe",
        ),
        (
            "macos",
            GameRuntime::Native,
            Package::MacOS,
            "Overcooked2.app",
        ),
    ] {
        let package = Package::for_platform(os, runtime);
        assert_eq!(package, expected);
        assert_eq!(package.executable(), executable);
        assert!(package.url(&Provider::Github).contains(match package {
            Package::Windows => "BepInEx_win_x86_",
            Package::MacOS => "BepInEx_macos_universal_",
        }));
    }
    for package in [Package::Windows, Package::MacOS] {
        assert_eq!(
            package.url(&Provider::Github).rsplit('/').next(),
            package.url(&Provider::Gitee).rsplit('/').next(),
        );
        assert_eq!(package.digest().len(), 71);
    }
}

#[test]
fn native_scripts_configure_the_executable_and_toggle_without_renaming_loaders() {
    let script = "#!/bin/sh\nexecutable_name=\"\"\nenabled=\"1\"\n  enabled=\"$2\"\nexec \"$executable_name\" \"$@\"\n";
    for package in [Package::MacOS] {
        let configured =
            replace_assignment(script, "executable_name", package.executable()).unwrap();
        assert!(configured.contains(&format!("executable_name=\"{}\"", package.executable())));
        let disabled = set_script_enabled(&configured, false).unwrap();
        assert_eq!(script_enabled(&disabled), Some(false));
        assert!(disabled.contains("  enabled=\"$2\""));
        assert_eq!(
            script_enabled(&set_script_enabled(&disabled, true).unwrap()),
            Some(true)
        );
    }
    assert!(
        replace_assignment(
            "executable_name=\"a\"\nexecutable_name=\"b\"",
            "executable_name",
            "c"
        )
        .is_err()
    );
    assert!(set_script_enabled("unrecognized script", true).is_err());
}

#[test]
fn macos_overlay_repair_restores_steam_libraries_after_doorstop_setup() {
    let script = "#!/bin/sh\nexecutable_name=\"\"\nenabled=\"1\"\nexport DYLD_LIBRARY_PATH=\"${doorstop_directory}:${DYLD_LIBRARY_PATH}\"\nif [ -z \"$DYLD_INSERT_LIBRARIES\" ]; then\n    export DYLD_INSERT_LIBRARIES=\"${doorstop_name}\"\nelse\n    export DYLD_INSERT_LIBRARIES=\"${doorstop_name}:${DYLD_INSERT_LIBRARIES}\"\nfi\n";
    let repaired = repair_macos_overlay_script(script).unwrap();
    let steam = repaired.find("$STEAM_DYLD_INSERT_LIBRARIES").unwrap();
    let doorstop = repaired
        .find("${doorstop_name}:${DYLD_INSERT_LIBRARIES}")
        .unwrap();
    assert!(
        repaired.contains(
            "[ -z \"$DYLD_INSERT_LIBRARIES\" ] && [ -n \"$STEAM_DYLD_INSERT_LIBRARIES\" ]"
        )
    );
    assert!(steam < doorstop);
    assert_eq!(repair_macos_overlay_script(&repaired).unwrap(), repaired);
    assert!(repair_macos_overlay_script("unrecognized").is_err());
    assert!(repair_macos_overlay_script("# STEAM_DYLD_INSERT_LIBRARIES\n").is_err());
    let comment = format!("# STEAM_DYLD_INSERT_LIBRARIES is not a repair\n{script}");
    assert!(
        repair_macos_overlay_script(&comment)
            .unwrap()
            .contains("export DYLD_INSERT_LIBRARIES=\"$STEAM_DYLD_INSERT_LIBRARIES\"")
    );
    assert_eq!(
        repair_macos_overlay_script(&script.replace('\n', "\r\n")).unwrap(),
        repaired
    );
    assert!(repair_macos_overlay_script(&format!("{script}{script}")).is_err());
    assert!(
        repair_macos_overlay_script(&script.replace(
            "${doorstop_name}:${DYLD_INSERT_LIBRARIES}",
            "${doorstop_name}"
        ))
        .is_err()
    );
}

#[cfg(unix)]
#[test]
fn macos_overlay_loader_order_preserves_existing_dyld_or_restores_steam() {
    let script = "export DYLD_LIBRARY_PATH=\"${doorstop_directory}:${DYLD_LIBRARY_PATH}\"\nif [ -z \"$DYLD_INSERT_LIBRARIES\" ]; then\n    export DYLD_INSERT_LIBRARIES=\"${doorstop_name}\"\nelse\n    export DYLD_INSERT_LIBRARIES=\"${doorstop_name}:${DYLD_INSERT_LIBRARIES}\"\nfi\n";
    let repaired = repair_macos_overlay_script(script).unwrap();
    for (dyld, steam, expected) in [
        ("", "", "libdoorstop.dylib"),
        (
            "",
            "/Steam/loader.dylib:/Steam/overlay.dylib",
            "libdoorstop.dylib:/Steam/loader.dylib:/Steam/overlay.dylib",
        ),
        (
            "/custom/library.dylib",
            "/Steam/overlay.dylib",
            "libdoorstop.dylib:/custom/library.dylib",
        ),
        (
            "/custom/library.dylib",
            "",
            "libdoorstop.dylib:/custom/library.dylib",
        ),
    ] {
        // Assign inside the shell to model the environment the script sees after SIP.
        let output = std::process::Command::new("/bin/sh")
                .arg("-c")
                .arg(format!("doorstop_name=libdoorstop.dylib\nDYLD_INSERT_LIBRARIES=\"$1\"\nSTEAM_DYLD_INSERT_LIBRARIES=\"$2\"\n{repaired}printf '%s' \"$DYLD_INSERT_LIBRARIES\""))
                .args(["overlay-test", dyld, steam])
                .output()
                .unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
    }
}

#[test]
fn older_doorstop_scripts_and_crlf_are_supported() {
    let script = "#!/bin/sh\r\nexecutable_name=\"\";\r\nexport DOORSTOP_ENABLE=TRUE\r\n";
    let changed = replace_assignment(script, "executable_name", "Overcooked2.app").unwrap();
    let changed = set_script_enabled(&changed, false).unwrap();
    assert!(!changed.contains('\r'));
    assert_eq!(script_enabled(&changed), Some(false));
}

#[test]
fn native_installer_prepares_a_real_script_and_loader_state() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/verification/native-bepinex")
        .join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(&root).unwrap();
    fs::write(
            root.join("run_bepinex.sh"),
            "#!/bin/sh\nexecutable_name=\"\"\nenabled=\"1\"\nexport DYLD_LIBRARY_PATH=\"${doorstop_directory}:${DYLD_LIBRARY_PATH}\"\nif [ -z \"$DYLD_INSERT_LIBRARIES\" ]; then\n    export DYLD_INSERT_LIBRARIES=\"${doorstop_name}\"\nelse\n    export DYLD_INSERT_LIBRARIES=\"${doorstop_name}:${DYLD_INSERT_LIBRARIES}\"\nfi\n",
        )
        .unwrap();
    fs::write(root.join("libdoorstop.dylib"), b"mac fixture").unwrap();
    for package in [Package::MacOS] {
        prepare_native(&root, package).unwrap();
        assert_eq!(native_state(&root, package), WinhttpState::ModEnabled);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_ne!(
                fs::metadata(root.join("run_bepinex.sh"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o100,
                0
            );
        }
    }
    let script_path = root.join("run_bepinex.sh");
    let disabled = set_script_enabled(&fs::read_to_string(&script_path).unwrap(), false).unwrap();
    fs::write(&script_path, &disabled).unwrap();
    repair_macos_overlay(&root).unwrap();
    repair_macos_overlay(&root).unwrap();
    assert_eq!(fs::read_to_string(&script_path).unwrap(), disabled);
    assert_eq!(
        native_state(&root, Package::MacOS),
        WinhttpState::ModDisabled
    );
    fs::write(&script_path, "unrecognized").unwrap();
    assert!(repair_macos_overlay(&root).is_err());
    assert_eq!(fs::read_to_string(&script_path).unwrap(), "unrecognized");
    fs::remove_file(root.join("run_bepinex.sh")).unwrap();
    assert!(prepare_native(&root, Package::MacOS).is_err());
    fs::remove_dir_all(root).unwrap();
}
