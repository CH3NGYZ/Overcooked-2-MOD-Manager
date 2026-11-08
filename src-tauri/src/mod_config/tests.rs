use super::*;
#[test]
fn config_directory_junction_is_rejected() {
    let root = std::env::temp_dir().join(format!("oc2-config-test-{}", uuid::Uuid::new_v4()));
    let game = root.join("game");
    let outside = root.join("outside");
    fs::create_dir_all(game.join("BepInEx")).unwrap();
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("example.cfg"), "private").unwrap();
    let link = game.join("BepInEx/config");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let status = std::process::Command::new("cmd.exe")
            .args(["/D", "/C", "mklink", "/J"])
            .arg(link.to_string_lossy().replace('/', "\\"))
            .arg(outside.to_string_lossy().replace('/', "\\"))
            .creation_flags(0x08000000)
            .output()
            .unwrap();
        assert!(
            status.status.success(),
            "junction creation failed: {}",
            String::from_utf8_lossy(&status.stderr)
        );
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    assert!(!exists(&game, "example"));
    assert_eq!(read(&game, "example").err().as_deref(), Some(UNSAFE));
    assert!(save(&game, "example", "", "changed").is_err());
    assert_eq!(
        fs::read_to_string(outside.join("example.cfg")).unwrap(),
        "private"
    );
    #[cfg(windows)]
    fs::remove_dir(&link).unwrap();
    #[cfg(unix)]
    fs::remove_file(&link).unwrap();
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn config_round_trip_preserves_bom_crlf_and_rejects_stale_or_unsafe_writes() {
    let game = std::env::temp_dir().join(format!("oc2-config-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(game.join("BepInEx/config")).unwrap();
    let guid = "com.ch3ngyz.plugin.HostUtilities";
    let path = game.join(format!("BepInEx/config/{guid}.cfg"));
    fs::write(&path, b"\xef\xbb\xbf[General]\r\nEnabled = true\r\n").unwrap();
    assert!(exists(&game, guid));
    assert!(!exists(&game, "missing"));
    for bad in ["../escape", "foo/bar", "foo\\bar", "foo:stream", "", ".."] {
        assert!(read(&game, bad).is_err());
    }
    let loaded = read(&game, guid).unwrap();
    let saved = save(
        &game,
        guid,
        &loaded.revision,
        "[General]\r\nEnabled = false\r\n",
    )
    .unwrap();
    assert_eq!(
        fs::read(&path).unwrap(),
        b"\xef\xbb\xbf[General]\r\nEnabled = false\r\n"
    );
    assert!(save(&game, guid, &loaded.revision, "stale").is_err());
    fs::write(&path, "external change").unwrap();
    assert!(save(&game, guid, &saved.revision, "overwrite").is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "external change");
    fs::write(&path, [0xff, 0xfe]).unwrap();
    assert!(read(&game, guid).is_err());
    fs::write(&path, vec![b'x'; LIMIT + 1]).unwrap();
    assert!(read(&game, guid).is_err());
    assert_eq!(
        fs::read_dir(game.join("BepInEx/config")).unwrap().count(),
        1
    );
    fs::remove_dir_all(&game).unwrap();
}
