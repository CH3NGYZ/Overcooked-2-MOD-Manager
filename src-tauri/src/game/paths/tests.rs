use super::*;
#[cfg(windows)]
use std::fs;

#[test]
fn rejects_parent_paths() {
    let root = std::env::temp_dir().join("oc2-manager-test");
    assert!(safe_game_path(&root, "../outside").is_err());
    assert!(safe_game_path(&root, "BepInEx/plugins/mod.dll:stream").is_err());
    assert!(safe_game_path(&root, "BepInEx/plugins/mod.dll").is_ok());
}

#[test]
#[cfg(windows)]
fn rejects_new_files_under_a_junction_outside_the_game() {
    let workspace = std::env::temp_dir().join(format!("oc2-path-test-{}", uuid::Uuid::new_v4()));
    let game = workspace.join("game");
    let outside = workspace.join("outside");
    fs::create_dir_all(&game).unwrap();
    fs::create_dir_all(&outside).unwrap();
    let link = game.join("BepInEx");
    let output = std::process::Command::new("cmd.exe")
        .args(["/D", "/C", "mklink", "/J"])
        .arg(&link)
        .arg(&outside)
        .output()
        .unwrap();
    assert!(output.status.success(), "failed to create test junction");
    let rejected = safe_game_path(&game, "BepInEx/plugins/New.dll").is_err();
    // Remove the junction itself before deleting either real directory.
    fs::remove_dir(link).unwrap();
    fs::remove_dir(game).unwrap();
    fs::remove_dir(outside).unwrap();
    fs::remove_dir(workspace).unwrap();
    assert!(
        rejected,
        "a missing file must not bypass its existing ancestor's containment check"
    );
}
