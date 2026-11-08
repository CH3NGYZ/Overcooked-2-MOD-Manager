//! 社区目录、官方归属、搜索状态、分页读取和落地各有唯一职责。

pub(crate) const DISCOVERY_ENABLED: bool = false;

mod catalog;
pub(crate) mod commands;
mod gitcode;
mod github;
mod merge;
mod ownership;
mod state;

#[cfg(test)]
pub use catalog::{GITEE_SOURCE_ID, SOURCE_ID, source};
pub(crate) use catalog::{REQUIRED_TOPICS, SearchResults, repository_from_coordinates};
pub use catalog::{is_repository, is_source, is_source_archive, source_id};
pub(crate) use ownership::visible_discoveries;
pub use state::CommunitySearch;
pub(crate) use state::SearchRateLimits;

#[cfg(test)]
mod test_support;
