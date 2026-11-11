//! 地图源的文档、源树、Release 条目与资产版本各有唯一实现；复用仓库源和下载管线。
pub(crate) mod progress;

mod assets;
mod community;
mod context;
mod document;
mod identity;
mod release_tags;
mod releases;
mod state;
mod transport;
mod tree;
pub(crate) mod view;

pub(crate) use context::MapSourceContext;
#[cfg(test)]
pub(crate) use document::parse_document;
pub(crate) use releases::{fetch_map_release_data, select_map_stage_asset};
pub(crate) use state::{
    available_maps, map_is_available, replace_maps_keeping_runtime_state, same_release_binding,
};
pub(crate) use tree::{
    bundled_map_tree, fill_missing_source_names, load_map_branch, load_map_tree,
};

#[cfg(test)]
mod bundled_tests;
#[cfg(test)]
mod declaration_tests;
#[cfg(test)]
mod refresh_tests;
#[cfg(test)]
mod tag_mode_tests;
#[cfg(test)]
mod tests;
