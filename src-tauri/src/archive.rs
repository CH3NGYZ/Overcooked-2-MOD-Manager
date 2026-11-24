//! Download transport, archive installation and persisted installation metadata.

mod control;
mod install;
mod integrity;
mod manifests;
mod map_snapshots;
mod paths;
mod progress;
pub(crate) mod release_download;
mod split_zip;
mod transfer;
mod verify_download;
mod zip_layout;

#[cfg(test)]
mod test_support;

pub(crate) use control::{
    app_update_in_progress, begin_app_update, cancel_download, is_download_cancelled,
    start_download,
};
#[cfg(test)]
pub(crate) use install::prepared_mod_zip_files;
pub(crate) use install::{
    copy_asset_pending, extract_framework_zip_pending, extract_local_mod_zip_pending,
    extract_prepared_mod_zip, prepare_repository_mod_zip,
};
#[cfg(windows)]
pub(crate) use integrity::sha256_reader;
pub(crate) use integrity::{
    validate_download_bytes, validate_download_digest, validate_download_file,
    validate_download_size, validate_zip, validate_zip_with_cancel,
};
pub(crate) use manifests::{
    InstallManifest, new_manifest, primary_translation, read_manifest, read_manifests,
    repository_package_names, single_language, write_manifest,
};
pub(crate) use map_snapshots::{read_map_relation_snapshots, write_map_relation_snapshots};
pub(crate) use paths::{clear_readonly, prune_empty_parent_dirs, validate_asset_file_name};
pub(crate) use progress::{
    DownloadPhase, DownloadProgress, DownloadRaceProgress, DownloadReporter, DownloadRoute,
    DownloadSourcePhase, DownloadSourceProgress, channel_download_progress, emit_progress,
    interactive_download_progress, observe_download_progress, publish_download_progress,
};
pub(crate) use transfer::{
    download_to_temp_with_threads_and_first_byte_timeout, first_nonempty_chunk,
    send_before_first_byte_deadline,
};
pub(crate) use verify_download::verify_temp_download;
