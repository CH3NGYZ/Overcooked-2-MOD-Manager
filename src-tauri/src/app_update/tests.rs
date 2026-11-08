use super::*;

fn target(os: &str, arch: &str, kind: &str) -> UpdateTarget {
    UpdateTarget::from_parts(os, arch, Some(kind)).unwrap()
}

#[test]
fn selects_the_supplied_platform_architecture_and_package_names() {
    let version = Version::new(1, 2, 0);
    let cases = [
        (
            target("windows", "x86_64", "exe"),
            "OC2ModManager_1.2.0_x64-setup.exe",
        ),
        (
            target("windows", "x86_64", "msi"),
            "OC2ModManager_1.2.0_x64_en-US.msi",
        ),
        (
            target("macos", "aarch64", "app"),
            "OC2ModManager_aarch64.app.tar.gz",
        ),
        (
            target("macos", "x86_64", "app"),
            "OC2ModManager_x64.app.tar.gz",
        ),
    ];
    for (selected, name) in cases {
        assert!(
            selected.matches_name(name, &version),
            "{selected:?}: {name}"
        );
        for (other, other_name) in cases {
            if other != selected {
                assert!(
                    !selected.matches_name(other_name, &version),
                    "wrong platform: {other_name}"
                );
            }
        }
    }
}

#[test]
fn mac_updates_use_an_arch_specific_app_archive_not_a_dmg() {
    let arm = target("macos", "aarch64", "app");
    let intel = target("macos", "x86_64", "app");
    let version = Version::new(1, 2, 0);
    assert!(intel.matches_name("OC2ModManager_1.2.0_x86_64.app.tar.gz", &version));
    assert!(arm.matches_name("OC2ModManager_1.2.0_aarch64.app.tar.gz", &version));
    for name in [
        "OC2ModManager_1.2.0_aarch64.dmg",
        "OC2ModManager.app.tar.gz",
        "OC2ModManager_x64.app.tar.gz.sig",
    ] {
        assert!(!arm.matches_name(name, &version));
    }
}

#[test]
fn rejects_other_versions_and_keeps_msi_locale_separate_from_app_version() {
    let msi = target("windows", "x86_64", "msi");
    let version = Version::new(1, 2, 0);
    assert!(msi.matches_name("OC2ModManager_1.2.0_x64_en-US.msi", &version));
    assert!(msi.matches_name("OC2ModManager_1.2.0_amd64_zh-CN.msi", &version));
    assert!(!msi.matches_name("OC2ModManager_1.2.0_arm64_en-US.msi", &version));
    assert!(!msi.matches_name("OC2ModManager_1.2.1_x64_en-US.msi", &version));
    assert!(!msi.matches_name("OC2ModManager_1.2.0_x64_.msi", &version));
    assert!(
        !target("windows", "x86_64", "exe")
            .matches_name("OC2ModManager_1.2.0_x64-setup.msi", &version)
    );
}

#[test]
fn manifest_keys_never_mix_windows_packages_or_mac_architectures() {
    for (os, arch, kind, key) in [
        ("windows", "x86_64", "exe", "windows-x86_64-nsis"),
        ("windows", "x86_64", "msi", "windows-x86_64-msi"),
        ("macos", "aarch64", "app", "darwin-aarch64-app"),
        ("macos", "x86_64", "app", "darwin-x86_64-app"),
    ] {
        assert_eq!(target(os, arch, kind).manifest_key(), key);
    }
    // Linux 的三个安装格式随发布目标一起移除：构造它们必须失败。
    assert!(UpdateTarget::from_parts("linux", "x86_64", Some("deb")).is_err());
    assert!(UpdateTarget::from_parts("linux", "x86_64", None).is_err());
    assert!(UpdateTarget::from_parts("windows", "i686", None).is_err());
}

#[test]
fn tauri_windows_arguments_preserve_paths_and_only_use_success_hook_relaunch() {
    use std::path::Path;
    let executable = Path::new("C:/Users/Test User/管理器/OC2ModManager.exe");
    let log = Path::new("C:/Users/Test User/OC2ModManager/logs/update-msi.log");
    let nsis = windows_installer_args("exe", executable, executable, log).unwrap();
    assert_eq!(nsis, ["/OC2AutoLaunch", "/D=C:/Users/Test User/管理器"]);
    let raw = nsis.join(std::ffi::OsStr::new(" "));
    assert_eq!(raw, "/OC2AutoLaunch /D=C:/Users/Test User/管理器");
    assert!(!raw.to_string_lossy().contains('"'));
    assert_eq!(
        windows_installer_args("msi", executable, executable, log).unwrap(),
        [
            "AUTOLAUNCHAPP=1",
            "INSTALLDIR=\"C:/Users/Test User/管理器\"",
            "/norestart",
            "/L*v",
            "\"C:/Users/Test User/OC2ModManager/logs/update-msi.log\"",
        ]
    );
    for (kind, exe, log) in [
        ("app", executable, log),
        ("exe", Path::new("OC2ModManager.exe"), log),
        ("exe", Path::new("C:/bad\"path/OC2ModManager.exe"), log),
        ("msi", executable, Path::new("C:/bad\npath/update.log")),
    ] {
        assert!(windows_installer_args(kind, exe, exe, log).is_err());
    }
}

