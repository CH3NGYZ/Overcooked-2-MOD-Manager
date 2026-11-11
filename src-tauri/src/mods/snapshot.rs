//! Trusted scan results. Caller-provided InstalledMod arrays never authorize files.
use crate::models::LocalModSnapshot;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
#[derive(Default)]
pub(crate) struct LocalModSnapshots {
    next: u64,
    active: u64,
    root: Option<PathBuf>,
    value: Option<LocalModSnapshot>,
    valid: Arc<AtomicBool>,
}
impl LocalModSnapshots {
    pub(crate) fn begin(&mut self, root: Option<&Path>) -> u64 {
        self.valid.store(false, Ordering::SeqCst);
        self.valid = Arc::new(AtomicBool::new(true));
        self.next += 1;
        self.active = self.next;
        self.root = root.map(Path::to_path_buf);
        self.value = None;
        self.active
    }
    pub(crate) fn publish(
        &mut self,
        generation: u64,
        root: Option<&Path>,
        mods: Vec<crate::models::InstalledMod>,
    ) -> Result<LocalModSnapshot, String> {
        if self.active != generation || self.root.as_deref() != root {
            return Err("diychef.staleScan".into());
        }
        let diy_level_target =
            super::plugin_target::unique(&mods, super::plugin_target::DIYLEVEL_GUID)
                .map(|item| item.dll_file.clone());
        let value = LocalModSnapshot {
            generation,
            mods,
            diy_level_target,
        };
        self.value = Some(value.clone());
        Ok(value)
    }
    pub(crate) fn get(&self, generation: u64, root: &Path) -> Result<LocalModSnapshot, String> {
        if self.active != generation || self.root.as_deref() != Some(root) {
            return Err("diychef.staleScan".into());
        }
        self.value.clone().ok_or_else(|| "diychef.staleScan".into())
    }
    pub(crate) fn invalidate(&mut self) {
        self.valid.store(false, Ordering::SeqCst);
        self.next += 1;
        self.active = self.next;
        self.value = None;
    }

    pub(crate) fn current(&self, root: &Path) -> Result<LocalModSnapshot, String> {
        self.get(self.active, root)
    }

    pub(crate) fn lease(&self, generation: u64, root: &Path) -> Result<Arc<AtomicBool>, String> {
        self.get(generation, root)?;
        Ok(self.valid.clone())
    }
}

/// All map entry points use a completed backend-owned scan; none may rescan DLLs
/// or infer a historical package path while that scan is pending or invalid.
pub(crate) fn level_roots(
    state: &crate::app_state::AppState,
    game_root: &Path,
) -> Result<(LocalModSnapshot, crate::level_sets::LevelSetRoots), String> {
    let game_root = crate::game::resolve_existing_ancestor(game_root)?;
    let snapshot = state
        .local_mod_snapshot
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?
        .current(&game_root)?;
    let candidates =
        super::plugin_target::eligible(&snapshot.mods, super::plugin_target::DIYLEVEL_GUID);
    if candidates.len() > 1 {
        return Err("framework.diyLevelAmbiguous".into());
    }
    let package_root = candidates
        .first()
        .map(|item| super::plugin_target::directory(&game_root, &item.dll_file, item.enabled))
        .transpose()?;
    Ok((
        snapshot,
        crate::level_sets::LevelSetRoots::new(game_root, package_root),
    ))
}

/// ZIP staging can work before a game is installed, but an installed game's
/// conflict report must never silently substitute an unknown/pending map root.
pub(crate) fn optional_level_roots(
    state: &crate::app_state::AppState,
) -> Result<Option<crate::level_sets::LevelSetRoots>, String> {
    crate::game::detect_game(&state.config.load_or_default())
        .path
        .map(|root| level_roots(state, Path::new(&root)).map(|(_, roots)| roots))
        .transpose()
}
#[cfg(test)]
mod tests;
