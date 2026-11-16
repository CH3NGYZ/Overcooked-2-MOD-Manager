//! Explicit, recoverable import from pre-2.0 installations.
mod resources;
use super::target;
use crate::{
    app_state::AppState,
    game,
    models::{ChefTarget, InstalledMod},
    mods, recycle_bin,
};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

pub(crate) struct RecoveredSource {
    pub target: ChefTarget,
    record_id: String,
    plugin_root: PathBuf,
    archived: Vec<resources::ArchivedResource>,
}

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

fn recovered_sources(game: &Path, recycle: &Path, generation: u64) -> Vec<RecoveredSource> {
    // A plugin can rename its DLL, so filenames cannot establish chef identity.
    // Validate records containing DLLs, then require their parsed GUID/version.
    let records: Vec<recycle_bin::VerifiedPayloadRecord> =
        match recycle_bin::verified_payload_records(recycle, game, |relative| {
            let normalized = relative.replace('\\', "/").to_ascii_lowercase();
            normalized.ends_with(".dll") || normalized.ends_with(".dll.disabled")
        }) {
            Ok(records) => records,
            Err(error) => {
                log::warn!("读取 DIYChef 历史资源回收记录失败：原因={}", error);
                return vec![];
            }
        };
    let Ok(canonical_game) = game.canonicalize() else {
        return vec![];
    };
    let mut sources = Vec::new();
    for record in records {
        if record.game_path != canonical_game || record.diychef_resources_imported {
            continue;
        }
        for file in &record.files {
            let file: &recycle_bin::VerifiedPayloadFile = file;
            let relative = file.relative.replace('\\', "/");
            let normalized = relative.to_ascii_lowercase();
            if !(normalized.ends_with(".dll") || normalized.ends_with(".dll.disabled")) {
                continue;
            }
            let Some((plugin_relative, _)) = relative.rsplit_once('/') else {
                continue;
            };
            let details = mods::read_dll_details(&file.path, &relative);
            let (Some(guid), Some(version)) = (details.guid, details.version) else {
                continue;
            };
            if !guid.eq_ignore_ascii_case(target::GUID)
                || mods::compare_mod_versions(&version, "2.0") != Some(Ordering::Less)
            {
                continue;
            }

            let resource_prefix = format!("{plugin_relative}/Resources/");
            let archived = record
                .files
                .iter()
                .filter_map(|candidate| {
                    let relative = candidate.relative.replace('\\', "/");
                    let tail = relative.get(resource_prefix.len()..).filter(|_| {
                        relative
                            .get(..resource_prefix.len())
                            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(&resource_prefix))
                    })?;
                    Some(resources::ArchivedResource {
                        relative: tail.to_string(),
                        path: candidate.path.clone(),
                    })
                })
                .collect::<Vec<_>>();
            let plugin_root = match target::child(game, game, plugin_relative) {
                Ok(path) => path,
                Err(error) => {
                    log::warn!(
                        "忽略不安全的 DIYChef 历史路径：文件={}，原因={}",
                        relative,
                        error
                    );
                    continue;
                }
            };
            let live_resources = target::child(game, &plugin_root, "Resources")
                .ok()
                .filter(|path| path.is_dir())
                .and_then(|path| {
                    let relative = path
                        .strip_prefix(game)
                        .ok()?
                        .to_string_lossy()
                        .replace('\\', "/");
                    recycle_bin::directory_files(game, &relative)
                        .ok()
                        .filter(|files| !files.is_empty())
                        .map(|_| path)
                });
            if archived.is_empty() && live_resources.is_none() {
                continue;
            }
            let enabled = !normalized.ends_with(".disabled");
            sources.push(RecoveredSource {
                target: ChefTarget {
                    id: format!("recycle:{}/{}", record.id, relative),
                    guid,
                    version: Some(version),
                    display_path: relative,
                    enabled,
                    generation,
                    compatibility: "editable".into(),
                    supports_custom_default: false,
                    supports_live_save: false,
                },
                record_id: record.id.clone(),
                plugin_root,
                archived,
            });
        }
    }
    sources
}

