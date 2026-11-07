//! DLL 启停的路径校验与原子改名；普通 MOD 与 DIYLevel 共用。
use crate::{archive, game, mods};
use std::{fs, path::Path};

pub(crate) fn set_local_mod_file_state(
    game_root: &Path,
    relative: &str,
    enabled: bool,
) -> Result<bool, String> {
    if !Path::new(&relative)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("dll"))
    {
        return Err("mods.notInPlugins".into());
    }
    let plugins_root = game::safe_game_path(&game_root, "BepInEx/plugins")?;
    let enabled_path = game::safe_game_path(&game_root, &relative)?;
    let disabled_path = game::safe_game_path(&game_root, &mods::disabled_dll_file(&relative))?;
    if !enabled_path.starts_with(&plugins_root) || !disabled_path.starts_with(&plugins_root) {
        return Err("mods.notInPlugins".into());
    }
    let (source, target) = if enabled {
        (&disabled_path, &enabled_path)
    } else {
        (&enabled_path, &disabled_path)
    };
    let source_exists = source.is_file();
    let target_exists = target.is_file();
    match (source_exists, target_exists) {
        (false, true) => return Ok(false),
        (false, false) => {
            return Err(if enabled {
                "mods.disabledDllMissing"
            } else {
                "mods.enabledDllMissing"
            }
            .into());
        }
        (true, true) => return Err("mods.toggleTargetExists".into()),
        (true, false) => {}
    }
    archive::clear_readonly(source)?;
    fs::rename(source, target).map_err(|error| error.to_string())?;
    Ok(true)
}

pub(crate) fn toggle_local_mod_file(
    game_root: &Path,
    relative: &str,
    enabled: bool,
) -> Result<(), String> {
    set_local_mod_file_state(game_root, relative, enabled).map(|_| ())
}
