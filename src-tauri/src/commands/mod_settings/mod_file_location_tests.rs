use super::*;
use std::fs;
use uuid::Uuid;

struct Workspace {
    root: PathBuf,
    game: PathBuf,
}

impl Workspace {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("oc2-mod-location-{}", Uuid::new_v4()));
        let game = root.join("game");
        fs::create_dir_all(&game).unwrap();
        Self { root, game }
    }

    fn file(&self, relative: &str) -> PathBuf {
        let path = self.game.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"test DLL").unwrap();
        path
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let root = fs::canonicalize(&self.root).unwrap();
        let temp = fs::canonicalize(std::env::temp_dir()).unwrap();
        assert!(root.starts_with(&temp) && root != temp);
        let _ = fs::remove_dir_all(root);
    }
}

#[test]
fn framework_config_uses_game_root_without_plugins_and_preserves_write_guards() {
    let workspace = Workspace::new();
    let state = AppState::with_config(ConfigService::for_test(workspace.root.join("manager")));
    let mut config = state.config.load().unwrap();
    config.game.path = workspace.game.to_string_lossy().into_owned();
    state.config.save(&config).unwrap();
    let (game, resolved) = resolve_config_game(&state).unwrap();
    assert_eq!(game, workspace.game);
    assert!(!crate::mod_config::exists(&game, "BepInEx"));
    assert!(crate::mod_config::read(&game, "BepInEx").is_err());
    assert!(!game.join("BepInEx/config").exists());

    let path = workspace.file("BepInEx/config/BepInEx.cfg");
    fs::write(
        &path,
        b"\xef\xbb\xbf[Logging.Console]\r\nEnabled = false\r\n",
    )
    .unwrap();
    assert!(!game.join("BepInEx/plugins").exists());
    let document = crate::mod_config::read(&game, "BepInEx").unwrap();
    assert_eq!(document.file_name, "BepInEx.cfg");
    let saved = save_config_document(
        &state,
        &game,
        "BepInEx",
        &resolved.game.path,
        &document.revision,
        "[Logging.Console]\r\nEnabled = true\r\n",
    )
    .unwrap();
    assert_eq!(
        fs::read(&path).unwrap(),
        b"\xef\xbb\xbf[Logging.Console]\r\nEnabled = true\r\n"
    );
    fs::write(&path, "external change").unwrap();
    assert_eq!(
        save_config_document(
            &state,
            &game,
            "BepInEx",
            &resolved.game.path,
            &saved.revision,
            "overwrite",
        )
        .err()
        .as_deref(),
        Some("modConfig.externallyModified")
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "external change");

    config.game.path = workspace
        .root
        .join("other-game")
        .to_string_lossy()
        .into_owned();
    state.config.save(&config).unwrap();
    assert_eq!(
        save_config_document(
            &state,
            &game,
            "BepInEx",
            &resolved.game.path,
            &saved.revision,
            "overwrite",
        )
        .err()
        .as_deref(),
        Some("game.directoryChangedReopenConfig")
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "external change");
}

#[test]
fn resolves_nested_enabled_dll_with_spaces_commas_and_unicode() {
    let workspace = Workspace::new();
    let relative = "BepInEx/plugins/测试 插件,包/Sample Plugin,测试.DLL";
    let path = workspace.file(relative);
    let resolved = resolve_mod_file_for_location(&workspace.game, relative).unwrap();
    assert_eq!(resolved, fs::canonicalize(&path).unwrap());
    assert_eq!(fs::read(path).unwrap(), b"test DLL");
}

#[test]
fn resolves_disabled_dll_from_canonical_or_disabled_row_path() {
    let workspace = Workspace::new();
    let path = workspace.file("BepInEx/plugins/Example.dll.disabled");
    for relative in [
        "BepInEx/plugins/Example.dll",
        "BepInEx/plugins/Example.dll.disabled",
    ] {
        assert_eq!(
            resolve_mod_file_for_location(&workspace.game, relative).unwrap(),
            fs::canonicalize(&path).unwrap()
        );
    }
}

#[test]
fn enabled_copy_wins_when_both_variants_exist_like_the_mod_scan() {
    let workspace = Workspace::new();
    let enabled = workspace.file("BepInEx/plugins/Example.dll");
    workspace.file("BepInEx/plugins/Example.dll.disabled");
    assert_eq!(
        resolve_mod_file_for_location(&workspace.game, "BepInEx/plugins/Example.dll").unwrap(),
        fs::canonicalize(enabled).unwrap()
    );
}

#[test]
fn resolves_current_file_after_state_changes_without_rescanning() {
    let workspace = Workspace::new();
    let relative = "BepInEx/plugins/Example.dll";
    let enabled = workspace.file(relative);
    let disabled = enabled.with_extension("dll.disabled");
    fs::rename(&enabled, &disabled).unwrap();
    assert_eq!(
        resolve_mod_file_for_location(&workspace.game, relative).unwrap(),
        fs::canonicalize(&disabled).unwrap()
    );
    fs::rename(&disabled, &enabled).unwrap();
    assert_eq!(
        resolve_mod_file_for_location(&workspace.game, relative).unwrap(),
        fs::canonicalize(&enabled).unwrap()
    );
}

#[test]
fn missing_dll_does_not_create_a_plugins_directory() {
    let workspace = Workspace::new();
    let error =
        resolve_mod_file_for_location(&workspace.game, "BepInEx/plugins/missing.dll").unwrap_err();
    assert_eq!(error, "mods.dllFileMissing");
    assert!(!workspace.game.join("BepInEx").exists());
}

#[test]
fn rejects_unsafe_paths_non_plugins_files_and_non_dll_files() {
    let workspace = Workspace::new();
    workspace.file("BepInEx/core/Example.dll");
    workspace.file("BepInEx/plugins/README.txt");
    for relative in [
        "",
        "../outside.dll",
        "C:/Games/Example.dll",
        "BepInEx/core/Example.dll",
        "BepInEx/plugins-extra/Example.dll",
        "BepInEx/plugins/../core/Example.dll",
        "BepInEx/plugins/Example.dll:stream.dll",
        "BepInEx/plugins/README.txt",
        "BepInEx/plugins/Example.dll.disabled.disabled",
    ] {
        assert!(
            resolve_mod_file_for_location(&workspace.game, relative).is_err(),
            "{relative}"
        );
    }
}

#[test]
fn rejects_a_directory_named_dll() {
    let workspace = Workspace::new();
    fs::create_dir_all(workspace.game.join("BepInEx/plugins/Directory.dll")).unwrap();
    assert!(
        resolve_mod_file_for_location(&workspace.game, "BepInEx/plugins/Directory.dll").is_err()
    );
}
