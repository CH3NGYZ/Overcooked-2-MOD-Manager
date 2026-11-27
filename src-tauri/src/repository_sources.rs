//! 仓库源的位置、声明、传输与缓存各归唯一职责；地图源复用同一读取链。

pub(crate) mod apply;
mod cache;
mod diagnostics;
mod document;
mod embedded;
mod location;
mod package;
pub(crate) mod replacements;
mod transport;
mod verified;

pub(crate) use cache::clear_source_cache;
pub(crate) use diagnostics::{error_category, safe_log_address};
pub(crate) use document::RepositorySourceDocument;
#[cfg(test)]
pub(crate) use document::parse_document;
pub(crate) use document::{nickname_fallback, source_address_key};
pub(crate) use embedded::{BUILTIN_INDEX, DEFAULT_INDEX_ID, bundled_repository_configs};
pub(crate) use location::SourceLocation;
pub(crate) use package::{SourcePackage, fetch as fetch_source_package};
pub(crate) use replacements::ReplacementSource;
pub(crate) use transport::package_document_path;
pub(crate) use transport::{load_document, load_text_document};
#[cfg(test)]
pub(crate) use verified::fetch_document_bytes;
pub(crate) use verified::{VerifiedDocument, load_verified_text_document};

mod preflight;
pub(crate) use preflight::{StartupProbe, cancel_startup_probe, probe_startup_source};
