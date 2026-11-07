use super::*;

/// macOS 上 Wine/CrossOver 的常驻宿主进程会带着启动过的程序路径活很久，
/// 不能算成「游戏在运行」—— 否则游戏退出后按钮永远停在「停止游戏」，
/// 所有要求游戏已停止的操作都会被拒。
#[test]
fn wine_host_processes_never_count_as_the_running_game() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/verification/game-processes");
    let windows_path =
        r"C:\Program Files (x86)\Steam\steamapps\common\Overcooked! 2\Overcooked2.exe";
    for name in [
        "wineserver",
        "wineboot.exe",
        "winedevice.exe",
        "winemenubuilder.exe",
        "winedbg",
        "services.exe",
        "explorer.exe",
        "cxstart.exe",
        "steam.exe",
    ] {
        assert!(
            !process_matches_game(&root, name, None, &[windows_path.into()]),
            "{name} 是 Wine 宿主进程，不能算作游戏"
        );
    }
    // 真正承载游戏的宿主进程仍然算运行中（macOS CrossOver 只有它们可判）
    for name in ["wine64-preloader", "cxwinehelper"] {
        assert!(process_matches_game(
            &root,
            name,
            None,
            &[windows_path.into()]
        ));
    }
}

#[test]
fn game_process_matching_covers_native_and_wine_without_treating_steam_as_the_game() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/verification/game-processes");
    for (name, executable) in [
        ("Overcooked2.exe", root.join("Overcooked2.exe")),
        (
            "Overcooked2",
            root.join("Overcooked2.app/Contents/MacOS/Overcooked2"),
        ),
    ] {
        assert!(process_matches_game(&root, name, Some(&executable), &[]));
    }
    // 平台已不发布的原生可执行文件名不再被当作本游戏的进程
    assert!(!process_matches_game(
        &root,
        "Overcooked2.x86_64",
        Some(&root.join("Overcooked2.x86_64")),
        &[]
    ));
    assert!(process_matches_game(
        &root,
        "wine-preloader",
        None,
        &[r"C:\Games\Overcooked2.exe".into()]
    ));
    assert!(!process_matches_game(
        &root,
        "steam.exe",
        None,
        &["-applaunch".into(), "728880".into()]
    ));
    assert!(!process_matches_game(
        &root,
        "wine",
        None,
        &["steam.exe".into()]
    ));
    assert!(!process_matches_game(
        &root,
        "Overcooked2.exe",
        Some(&root.join("other/Overcooked2.exe")),
        &[]
    ));
}
