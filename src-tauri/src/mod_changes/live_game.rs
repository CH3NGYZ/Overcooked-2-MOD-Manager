//! Only map data and manager metadata may change while the game is running.
use super::{
    application_progress::OwnedMutation, guard, model::NodeKind, mutations, session::PlanSession,
    transaction::*,
};
use crate::{app_state::AppState, mods::file_state::is_mod_file};
use std::path::Path;

/// Avoid preparing downloads for MOD actions the trusted solver already knows about.
pub(super) fn check_plan(state: &AppState, session: &PlanSession) -> Result<(), String> {
    if session.plan.result.actions.iter().any(|action| {
        session
            .trusted
            .files
            .get(&action.node_id)
            .is_some_and(|files| files.summary.kind == NodeKind::Mod)
    }) {
        guard::ensure_game_stopped(state, &session.trusted.game_root)?;
    }
    Ok(())
}

/// Inspect concrete writes, including members of a map tree and files that
/// have no independently visible solver action (e.g. a bundled config file).
pub(super) fn touches_mods(
    session: &PlanSession,
    roots: &TransactionRoots,
    changes: &[OwnedMutation],
) -> Result<bool, String> {
    let solved = session.solved.as_ref().ok_or("modChanges.notPrepared")?;
    let map_roots = mutations::map_roots(session, solved)?;
    let outside_maps = |path: &TransactionPath| {
        path.root == RootKind::Game
            && !map_roots
                .iter()
                .any(|root| mutations::within(&path.relative, root))
    };
    let dll = |path: &TransactionPath| {
        path.root == RootKind::Game && is_mod_file(Path::new(&path.relative))
    };
    for owned in changes {
        let affects_mod = match &owned.mutation {
            Mutation::File(change) => outside_maps(&change.target) || dll(&change.target),
            Mutation::Tree(change) if change.target.root == RootKind::Game => {
                outside_maps(&change.target)
                    || (change.expected != Fingerprint::Missing
                        && contains_dll(&change.target.resolve(roots)?)?)
                    || match &change.source {
                        Some(source) => contains_dll(&source.path)?,
                        None => false,
                    }
            }
            Mutation::Tree(_) => false,
            Mutation::Move(change) => {
                let in_game =
                    change.from.root == RootKind::Game || change.to.root == RootKind::Game;
                outside_maps(&change.from)
                    || outside_maps(&change.to)
                    || (in_game
                        && match change.expected {
                            Fingerprint::Directory { .. } => {
                                contains_dll(&change.from.resolve(roots)?)?
                            }
                            _ => dll(&change.from) || dll(&change.to),
                        })
            }
        };
        if affects_mod {
            return Ok(true);
        }
    }
    Ok(false)
}

fn contains_dll(tree: &Path) -> Result<bool, String> {
    for entry in walkdir::WalkDir::new(tree)
        .follow_links(false)
        .follow_root_links(false)
    {
        let entry = entry.map_err(|_| "modChanges.targetChanged")?;
        if entry.file_type().is_file() && is_mod_file(entry.path()) {
            return Ok(true);
        }
    }
    Ok(false)
}