#[test]
fn relocation_arguments_keep_the_old_directory_for_installer_safety_hooks() {
    use std::path::Path;
    let executable = Path::new("C:/game/BepInEx/plugins/Manager/OC2ModManager.exe");
    let target = Path::new("C:/Users/Test/AppData/Local/OC2ModManager/OC2ModManager.exe");
    let log = Path::new("C:/Users/Test/OC2ModManager/logs/update-msi.log");
    for kind in ["exe", "msi"] {
        let args = windows_installer_args(kind, executable, target, log).unwrap();
        assert!(
            args.iter()
                .any(|arg| arg.to_string_lossy().contains("BepInEx/plugins/Manager"))
        );
        assert!(
            args.iter()
                .any(|arg| arg == "/OC2AutoLaunch" || arg == "AUTOLAUNCHAPP=1")
        );
        assert!(!args.iter().any(|arg| arg == "/R" || arg == "/ARGS"));
        if kind == "msi" {
            assert!(
                args.iter()
                    .any(|arg| arg == "INSTALLDIR=\"C:/Users/Test/AppData/Local/OC2ModManager\"")
            );
            assert!(
                args.iter()
                    .any(|arg| arg == "OC2MIGRATEFROM=\"C:/game/BepInEx/plugins/Manager\"")
            );
        }
    }
    let alias = Path::new("C:/game/alias/Manager/OC2ModManager.exe");
    assert!(
        windows_installer_args("msi", alias, target, log)
            .unwrap()
            .iter()
            .any(|arg| arg == "OC2MIGRATEFROM=\"C:/game/alias/Manager\"")
    );
}

#[test]
fn tauri_manifest_metadata_covers_all_desktop_formats_and_rejects_mismatches() {
    let cases = [
        (
            "windows",
            "x86_64",
            "exe",
            "OC2ModManager_1.2.0_x64-setup.exe",
        ),
        (
            "windows",
            "x86_64",
            "msi",
            "OC2ModManager_1.2.0_x64_en-US.msi",
        ),
        (
            "macos",
            "aarch64",
            "app",
            "OC2ModManager_aarch64.app.tar.gz",
        ),
        ("macos", "x86_64", "app", "OC2ModManager_x64.app.tar.gz"),
    ];
    let platforms = cases
        .iter()
        .map(|(os, arch, kind, name)| {
            (
                target(os, arch, kind).manifest_key(),
                serde_json::json!({
                    "url": format!("https://example.com/releases/download/v1.2.0/{name}"),
                    "signature": TEST_SIGNATURE,
                }),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    let release: tauri_plugin_updater::RemoteRelease = serde_json::from_value(serde_json::json!({
        "version": "1.2.0", "platforms": platforms,
    }))
    .unwrap();
    for (os, arch, kind, name) in cases {
        let key = target(os, arch, kind).manifest_key();
        let url = release.download_url(&key).unwrap();
        let signature = release.signature(&key).unwrap();
        let info: AppUpdateInfo = serde_json::from_value(serde_json::json!({
            "version": "1.2.0", "current_version": "1.1.0",
            "installer_name": name, "installer_type": kind,
            "installer_url": url, "installer_signature": signature,
        }))
        .unwrap();
        assert!(validate_update_metadata("1.2.0", url, signature, &info).is_ok());
        assert!(validate_update_metadata("1.2.1", url, signature, &info).is_err());
        assert!(validate_update_metadata("1.2.0", url, " ", &info).is_err());
        assert!(validate_update_metadata("1.2.0", url, "changed signature", &info).is_err());
        let changed_host = url::Url::parse(&format!("https://other.example/{name}")).unwrap();
        assert!(validate_update_metadata("1.2.0", &changed_host, signature, &info).is_err());
        let other = url::Url::parse("https://example.com/wrong-platform.exe").unwrap();
        assert!(validate_update_metadata("1.2.0", &other, signature, &info).is_err());
    }
    assert!(release.download_url("android-aarch64").is_err());
    assert!(release.download_url("ios-aarch64").is_err());
}

// Public fixtures generated solely for this test; no production private key.

#[test]
fn verifies_real_signatures_payloads_and_authenticated_versions() {
    let bytes = b"OC2 signed update fixture";
    assert!(verify_with_key(bytes, TEST_SIGNATURE, TEST_KEY, "1.2.0").is_ok());
    assert!(verify_with_key(bytes, TEST_SIGNATURE, TEST_KEY, "v1.2.0").is_ok());
    assert!(verify_with_key(b"tampered", TEST_SIGNATURE, TEST_KEY, "1.2.0").is_err());
    assert!(verify_with_key(bytes, TEST_SIGNATURE, TEST_KEY, "9.0.0").is_err());
    let comment = String::from_utf8(B64.decode(TEST_SIGNATURE).unwrap()).unwrap();
    for changed in [
        comment.replace("version:1.2.0", "version:9.0.0"),
        comment.replace("\tversion:1.2.0", ""),
    ] {
        assert!(verify_with_key(bytes, &B64.encode(changed), TEST_KEY, "9.0.0").is_err());
    }
    assert!(verify_with_key(bytes, "not a signature", TEST_KEY, "1.2.0").is_err());
}
