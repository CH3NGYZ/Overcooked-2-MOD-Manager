use super::*;
use std::fs;

#[test]
fn native_mac_selection_accepts_game_root_and_actual_app_bundle() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/verification/native-mac-selection")
        .join(uuid::Uuid::new_v4().to_string());
    let root = fixture.join("Steam/steamapps/common/Overcooked! 2");
    fs::create_dir_all(&root).unwrap();
    assert!(!executable_exists_for_package(&root, Package::MacOS));
    assert_eq!(
        selected_root_for_package(&root, Package::MacOS).unwrap_err(),
        "game.appNotFound"
    );
    let app = root.join("Overcooked2.app");
    fs::create_dir_all(&app).unwrap();
    assert!(selected_root_for_package(&root, Package::MacOS).is_err());
    assert!(selected_root_for_package(&app, Package::MacOS).is_err());
    fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
    fs::write(app.join("Contents/MacOS/Overcooked2"), b"fixture").unwrap();
    let expected = fs::canonicalize(&root).unwrap();
    assert_eq!(
        selected_root_for_package(&root, Package::MacOS).unwrap(),
        expected
    );
    assert_eq!(
        selected_root_for_package(&app, Package::MacOS).unwrap(),
        expected
    );
    // 选到包内 `Contents/MacOS`（甚至其中的可执行文件）同样回到游戏根。
    for selected in [
        app.join("Contents/MacOS"),
        app.join("Contents/MacOS/Overcooked2"),
    ] {
        assert_eq!(
            selected_root_for_package(&selected, Package::MacOS).unwrap(),
            expected
        );
    }
    assert!(executable_exists_for_package(&root, Package::MacOS));
    assert!(!executable_exists_for_package(&root, Package::Windows));
    fs::write(root.join("Overcooked2.exe"), b"fixture").unwrap();
    // 两个包都能接受游戏根目录
    for package in [Package::Windows, Package::MacOS] {
        assert_eq!(selected_root_for_package(&root, package).unwrap(), expected);
    }
    /*
     * `.app` bundle 只在 macOS 包下被接受（上面已断言它会向上回溯到
     * 游戏根）；Windows 包必须拒绝 —— 它不是游戏目录。
     */
    assert!(selected_root_for_package(&app, Package::Windows).is_err());
    fs::remove_dir_all(fixture).unwrap();
}

/// 用户选中的 `.app` / `Contents/MacOS` 必须回到同一个游戏根字符串。
#[test]
fn selection_root_maps_macos_bundle_and_contents_paths_to_the_game_root() {
    let root = Path::new("/Games/Overcooked! 2").to_path_buf();
    let app = root.join("Overcooked2.app");
    for selected in [
        root.clone(),
        app.clone(),
        app.join("Contents/MacOS"),
        app.join("Contents/MacOS/Overcooked2"),
    ] {
        assert_eq!(selection_root(&selected, Package::MacOS), root);
    }
    // Windows 包不做向上回溯：`.app` 不是合法游戏根。
    assert_eq!(selection_root(&app, Package::Windows), app);
}

#[test]
fn selection_expands_home_prefix_but_not_named_accounts() {
    let Some(home) = dirs::home_dir() else {
        return;
    };
    assert_eq!(expand_home(Path::new("~")), home);
    assert_eq!(
        expand_home(Path::new("~/Games/Overcooked! 2")),
        home.join("Games/Overcooked! 2")
    );
    // `~user` 需要真实账户查询，不猜测。
    assert_eq!(
        expand_home(Path::new("~someone/game")),
        PathBuf::from("~someone/game")
    );
}
