//! Release 的坐标、版本、投影与请求各自拥有实现；MOD 和地图共用同一调用链。

mod assets;
mod client;
mod error;
pub(crate) use error::ReleaseError;
mod fetch;
pub(crate) mod pagination;
mod projection;
mod request;
pub(crate) use request::RepositoryRequest;
mod transport;
mod version;

pub(crate) use crate::providers::{
    release_asset_reference, repository_cache_key, repository_coordinates, repository_file_name,
    repository_file_url,
};
pub(crate) use assets::select_install_asset;
pub(crate) use client::{APP_USER_AGENT, build_client, build_download_client};
pub(crate) use fetch::{fetch_releases, fetch_repository_default_branch};
pub(crate) use projection::filter_channel;
pub(crate) use version::{compare_release_versions, is_prerelease_version, parse_version};

#[cfg(test)]
pub(crate) use assets::asset_matches;
#[cfg(test)]
pub(crate) use projection::parse_release_json;
#[cfg(test)]
pub(crate) use transport::fetch_release_api;
#[cfg(test)]
mod test_support;
