use super::{Session, baseline_draft, check_draft, game_root, get, validate, view};
use crate::diychef::{catalog, prefer, storage, target, validation};
use crate::{app_state::AppState, game, models::*, mods, recycle_bin};
use std::fs;
use std::path::{Path, PathBuf};

enum DeletedResource {
    Recycled(String),
    EmptyTree(PathBuf),
}

/// Resolve one custom chef's resource directory from the session-owned target.
///
/// The catalog is the authority for chef identity; the frontend never supplies
/// a path. `target::child` then resolves the path under the selected game root,
/// while the direct-parent check prevents a forged catalog entry from reaching
/// `Resources/HATS` or a nested/escaped directory.
fn custom_resource_path(
    game_root: &Path,
    mod_root: &Path,
    source_kind: &str,
    config_name: &str,
) -> Result<PathBuf, String> {
    if source_kind != "custom" {
        return Err("diychef.invalidResource".into());
    }
    let resources = target::child(game_root, mod_root, "Resources")?;
    let resource = target::child(game_root, mod_root, &format!("Resources/{config_name}"))?;
    if config_name.eq_ignore_ascii_case("HATS")
        || !catalog::token(config_name)
        || resource.parent() != Some(resources.as_path())
    {
        return Err("diychef.invalidResource".into());
    }
    Ok(resource)
}
fn check_delete_allowed(source_kind: &str, source_present: bool) -> Result<(), String> {
    if source_kind != "custom" || !source_present {
        return Err("diychef.invalidResource".into());
    }
    Ok(())
}

fn resource_game_relative(game_root: &Path, resource: &Path) -> Result<String, String> {
    Ok(resource
        .strip_prefix(game_root)
        .map_err(|_| "archive.pathEscape")?
        .to_string_lossy()
        .replace('\\', "/"))
}

fn recycle_chef_resource(
    recycle_root: &Path,
    manifests_dir: &Path,
    game_root: &Path,
    mod_root: &Path,
    resource: &Path,
    config_name: &str,
) -> Result<DeletedResource, String> {
    let relative = resource_game_relative(game_root, resource)?;
    let files = recycle_bin::directory_files(game_root, &relative)?;
    if files.is_empty() {
        let staged = mod_root.join(format!(".oc2-diychef-delete-{}", uuid::Uuid::new_v4()));
        fs::rename(resource, &staged).map_err(|_| "diychef.saveFailed")?;
        return Ok(DeletedResource::EmptyTree(staged));
    }
    let record = recycle_bin::recycle_directories(
        recycle_root,
        manifests_dir,
        game_root,
        &[relative],
        format!("DIYChef: {config_name}"),
        false,
    )?;
    Ok(DeletedResource::Recycled(record.id))
}

fn restore_resource(
    recycle_root: &Path,
    manifests_dir: &Path,
    game_root: &Path,
    resource: &Path,
    deleted: &DeletedResource,
) -> Result<(), String> {
    let result = match deleted {
        DeletedResource::Recycled(id) => {
            recycle_bin::restore(recycle_root, manifests_dir, game_root, &id)
        }
        DeletedResource::EmptyTree(staged) => {
            fs::rename(staged, resource).map_err(|_| "diychef.writtenReadFailed".into())
        }
    };
    result.map_err(|_| "diychef.writtenReadFailed".to_string())?;
    Ok(())
}

fn finish_resource_delete(deleted: DeletedResource) {
    if let DeletedResource::EmptyTree(staged) = deleted
        && let Err(error) = fs::remove_dir_all(staged)
    {
        log::warn!("清理空的 DIYChef 厨师目录失败：{error}");
    }
}

fn restore_deleted_resource(
    state: &AppState,
    session: &Session,
    resource: &Path,
    deleted: &DeletedResource,
    expected_revision: &str,
    original: &storage::Stored,
) -> Result<(), String> {
    let restored = restore_deleted_config(session, expected_revision, original)?;
    if restore_resource(
        &state.config.mod_recycle_bin_dir(),
        &state.config.manifests_dir(),
        &session.game_root,
        resource,
        deleted,
    )
    .is_err()
    {
        let deleted_bytes = session
            .stored
            .document
            .encode(&session.stored.document.text());
        if storage::write(
            &session.game_root,
            &session.mod_root,
            &session.target.id,
            &session.bindings_dir,
            &restored.source.revision,
            &deleted_bytes,
            &session.stored.hat_bindings,
            false,
        )
        .is_err()
        {
            return Err("diychef.writtenReadFailed".into());
        }
        return Err("diychef.writtenReadFailed".into());
    }
    state
        .diychef_watch
        .mark_resources_changed(&session.mod_root);
    Ok(())
}

fn commit_deleted_session(state: &AppState, id: &str, session: Session) {
    let mut sessions = state
        .diychef_sessions
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    sessions.values.insert(id.into(), session);
}

fn restore_deleted_config(
    session: &Session,
    expected_revision: &str,
    original: &storage::Stored,
) -> Result<storage::Stored, String> {
    let original_bytes = original.document.encode(&original.document.text());
    storage::write(
        &session.game_root,
        &session.mod_root,
        &session.target.id,
        &session.bindings_dir,
        expected_revision,
        &original_bytes,
        &original.hat_bindings,
        false,
    )
}

