//! Recoverable removal: staged files, journals and active manifests form one transaction.

mod delete;
mod errors;
mod file_io;
mod manifests;
mod paths;
mod records;
mod repository;
mod restore;
mod selection;
mod staging;

#[cfg(test)]
mod tests;

pub(crate) use delete::{
    delete, delete_all, purge_mod_dll, purge_package, purge_selected_mod, purge_unmanaged,
};
pub(crate) use records::{list, original_game_path};
pub(crate) use repository::recycle_repository_packages_before_install;
pub(crate) use restore::restore;
pub(crate) use selection::{
    recycle_mod_dll, recycle_package, recycle_selected_mod, recycle_unmanaged,
};
pub(crate) use staging::stage;
