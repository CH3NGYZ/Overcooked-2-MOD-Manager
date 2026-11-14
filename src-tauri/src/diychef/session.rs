//! Sessions hold backend-owned paths, baselines and bounded editor state.
#[cfg(test)]
mod external_files_tests;
mod mutations;
#[cfg(test)]
mod native_fallback_tests;
pub(crate) use mutations::{initialize, save};

use super::{
    catalog::{self, Catalog},
    prefer,
    storage::{self, Stored},
    target, validation,
};
use crate::{app_state::AppState, game, models::*};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
#[derive(Default)]
pub(crate) struct ChefSessions {
    pub(super) values: HashMap<String, Session>,
}
#[derive(Clone)]
pub(crate) struct Session {
    pub id: String,
    pub game_root: PathBuf,
    pub game_settings: GameConfig,
    pub mod_root: PathBuf,
    pub target: ChefTarget,
    pub catalog: Catalog,
    pub resources_revision: u64,
    pub stored: Stored,
    pub bindings_dir: PathBuf,
}
pub(super) fn game_root(state: &AppState) -> Result<PathBuf, String> {
    let game = game::detect_game(&state.config.load_or_default());
    if !game.game_exists {
        return Err("diychef.gameMissing".into());
    }
    let path = game.path.ok_or("diychef.gameMissing")?;
    game::resolve_existing_ancestor(std::path::Path::new(&path))
}
pub(crate) fn open(
    state: &AppState,
    generation: u64,
    target_id: Option<&str>,
    before_read: impl FnOnce(Option<(&Path, &ChefTarget)>) -> u64,
) -> Result<ChefEditorOpen, String> {
    let root = game_root(state)?;
    let snapshot = state
        .local_mod_snapshot
        .lock()
        .map_err(|e| e.to_string())?
        .get(generation, &root)?;
    let candidates = target::eligible(&target::candidates(&snapshot.mods, generation));
    let legacy_targets = super::migration::legacy_targets(&snapshot.mods, generation);
    let Some(target) = target::select(&candidates, target_id)? else {
        before_read(None);
        return Ok(ChefEditorOpen {
            status: if candidates.is_empty() {
                "not_installed"
            } else {
                "select_target"
            }
            .into(),
            candidates,
            snapshot: None,
            legacy_targets,
            migration_available: false,
        });
    };
    let migration_available = !legacy_targets.is_empty() && super::migration::modern(&target);
    let mod_root = target::mod_root(&root, &target)?;
    // Observe before either resource or TXT read so an external change cannot
    // fall between the baseline snapshot and starting the OS subscription.
    let resources_revision = before_read(Some((&mod_root, &target)));
    let catalog = catalog::read(&root, &mod_root)?;
    let bindings_dir = state.config.config_path().with_file_name("diychef");
    let stored = storage::read(&root, &mod_root, &target.id, &bindings_dir)?;
    let mut session = Session {
        id: uuid::Uuid::new_v4().to_string(),
        game_root: root,
        game_settings: state.config.load_or_default().game,
        mod_root,
        target,
        catalog,
        resources_revision,
        stored,
        bindings_dir,
    };
    {
        let sessions = state.diychef_sessions.lock().map_err(|e| e.to_string())?;
        // A prefer-only notification can be our own save echo. Reusing an
        // identical baseline keeps repeated checks from evicting the session
        // still used by an unchanged frontend draft and its selection.
        if let Some(existing) = sessions.values.values().find(|existing| {
            existing.game_root == session.game_root
                && existing.mod_root == session.mod_root
                && existing.target.id == session.target.id
                && existing.target.generation == session.target.generation
                && existing.stored.source.revision == session.stored.source.revision
                && existing.catalog.revision == session.catalog.revision
                && existing.resources_revision == session.resources_revision
        }) {
            session.id.clone_from(&existing.id);
        }
    }
    let view = view(&session)?;
    let mut sessions = state.diychef_sessions.lock().map_err(|e| e.to_string())?;
    if sessions.values.len() >= 16 && !sessions.values.contains_key(&session.id) {
        sessions.values.clear();
    }
    sessions.values.insert(session.id.clone(), session);
    Ok(ChefEditorOpen {
        status: "ready".into(),
        candidates,
        snapshot: Some(view),
        legacy_targets,
        migration_available,
    })
}
pub(crate) fn get(state: &AppState, id: &str) -> Result<Session, String> {
    let s = state
        .diychef_sessions
        .lock()
        .map_err(|e| e.to_string())?
        .values
        .get(id)
        .cloned()
        .ok_or("diychef.sessionExpired")?;
    let root = game_root(state)?;
    state
        .local_mod_snapshot
        .lock()
        .map_err(|e| e.to_string())?
        .get(s.target.generation, &root)?;
    if root != s.game_root {
        return Err("diychef.targetChanged".into());
    }
    Ok(s)
}
fn baseline_draft(s: &Session) -> ChefDraft {
    let mut selection = s.stored.document.selection(&s.catalog);
    let mut bindings = s.stored.hat_bindings.clone();
    bindings.retain(|id, hat| s.catalog.chefs.iter().any(|chef| chef.id == *id)
        && matches!(hat, HatReference::Known { hat_id } if s.catalog.hats.iter().any(|item| item.id == *hat_id && item.resource_state != "invalid")));
    // TXT is an external input only at the read boundary. A submitted draft is
    // never re-imported, so stale entry projections cannot overwrite its map.
    bindings.extend(selection.hat_bindings);
    selection.hat_bindings = bindings;
    ChefDraft {
        base_revision: s.stored.source.revision.clone(),
        catalog_revision: s.catalog.revision.clone(),
        target_generation: s.target.generation,
        entries: selection.entries,
        hat_bindings: selection.hat_bindings,
        lobby_switch: selection.lobby_switch,
        removed_problem_row_ids: vec![],
        acknowledged_change_ids: vec![],
        reset_intent: None,
    }
}
pub(crate) fn view(s: &Session) -> Result<ChefEditorSnapshot, String> {
    let mut draft = baseline_draft(s);
    let mut selection = ChefSelection {
        entries: draft.entries.clone(),
        hat_bindings: draft.hat_bindings.clone(),
        lobby_switch: draft.lobby_switch,
    };
    if s.stored.source.state == "missing" {
        selection = prefer::Document::parse(prefer::DEFAULT_CONTENT)?.selection(&s.catalog);
        for entry in &mut selection.entries {
            entry.entry_id = "default:black_cat".into();
            entry.row_id = None;
        }
        selection.hat_bindings.extend(draft.hat_bindings.clone());
        draft.hat_bindings = selection.hat_bindings.clone();
        draft.entries = selection.entries.clone();
        draft.lobby_switch = selection.lobby_switch;
    }
    let diagnostics = validation::validate(
        &s.stored.document,
        &s.catalog,
        &draft,
        s.target.supports_custom_default,
    )?
    .diagnostics;
    Ok(ChefEditorSnapshot {
        session_id: s.id.clone(),
        target: s.target.clone(),
        source: s.stored.source.clone(),
        catalog_revision: s.catalog.revision.clone(),
        chefs: s.catalog.chefs.clone(),
        hats: s.catalog.hats.clone(),
        document_rows: s.stored.document.rows(),
        selection,
        diagnostics,
    })
}
pub(super) fn check_draft(s: &Session, d: &ChefDraft) -> Result<(), String> {
    if d.base_revision != s.stored.source.revision
        || d.catalog_revision != s.catalog.revision
        || d.target_generation != s.target.generation
    {
        return Err("diychef.conflict".into());
    }
    Ok(())
}
pub(crate) fn validate(
    state: &AppState,
    id: &str,
    d: &ChefDraft,
) -> Result<ChefValidation, String> {
    let s = get(state, id)?;
    check_draft(&s, d)?;
    validation::validate(
        &s.stored.document,
        &s.catalog,
        d,
        s.target.supports_custom_default,
    )
}
#[derive(Clone, Debug)]
pub(crate) struct PreviewContext {
    pub session_id: String,
    pub resources_revision: u64,
    pub game_root: PathBuf,
    pub chef_name: String,
    pub chef_directory: Option<PathBuf>,
    pub hat_value: String,
    pub hat_directory: Option<PathBuf>,
}
pub(crate) fn resolve_preview_context(
    state: &AppState,
    session_id: &str,
    chef_id: &str,
    hat_id: &str,
) -> Result<PreviewContext, String> {
    let (game_root, mod_root, resources_revision, chef, hat) =
        with_preview_session(state, session_id, |s| {
            let chef = s
                .catalog
                .chefs
                .iter()
                .find(|x| x.id == chef_id)
                .ok_or("diychef.invalidResource")?;
            let hat = s
                .catalog
                .hats
                .iter()
                .find(|x| x.id == hat_id)
                .ok_or("diychef.invalidResource")?;
            Ok((
                s.game_root.clone(),
                s.mod_root.clone(),
                s.resources_revision,
                chef.clone(),
                hat.clone(),
            ))
        })?;
    if chef.resource_state == "invalid" || hat.resource_state == "invalid" {
        return Err("diychef.invalidResource".into());
    }
    let chef_directory = if chef.source_kind == "custom" {
        Some(target::child(
            &game_root,
            &mod_root,
            &format!("Resources/{}", chef.config_name),
        )?)
    } else {
        None
    };
    let hat_directory = if hat.kind == "custom" {
        Some(target::child(
            &game_root,
            &mod_root,
            &format!("Resources/HATS/{}", hat.config_value),
        )?)
    } else {
        None
    };
    Ok(PreviewContext {
        session_id: session_id.into(),
        resources_revision,
        game_root,
        chef_name: if chef.source_kind == "custom" {
            chef.config_name.clone()
        } else {
            chef.head_name.clone()
        },
        chef_directory,
        hat_value: hat.config_value.clone(),
        hat_directory,
    })
}

