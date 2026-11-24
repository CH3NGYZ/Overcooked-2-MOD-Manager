use super::{inventory, mutations, session::PlanSession, transaction::verify_frozen_targets};
use crate::{app_state::AppState, game, models::AppConfig};
use std::path::{Path, PathBuf};

const TARGET_CHANGED: &str = "modChanges.targetChanged";

pub(super) fn current_target(state: &AppState) -> Result<(PathBuf, AppConfig), String> {
    let config = state.config.load().map_err(|_| TARGET_CHANGED)?;
    let detected = game::detect_game(&config).path.ok_or(TARGET_CHANGED)?;
    let root = game::resolve_existing_ancestor(Path::new(&detected)).map_err(|_| TARGET_CHANGED)?;
    Ok((root, config))
}

pub(super) fn check_scope(
    state: &AppState,
    session: &PlanSession,
    root: &Path,
    config: &AppConfig,
) -> Result<String, String> {
    if root != session.trusted.game_root {
        return Err(TARGET_CHANGED.into());
    }
    let scope = inventory::replan_scope(state, root, config)?;
    if session.trusted.replan_scope.as_deref() != Some(scope.as_str()) {
        return Err(TARGET_CHANGED.into());
    }
    Ok(scope)
}

pub(super) fn check_revision(
    state: &AppState,
    session: &PlanSession,
    root: &Path,
    scope: &str,
    recovered: bool,
    frozen: Option<&[super::application_progress::OwnedMutation]>,
) -> Result<(), String> {
    let revision = inventory::revision_for_scope(state, root, scope)?;
    let (current_root, config) = current_target(state)?;
    check_scope(state, session, &current_root, &config)?;
    if revision == session.trusted.revision {
        return Ok(());
    }
    if recovered || !session.may_replan_inventory() {
        return Err(TARGET_CHANGED.into());
    }
    let roots = mutations::transaction_roots(state, session)?;
    if let Some(batch) = &session.map_payload {
        batch.verify_claims(&roots, session)?;
    }
    if !session.release_target_claims.is_empty() {
        let solved = session.solved.as_ref().ok_or(TARGET_CHANGED)?;
        session
            .release_target_claims
            .verify(state, session, solved)?;
    }
    if let Some(changes) = frozen {
        verify_frozen_targets(&roots, changes.iter().map(|change| &change.mutation))
            .map_err(|_| TARGET_CHANGED)?;
    } else if !session.plan.retired_mod_copies.is_empty() {
        return Err(TARGET_CHANGED.into());
    }
    Err("modChanges.inventoryChanged".into())
}
