use super::{Session, check_draft, game_root, get, validate, view};
use crate::diychef::{catalog, prefer, storage, target, validation};
use crate::{app_state::AppState, game, models::*, mods};
fn recheck_target(state: &AppState, s: &Session) -> Result<(), String> {
    let config = state.config.load_or_default();
    let current =
        mods::scan_mods_with_manifests(&s.game_root, &config, Some(&state.config.manifests_dir()));
    let candidates = target::candidates(&current, s.target.generation);
    let selected =
        target::select(&candidates, Some(&s.target.id))?.ok_or("diychef.targetChanged")?;
    if selected.compatibility != s.target.compatibility
        || selected.version != s.target.version
        || selected.enabled != s.target.enabled
        || target::mod_root(&s.game_root, &selected)? != s.mod_root
    {
        return Err("diychef.targetChanged".into());
    }
    Ok(())
}
pub(crate) fn save(
    state: &AppState,
    id: &str,
    d: &ChefDraft,
) -> Result<ChefEditorSnapshot, String> {
    let mut s = get(state, id)?;
    check_draft(&s, d)?;
    // Initialization establishes the disk baseline separately, so a missing
    // draft cannot bypass its exclusive-create and conflict handling via save.
    if s.stored.source.state != "present" {
        return Err("diychef.conflict".into());
    }
    recheck_target(state, &s)?;
    let c = catalog::read(&s.game_root, &s.mod_root)?;
    if c.revision != s.catalog.revision {
        return Err("diychef.conflict".into());
    }
    let validation = validate(state, id, d)?;
    if validation.save_blocking {
        if validation.diagnostics.iter().any(|problem| {
            problem.code == validation::NATIVE_DEFAULT_REQUIRED && problem.save_blocking
        }) {
            return Err(validation::NATIVE_DEFAULT_REQUIRED.into());
        }
        return Err("diychef.invalidDraft".into());
    }
    if validation
        .changes
        .iter()
        .any(|x| !d.acknowledged_change_ids.contains(&x.id))
    {
        return Err("diychef.confirmChanges".into());
    }
    let (bytes, _) = crate::diychef::prefer::plan(&s.stored.document, d, &s.catalog)?;
    let _operation = state.operation.lock().map_err(|e| e.to_string())?;
    game::ensure_game_stopped(&s.game_root)?;
    check_live(state, &s)?;
    s.stored = storage::write(
        &s.game_root,
        &s.mod_root,
        &s.target.id,
        &s.bindings_dir,
        &d.base_revision,
        &bytes,
        &d.hat_bindings,
        false,
    )?;
    let view = view(&s)?;
    state
        .diychef_sessions
        .lock()
        .map_err(|e| e.to_string())?
        .values
        .insert(id.into(), s);
    Ok(view)
}
pub(super) fn check_live(state: &AppState, s: &Session) -> Result<(), String> {
    if game_root(state)? != s.game_root || target::mod_root(&s.game_root, &s.target)? != s.mod_root
    {
        return Err("diychef.targetChanged".into());
    }
    state
        .local_mod_snapshot
        .lock()
        .map_err(|e| e.to_string())?
        .get(s.target.generation, &s.game_root)?;
    Ok(())
}
pub(crate) fn initialize(
    state: &AppState,
    id: &str,
    expected: &str,
) -> Result<ChefEditorSnapshot, String> {
    let mut s = get(state, id)?;
    recheck_target(state, &s)?;
    let _operation = state.operation.lock().map_err(|e| e.to_string())?;
    game::ensure_game_stopped(&s.game_root)?;
    check_live(state, &s)?;
    let (bytes, hat_bindings) = initialization_plan(&s)?;
    s.stored = storage::write(
        &s.game_root,
        &s.mod_root,
        &s.target.id,
        &s.bindings_dir,
        expected,
        &bytes,
        &hat_bindings,
        true,
    )?;
    let v = view(&s)?;
    state
        .diychef_sessions
        .lock()
        .map_err(|e| e.to_string())?
        .values
        .insert(id.into(), s);
    Ok(v)
}

/// Build the missing-file baseline using the same complete selection shown by
/// the editor; valid saved bindings take precedence over default automatic hats.
pub(super) fn initialization_plan(
    s: &Session,
) -> Result<(Vec<u8>, std::collections::BTreeMap<String, HatReference>), String> {
    // The missing-file snapshot already merges valid manager bindings with the
    // default chef. Generate from it, so initialization cannot reset a saved hat.
    let selection = view(s)?.selection;
    let mut draft = super::baseline_draft(s);
    draft.entries = selection.entries;
    draft.hat_bindings = selection.hat_bindings;
    draft.lobby_switch = selection.lobby_switch;
    let default = prefer::Document::parse(prefer::DEFAULT_CONTENT)?;
    let (bytes, _) = prefer::plan(&default, &draft, &s.catalog)?;
    Ok((bytes, draft.hat_bindings))
}
