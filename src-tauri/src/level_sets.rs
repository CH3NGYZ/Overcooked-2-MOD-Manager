//! 关卡集领域入口；子模块分别拥有扫描、识别、安装和文件事务。
//! 停用地图移出 plugins 树，避免作者随包 DLL 仍被 BepInEx 加载。

mod bepinex_layer;
mod conflicts;
mod flatten;
mod identity;
mod info_cache;
mod install;
mod installed_versions;
mod operations;
mod paths;
mod plugin_assignment;
mod plugin_registry;
mod requirements;
mod scan;
mod zip_info;
mod zip_inspect;
mod zip_plugins;

pub(crate) mod staging;
pub(crate) mod view;

pub use conflicts::find_install_conflicts_with_workers;
pub use flatten::flatten_level_sets;
pub use identity::level_set_identity_matches;
pub use info_cache::clear_info_cache;
pub(crate) use info_cache::read_info_metadata;
pub use install::install_parsed_sets;
pub use installed_versions::installed_mapset_version;
pub use operations::locate_level_set_directory;
pub use operations::remove_level_set;
pub use operations::set_all_level_sets_enabled;
pub use operations::set_level_set_enabled;
pub use paths::disabled_root;
pub(crate) use paths::legacy_disabled_root;
pub(crate) use paths::levels_root;
#[cfg(test)]
pub use plugin_registry::LEVEL_SET_PLUGIN_REGISTRY;
pub use plugin_registry::registered_plugin_paths;
pub use requirements::runtime_relation;
pub use scan::LevelSetOnDisk;
#[cfg(test)]
pub use scan::scan_level_sets;
pub use scan::scan_level_sets_with_progress;
pub use zip_inspect::classify_zip;
#[cfg(test)]
pub use zip_inspect::inspect_zip;
pub use zip_inspect::inspect_zip_with_progress;

#[cfg(test)]
mod cache_and_scan_tests;
#[cfg(test)]
mod conflict_tests;
#[cfg(test)]
mod identity_tests;
#[cfg(test)]
mod map_management_tests;
#[cfg(test)]
mod mapset_version_tests;
#[cfg(test)]
mod plugin_tests;
#[cfg(test)]
mod requires_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod zip_tests;

pub(crate) mod preview;
pub(crate) mod preview_reader;
