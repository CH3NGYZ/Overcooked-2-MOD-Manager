//! Explicit, recoverable import from pre-2.0 installations.
mod resources;
use super::target;
use crate::{
    app_state::AppState,
    game,
    models::{ChefTarget, InstalledMod},
    mods, recycle_bin,
};
use std::{cmp::Ordering, path::Path};

pub(crate) fn modern(target: &ChefTarget) -> bool {
    target
        .version
        .as_deref()
        .and_then(|version| mods::compare_mod_versions(version, "2.0"))
        .is_some_and(|order| order != Ordering::Less)
}
pub(crate) fn legacy_targets(installed: &[InstalledMod], generation: u64) -> Vec<ChefTarget> {
    let candidates = target::candidates(installed, generation);
    if candidates.len() < 2 {
        return vec![];
    }
    candidates
        .into_iter()
        .filter(|candidate| {
            candidate
                .version
                .as_deref()
                .and_then(|version| mods::compare_mod_versions(version, "2.0"))
                == Some(Ordering::Less)
        })
        .collect()
}
pub(crate) fn migrate(state: &AppState, generation: u64, target_id: &str) -> Result<(), String> {
    let _operation = state.operation.lock().map_err(|_| "lock.poisoned")?;
    let root = super::session::game_root(state)?;
    let snapshot = state
        .local_mod_snapshot
        .lock()
        .map_err(|_| "lock.poisoned")?
        .get(generation, &root)?;
    let config = state.config.load_or_default();
    game::ensure_game_stopped(&root)?;
    // A successful generation is necessary; re-scan before moving directories.
    let installed =
        mods::scan_mods_with_manifests(&root, &config, Some(&state.config.manifests_dir()));
    let selected = target::select(&target::candidates(&installed, generation), Some(target_id))?
        .ok_or("diychef.targetChanged")?;
    let previous = target::select(
        &target::candidates(&snapshot.mods, generation),
        Some(target_id),
    )?
    .ok_or("diychef.targetChanged")?;
    let old = legacy_targets(&installed, generation);
    let before = legacy_targets(&snapshot.mods, generation);
    let signature = |values: &[ChefTarget]| {
        values
            .iter()
            .map(|item| (item.id.clone(), item.version.clone()))
            .collect::<Vec<_>>()
    };
    if signature(&old) != signature(&before) || selected.version != previous.version {
        return Err("diychef.targetChanged".into());
    }
    if old.is_empty() || !modern(&selected) {
        return Err("diychef.migrationUnavailable".into());
    }
    state.invalidate_local_mod_snapshot()?;
    let result = migrate_targets(
        &root,
        &state.config.manifests_dir(),
        &state.config.mod_recycle_bin_dir(),
        &selected,
        &old,
        &installed,
    );
    // Both success and partial failure require a fresh generation before reuse.
    result
}

fn migrate_targets(
    game: &Path,
    manifests: &Path,
    recycle: &Path,
    selected: &ChefTarget,
    old: &[ChefTarget],
    installed: &[InstalledMod],
) -> Result<(), String> {
    if !modern(selected) {
        return Err("diychef.migrationUnavailable".into());
    }
    let destination = target::mod_root(game, selected)?;
    let plugins = game::safe_game_path(game, "BepInEx/plugins")?;
    let mut roots = vec![];
    for item in old {
        let directory = target::mod_root(game, item)?;
        if directory == plugins
            || directory.starts_with(&destination)
            || destination.starts_with(&directory)
            || installed
                .iter()
                .filter(|peer| {
                    !old.iter()
                        .any(|old| old.display_path == peer.dll_file && old.enabled == peer.enabled)
                })
                .any(|peer| {
                    game::safe_game_path(game, &peer.dll_file)
                        .is_ok_and(|path| path.starts_with(&directory))
                })
        {
            return Err("diychef.migrationSharedDirectory".into());
        }
        if !roots.contains(&directory) {
            roots.push(directory);
        }
    }
    if roots.iter().enumerate().any(|(index, root)| {
        roots
            .iter()
            .skip(index + 1)
            .any(|peer| root.starts_with(peer) || peer.starts_with(root))
    }) {
        return Err("diychef.migrationSharedDirectory".into());
    }
    let directories = roots
        .iter()
        .map(|root| {
            root.strip_prefix(game)
                .map(|relative| relative.to_string_lossy().replace('\\', "/"))
                .map_err(|_| "archive.pathEscape")
        })
        .collect::<Result<Vec<_>, _>>()?;
    let map_plugins = crate::level_sets::registered_plugin_paths(manifests)
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    // Registered map DLLs are deliberately absent from the ordinary MOD scan.
    // Check the actual files too, including disabled variants, before copying.
    for directory in &directories {
        let files = recycle_bin::directory_files(game, directory)?;
        if files
            .iter()
            .any(|file| map_plugins.contains(&mods::canonical_dll_name(file).to_lowercase()))
        {
            return Err("diychef.migrationSharedDirectory".into());
        }
    }
    let mut copied = resources::Copies::default();
    let result = (|| {
        for source in &roots {
            copied.import(game, source, &destination)?;
        }
        recycle_bin::recycle_directories(
            recycle,
            manifests,
            game,
            &directories,
            "DIYChef (<2.0)".into(),
        )?;
        Ok(())
    })();
    // If the existing journal could not restore every old file, keep imported
    // resources as additional recovery material instead of deleting them.
    if result
        .as_ref()
        .is_err_and(|error| error != "recycle.partialFailure")
        && !copied.rollback()
    {
        return Err("recycle.partialFailure".into());
    }
    result
}

#[cfg(test)]
mod tests;
