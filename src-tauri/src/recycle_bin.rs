//! Recoverable removal: staged files, journals and active manifests form one transaction.

mod delete;
mod directories;
mod errors;
mod file_io;
mod legacy_resources;
mod manifests;
mod paths;
mod payload;
mod records;
mod repository;
mod restore;
mod selection;
mod staging;

#[cfg(test)]
mod tests;

pub(crate) use delete::{
    delete, delete_all, purge_entry, purge_mod_dll, purge_package, purge_selected_mod,
    purge_unmanaged,
};
pub(crate) use directories::{directory_files, recycle_directories};
pub(crate) use legacy_resources::ensure_resource_root_unshared;
pub(crate) use payload::{VerifiedPayloadFile, VerifiedPayloadRecord, verified_payload_records};
pub(crate) use records::{list, original_game_path, set_diychef_resources_imported};
pub(crate) use repository::recycle_repository_packages_before_install;
pub(crate) use restore::restore;
pub(crate) use selection::{
    recycle_mod_dll, recycle_package, recycle_selected_mod, recycle_unmanaged,
};
pub(crate) use staging::stage;
