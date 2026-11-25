use super::{
    application_progress::OwnedMutation,
    evaluate::{relation_active, targets},
    guard, inventory,
    session::PlanSession,
    transaction::{
        Mutation, RecoveryEntry, RecoveryPathKind, RootKind, TransactionPath, TransactionRoots,
    },
};
use crate::{
    app_state::AppState, display_path::display_path, file_metadata::FileIdentity, game, mods,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
};

const CONFLICT_CHANGED: &str = "modChanges.recoveryConflictChanged";
const MATERIAL_PROTECTED: &str = "modChanges.recoveryMaterialProtected";

pub(super) fn revisions(records: &[RecoveryEntry]) -> BTreeMap<String, String> {
    records
        .iter()
        .map(|record| (record.id.clone(), record.revision.clone()))
        .collect()
}

pub(super) fn check_plan_revisions(
    state: &AppState,
    session: &PlanSession,
    own_transaction: Option<&str>,
) -> Result<Vec<RecoveryEntry>, String> {
    let records = guard::records_for_plan(state, own_transaction)?;
    if revisions(&records) != session.recovery_revisions {
        return Err(CONFLICT_CHANGED.into());
    }
    Ok(records)
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Conflict {
    transaction_id: String,
    paths: Vec<String>,
}

#[derive(Default)]
pub(super) struct Review {
    pub conflicts: Vec<Conflict>,
    pub approval: Option<String>,
}

impl Review {
    pub(super) fn verify(
        &self,
        offered: Option<&str>,
        confirmed: Option<&str>,
    ) -> Result<(), String> {
        match (self.approval.as_deref(), offered, confirmed) {
            (None, None, None) => Ok(()),
            (Some(current), Some(previous), Some(approved))
                if current == previous && approved == current =>
            {
                Ok(())
            }
            (Some(_), _, None) => Err(guard::REQUIRED.into()),
            _ => Err(CONFLICT_CHANGED.into()),
        }
    }
}

struct Location {
    components: Vec<OsString>,
    identity: Option<FileIdentity>,
}

impl Location {
    fn read(path: &Path) -> Result<Self, String> {
        let canonical = game::resolve_existing_ancestor(path).map_err(|_| guard::REQUIRED)?;
        let components = canonical
            .components()
            .map(|component| component.as_os_str().to_owned())
            .collect();
        let identity = match fs::metadata(&canonical) {
            Ok(metadata) if metadata.is_file() => {
                FileIdentity::capture(&canonical).map_err(|_| guard::REQUIRED)?
            }
            Ok(_) => None,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => return Err(guard::REQUIRED.into()),
        };
        Ok(Self {
            components,
            identity,
        })
    }

    fn overlaps(&self, other: &Self) -> bool {
        self.components
            .iter()
            .zip(&other.components)
            .all(|(left, right)| same_component(left, right))
            || self
                .identity
                .as_ref()
                .is_some_and(|identity| Some(identity) == other.identity.as_ref())
    }
}

#[cfg(windows)]
fn same_component(left: &OsStr, right: &OsStr) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use winapi::{shared::minwindef::TRUE, um::stringapiset::CompareStringOrdinal};
    const CSTR_EQUAL: i32 = 2;
    if left == right {
        return true;
    }
    let left = left.encode_wide().collect::<Vec<_>>();
    let right = right.encode_wide().collect::<Vec<_>>();
    // SAFETY: slices remain live; the lengths describe their initialized contents.
    let result = unsafe {
        CompareStringOrdinal(
            left.as_ptr(),
            left.len() as i32,
            right.as_ptr(),
            right.len() as i32,
            TRUE,
        )
    };
    result == CSTR_EQUAL || result == 0
}

#[cfg(not(windows))]
use unicode_component_eq as same_component;

#[cfg(any(not(windows), test))]
fn unicode_component_eq(left: &OsStr, right: &OsStr) -> bool {
    let normalizer = icu_normalizer::ComposingNormalizer::new_nfc();
    left == right
        || normalizer.normalize(&left.to_string_lossy().to_lowercase())
            == normalizer.normalize(&right.to_string_lossy().to_lowercase())
}

fn add_endpoint(
    paths: &mut BTreeSet<PathBuf>,
    roots: &TransactionRoots,
    target: &TransactionPath,
) -> Result<(), String> {
    paths.insert(target.resolve(roots)?);
    Ok(())
}

fn add_dll_variants(paths: &mut BTreeSet<PathBuf>, game_root: &Path, relative: &str) {
    let enabled = mods::canonical_dll_name(relative);
    paths.insert(game_root.join(mods::disabled_dll_file(&enabled)));
    paths.insert(game_root.join(enabled));
}

