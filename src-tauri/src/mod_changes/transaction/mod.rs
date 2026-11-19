//! Persistent filesystem transaction shared by every action in a confirmed change plan.
//! This layer neither downloads nor solves dependencies and never acquires AppState locks.
mod durable;
mod engine;
mod fingerprint;
mod paths;
mod permissions;
mod prepare;
mod recovery;
mod tree;
mod types;

pub(crate) use durable::rename as durable_rename;
pub(super) use engine::RecordProgress;
pub(crate) use paths::{has_single_link, key as path_key};
pub(crate) use prepare::prepare;
pub(crate) use recovery::remove_private_tree;
pub(crate) use recovery::{has_pending, recover_all};
pub(crate) use tree::copy_tree as stage_tree;
pub(crate) use tree::preserve_replaced_permissions as preserve_replaced_tree_permissions;
#[cfg(test)]
pub(crate) use types::JournalState;
pub(crate) use types::{
    FileChange, Fingerprint, MoveChange, Mutation, RootKind, StagedFile, TransactionPath,
    TransactionRoots, TreeChange,
};

pub(crate) struct PreparedTransaction {
    directory: std::path::PathBuf,
    journal: types::Journal,
}

#[cfg(test)]
mod application_progress_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tree_tests;
