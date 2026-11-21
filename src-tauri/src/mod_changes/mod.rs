//! Whole-chain relationship evaluation, planning, verified staging and durable file transactions.
mod actions;
pub(crate) mod application_progress;
pub(crate) mod candidates;
pub(crate) mod evaluate;
pub(crate) mod execution;
pub(crate) mod guard;
mod install_notes;
pub(crate) mod inventory;
pub(crate) mod inventory_maps;
pub(crate) mod level_trees;
mod live_game;
pub(crate) mod map_payload;
pub(crate) mod model;
pub(crate) mod mutations;
pub(crate) mod payload;
pub(crate) mod planning;
pub(crate) mod preparation;
pub(crate) mod progress;
pub(crate) mod session;
pub(crate) mod solve;
pub(crate) mod transaction;

pub(crate) use evaluate::evaluate;
#[cfg(test)]
pub(crate) use solve::solve;
#[cfg(test)]
pub(crate) use solve::solve_with_inventory;

#[cfg(test)]
mod orchestration_tests;
#[cfg(test)]
mod tests;
