//! 仓库源的位置、声明、传输与缓存各归唯一职责；地图源复用同一读取链。

pub(crate) mod apply;
mod cache;
mod diagnostics;
mod document;
mod embedded;
mod location;
mod transport;
mod verified;

pub(crate) use cache::clear_source_cache;
pub(crate) use diagnostics::{error_category, safe_log_address};
#[cfg(test)]
pub(crate) use document::parse_document;
pub(crate) use document::{nickname_fallback, source_address_key};
pub(crate) use embedded::{
    BUILTIN_INDEX, DEFAULT_INDEX_GITEE_URL, DEFAULT_INDEX_GITHUB_URL, DEFAULT_INDEX_ID,
    bundled_repository_configs, default_index_url,
};
pub(crate) use location::SourceLocation;
pub(crate) use transport::{SOURCE_DOCUMENT_TIMEOUT, load_document, load_text_document};
pub(crate) use verified::{VerifiedDocument, fetch_document_bytes, load_verified_text_document};

mod preflight;
pub(crate) use preflight::{StartupProbe, cancel_startup_probe, probe_startup_source};
