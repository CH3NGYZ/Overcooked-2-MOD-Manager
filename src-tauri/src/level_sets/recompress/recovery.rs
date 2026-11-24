use super::replacement::{digest, is_current, open_existing, overwrite, verified_backup};
use crate::{file_metadata::FileIdentity, models::LevelCompressionRecovery};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, OnceLock},
};

#[derive(Clone)]
struct Recovery {
    id: String,
    game_root: PathBuf,
    source: PathBuf,
    backup: PathBuf,
    backup_hash: Vec<u8>,
    identity: FileIdentity,
    source_hash: Option<Vec<u8>>,
    restore_error: String,
    restoring: bool,
}

pub(super) const SOURCE_CHANGED: &str = "levelSet.recoverySourceChanged";
const NOT_FOUND: &str = "levelSet.recoveryNotFound";
const REQUIRED: &str = "levelSet.recoveryRequired";

#[derive(Default)]
struct State {
    pending: Vec<Recovery>,
    suppressed: HashSet<PathBuf>,
}

#[derive(Default)]
struct Registry(Mutex<State>);

fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(Default::default)
}

fn path_key(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(crate::mod_changes::transaction::path_key(path))
    }
    #[cfg(not(windows))]
    {
        path.to_owned()
    }
}

fn root_key(path: &Path) -> PathBuf {
    path_key(&path.canonicalize().unwrap_or_else(|_| path.to_owned()))
}

impl Registry {
    fn state(&self) -> MutexGuard<'_, State> {
        self.0.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn register(
        &self,
        target: (&Path, &Path),
        backup: &Path,
        identity: FileIdentity,
        backup_hash: &[u8],
        source_hash: Option<Vec<u8>>,
        restore_error: &str,
    ) -> String {
        let (game_root, source) = target;
        let id = uuid::Uuid::new_v4().to_string();
        self.state().pending.push(Recovery {
            id: id.clone(),
            game_root: game_root.to_owned(),
            source: source.to_owned(),
            backup: backup.to_owned(),
            backup_hash: backup_hash.to_vec(),
            identity,
            source_hash,
            restore_error: restore_error.to_owned(),
            restoring: false,
        });
        id
    }

    fn blocks_conversion(&self, path: &Path) -> bool {
        let key = path_key(path);
        let state = self.state();
        state.suppressed.contains(&key)
            || state
                .pending
                .iter()
                .any(|item| path_key(&item.source) == key)
    }

    fn has_pending(&self, game_root: &Path) -> bool {
        let root = root_key(game_root);
        self.state()
            .pending
            .iter()
            .any(|item| path_key(&item.game_root) == root)
    }

    fn list(&self, game_root: &Path) -> Vec<LevelCompressionRecovery> {
        let root = root_key(game_root);
        self.state()
            .pending
            .iter()
            .filter(|item| path_key(&item.game_root) == root)
            .map(|item| LevelCompressionRecovery {
                id: item.id.clone(),
                source_path: crate::display_path::display_path(&item.source),
                backup_path: crate::display_path::display_path(&item.backup),
                restore_error: Some(item.restore_error.clone()),
            })
            .collect()
    }

    fn begin(&self, id: &str, root: Option<&Path>) -> Result<Recovery, String> {
        let mut state = self.state();
        let item = state
            .pending
            .iter_mut()
            .find(|item| item.id == id && root.is_none_or(|root| path_key(&item.game_root) == root))
            .ok_or(NOT_FOUND)?;
        if item.restoring {
            return Err(REQUIRED.into());
        }
        item.restoring = true;
        Ok(item.clone())
    }

    fn finish(&self, item: Recovery, result: &Result<(), String>) {
        let mut state = self.state();
        let Some(index) = state
            .pending
            .iter()
            .position(|current| current.id == item.id)
        else {
            return;
        };
        if let Err(error) = result {
            state.pending[index] = Recovery {
                restore_error: error.clone(),
                restoring: false,
                ..item
            };
        } else {
            state.pending.remove(index);
        }
    }

