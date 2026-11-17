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

pub(crate) use prepare::prepare;
pub(crate) use recovery::{has_pending, recover_all};
pub(crate) use tree::copy_tree as stage_tree;
pub(crate) use tree::preserve_replaced_permissions as preserve_replaced_tree_permissions;
#[cfg(test)]
pub(crate) use types::JournalState;
pub(crate) use types::{
    FileChange, Fingerprint, MoveChange, Mutation, RootKind, TransactionPath, TransactionRoots,
    TreeChange,
};

pub(crate) struct PreparedTransaction {
    directory: std::path::PathBuf,
    journal: types::Journal,
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tree_tests;
