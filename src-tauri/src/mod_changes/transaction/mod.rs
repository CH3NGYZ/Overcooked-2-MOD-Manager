//! Persistent filesystem transaction shared by every action in a confirmed change plan.
//! This layer neither downloads nor solves dependencies and never acquires AppState locks.
mod check;
mod diagnostics;
mod durable;
mod engine;
mod fingerprint;
mod paths;
mod permissions;
mod prepare;
mod publication;
mod recovery;
mod recovery_control;
mod tree;
mod types;
#[cfg(windows)]
mod volume;

pub(super) use check::needs_recovery;
pub(crate) use durable::rename as durable_rename;
pub(super) use engine::RecordProgress;
pub(crate) use paths::{has_single_link, key as path_key};
#[cfg(test)]
pub(crate) use prepare::prepare;
pub(super) use prepare::{prepare_for_operation, verify_frozen_targets};
pub(crate) use recovery::remove_private_tree;
pub(crate) use recovery_control::{
    abandon_recovery, has_pending, has_retained_material, list_recoveries, pause_recovery,
    recover_all, retry_recovery,
};
pub(crate) use tree::copy_tree as stage_tree;
pub(crate) use tree::preserve_replaced_permissions as preserve_replaced_tree_permissions;
#[cfg(test)]
pub(crate) use types::JournalState;
pub(crate) use types::{
    FileChange, Fingerprint, MoveChange, Mutation, RecoveryDecisionState, RecoveryEntry,
    RecoveryPath, RecoveryPathKind, RecoveryReport, RootKind, StagedFile, StagedTree,
    TransactionPath, TransactionRoots, TreeChange,
};
#[cfg(windows)]
pub(super) use volume::check_write_roots;

pub(crate) struct PreparedTransaction {
    directory: std::path::PathBuf,
    journal: types::Journal,
}

pub(crate) const DIRECTORY_NAME: &str = "mod-change-transactions";

#[cfg(test)]
mod application_progress_tests;
#[cfg(test)]
mod publication_tests;
#[cfg(test)]
mod recovery_control_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tree_tests;
#[cfg(all(test, windows))]
mod volume_tests;