    fn retry(
        &self,
        id: &str,
        root: Option<&Path>,
        ensure_stopped: &dyn Fn(&Path) -> Result<(), String>,
    ) -> Result<(), String> {
        let mut item = self.begin(id, root)?;
        let result = restore_unchanged(&mut item, ensure_stopped);
        self.finish(item, &result);
        result
    }

    fn restore_all(&self, ensure_stopped: &dyn Fn(&Path) -> Result<(), String>) -> usize {
        let ids = self
            .state()
            .pending
            .iter()
            .map(|item| item.id.clone())
            .collect::<Vec<_>>();
        for id in ids {
            let _ = self.retry(&id, None, ensure_stopped);
        }
        self.state().pending.len()
    }

    fn ignore(&self, game_root: &Path, id: &str) -> Result<(), String> {
        let root = root_key(game_root);
        let mut state = self.state();
        let index = state
            .pending
            .iter()
            .position(|item| item.id == id && path_key(&item.game_root) == root)
            .ok_or(NOT_FOUND)?;
        if state.pending[index].restoring {
            return Err(REQUIRED.into());
        }
        let item = state.pending.remove(index);
        state.suppressed.insert(path_key(&item.source));
        Ok(())
    }
}

pub(super) fn register(
    target: (&Path, &Path),
    backup: &Path,
    identity: FileIdentity,
    backup_hash: &[u8],
    source_hash: Option<Vec<u8>>,
    restore_error: &str,
) -> String {
    registry().register(
        target,
        backup,
        identity,
        backup_hash,
        source_hash,
        restore_error,
    )
}

pub(super) fn restore_all() -> usize {
    registry().restore_all(&crate::game::ensure_game_stopped)
}

pub(super) fn any_pending() -> bool {
    !registry().state().pending.is_empty()
}

pub(super) fn blocks_conversion(path: &Path) -> bool {
    registry().blocks_conversion(path)
}

pub(crate) fn has_pending_recovery(game_root: &Path) -> bool {
    registry().has_pending(game_root)
}

pub(crate) fn list_recoveries(game_root: &Path) -> Vec<LevelCompressionRecovery> {
    registry().list(game_root)
}

pub(crate) fn retry_recovery(game_root: &Path, id: &str) -> Result<(), String> {
    registry().retry(
        id,
        Some(&root_key(game_root)),
        &crate::game::ensure_game_stopped,
    )
}

pub(crate) fn ignore_recovery(game_root: &Path, id: &str) -> Result<(), String> {
    registry().ignore(game_root, id)
}

fn restore_unchanged(
    item: &mut Recovery,
    ensure_stopped: &dyn Fn(&Path) -> Result<(), String>,
) -> Result<(), String> {
    let Some(expected) = &item.source_hash else {
        return Err(SOURCE_CHANGED.into());
    };
    let (mut file, identity) =
        open_existing(&item.source, true).map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound | std::io::ErrorKind::InvalidInput => {
                SOURCE_CHANGED.into()
            }
            _ => error.to_string(),
        })?;
    if identity != item.identity {
        return Err(SOURCE_CHANGED.into());
    }
    let bytes =
        verified_backup(&item.backup, &item.backup_hash).map_err(|error| error.to_string())?;
    ensure_stopped(&item.game_root)?;
    if digest(&mut file).map_err(|error| error.to_string())? != *expected
        || !is_current(&item.source, &file, &item.identity)
    {
        return Err(SOURCE_CHANGED.into());
    }
    if let Err(error) = overwrite(&mut file, bytes.as_slice(), &item.backup_hash) {
        item.source_hash = digest(&mut file).ok();
        return Err(error.to_string());
    }
    is_current(&item.source, &file, &item.identity)
        .then_some(())
        .ok_or_else(|| SOURCE_CHANGED.into())
}

#[cfg(test)]
pub(super) fn remove_fixture(id: &str) {
    registry().state().pending.retain(|item| item.id != id);
}

#[cfg(test)]
mod tests;
