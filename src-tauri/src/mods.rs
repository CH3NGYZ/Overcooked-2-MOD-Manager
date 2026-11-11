//! MOD metadata, scanning, binding and file operations have one owner per responsibility.

mod binding;
pub(crate) mod file_state;
mod identity;
pub(crate) mod install;
mod metadata;
mod relations;
mod scan;
pub(crate) mod snapshot;
pub(crate) mod status;
#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod tests;

pub(crate) use binding::rebind_installed_mods;
pub(crate) use file_state::{canonical_dll_name, disabled_dll_file, plugin_dll_files};
pub(crate) use identity::{compare_mod_versions, is_safe_replacement_mod, note_key};
pub(crate) use metadata::{
    DllDetails, MAX_DLL_BYTES, file_stem_name, parse_bepin_plugin_metadata,
    placeholder_dll_details, plugin_name_from_file, read_dll_details,
};
pub(crate) use relations::{
    effective_map_relations, effective_relations, select_release_relations,
};
pub(crate) use scan::scan_mods_with_manifests;
pub(crate) use status::{local_update_version, mod_update_state, remote_update_version};
#[cfg(test)]
pub(crate) use test_support::plugin_blob;