pub(crate) fn require_session(state: &AppState, id: &str) -> Result<(), String> {
    with_preview_session(state, id, |_| Ok(()))
}

/// Preview is read-only: authorize the exact scan/target/root on every call,
/// without probing processes, BepInEx backups or cloning the complete catalog.
/// Save/initialize retain their independent live game-stopped checks.
fn with_preview_session<T>(
    state: &AppState,
    id: &str,
    select: impl FnOnce(&Session) -> Result<T, String>,
) -> Result<T, String> {
    let current_game = state.config.load_or_default().game;
    let (root, mod_root, target, settings, value) = {
        let sessions = state.diychef_sessions.lock().map_err(|e| e.to_string())?;
        let s = sessions.values.get(id).ok_or("diychef.sessionExpired")?;
        (
            s.game_root.clone(),
            s.mod_root.clone(),
            s.target.clone(),
            s.game_settings.clone(),
            select(s)?,
        )
    };
    if current_game != settings {
        return Err("diychef.targetChanged".into());
    }
    // Empty settings use the root already authorized by a successful scan. A
    // changed runtime/path invalidates that generation and must reopen a session.
    let selected_path = if settings.path.trim().is_empty() {
        root.as_path()
    } else {
        std::path::Path::new(&settings.path)
    };
    if game::selected_game_root(selected_path, &settings).map_err(|_| "diychef.targetChanged")?
        != root
        || target::mod_root(&root, &target)? != mod_root
    {
        return Err("diychef.targetChanged".into());
    }
    state
        .local_mod_snapshot
        .lock()
        .map_err(|e| e.to_string())?
        .get(target.generation, &root)?;
    Ok(value)
}

#[cfg(test)]
mod tests;
