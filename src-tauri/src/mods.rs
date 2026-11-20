//! MOD metadata, scanning, binding and file operations have one owner per responsibility.

mod binding;
pub(crate) mod file_state;
mod identity;
pub(crate) mod install;
mod metadata;
pub(crate) mod plugin_target;
mod relations;
pub(crate) mod repository_download;
mod scan;
pub(crate) mod snapshot;
pub(crate) mod status;
#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod tests;
pub(crate) mod watch;

pub(crate) use binding::{
    installed_prerelease, legacy_diylevel_stub, rebind_installed_mods, repository_matches_identity,
};
pub(crate) use file_state::{canonical_dll_name, disabled_dll_file, manifest_dll_key};
pub(crate) use identity::{
    PluginIdentity, compare_mod_versions, is_newer_duplicate, is_safe_replacement_mod, note_key,
    same_plugin_identity, same_release_plugin_identity,
};
#[cfg(test)]
pub(crate) use metadata::parse_bepin_plugin_metadata;
pub(crate) use metadata::{
    DllDetails, MAX_DLL_BYTES, file_stem_name, placeholder_dll_details, read_dll_details,
};
pub(crate) use relations::{
    effective_map_relations, effective_relations, select_release_relations,
};
pub(crate) use scan::scan_mods_with_manifests;
pub(crate) use status::local_update_version;
#[cfg(test)]
pub(crate) use test_support::plugin_blob;
