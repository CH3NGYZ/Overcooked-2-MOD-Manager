//! Sessions hold backend-owned paths, baselines and bounded editor state.
mod mutations;
use mutations::check_live;
pub(crate) use mutations::{initialize, save};

use super::{
    catalog::{self, Catalog},
    storage::{self, Stored},
    target, validation,
};
use crate::{app_state::AppState, game, models::*};
use std::{collections::HashMap, path::PathBuf};
#[derive(Default)]
pub(crate) struct ChefSessions {
    pub(super) values: HashMap<String, Session>,
}
#[derive(Clone)]
pub(crate) struct Session {
    pub id: String,
    pub game_root: PathBuf,
    pub mod_root: PathBuf,
    pub target: ChefTarget,
    pub catalog: Catalog,
    pub stored: Stored,
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
) -> Result<ChefEditorOpen, String> {
    let root = game_root(state)?;
    let snapshot = state
        .local_mod_snapshot
        .lock()
        .map_err(|e| e.to_string())?
        .get(generation, &root)?;
    let candidates = target::eligible(&target::candidates(&snapshot.mods, generation));
    let Some(target) = target::select(&candidates, target_id)? else {
        return Ok(ChefEditorOpen {
            status: if candidates.is_empty() {
                "not_installed"
            } else {
                "select_target"
            }
            .into(),
            candidates,
            snapshot: None,
        });
    };
    let mod_root = target::mod_root(&root, &target)?;
    let catalog = catalog::read(&root, &mod_root)?;
    let stored = storage::read(&root, &mod_root, &target.id)?;
    let session = Session {
        id: uuid::Uuid::new_v4().to_string(),
        game_root: root,
        mod_root,
        target,
        catalog,
        stored,
    };
    let view = view(&session)?;
    let mut sessions = state.diychef_sessions.lock().map_err(|e| e.to_string())?;
    if sessions.values.len() >= 16 {
        sessions.values.clear();
    }
    sessions.values.insert(session.id.clone(), session);
    Ok(ChefEditorOpen {
        status: "ready".into(),
        candidates,
        snapshot: Some(view),
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
    let selection = s.stored.document.selection(&s.catalog);
    ChefDraft {
        base_revision: s.stored.source.revision.clone(),
        catalog_revision: s.catalog.revision.clone(),
        target_generation: s.target.generation,
        entries: selection.entries,
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
        lobby_switch: draft.lobby_switch,
    };
    if s.stored.source.state == "missing" {
        selection.entries = vec![ChefEntry {
            entry_id: "default:black_cat".into(),
            row_id: None,
            chef_ref: ChefReference::Known {
                chef_id: "native:Chef_Black_cat".into(),
            },
            hat_ref: HatReference::Known {
                hat_id: "hat:".into(),
            },
        }];
        draft.entries = selection.entries.clone();
    }
    let diagnostics = validation::validate(&s.stored.document, &s.catalog, &draft)?.diagnostics;
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
    let mut result = validation::validate(&s.stored.document, &s.catalog, d)?;
    if s.target.compatibility != "fastinit_2_2" {
        result.diagnostics.push(validation::diagnostic(
            "diychef.unsupportedImplementation",
            None,
            true,
        ));
        result.save_blocking = true;
    }
    Ok(result)
}
#[derive(Clone, Debug)]
pub(crate) struct PreviewContext {
    pub session_id: String,
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
    let s = get(state, session_id)?;
    check_live(state, &s)?;
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
    if chef.resource_state == "invalid" || hat.resource_state == "invalid" {
        return Err("diychef.invalidResource".into());
    }
    let chef_directory = if chef.source_kind == "custom" {
        Some(target::child(
            &s.game_root,
            &s.mod_root,
            &format!("Resources/{}", chef.config_name),
        )?)
    } else {
        None
    };
    let hat_directory = if hat.kind == "custom" {
        Some(target::child(
            &s.game_root,
            &s.mod_root,
            &format!("Resources/HATS/{}", hat.config_value),
        )?)
    } else {
        None
    };
    Ok(PreviewContext {
        session_id: session_id.into(),
        game_root: s.game_root,
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
    get(state, id).map(|_| ())
}

#[cfg(test)]
mod tests;