fn read_paths(
    state: &AppState,
    session: &PlanSession,
    roots: &TransactionRoots,
    paths: &mut BTreeSet<PathBuf>,
) -> Result<(), String> {
    paths.extend(session.payload_fingerprints.keys().cloned());
    for target in session.release_target_claims.proof_read_paths() {
        add_endpoint(paths, roots, &target)?;
    }
    // The ownership index reads all install manifests as well as both shared map records.
    paths.insert(state.config.manifests_dir());
    paths.insert(state.config.config_path());
    paths.insert(crate::bepinex::core_dir(&roots.game_root));
    let solved = session.solved.as_ref().ok_or("modChanges.notPrepared")?;
    let installed = &session.trusted.inventory;
    let affected = inventory::affected_nodes(
        installed,
        &session.request,
        solved,
        &session.unbound_toggle_roots(),
    );
    let mut reads = affected.clone();
    for graph in [installed, solved] {
        for node in graph.nodes.iter().filter(|node| node.enabled) {
            if let Some(candidate) = &node.current {
                for owned in &candidate.relations {
                    if !relation_active(candidate, owned, false) {
                        continue;
                    }
                    for (index, _) in targets(graph, &owned.relation.guid, false) {
                        let counterpart = graph.nodes[index].id.as_str();
                        if affected.contains(node.id.as_str()) || affected.contains(counterpart) {
                            reads.insert(&node.id);
                            reads.insert(counterpart);
                        }
                    }
                }
            }
        }
    }
    for files in reads
        .into_iter()
        .filter_map(|id| session.trusted.files.get(id))
    {
        if let Some(path) = &files.map_path {
            paths.insert(roots.game_root.join(path));
        }
        for member in &files.summary.members {
            add_dll_variants(paths, &roots.game_root, &member.dll_file);
        }
    }
    Ok(())
}

pub(super) fn review(
    state: &AppState,
    session: &PlanSession,
    roots: &TransactionRoots,
    changes: &[OwnedMutation],
    own_transaction: Option<&str>,
) -> Result<Review, String> {
    let records = check_plan_revisions(state, session, own_transaction)?;
    if records.is_empty() || changes.is_empty() {
        return Ok(Review::default());
    }
    let mut reads = BTreeSet::new();
    let mut writes = BTreeSet::new();
    for change in changes {
        match &change.mutation {
            Mutation::File(change) => {
                add_endpoint(&mut writes, roots, &change.target)?;
                if change.target.root == RootKind::Game
                    && mods::file_state::is_mod_file(Path::new(&change.target.relative))
                {
                    add_dll_variants(&mut reads, &roots.game_root, &change.target.relative);
                }
                reads.extend(change.source.iter().map(|source| source.path.clone()));
            }
            Mutation::Tree(change) => {
                add_endpoint(&mut writes, roots, &change.target)?;
                reads.extend(change.source.iter().map(|source| source.path.clone()));
            }
            Mutation::Move(change) => {
                add_endpoint(&mut writes, roots, &change.from)?;
                add_endpoint(&mut writes, roots, &change.to)?;
            }
        }
    }
    if records.iter().any(|record| {
        record
            .paths
            .iter()
            .any(|path| path.kind != RecoveryPathKind::Journal)
    }) {
        read_paths(state, session, roots, &mut reads)?;
    }
    let read_locations = reads
        .iter()
        .map(|path| Location::read(path))
        .collect::<Result<Vec<_>, _>>()?;
    let write_locations = writes
        .iter()
        .map(|path| Location::read(path))
        .collect::<Result<Vec<_>, _>>()?;
    let mut conflicts = Vec::new();
    let mut revisions = BTreeMap::new();
    for record in records {
        let mut paths = BTreeSet::new();
        for affected in &record.paths {
            let location = Location::read(&affected.path)?;
            let write_overlap = write_locations.iter().any(|path| path.overlaps(&location));
            if write_overlap
                && matches!(
                    affected.kind,
                    RecoveryPathKind::Backup
                        | RecoveryPathKind::Staging
                        | RecoveryPathKind::Journal
                )
            {
                return Err(MATERIAL_PROTECTED.into());
            }
            if write_overlap || read_locations.iter().any(|path| path.overlaps(&location)) {
                paths.insert(display_path(&affected.path));
            }
        }
        if !paths.is_empty() {
            revisions.insert(record.id.clone(), record.revision);
            conflicts.push(Conflict {
                transaction_id: record.id,
                paths: paths.into_iter().collect(),
            });
        }
    }
    let approval = if conflicts.is_empty() {
        None
    } else {
        let evidence = serde_json::to_vec(&(
            session.operation_id(),
            &session.trusted.revision,
            changes
                .iter()
                .map(|change| &change.mutation)
                .collect::<Vec<_>>(),
            &reads,
            &writes,
            &conflicts,
            revisions,
        ))
        .map_err(|_| guard::REQUIRED)?;
        Some(format!("{:x}", Sha256::digest(evidence)))
    };
    Ok(Review {
        conflicts,
        approval,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_path_components_match_across_normalization_forms() {
        let parent = tempfile::tempdir().unwrap();
        let composed = Location::read(&parent.path().join("Café")).unwrap();
        let decomposed = Location::read(&parent.path().join("CAFE\u{301}")).unwrap();
        let different = Location::read(&parent.path().join("Cafe")).unwrap();
        assert!(composed.identity.is_none() && decomposed.identity.is_none());
        let matches = |other: &Location| {
            composed
                .components
                .iter()
                .zip(&other.components)
                .all(|(left, right)| unicode_component_eq(left, right))
        };
        assert!(matches(&decomposed));
        assert!(!matches(&different));
        #[cfg(not(windows))]
        assert!(composed.overlaps(&decomposed));
    }
}