fn deletion_plan(s: &Session, chef_id: &str) -> Result<(ChefDraft, Vec<u8>), String> {
    let mut draft = baseline_draft(s);
    draft.entries.retain(
        |entry| !matches!(&entry.chef_ref, ChefReference::Known { chef_id: id } if id == chef_id),
    );
    draft.hat_bindings.remove(chef_id);

    let checked = validation::validate(
        &s.stored.document,
        &s.catalog,
        &draft,
        s.target.supports_custom_default,
    )?;
    if checked.save_blocking {
        if checked.diagnostics.iter().any(|problem| {
            problem.code == validation::NATIVE_FALLBACK_REQUIRED && problem.save_blocking
        }) {
            return Err(validation::NATIVE_FALLBACK_REQUIRED.into());
        }
        if checked.diagnostics.iter().any(|problem| {
            problem.code == validation::NATIVE_DEFAULT_REQUIRED && problem.save_blocking
        }) {
            return Err(validation::NATIVE_DEFAULT_REQUIRED.into());
        }
        return Err("diychef.invalidDraft".into());
    }

    let (bytes, _) = prefer::plan(&s.stored.document, &draft, &s.catalog)?;
    Ok((draft, bytes))
}
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

/// Delete one custom chef and its manager-owned state.
///
/// The resource directory is staged outside `Resources` before the
/// prefer/binding pair is replaced. A failed write restores the directory so
/// the catalog and configuration cannot be left half-updated.
pub(crate) fn delete_resource(
    state: &AppState,
    id: &str,
    chef_id: &str,
) -> Result<ChefEditorSnapshot, String> {
    let mut s = get(state, id)?;
    let chef = s
        .catalog
        .chefs
        .iter()
        .find(|chef| chef.id == chef_id)
        .cloned()
        .ok_or("diychef.invalidResource")?;
    let resource = custom_resource_path(
        &s.game_root,
        &s.mod_root,
        &chef.source_kind,
        &chef.config_name,
    )?;

    // Perform the inexpensive session check before waiting for the global
    // operation lock, then repeat the live target/catalog/revision checks while
    // holding the lock immediately before the filesystem mutation.
    recheck_target(state, &s)?;
    let _operation = state.operation.lock().map_err(|e| e.to_string())?;
    game::ensure_game_stopped(&s.game_root)?;
    check_live(state, &s)?;
    recheck_target(state, &s)?;

    let live_catalog = catalog::read(&s.game_root, &s.mod_root)?;
    if live_catalog.revision != s.catalog.revision {
        return Err("diychef.conflict".into());
    }
    if state.diychef_watch.resources_revision(&s.mod_root) != s.resources_revision {
        return Err("diychef.conflict".into());
    }

    check_delete_allowed(&chef.source_kind, s.stored.source.state == "present")?;
    let (draft, bytes) = deletion_plan(&s, chef_id)?;
    let old_stored = s.stored.clone();

    let metadata = fs::symlink_metadata(&resource).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            "diychef.invalidResource".to_string()
        } else {
            "diychef.readFailed".to_string()
        }
    })?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        return Err("archive.pathEscape".into());
    }
    // Record resource files in the shared recoverable removal journal before
    // changing prefer/bindings. Empty resource trees use an adjacent rename.
    let deleted = recycle_chef_resource(
        &state.config.mod_recycle_bin_dir(),
        &state.config.manifests_dir(),
        &s.game_root,
        &s.mod_root,
        &resource,
        &chef.config_name,
    )?;
    let saved = match storage::write(
        &s.game_root,
        &s.mod_root,
        &s.target.id,
        &s.bindings_dir,
        &s.stored.source.revision,
        &bytes,
        &draft.hat_bindings,
        false,
    ) {
        Ok(value) => value,
        Err(error) => {
            if restore_resource(
                &state.config.mod_recycle_bin_dir(),
                &state.config.manifests_dir(),
                &s.game_root,
                &resource,
                &deleted,
            )
            .is_err()
            {
                return Err("diychef.writtenReadFailed".into());
            }
            state.diychef_watch.mark_resources_changed(&s.mod_root);
            return Err(error);
        }
    };
    // Keep the staged directory until every fallible read/preview step has
    // completed, so failures can put both the resources and config back.
    let saved_revision = saved.source.revision.clone();
    s.stored = saved;
    s.catalog = match catalog::read(&s.game_root, &s.mod_root) {
        Ok(catalog) => catalog,
        Err(error) => {
            return match restore_deleted_resource(
                state,
                &s,
                &resource,
                &deleted,
                &saved_revision,
                &old_stored,
            ) {
                Ok(()) => Err(error),
                Err(_) => Err("diychef.writtenReadFailed".into()),
            };
        }
    };
    s.resources_revision = state.diychef_watch.mark_resources_changed(&s.mod_root);
    let snapshot = match view(&s) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            return match restore_deleted_resource(
                state,
                &s,
                &resource,
                &deleted,
                &saved_revision,
                &old_stored,
            ) {
                Ok(()) => Err(error),
                Err(_) => Err("diychef.writtenReadFailed".into()),
            };
        }
    };
    finish_resource_delete(deleted);
    commit_deleted_session(state, id, s);
    Ok(snapshot)
}

#[cfg(test)]
#[path = "mutations/tests.rs"]
mod tests;

#[cfg(windows)]
fn is_reparse_point(info: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    info.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_: &fs::Metadata) -> bool {
    false
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
            problem.code == validation::NATIVE_FALLBACK_REQUIRED && problem.save_blocking
        }) {
            return Err(validation::NATIVE_FALLBACK_REQUIRED.into());
        }
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
