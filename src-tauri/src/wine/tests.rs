use super::*;

#[test]
fn windows_steam_paths_resolve_inside_their_own_prefix() {
    let prefix = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/verification/wine-paths");
    let steam = prefix.join("drive_c/Program Files (x86)/Steam");
    for path in [
        r"C:\\Program Files (x86)\\Steam",
        r"c:\Program Files (x86)\Steam",
    ] {
        assert_eq!(host_library_path(&steam, path), Some(steam.clone()));
    }
    assert_eq!(
        prefix_for_path(&steam.join("steamapps/common/Overcooked! 2")),
        Some(prefix)
    );
    for path in [
        r"C:Steam",
        r"C:\..\outside",
        r"C:\folder:stream",
        "relative",
        "/host/Steam",
    ] {
        assert!(host_library_path(&steam, path).is_none(), "{path}");
    }
}

#[cfg(unix)]
#[test]
fn secondary_drive_mappings_use_wine_dosdevices_not_host_drive_letters() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/verification/wine-drives")
        .join(uuid::Uuid::new_v4().to_string());
    let prefix = root.join("prefix");
    let library = root.join("library");
    std::fs::create_dir_all(prefix.join("dosdevices")).unwrap();
    std::fs::create_dir_all(&library).unwrap();
    std::os::unix::fs::symlink(&library, prefix.join("dosdevices/d:")).unwrap();
    let steam = prefix.join("drive_c/Steam");
    assert_eq!(
        host_library_path(&steam, r"D:\SteamLibrary"),
        Some(library.join("SteamLibrary"))
    );
    assert!(host_library_path(&steam, r"E:\SteamLibrary").is_none());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn wine_steam_discovery_does_not_accept_native_steam_or_empty_prefixes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/verification/wine-steam")
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(root.join("drive_c/Steam")).unwrap();
    assert!(steam_roots_in_prefix(&root).is_empty());
    std::fs::write(root.join("drive_c/Steam/steam.exe"), b"fixture").unwrap();
    assert_eq!(
        steam_roots_in_prefix(&root),
        vec![root.join("drive_c/Steam")]
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn wine_winhttp_override_preserves_unrelated_libraries() {
    assert_eq!(dll_overrides(""), "winhttp=n,b");
    assert_eq!(
        dll_overrides("d3d11=n;WinHTTP,dxgi=b;version=n,b"),
        "d3d11=n;dxgi=b;version=n,b;winhttp=n,b"
    );
}

#[test]
fn persistent_override_uses_wine_registry_without_shell_interpolation() {
    let mut command = std::process::Command::new("wine");
    let prefix =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("target/verification/Prefix with spaces");
    command.env("WINEPREFIX", &prefix);
    let command = winhttp_registry_command(command);
    let args = command.get_args().collect::<Vec<_>>();
    assert_eq!(args[0], "reg.exe");
    assert_eq!(args[2], r"HKEY_CURRENT_USER\Software\Wine\DllOverrides");
    assert_eq!(args[4], "winhttp");
    assert_eq!(args[8], "native,builtin");
    assert!(
        command
            .get_envs()
            .any(|(key, value)| { key == "WINEPREFIX" && value == Some(prefix.as_os_str()) })
    );
}