pub(crate) fn available_legacy_targets(
    installed: &[InstalledMod],
    game: &Path,
    recycle: &Path,
    generation: u64,
) -> (Vec<ChefTarget>, Vec<RecoveredSource>) {
    let active = legacy_targets(installed, generation);
    let recovered = recovered_sources(game, recycle, generation);
    let mut visible = active;
    visible.extend(recovered.iter().map(|source| source.target.clone()));
    (visible, recovered)
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
    let recovered = recovered_sources(&root, &state.config.mod_recycle_bin_dir(), generation);
    if (old.is_empty() && recovered.is_empty()) || !modern(&selected) {
        return Err("diychef.migrationUnavailable".into());
    }
    state.invalidate_local_mod_snapshot()?;
    let result = migrate_targets(
        &root,
        &state.config.manifests_dir(),
        &state.config.mod_recycle_bin_dir(),
        &selected,
        &old,
        &recovered,
        &installed,
        &mut recycle_bin::set_diychef_resources_imported,
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
    recovered: &[RecoveredSource],
    installed: &[InstalledMod],
    write_marker: &mut impl FnMut(&Path, &str, bool) -> Result<(), String>,
) -> Result<(), String> {
    if !modern(selected) {
        return Err("diychef.migrationUnavailable".into());
    }
    let destination = target::mod_root(game, selected)?;
    let plugins = game::safe_game_path(game, "BepInEx/plugins")?;
    // A physical old installation can have a live Resources tree and several
    // uninstall records. Merge them once, so model parts remain in one folder.
    let mut sources = BTreeMap::<PathBuf, Vec<resources::ArchivedResource>>::new();
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
        sources.entry(directory).or_default();
    }
    for source in recovered {
        let directory = &source.plugin_root;
        sources
            .entry(directory.clone())
            .or_default()
            .extend(source.archived.iter().cloned());
        // The updated plugin may reuse the old install directory. Its archived
        // payload is still useful, but that live destination must never be recycled.
        if directory == &destination {
            continue;
        }
        if !directory
            .try_exists()
            .map_err(|_| "diychef.migrationFailed")?
        {
            continue;
        }
        if directory == &plugins
            || directory.starts_with(&destination)
            || destination.starts_with(directory)
            || installed
                .iter()
                .filter(|peer| {
                    !old.iter()
                        .any(|old| old.display_path == peer.dll_file && old.enabled == peer.enabled)
                })
                .any(|peer| {
                    game::safe_game_path(game, &peer.dll_file)
                        .is_ok_and(|path| path.starts_with(directory))
                })
        {
            return Err("diychef.migrationSharedDirectory".into());
        }
    }
    let roots = sources
        .keys()
        .filter(|root| *root != &destination)
        .collect::<Vec<_>>();
    if roots.iter().enumerate().any(|(index, root)| {
        roots
            .iter()
            .skip(index + 1)
            .any(|peer| root.starts_with(peer) || peer.starts_with(root))
    }) {
        return Err("diychef.migrationSharedDirectory".into());
    }
    let map_plugins = crate::level_sets::registered_plugin_paths(manifests)
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    let old_dlls = old
        .iter()
        .map(|item| mods::canonical_dll_name(&item.display_path).to_lowercase())
        .collect::<BTreeSet<_>>();
    // Registered map DLLs are deliberately absent from the ordinary MOD scan.
    // Check the actual files too, including disabled variants, before copying.
    let mut directories = vec![];
    for root in roots {
        if !root.try_exists().map_err(|_| "diychef.migrationFailed")? {
            continue;
        }
        let relative = root
            .strip_prefix(game)
            .map_err(|_| "archive.pathEscape")?
            .to_string_lossy()
            .replace('\\', "/");
        // Imported migrations bypass legacy_resource_files inside staging so
        // archived payloads can be kept.  Re-run its manifest ownership rule
        // explicitly before any copy or directory recycle.
        recycle_bin::ensure_resource_root_unshared(manifests, &relative, &old_dlls)?;
        // A failed scan must abort before copying. Otherwise a linked or
        // unreadable non-model file could leave the old DLL installed forever.
        let files = recycle_bin::directory_files(game, &relative)?;
        if files
            .iter()
            .any(|file| map_plugins.contains(&mods::canonical_dll_name(file).to_lowercase()))
        {
            return Err("diychef.migrationSharedDirectory".into());
        }
        if !files.is_empty() {
            directories.push(relative);
        }
    }
    let mut copied = resources::Copies::default();
    let result = (|| {
        for (root, archived) in &sources {
            let live_root = (root != &destination && root.exists()).then_some(root.as_path());
            copied.import(game, live_root, archived, &destination)?;
        }
        let recovered_ids = recovered
            .iter()
            .map(|source| source.record_id.clone())
            .collect::<BTreeSet<_>>();
        let mut marked: Vec<String> = Vec::new();
        for id in recovered_ids {
            if let Err(error) = write_marker(recycle, &id, true) {
                if !rollback_markers(recycle, &marked, write_marker) {
                    return Err("recycle.partialFailure".into());
                }
                return Err(error);
            }
            marked.push(id);
        }
        if !directories.is_empty() {
            if let Err(error) = recycle_bin::recycle_directories(
                recycle,
                manifests,
                game,
                &directories,
                "DIYChef (<2.0)".into(),
                true,
            ) {
                if error != "recycle.partialFailure" {
                    if !rollback_markers(recycle, &marked, write_marker) {
                        return Err("recycle.partialFailure".into());
                    }
                }
                return Err(error);
            }
        }
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

fn rollback_markers(
    recycle: &Path,
    marked: &[String],
    write_marker: &mut impl FnMut(&Path, &str, bool) -> Result<(), String>,
) -> bool {
    let mut complete = true;
    for id in marked.iter().rev() {
        if let Err(error) = write_marker(recycle, id, false) {
            log::warn!(
                "撤销 DIYChef 导入标记失败，保留导入副本：ID={}，原因={}",
                id,
                error
            );
            complete = false;
        }
    }
    complete
}

#[cfg(test)]
mod tests;
